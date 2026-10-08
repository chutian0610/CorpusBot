use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use crate::audit::{AuditSink, FileAuditSink};
use crate::error::{AgentError, Result};
use crate::llm::{LlmClient, LlmRequest, LlmResponse, StructuredOutput};
use crate::workflow::{
    Transition, WorkflowContext, WorkflowKernel, WorkflowNode, WorkflowNodeHandler, WorkflowOutcome,
};
use async_trait::async_trait;
use serde::{
    Deserialize, Serialize,
    de::{Deserializer, SeqAccess, Visitor},
};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EntityAnalysis {
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub summary: String,
    #[serde(default)]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub importance: Option<Importance>,
    #[serde(default)]
    pub evidence: Vec<CandidateEvidence>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConceptAnalysis {
    pub name: String,
    #[serde(default, alias = "summary")]
    pub definition: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub importance: Option<Importance>,
    #[serde(default)]
    pub evidence: Vec<CandidateEvidence>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Importance {
    Core,
    Supporting,
    Incidental,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CandidateEvidence {
    pub quote: String,
    #[serde(default)]
    pub section: Option<String>,
}

fn string_or_vec<'de, D>(deserializer: D) -> std::result::Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct StringOrVec;

    impl<'de> Visitor<'de> for StringOrVec {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string or array of strings")
        }

        fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            let value = value.trim();
            Ok(if value.is_empty() {
                Vec::new()
            } else {
                vec![value.to_owned()]
            })
        }

        fn visit_unit<E>(self) -> std::result::Result<Self::Value, E> {
            Ok(Vec::new())
        }

        fn visit_seq<S>(self, mut access: S) -> std::result::Result<Self::Value, S::Error>
        where
            S: SeqAccess<'de>,
        {
            let mut values = Vec::new();
            while let Some(value) = access.next_element::<String>()? {
                values.push(value);
            }
            Ok(values)
        }
    }

    deserializer.deserialize_any(StringOrVec)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DraftSection {
    pub heading: String,
    #[serde(default, deserialize_with = "string_or_vec")]
    pub paragraphs: Vec<String>,
    #[serde(default, deserialize_with = "string_or_vec")]
    pub bullets: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceAnalysis {
    pub title: String,
    pub summary: String,
    #[serde(default)]
    pub entities: Vec<EntityAnalysis>,
    #[serde(default)]
    pub concepts: Vec<ConceptAnalysis>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EntityDraft {
    pub name: String,
    #[serde(default, deserialize_with = "string_or_vec")]
    pub aliases: Vec<String>,
    pub summary: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub related: Vec<String>,
    #[serde(default)]
    pub sections: Vec<DraftSection>,
    #[serde(default)]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub importance: Option<Importance>,
    #[serde(default)]
    pub evidence: Vec<CandidateEvidence>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConceptDraft {
    pub name: String,
    pub definition: String,
    #[serde(default, deserialize_with = "string_or_vec")]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub related: Vec<String>,
    #[serde(default)]
    pub sections: Vec<DraftSection>,
    #[serde(default)]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub importance: Option<Importance>,
    #[serde(default)]
    pub evidence: Vec<CandidateEvidence>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DraftPlan {
    #[serde(default)]
    pub source_summary: String,
    #[serde(default)]
    pub entities: Vec<EntityDraft>,
    #[serde(default)]
    pub concepts: Vec<ConceptDraft>,
}

#[derive(Clone, Debug)]
pub enum AnalysisCandidate {
    Entity(EntityAnalysis),
    Concept(ConceptAnalysis),
}

#[derive(Serialize)]
struct CandidateBatchJson<'a> {
    pages: Vec<CandidateBatchPage<'a>>,
}

impl<'a> CandidateBatchJson<'a> {
    fn new(batch: &'a [AnalysisCandidate]) -> Self {
        Self {
            pages: batch
                .iter()
                .map(|candidate| match candidate {
                    AnalysisCandidate::Entity(entity) => CandidateBatchPage::Entity(entity),
                    AnalysisCandidate::Concept(concept) => CandidateBatchPage::Concept(concept),
                })
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "page_type", rename_all = "snake_case")]
enum CandidateBatchPage<'a> {
    Entity(&'a EntityAnalysis),
    Concept(&'a ConceptAnalysis),
}

#[derive(Deserialize)]
#[serde(tag = "page_type", rename_all = "snake_case")]
enum BatchDraftPage {
    Entity(EntityDraft),
    Concept(ConceptDraft),
}

pub const ANALYZE_PROMPT_ID: &str = "analyze-source-v2";
pub const DRAFT_PROMPT_ID: &str = "generate-drafts-v5";
pub const QUERY_PROMPT_ID: &str = "answer-query-v1";

pub struct SourceAgent<C> {
    client: Arc<C>,
    max_draft_tokens: u64,
}

impl<C> SourceAgent<C>
where
    C: LlmClient + 'static,
{
    pub fn new(client: C) -> Self {
        Self {
            client: Arc::new(client),
            max_draft_tokens: crate::config::DEFAULT_DRAFT_MAX_TOKENS,
        }
    }

    pub fn with_max_draft_tokens(mut self, max_draft_tokens: u64) -> Self {
        self.max_draft_tokens = max_draft_tokens;
        self
    }

    pub async fn analyze_source(
        &self,
        source_title: &str,
        markdown: &str,
    ) -> Result<SourceAnalysis> {
        let request = analyze_request(source_title, markdown, self.max_draft_tokens);
        let response = self.client.complete(request).await?;
        parse_json(&response)
    }

    pub async fn generate_drafts(
        &self,
        template_name: &str,
        source_title: &str,
        source_markdown: &str,
        analysis: &SourceAnalysis,
        related_pages: &[(String, String, String)],
    ) -> Result<DraftPlan> {
        let template = corpusbot_core::Template::ALL
            .into_iter()
            .find(|template| template.as_str() == template_name)
            .ok_or_else(|| {
                AgentError::Configuration(format!("unknown template: {template_name}"))
            })?;
        let workspace_root = tempfile::tempdir()?;
        let plan = self
            .generate_drafts_audited(
                workspace_root.path(),
                "non-audited-generate-drafts",
                "manifest",
                template,
                source_title,
                source_markdown,
                analysis,
                related_pages,
            )
            .await?;
        workspace_root.close()?;
        Ok(plan)
    }

    pub async fn answer_question(
        &self,
        question: &str,
        context: &[QueryContextPage],
    ) -> Result<QueryAnswer> {
        let request = query_request(question, context);
        let response = self.client.complete(request).await?;
        let answer: RawQueryAnswer = parse_json(&response)?;
        Ok(Self::validate_answer(answer, context))
    }

    pub fn validate_answer(answer: RawQueryAnswer, context: &[QueryContextPage]) -> QueryAnswer {
        let mut citations = Vec::new();
        let mut warnings = Vec::new();
        for raw in answer.citations {
            let Some(page) = context
                .iter()
                .find(|page| page.path == raw.path && page.revision.key() == raw.revision.key())
            else {
                warnings.push("citation removed: unknown path or revision".to_owned());
                continue;
            };
            let quote = normalize_whitespace(&raw.quote);
            let content = normalize_whitespace(&page.markdown);
            if quote.is_empty() || !content.contains(&quote) {
                warnings.push("citation quote was not found".to_owned());
                continue;
            }
            citations.push(Citation {
                number: citations.len() as u32 + 1,
                path: page.path.clone(),
                title: page.title.clone(),
                quote: raw.quote,
                resource_revision: page.revision.clone(),
            });
        }

        let insufficient_evidence = citations.is_empty();
        let final_answer = if insufficient_evidence {
            warnings.push("all citations were invalid".to_owned());
            "当前 Wiki 证据不足，无法给出可验证的回答。".to_owned()
        } else {
            answer.answer
        };
        QueryAnswer {
            answer: final_answer,
            citations,
            revision_manifest_id: context
                .first()
                .map_or_else(String::new, |page| page.revision_manifest_id.clone()),
            warnings,
            insufficient_evidence,
        }
    }

    pub async fn analyze_source_audited(
        &self,
        workspace_root: &Path,
        run_id: &str,
        manifest_id: &str,
        source_title: &str,
        markdown: &str,
    ) -> Result<SourceAnalysis> {
        let sink = Arc::new(FileAuditSink::new(workspace_root));
        let handler = Arc::new(AnalyzeHandler {
            client: self.client.clone(),
            audit: sink.clone(),
            max_draft_tokens: self.max_draft_tokens,
            source_title: source_title.to_owned(),
            markdown: markdown.to_owned(),
        });
        let kernel: WorkflowKernel<AnalysisState> = WorkflowKernel::new(WorkflowNode::Analyze)
            .handler(WorkflowNode::Analyze, handler)
            .handler(
                WorkflowNode::ValidateAnalysis,
                Arc::new(ValidateAnalysisHandler),
            );
        let context = WorkflowContext::new(
            AnalysisState { analysis: None },
            run_id,
            Some(manifest_id.to_owned()),
            sink,
        );
        match kernel.run(context).await? {
            WorkflowOutcome::Completed { state } => state
                .analysis
                .ok_or_else(|| AgentError::Schema("workflow produced no analysis".to_owned())),
            WorkflowOutcome::Rejected { reason } => Err(AgentError::Schema(reason)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn generate_drafts_audited(
        &self,
        workspace_root: &Path,
        run_id: &str,
        manifest_id: &str,
        template: corpusbot_core::Template,
        source_title: &str,
        source_markdown: &str,
        analysis: &SourceAnalysis,
        related_pages: &[(String, String, String)],
    ) -> Result<DraftPlan> {
        let sink = Arc::new(FileAuditSink::new(workspace_root));
        let candidates = normalize_analysis(analysis, template);
        let analysis_candidates = candidates
            .entities
            .iter()
            .cloned()
            .map(AnalysisCandidate::Entity)
            .chain(
                candidates
                    .concepts
                    .iter()
                    .cloned()
                    .map(AnalysisCandidate::Concept),
            )
            .collect::<Vec<_>>();
        let batches = draft_candidate_batches(&analysis_candidates);
        let handler = Arc::new(DraftHandler {
            client: self.client.clone(),
            audit: sink.clone(),
            max_draft_tokens: self.max_draft_tokens,
            template,
            source_title: source_title.to_owned(),
            source_excerpts: source_excerpts(source_markdown, &analysis_candidates),
            batches,
            related_pages: related_pages.to_vec(),
        });
        let kernel: WorkflowKernel<DraftState> = WorkflowKernel::new(WorkflowNode::GenerateDraft)
            .handler(WorkflowNode::GenerateDraft, handler)
            .handler(
                WorkflowNode::ValidateDraft,
                Arc::new(ValidateDraftHandler { template }),
            );
        let context = WorkflowContext::new(
            DraftState {
                plan: None,
                completed_batches: BTreeSet::new(),
            },
            run_id,
            Some(manifest_id.to_owned()),
            sink,
        );
        match kernel.run(context).await? {
            WorkflowOutcome::Completed { state } => state
                .plan
                .map(|mut plan| {
                    plan.source_summary = analysis.summary.clone();
                    plan
                })
                .ok_or_else(|| AgentError::Schema("workflow produced no draft plan".to_owned())),
            WorkflowOutcome::Rejected { reason } => Err(AgentError::Schema(reason)),
        }
    }

    pub async fn answer_question_audited(
        &self,
        workspace_root: &Path,
        run_id: &str,
        manifest_id: &str,
        question: &str,
        context_pages: &[QueryContextPage],
    ) -> Result<QueryAnswer> {
        let sink = Arc::new(FileAuditSink::new(workspace_root));
        let initial = QueryState {
            context: context_pages.to_vec(),
            answer: None,
        };
        let retrieve = Arc::new(RetrieveContextHandler);
        let answer = Arc::new(AnswerHandler {
            client: self.client.clone(),
            audit: sink.clone(),
            question: question.to_owned(),
        });
        let validate = Arc::new(ValidateAnswerHandler {
            manifest_id: manifest_id.to_owned(),
        });
        let kernel: WorkflowKernel<QueryState> = WorkflowKernel::new(WorkflowNode::RetrieveContext)
            .handler(WorkflowNode::RetrieveContext, retrieve)
            .handler(WorkflowNode::GenerateDraft, answer)
            .handler(WorkflowNode::ValidateDraft, validate);
        let context = WorkflowContext::new(initial, run_id, Some(manifest_id.to_owned()), sink);
        match kernel.run(context).await? {
            WorkflowOutcome::Completed { state } => state
                .answer
                .ok_or_else(|| AgentError::Schema("workflow produced no answer".to_owned())),
            WorkflowOutcome::Rejected { reason } => Err(AgentError::Schema(reason)),
        }
    }
}

#[derive(Clone, Debug, Default)]
struct AnalysisState {
    analysis: Option<SourceAnalysis>,
}

#[derive(Clone, Debug, Default)]
struct DraftState {
    plan: Option<DraftPlan>,
    completed_batches: BTreeSet<usize>,
}

#[derive(Clone, Debug, Default)]
struct QueryState {
    context: Vec<QueryContextPage>,
    answer: Option<QueryAnswer>,
}

struct AnalyzeHandler<C> {
    client: Arc<C>,
    audit: Arc<FileAuditSink>,
    max_draft_tokens: u64,
    source_title: String,
    markdown: String,
}

#[async_trait]
impl<C: LlmClient + 'static> WorkflowNodeHandler<AnalysisState> for AnalyzeHandler<C> {
    async fn run(
        &self,
        context: &mut WorkflowContext<AnalysisState>,
    ) -> Result<Transition<AnalysisState>> {
        let request = analyze_request(&self.source_title, &self.markdown, self.max_draft_tokens);
        let (response_ref, decision, parsed) = complete_audited(
            self.client.as_ref(),
            &self.audit,
            WorkflowNode::Analyze,
            context,
            &request,
            "current",
        )
        .await?;
        context.set_output_ref(response_ref);
        context.set_decision(decision);
        match parsed {
            Some(analysis) => Ok(Transition::Next {
                state: AnalysisState {
                    analysis: Some(analysis),
                },
                node: WorkflowNode::ValidateAnalysis,
            }),
            None => Ok(Transition::Retry {
                state: context.state.clone(),
                reason: if output_limit_reached(context) {
                    "analysis output reached token limit".to_owned()
                } else {
                    "analysis output did not match schema".to_owned()
                },
            }),
        }
    }
}

struct ValidateAnalysisHandler;

#[async_trait]
impl WorkflowNodeHandler<AnalysisState> for ValidateAnalysisHandler {
    async fn run(
        &self,
        context: &mut WorkflowContext<AnalysisState>,
    ) -> Result<Transition<AnalysisState>> {
        let Some(analysis) = &context.state.analysis else {
            context.set_decision("missing analysis");
            return Ok(Transition::Retry {
                state: context.state.clone(),
                reason: "analysis is missing".to_owned(),
            });
        };
        if analysis.title.trim().is_empty() || analysis.summary.trim().is_empty() {
            context.set_decision("analysis rejected");
            return Ok(Transition::Retry {
                state: context.state.clone(),
                reason: "analysis title or summary is empty".to_owned(),
            });
        }
        context.set_decision("analysis accepted");
        Ok(Transition::Done {
            state: context.state.clone(),
        })
    }
}

struct DraftHandler<C> {
    client: Arc<C>,
    audit: Arc<FileAuditSink>,
    max_draft_tokens: u64,
    template: corpusbot_core::Template,
    source_title: String,
    source_excerpts: String,
    batches: Vec<Vec<AnalysisCandidate>>,
    related_pages: Vec<(String, String, String)>,
}

#[async_trait]
impl<C: LlmClient + 'static> WorkflowNodeHandler<DraftState> for DraftHandler<C> {
    async fn run(
        &self,
        context: &mut WorkflowContext<DraftState>,
    ) -> Result<Transition<DraftState>> {
        let mut completed_batches = context.state.completed_batches.clone();
        let mut merged = context.state.plan.clone().unwrap_or_default();
        merged.source_summary = String::new();
        for (batch_index, batch) in self.batches.iter().enumerate() {
            if completed_batches.contains(&batch_index) {
                continue;
            }
            let batch_json = serde_json::to_string_pretty(&CandidateBatchJson::new(batch))?;
            tracing::info!(
                operation = "generate_drafts",
                batch_index = batch_index + 1,
                batch_total = self.batches.len(),
                candidate_count = batch.len(),
                candidate_json = %batch_json,
                "draft batch request prepared"
            );
            let request = draft_request(
                self.template.as_str(),
                &self.source_title,
                &self.source_excerpts,
                &batch_json,
                batch_index + 1,
                self.batches.len(),
                self.related_pages.as_slice(),
                self.max_draft_tokens,
            )?;
            let (response_ref, decision, parsed): (
                String,
                &'static str,
                Option<serde_json::Value>,
            ) = complete_audited(
                self.client.as_ref(),
                &self.audit,
                WorkflowNode::GenerateDraft,
                context,
                &request,
                &format!("batch-{}", batch_index + 1),
            )
            .await?;
            context.set_output_ref(response_ref.clone());
            context.set_decision(format!(
                "batch {}/{}: {decision}",
                batch_index + 1,
                self.batches.len()
            ));
            match parsed {
                Some(raw_plan) => match draft_plan_from_value(raw_plan) {
                    Ok(plan) => {
                        merge_draft_plan(&mut merged, plan, self.template);
                        completed_batches.insert(batch_index);
                    }
                    Err(_) => {
                        tracing::error!(
                            operation = "generate_drafts",
                            batch_index = batch_index + 1,
                            batch_total = self.batches.len(),
                            candidate_count = batch.len(),
                            candidate_json = %batch_json,
                            response_artifact = %response_ref,
                            "draft batch page types could not be normalized"
                        );
                        return Ok(Transition::Retry {
                            state: DraftState {
                                plan: Some(merged),
                                completed_batches,
                            },
                            reason: format!(
                                "draft batch {}/{} did not match schema",
                                batch_index + 1,
                                self.batches.len()
                            ),
                        });
                    }
                },
                None => {
                    tracing::error!(
                        operation = "generate_drafts",
                        batch_index = batch_index + 1,
                        batch_total = self.batches.len(),
                        candidate_count = batch.len(),
                        candidate_json = %batch_json,
                        response_artifact = %response_ref,
                        "draft batch schema rejected"
                    );
                    return Ok(Transition::Retry {
                        state: DraftState {
                            plan: Some(merged),
                            completed_batches,
                        },
                        reason: if output_limit_reached(context) {
                            format!(
                                "draft batch {}/{} reached token limit",
                                batch_index + 1,
                                self.batches.len()
                            )
                        } else {
                            format!(
                                "draft batch {}/{} did not match schema",
                                batch_index + 1,
                                self.batches.len()
                            )
                        },
                    });
                }
            }
        }

        Ok(Transition::Next {
            state: DraftState {
                plan: Some(merged),
                completed_batches,
            },
            node: WorkflowNode::ValidateDraft,
        })
    }
}

struct ValidateDraftHandler {
    template: corpusbot_core::Template,
}

#[async_trait]
impl WorkflowNodeHandler<DraftState> for ValidateDraftHandler {
    async fn run(
        &self,
        context: &mut WorkflowContext<DraftState>,
    ) -> Result<Transition<DraftState>> {
        let Some(plan) = &context.state.plan else {
            context.set_decision("draft rejected");
            return Ok(Transition::Retry {
                state: context.state.clone(),
                reason: "draft plan is missing".to_owned(),
            });
        };
        if let Some(reason) = validate_draft_plan(plan, self.template) {
            context.set_decision("draft rejected");
            return Ok(Transition::Retry {
                state: context.state.clone(),
                reason,
            });
        }
        context.set_decision("draft accepted");
        Ok(Transition::Done {
            state: context.state.clone(),
        })
    }
}

struct RetrieveContextHandler;

#[async_trait]
impl WorkflowNodeHandler<QueryState> for RetrieveContextHandler {
    async fn run(
        &self,
        context: &mut WorkflowContext<QueryState>,
    ) -> Result<Transition<QueryState>> {
        context.set_decision(format!(
            "captured {} evidence pages",
            context.state.context.len()
        ));
        Ok(Transition::Next {
            state: context.state.clone(),
            node: WorkflowNode::GenerateDraft,
        })
    }
}

struct AnswerHandler<C> {
    client: Arc<C>,
    audit: Arc<FileAuditSink>,
    question: String,
}

#[async_trait]
impl<C: LlmClient + 'static> WorkflowNodeHandler<QueryState> for AnswerHandler<C> {
    async fn run(
        &self,
        context: &mut WorkflowContext<QueryState>,
    ) -> Result<Transition<QueryState>> {
        let request = query_request(&self.question, &context.state.context);
        let (response_ref, decision, parsed) = complete_audited(
            self.client.as_ref(),
            &self.audit,
            WorkflowNode::GenerateDraft,
            context,
            &request,
            "1",
        )
        .await?;
        context.set_output_ref(response_ref);
        context.set_decision(decision);
        match parsed {
            Some(raw_answer) => {
                let mut answer =
                    SourceAgent::<C>::validate_answer(raw_answer, &context.state.context);
                answer.revision_manifest_id = context.input_manifest_id.clone().unwrap_or_default();
                Ok(Transition::Next {
                    state: QueryState {
                        context: context.state.context.clone(),
                        answer: Some(answer),
                    },
                    node: WorkflowNode::ValidateDraft,
                })
            }
            None => Ok(Transition::Retry {
                state: context.state.clone(),
                reason: "answer output did not match schema".to_owned(),
            }),
        }
    }
}

struct ValidateAnswerHandler {
    manifest_id: String,
}

#[async_trait]
impl WorkflowNodeHandler<QueryState> for ValidateAnswerHandler {
    async fn run(
        &self,
        context: &mut WorkflowContext<QueryState>,
    ) -> Result<Transition<QueryState>> {
        let Some(answer) = &context.state.answer else {
            context.set_decision("answer rejected");
            return Ok(Transition::Retry {
                state: context.state.clone(),
                reason: "answer is missing".to_owned(),
            });
        };
        if answer.revision_manifest_id != self.manifest_id {
            context.set_decision("answer rejected");
            return Ok(Transition::Reject {
                reason: "answer manifest does not match retrieved context".to_owned(),
            });
        }
        for citation in &answer.citations {
            if !context.state.context.iter().any(|page| {
                page.path == citation.path
                    && page.revision == citation.resource_revision
                    && normalize_whitespace(page.markdown.as_str())
                        .contains(&normalize_whitespace(&citation.quote))
            }) {
                context.set_decision("answer rejected");
                return Ok(Transition::Reject {
                    reason: "answer citation is not verifiable".to_owned(),
                });
            }
        }
        context.set_decision(format!(
            "answer accepted with {} citations",
            answer.citations.len()
        ));
        Ok(Transition::Done {
            state: context.state.clone(),
        })
    }
}

fn analyze_request(source_title: &str, markdown: &str, max_tokens: u64) -> LlmRequest {
    LlmRequest {
        operation: "analyze_source".to_owned(),
        system: ANALYZE_SYSTEM.to_owned(),
        prompt: format!("# Source title\n\n{source_title}\n\n# Source markdown\n\n{markdown}"),
        prompt_template_id: ANALYZE_PROMPT_ID.to_owned(),
        temperature: Some(0.1),
        max_tokens: Some(max_tokens),
        structured_output: None,
    }
}

fn draft_response_format() -> StructuredOutput {
    let string_array = serde_json::json!({
        "type": "array",
        "items": {"type": "string"}
    });
    let evidence = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["quote", "section"],
        "properties": {
            "quote": {"type": "string"},
            "section": {"type": "string"}
        }
    });
    let section = serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["heading", "paragraphs", "bullets"],
        "properties": {
            "heading": {"type": "string"},
            "paragraphs": {"type": "array", "items": {"type": "string"}},
            "bullets": {"type": "array", "items": {"type": "string"}}
        }
    });
    let page_fields = serde_json::json!({
        "aliases": string_array,
        "tags": string_array,
        "related": string_array,
        "sections": {"type": "array", "items": section},
        "confidence": {"type": "number", "minimum": 0.0, "maximum": 1.0},
        "importance": {
            "type": "string",
            "enum": ["core", "supporting", "incidental"]
        },
        "evidence": {"type": "array", "items": evidence}
    });
    let mut entity = page_fields.clone();
    let entity_object = entity.as_object_mut().expect("entity fields are an object");
    entity_object.insert("page_type".to_owned(), serde_json::json!("entity"));
    entity_object.insert("name".to_owned(), serde_json::json!({"type": "string"}));
    entity_object.insert("summary".to_owned(), serde_json::json!({"type": "string"}));
    let mut concept = page_fields;
    let concept_object = concept
        .as_object_mut()
        .expect("concept fields are an object");
    concept_object.insert("page_type".to_owned(), serde_json::json!("concept"));
    concept_object.insert("name".to_owned(), serde_json::json!({"type": "string"}));
    concept_object.insert(
        "definition".to_owned(),
        serde_json::json!({"type": "string"}),
    );

    StructuredOutput {
        name: "corpusbot_draft_batch".to_owned(),
        schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["pages"],
            "properties": {
                "pages": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 4,
                    "items": {
                        "anyOf": [
                            {
                                "type": "object",
                                "additionalProperties": false,
                                "required": [
                                    "page_type", "name", "aliases", "summary", "tags",
                                    "related", "sections", "confidence", "importance", "evidence"
                                ],
                                "properties": entity
                            },
                            {
                                "type": "object",
                                "additionalProperties": false,
                                "required": [
                                    "page_type", "name", "aliases", "definition", "tags",
                                    "related", "sections", "confidence", "importance", "evidence"
                                ],
                                "properties": concept
                            }
                        ]
                    }
                }
            }
        }),
    }
}

#[allow(clippy::too_many_arguments)]
fn draft_request(
    template_name: &str,
    source_title: &str,
    source_excerpts: &str,
    candidate_json: &str,
    batch_index: usize,
    batch_count: usize,
    related_pages: &[(String, String, String)],
    max_tokens: u64,
) -> Result<LlmRequest> {
    let related = related_pages
        .iter()
        .map(|(path, title, excerpt)| format!("## {path}\n\nTitle: {title}\n\n{excerpt}"))
        .collect::<Vec<_>>()
        .join("\n\n");
    Ok(LlmRequest {
        operation: "generate_drafts".to_owned(),
        system: DRAFT_SYSTEM.replace("{template}", template_name),
        prompt: format!(
            "# Source title\n\n{source_title}\n\n# Source excerpts\n\n{source_excerpts}\n\n# Candidates for this batch\n\n{candidate_json}\n\n# Existing related pages\n\n{related}\n\n# Batch\n\n{batch_index} of {batch_count}"
        ),
        prompt_template_id: DRAFT_PROMPT_ID.to_owned(),
        temperature: Some(0.2),
        max_tokens: Some(max_tokens),
        structured_output: Some(draft_response_format()),
    })
}

fn query_request(question: &str, context: &[QueryContextPage]) -> LlmRequest {
    let evidence = context
        .iter()
        .enumerate()
        .map(|(index, page)| {
            format!(
                "[{}] path: {}\ntitle: {}\npage_type: {}\nrevision: {}\ncontent:\n{}",
                index + 1,
                page.path,
                page.title,
                page.page_type,
                page.revision.key(),
                page.markdown
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    LlmRequest {
        operation: "answer_query".to_owned(),
        system: QUERY_SYSTEM.to_owned(),
        prompt: format!("# Question\n\n{question}\n\n# Evidence\n\n{evidence}"),
        prompt_template_id: QUERY_PROMPT_ID.to_owned(),
        temperature: Some(0.1),
        max_tokens: Some(3000),
        structured_output: None,
    }
}

async fn complete_audited<C: LlmClient, S, T>(
    client: &C,
    audit: &FileAuditSink,
    node: WorkflowNode,
    context: &mut WorkflowContext<S>,
    request: &LlmRequest,
    artifact_label: &str,
) -> Result<(String, &'static str, Option<T>)>
where
    T: serde::de::DeserializeOwned,
{
    let attempt = context.attempt(node);
    let request_json = serde_json::to_vec_pretty(request)?;
    let artifact_name = if artifact_label == "current" {
        format!("{}-{attempt}-request.json", node_name(node))
    } else {
        format!(
            "{}-{artifact_label}-{attempt}-request.json",
            node_name(node)
        )
    };
    let request_ref = audit
        .store_artifact(&context.run_id.clone(), &artifact_name, &request_json)
        .await?;
    context.set_output_ref(request_ref.clone());
    let started = Instant::now();
    match client.complete(request.clone()).await {
        Ok(response) => {
            let latency = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
            let artifact_name = if artifact_label == "current" {
                format!("{}-{attempt}-response.json", node_name(node))
            } else {
                format!(
                    "{}-{artifact_label}-{attempt}-response.json",
                    node_name(node)
                )
            };
            let response_ref = audit
                .store_artifact(
                    &context.run_id.clone(),
                    &artifact_name,
                    &serde_json::to_vec_pretty(&serde_json::json!({
                        "provider": response.provider,
                        "model": response.model,
                        "response_id": response.response_id,
                        "text": response.text,
                        "max_tokens": request.max_tokens,
                        "finish_reason": response.finish_reason,
                        "truncated": response.finish_reason.as_deref() == Some("length"),
                    }))?,
                )
                .await?;
            context.record_llm_response(&response, request, latency);
            match parse_json::<T>(&response) {
                Ok(parsed) => Ok((response_ref, "llm output parsed", Some(parsed))),
                Err(error) => {
                    let truncated = response.finish_reason.as_deref() == Some("length");
                    tracing::error!(
                        operation = %request.operation,
                        node = %node_name(node),
                        provider = %response.provider,
                        model = %response.model,
                        prompt_tokens = response.prompt_tokens,
                        completion_tokens = response.completion_tokens,
                        max_tokens = request.max_tokens,
                        finish_reason = response.finish_reason,
                        artifact = %response_ref,
                        error = %error,
                        text_preview = %truncate_chars(&response.text, 500),
                        "LLM output failed schema parsing"
                    );
                    context.set_decision(if truncated {
                        "schema rejected: output token limit reached"
                    } else {
                        "schema rejected"
                    });
                    context.set_output_ref(response_ref.clone());
                    Ok((response_ref, "schema rejected", None))
                }
            }
        }
        Err(_error) => {
            context.set_decision("provider call failed");
            Ok((request_ref, "provider failed", None))
        }
    }
}

fn node_name(node: WorkflowNode) -> &'static str {
    match node {
        WorkflowNode::Analyze => "analyze",
        WorkflowNode::ValidateAnalysis => "validate-analysis",
        WorkflowNode::RepairAnalysis => "repair-analysis",
        WorkflowNode::RetrieveContext => "retrieve-context",
        WorkflowNode::GenerateDraft => "generate-draft",
        WorkflowNode::ValidateDraft => "validate-draft",
        WorkflowNode::RepairDraft => "repair-draft",
        WorkflowNode::SelfAudit => "self-audit",
        WorkflowNode::Commit => "commit",
    }
}

fn output_limit_reached<S>(context: &WorkflowContext<S>) -> bool {
    context
        .last_response()
        .is_some_and(|response| response.finish_reason.as_deref() == Some("length"))
}

fn normalize_analysis(
    analysis: &SourceAnalysis,
    template: corpusbot_core::Template,
) -> SourceAnalysis {
    let mut entities = BTreeMap::new();
    for candidate in &analysis.entities {
        let Ok(identity) = corpusbot_core::PageIdentity::new(
            template,
            corpusbot_core::PageType::Entity,
            &candidate.name,
        ) else {
            continue;
        };
        let confidence = normalize_confidence(candidate.confidence);
        let importance = normalize_importance(candidate.importance, confidence);
        let candidate = EntityAnalysis {
            name: identity.canonical_name().to_owned(),
            aliases: candidate.aliases.clone(),
            summary: candidate.summary.trim().to_owned(),
            confidence: Some(confidence),
            importance: Some(importance),
            evidence: candidate.evidence.clone(),
        };
        merge_entity_candidate(&mut entities, candidate);
    }

    let mut concepts = BTreeMap::new();
    for candidate in &analysis.concepts {
        let Ok(identity) = corpusbot_core::PageIdentity::new(
            template,
            corpusbot_core::PageType::Concept,
            &candidate.name,
        ) else {
            continue;
        };
        let confidence = normalize_confidence(candidate.confidence);
        let importance = normalize_importance(candidate.importance, confidence);
        let candidate = ConceptAnalysis {
            name: identity.canonical_name().to_owned(),
            definition: candidate.definition.trim().to_owned(),
            aliases: candidate.aliases.clone(),
            confidence: Some(confidence),
            importance: Some(importance),
            evidence: candidate.evidence.clone(),
        };
        merge_concept_candidate(&mut concepts, candidate);
    }

    let mut entities = entities.into_values().collect::<Vec<_>>();
    let mut concepts = concepts.into_values().collect::<Vec<_>>();
    entities.sort_by(|left, right| {
        importance_rank(left.importance)
            .cmp(&importance_rank(right.importance))
            .then(
                normalize_confidence(left.confidence)
                    .total_cmp(&normalize_confidence(right.confidence))
                    .reverse(),
            )
            .then(left.name.cmp(&right.name))
    });
    concepts.sort_by(|left, right| {
        importance_rank(left.importance)
            .cmp(&importance_rank(right.importance))
            .then(
                normalize_confidence(left.confidence)
                    .total_cmp(&normalize_confidence(right.confidence))
                    .reverse(),
            )
            .then(left.name.cmp(&right.name))
    });
    SourceAnalysis {
        title: analysis.title.clone(),
        summary: analysis.summary.clone(),
        entities,
        concepts,
    }
}

fn normalize_confidence(value: Option<f32>) -> f32 {
    match value {
        Some(value) if value > 1.0 && value <= 100.0 => (value / 100.0).clamp(0.0, 1.0),
        Some(value) => value.clamp(0.0, 1.0),
        None => 0.6,
    }
}

fn normalize_importance(value: Option<Importance>, confidence: f32) -> Importance {
    value.unwrap_or(if confidence >= 0.75 {
        Importance::Core
    } else {
        Importance::Supporting
    })
}

fn importance_rank(value: Option<Importance>) -> u8 {
    match value {
        Some(Importance::Core) => 0,
        Some(Importance::Supporting) => 1,
        Some(Importance::Incidental) => 2,
        None => 1,
    }
}

fn merge_entity_candidate(
    candidates: &mut BTreeMap<String, EntityAnalysis>,
    candidate: EntityAnalysis,
) {
    let key = corpusbot_core::PageIdentity::normalize(&candidate.name).unwrap_or_default();
    match candidates.get_mut(&key) {
        Some(existing) => {
            for alias in candidate.aliases {
                if !existing.aliases.contains(&alias) {
                    existing.aliases.push(alias);
                }
            }
            if importance_rank(candidate.importance) < importance_rank(existing.importance) {
                existing.importance = candidate.importance;
            }
            if normalize_confidence(candidate.confidence)
                > normalize_confidence(existing.confidence)
            {
                existing.summary = candidate.summary;
                existing.confidence = candidate.confidence;
            }
            for evidence in candidate.evidence {
                if !existing
                    .evidence
                    .iter()
                    .any(|existing| existing.quote == evidence.quote)
                {
                    existing.evidence.push(evidence);
                }
            }
        }
        None => {
            candidates.insert(key, candidate);
        }
    }
}

fn merge_concept_candidate(
    candidates: &mut BTreeMap<String, ConceptAnalysis>,
    candidate: ConceptAnalysis,
) {
    let key = corpusbot_core::PageIdentity::normalize(&candidate.name).unwrap_or_default();
    match candidates.get_mut(&key) {
        Some(existing) => {
            if importance_rank(candidate.importance) < importance_rank(existing.importance) {
                existing.importance = candidate.importance;
            }
            if normalize_confidence(candidate.confidence)
                > normalize_confidence(existing.confidence)
            {
                existing.definition = candidate.definition;
                existing.confidence = candidate.confidence;
            }
            for evidence in candidate.evidence {
                if !existing
                    .evidence
                    .iter()
                    .any(|existing| existing.quote == evidence.quote)
                {
                    existing.evidence.push(evidence);
                }
            }
        }
        None => {
            candidates.insert(key, candidate);
        }
    }
}

fn draft_candidate_batches(candidates: &[AnalysisCandidate]) -> Vec<Vec<AnalysisCandidate>> {
    let mut candidates = candidates.to_vec();
    candidates.sort_by(|left, right| {
        importance_rank(candidate_importance(left))
            .cmp(&importance_rank(candidate_importance(right)))
            .then(
                normalize_confidence(candidate_confidence(left))
                    .total_cmp(&normalize_confidence(candidate_confidence(right)))
                    .reverse(),
            )
            .then(candidate_name(left).cmp(candidate_name(right)))
    });
    candidates.chunks(4).map(|batch| batch.to_vec()).collect()
}

fn candidate_importance(candidate: &AnalysisCandidate) -> Option<Importance> {
    match candidate {
        AnalysisCandidate::Entity(entity) => entity.importance,
        AnalysisCandidate::Concept(concept) => concept.importance,
    }
}

fn candidate_confidence(candidate: &AnalysisCandidate) -> Option<f32> {
    match candidate {
        AnalysisCandidate::Entity(entity) => entity.confidence,
        AnalysisCandidate::Concept(concept) => concept.confidence,
    }
}

fn candidate_name(candidate: &AnalysisCandidate) -> &str {
    match candidate {
        AnalysisCandidate::Entity(entity) => &entity.name,
        AnalysisCandidate::Concept(concept) => &concept.name,
    }
}

fn source_excerpts(markdown: &str, candidates: &[AnalysisCandidate]) -> String {
    let mut terms = Vec::new();
    for candidate in candidates {
        terms.push(candidate_name(candidate).to_lowercase());
        match candidate {
            AnalysisCandidate::Entity(entity) => {
                terms.extend(entity.aliases.iter().map(|alias| alias.to_lowercase()));
            }
            AnalysisCandidate::Concept(concept) => {
                terms.extend(
                    concept
                        .evidence
                        .iter()
                        .map(|evidence| evidence.quote.to_lowercase()),
                );
            }
        }
        match candidate {
            AnalysisCandidate::Entity(entity) => {
                terms.extend(
                    entity
                        .evidence
                        .iter()
                        .map(|evidence| evidence.quote.to_lowercase()),
                );
            }
            AnalysisCandidate::Concept(concept) => {
                terms.extend(
                    concept
                        .evidence
                        .iter()
                        .map(|evidence| evidence.quote.to_lowercase()),
                );
            }
        }
    }

    let mut chunks = Vec::new();
    let mut current = String::new();
    for paragraph in markdown.split("\n\n") {
        if !current.is_empty() && current.chars().count() + paragraph.chars().count() > 9_000 {
            chunks.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(paragraph);
    }
    if !current.is_empty() {
        chunks.push(current);
    }

    let mut scored = chunks
        .into_iter()
        .enumerate()
        .map(|(index, chunk)| {
            let lowercase = chunk.to_lowercase();
            let score = terms
                .iter()
                .filter(|term| !term.is_empty() && lowercase.contains(term.as_str()))
                .count();
            (index, chunk, score)
        })
        .collect::<Vec<_>>();
    scored.sort_by(|left, right| right.2.cmp(&left.2).then(left.0.cmp(&right.0)));
    scored.truncate(3);
    scored.sort_by_key(|(index, _, _)| *index);
    scored
        .into_iter()
        .enumerate()
        .map(|(index, (_, chunk, _))| format!("## Excerpt {}\n\n{chunk}", index + 1))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn merge_draft_plan(plan: &mut DraftPlan, batch: DraftPlan, template: corpusbot_core::Template) {
    for entity in batch.entities {
        let identity = corpusbot_core::PageIdentity::new(
            template,
            corpusbot_core::PageType::Entity,
            &entity.name,
        )
        .ok();
        match identity.map(|identity| identity.key()) {
            Some(key)
                if plan.entities.iter().any(|existing| {
                    corpusbot_core::PageIdentity::new(
                        template,
                        corpusbot_core::PageType::Entity,
                        &existing.name,
                    )
                    .is_ok_and(|existing| existing.key() == key)
                }) =>
            {
                if let Some(existing) = plan.entities.iter_mut().find(|existing| {
                    corpusbot_core::PageIdentity::new(
                        template,
                        corpusbot_core::PageType::Entity,
                        &existing.name,
                    )
                    .is_ok_and(|existing| existing.key() == key)
                }) {
                    for alias in entity.aliases {
                        if !existing.aliases.contains(&alias) {
                            existing.aliases.push(alias);
                        }
                    }
                    existing.evidence.extend(entity.evidence);
                    if existing.importance.is_none() {
                        existing.importance = entity.importance;
                    }
                }
            }
            _ => plan.entities.push(entity),
        }
    }

    for concept in batch.concepts {
        let identity = corpusbot_core::PageIdentity::new(
            template,
            corpusbot_core::PageType::Concept,
            &concept.name,
        )
        .ok();
        match identity.map(|identity| identity.key()) {
            Some(key)
                if plan.concepts.iter().any(|existing| {
                    corpusbot_core::PageIdentity::new(
                        template,
                        corpusbot_core::PageType::Concept,
                        &existing.name,
                    )
                    .is_ok_and(|existing| existing.key() == key)
                }) =>
            {
                if let Some(existing) = plan.concepts.iter_mut().find(|existing| {
                    corpusbot_core::PageIdentity::new(
                        template,
                        corpusbot_core::PageType::Concept,
                        &existing.name,
                    )
                    .is_ok_and(|existing| existing.key() == key)
                }) {
                    for alias in concept.aliases {
                        if !existing.aliases.contains(&alias) {
                            existing.aliases.push(alias);
                        }
                    }
                    existing.evidence.extend(concept.evidence);
                    if existing.importance.is_none() {
                        existing.importance = concept.importance;
                    }
                }
            }
            _ => plan.concepts.push(concept),
        }
    }
}

fn draft_plan_from_value(mut value: serde_json::Value) -> Result<DraftPlan> {
    if let Some(pages) = value
        .get_mut("pages")
        .and_then(serde_json::Value::as_array_mut)
    {
        for page in pages {
            move_page_level_bullets(page);
        }
    }

    if let Some(pages) = value.get("pages") {
        let pages: Vec<BatchDraftPage> = serde_json::from_value(pages.clone())
            .map_err(|error| AgentError::Schema(format!("invalid pages array: {error}")))?;
        let mut plan = DraftPlan::default();
        for page in pages {
            match page {
                BatchDraftPage::Entity(entity) => plan.entities.push(entity),
                BatchDraftPage::Concept(concept) => plan.concepts.push(concept),
            }
        }
        return Ok(plan);
    }

    let entity_values = value
        .get("entities")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let concept_values = value
        .get("concepts")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let (entity_values, concept_values) =
        split_misnested_legacy_pages(entity_values, concept_values);

    let entities: Vec<EntityDraft> =
        serde_json::from_value(serde_json::Value::Array(entity_values))
            .map_err(|error| AgentError::Schema(format!("invalid entities array: {error}")))?;
    let concepts: Vec<ConceptDraft> =
        serde_json::from_value(serde_json::Value::Array(concept_values))
            .map_err(|error| AgentError::Schema(format!("invalid concepts array: {error}")))?;
    Ok(DraftPlan {
        source_summary: value
            .get("source_summary")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        entities,
        concepts,
    })
}

fn move_page_level_bullets(page: &mut serde_json::Value) {
    let Some(object) = page.as_object_mut() else {
        return;
    };
    let bullets = match object.remove("bullets") {
        Some(serde_json::Value::Array(bullets)) => bullets,
        Some(serde_json::Value::String(bullet)) => vec![serde_json::Value::String(bullet)],
        Some(bullets @ serde_json::Value::Null) => {
            object.insert("bullets".to_owned(), bullets);
            return;
        }
        Some(bullets) => {
            object.insert("bullets".to_owned(), bullets);
            return;
        }
        None => return,
    };

    let section = serde_json::json!({
        "heading": "Key points",
        "paragraphs": [],
        "bullets": bullets,
    });
    match object
        .get_mut("sections")
        .and_then(serde_json::Value::as_array_mut)
    {
        Some(sections) if !sections.is_empty() => {
            if let Some(last) = sections
                .last_mut()
                .and_then(serde_json::Value::as_object_mut)
            {
                match last.get_mut("bullets") {
                    Some(serde_json::Value::Array(existing)) => {
                        existing.extend(section["bullets"].as_array().cloned().unwrap_or_default())
                    }
                    _ => {
                        last.insert("bullets".to_owned(), section["bullets"].clone());
                    }
                }
            }
        }
        _ => {
            let sections = object
                .entry("sections")
                .or_insert_with(|| serde_json::Value::Array(Vec::new()));
            if let Some(sections) = sections.as_array_mut() {
                sections.push(section);
            }
        }
    }
}

fn split_misnested_legacy_pages(
    entity_values: Vec<serde_json::Value>,
    concept_values: Vec<serde_json::Value>,
) -> (Vec<serde_json::Value>, Vec<serde_json::Value>) {
    let mut all_values = entity_values;
    all_values.extend(concept_values);
    let mut normalized_entities = Vec::new();
    let mut normalized_concepts = Vec::new();

    for value in all_values {
        let mut contained_pages = false;
        if let Some(nested_pages) = value.get("pages").and_then(serde_json::Value::as_array) {
            contained_pages = true;
            for page in nested_pages.clone() {
                if is_legacy_concept_page(&page) {
                    normalized_concepts.push(page);
                } else {
                    normalized_entities.push(page);
                }
            }
        }
        if let Some(nested_entities) = value.get("entities").and_then(serde_json::Value::as_array) {
            contained_pages = true;
            normalized_entities.extend(nested_entities.clone());
        }
        if let Some(nested_concepts) = value.get("concepts").and_then(serde_json::Value::as_array) {
            contained_pages = true;
            normalized_concepts.extend(nested_concepts.clone());
        }
        if !contained_pages {
            if is_legacy_concept_page(&value) {
                normalized_concepts.push(value);
            } else {
                normalized_entities.push(value);
            }
        }
    }
    (normalized_entities, normalized_concepts)
}

fn is_legacy_concept_page(value: &serde_json::Value) -> bool {
    value.get("page_type").and_then(serde_json::Value::as_str) == Some("concept")
        || (value.get("definition").is_some() && value.get("summary").is_none())
}

fn validate_draft_plan(plan: &DraftPlan, template: corpusbot_core::Template) -> Option<String> {
    if plan.source_summary.chars().count() > 4000 {
        return Some("source summary is too long".to_owned());
    }
    for entity in &plan.entities {
        if entity.name.trim().is_empty()
            || entity.summary.trim().chars().count() < 16
            || !page_sections_acceptable(entity.importance, &entity.sections)
            || corpusbot_core::PageIdentity::new(
                template,
                corpusbot_core::PageType::Entity,
                &entity.name,
            )
            .is_err()
        {
            return Some(format!("invalid entity {}", entity.name));
        }
    }
    for concept in &plan.concepts {
        if concept.name.trim().is_empty()
            || concept.definition.trim().chars().count() < 16
            || !page_sections_acceptable(concept.importance, &concept.sections)
            || corpusbot_core::PageIdentity::new(
                template,
                corpusbot_core::PageType::Concept,
                &concept.name,
            )
            .is_err()
        {
            return Some(format!("invalid concept {}", concept.name));
        }
    }
    None
}

fn has_rich_sections(sections: &[DraftSection], minimum_count: usize) -> bool {
    sections.len() >= minimum_count
        && sections.iter().all(|section| {
            !section.heading.trim().is_empty()
                && (section
                    .paragraphs
                    .iter()
                    .any(|paragraph| paragraph.trim().chars().count() >= 20)
                    || section
                        .bullets
                        .iter()
                        .any(|bullet| bullet.trim().chars().count() >= 8))
        })
}

fn page_sections_acceptable(importance: Option<Importance>, sections: &[DraftSection]) -> bool {
    if importance == Some(Importance::Incidental) {
        return true;
    }
    has_rich_sections(
        sections,
        match importance {
            Some(Importance::Supporting) => 1,
            _ => 2,
        },
    )
}

fn parse_json<T: for<'de> Deserialize<'de>>(response: &LlmResponse) -> Result<T> {
    let mut text = strip_reasoning(&response.text);
    if text.starts_with("```") {
        text = text.trim_start_matches("```json").trim_start_matches("```");
        text = text.trim_end_matches("```").trim();
    }
    let start = text
        .find('{')
        .ok_or_else(|| AgentError::Schema("response does not contain a JSON object".to_owned()))?;
    let end = text
        .rfind('}')
        .ok_or_else(|| AgentError::Schema("response JSON object is unterminated".to_owned()))?;
    if start >= end {
        return Err(AgentError::Schema("response JSON is malformed".to_owned()));
    }
    let json = &text[start..=end];
    match serde_json::from_str::<T>(json) {
        Ok(value) => Ok(value),
        Err(original_error) => {
            let repaired = repair_unescaped_quotes(json);
            let repaired = repair_missing_section_values(repaired);
            let repaired = repair_misnested_page_bullets(repaired);
            serde_json::from_str::<T>(&repaired)
                .map_err(|_| AgentError::Schema(format!("invalid JSON: {original_error}")))
        }
    }
}

fn repair_missing_section_values(json: String) -> String {
    json.replace("{\"paragraph\"}", "{\"paragraphs\":[]}")
        .replace("{\"bullets\"}", "{\"bullets\":[]}")
}

/// Repairs a provider mistake in batch drafts: `sections` is left open, then
/// the page-level fields are emitted as a separate object. Reopening that
/// object as a page-level `bullets` key restores the intended bracket shape.
fn repair_misnested_page_bullets(json: String) -> String {
    json.replace(r#"]},{"bullets":"#, r#"]}],"bullets":"#)
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_owned()
    } else {
        value.chars().take(max_chars).collect()
    }
}

/// Repairs the common provider mistake of leaving unescaped ASCII quotes
/// inside string values (for example: `强调"证据"优先`). A quote is only
/// treated as a JSON delimiter when a structural token follows it.
fn repair_unescaped_quotes(json: &str) -> String {
    let mut repaired = String::with_capacity(json.len() + 16);
    let mut in_string = false;
    let mut escaped = false;
    let mut chars = json.chars().peekable();

    while let Some(character) = chars.next() {
        if !in_string {
            if character == '"' {
                in_string = true;
            }
            repaired.push(character);
            continue;
        }

        if escaped {
            escaped = false;
            repaired.push(character);
            continue;
        }

        match character {
            '\\' => {
                escaped = true;
                repaired.push(character);
            }
            '"' => {
                let follows_structure = chars
                    .peek()
                    .is_none_or(|next| matches!(next, ',' | ':' | '}' | ']'))
                    || chars
                        .clone()
                        .find(|character| !character.is_whitespace())
                        .is_some_and(|next| matches!(next, ',' | ':' | '}' | ']'));
                if follows_structure {
                    in_string = false;
                    repaired.push(character);
                } else {
                    repaired.push('\\');
                    repaired.push(character);
                }
            }
            character => repaired.push(character),
        }
    }

    repaired
}

fn strip_reasoning(text: &str) -> &str {
    let trimmed = text.trim();
    let Some(start) = trimmed.find("<think>") else {
        return trimmed;
    };
    let Some(end_offset) = trimmed[start..].find("</think>") else {
        return trimmed;
    };
    let end = start + end_offset + "</think>".len();
    trimmed[end..].trim()
}

const ANALYZE_SYSTEM: &str = r#"You are a precise research analyst. Return only a JSON object, without Markdown fences.
Required shape:
{"title":"string","summary":"string","entities":[{"name":"string","aliases":["string"],"summary":"string","confidence":0.0,"importance":"core|supporting|incidental","evidence":[{"quote":"exact source text","section":"heading"}]}],"concepts":[{"name":"string","aliases":["string"],"definition":"string","confidence":0.0,"importance":"core|supporting|incidental","evidence":[{"quote":"exact source text","section":"heading"}]}]}
Inventory every salient entity and reusable concept. Do not impose an arbitrary page count. Include central, supporting, and incidental candidates when they are explicitly present. Every entity and concept MUST include `aliases`; use `[]` when there are none. Confidence is 0.0-1.0. Evidence quotes must be copied exactly from the source. Use concise evidence-backed wording. Do not invent facts. All JSON string values must be valid JSON. Escape any ASCII double quote inside text as \" or, preferably, use 「」 for quoted terms."#;

const DRAFT_SYSTEM: &str = r#"You are a wiki editor. Return exactly one valid JSON object. Do not emit reasoning, <think>, Markdown fences, or text before or after JSON.
Generate only the candidate pages listed in this batch. Do not add or omit pages.

Use exactly this JSON grammar:
root ::= {"pages": [page]}
page ::= entity_page | concept_page
entity_page ::= {"page_type":"entity","name":"string","aliases":[string],"summary":"string","tags":[string],"related":[string],"sections":[section],"confidence":number,"importance":"core"|"supporting"|"incidental","evidence":[evidence]}
concept_page ::= {"page_type":"concept","name":"string","aliases":[string],"definition":"string","tags":[string],"related":[string],"sections":[section],"confidence":number,"importance":"core"|"supporting"|"incidental","evidence":[evidence]}
section ::= {"heading":"string","paragraphs":[string],"bullets":[string]}
evidence ::= {"quote":"exact source text","section":"string"}

Allowed entity-page keys are exactly: page_type, name, aliases, summary, tags, related, sections, confidence, importance, evidence.
Allowed concept-page keys are exactly: page_type, name, aliases, definition, tags, related, sections, confidence, importance, evidence.
Allowed section keys are exactly: heading, paragraphs, bullets.
`bullets` is allowed only inside a section. A page must not have a page-level `bullets` key. Do not close a page object until every page-level field has been written.
The pages array must contain exactly one object per candidate in this batch.

Core pages need two or three substantive sections. Supporting pages need one or two. Incidental pages may be a concise stub. Keep sections to at most two paragraphs and three bullets. Use only facts stated in the source or candidate metadata. Tags are lowercase kebab-case. Use exact names from Candidate metadata or Existing related pages for related; omit related when uncertain. Do not invent facts. Every returned entity and concept MUST include `aliases`; use `[]` when there are none. All JSON string values must be valid JSON. Escape any ASCII double quote inside text as \" or, preferably, use 「」 for quoted terms."#;
const QUERY_SYSTEM: &str = r#"You are a wiki research assistant. Answer only from numbered evidence.
Return exactly one JSON object and no other content. Do not emit reasoning, <think>, Markdown fences, or text before or after JSON.
{"answer":"string with [number] citations","citations":[{"number":1,"path":"wiki/page.md","quote":"exact evidence quote","revision":{"kind":"content","value":{"sha256":"..."}}}]}
Quotes must be copied exactly from evidence. If evidence is insufficient, use an empty citations array.
Use unescaped double quotes only as JSON string delimiters. Inside any JSON string, escape double quotes as \" or, preferably, use 「」 for quoted terms."#;

#[derive(Clone, Debug, PartialEq)]
pub struct QueryContextPage {
    pub path: String,
    pub title: String,
    pub page_type: String,
    pub revision: corpusbot_core::Revision,
    pub revision_manifest_id: String,
    pub markdown: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryAnswer {
    pub answer: String,
    pub citations: Vec<Citation>,
    pub revision_manifest_id: String,
    pub warnings: Vec<String>,
    pub insufficient_evidence: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Citation {
    pub number: u32,
    pub path: String,
    pub title: String,
    pub quote: String,
    pub resource_revision: corpusbot_core::Revision,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RawQueryAnswer {
    pub answer: String,
    #[serde(default)]
    pub citations: Vec<RawCitation>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RawCitation {
    pub path: String,
    pub quote: String,
    pub revision: corpusbot_core::Revision,
}

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::FakeLlmClient;

    struct LengthLimitedClient;

    #[async_trait::async_trait]
    impl crate::llm::LlmClient for LengthLimitedClient {
        async fn complete(
            &self,
            _request: crate::llm::LlmRequest,
        ) -> crate::Result<crate::llm::LlmResponse> {
            Ok(crate::llm::LlmResponse {
                text: "truncated".to_owned(),
                provider: "test".to_owned(),
                model: "test-model".to_owned(),
                prompt_tokens: 10,
                completion_tokens: 100,
                response_id: Some("length".to_owned()),
                finish_reason: Some("length".to_owned()),
            })
        }
    }

    #[tokio::test]
    async fn parses_analysis_json() -> Result<()> {
        let client = FakeLlmClient::new([r#"```json
{"title":"Raft","summary":"Consensus","entities":[],"concepts":[]}
```"#]);
        let agent = SourceAgent::new(client);
        let analysis = agent.analyze_source("Raft", "# Raft").await?;
        assert_eq!(analysis.title, "Raft");
        Ok(())
    }

    #[tokio::test]
    async fn audits_output_token_limit_reached() -> Result<()> {
        let root = tempfile::tempdir()?;
        let result = SourceAgent::new(LengthLimitedClient)
            .analyze_source_audited(root.path(), "length-limit", "manifest-1", "Raft", "# Raft")
            .await;
        assert!(result.is_err());

        let events =
            std::fs::read_to_string(root.path().join(".wiki-db/audit/length-limit/events.jsonl"))?;
        assert!(events.contains(r#""finish_reason":"length""#));
        assert!(events.contains(r#""truncated":true"#));
        assert!(events.contains("analysis output reached token limit"));
        Ok(())
    }

    #[tokio::test]
    async fn uses_configured_draft_token_budget() -> Result<()> {
        let client = FakeLlmClient::new([
            r#"{"title":"Raft","summary":"Consensus","entities":[{"name":"Raft","summary":"Consensus algorithm."}],"concepts":[]}"#,
            r#"{"source_summary":"Consensus","entities":[],"concepts":[]}"#,
        ]);
        let agent = SourceAgent::new(client).with_max_draft_tokens(15000);
        let analysis = agent.analyze_source("Raft", "# Raft").await?;
        agent
            .generate_drafts("research", "Raft", "# Raft", &analysis, &[])
            .await?;

        let calls = agent.client.calls();
        assert_eq!(calls[1].max_tokens, Some(15000));
        assert!(calls[1].prompt.contains("page_type"));
        assert!(calls[1].prompt.contains("# Batch\n\n1 of 1"));
        Ok(())
    }

    #[test]
    fn batches_scale_with_candidate_inventory() {
        let mut analysis = SourceAnalysis {
            title: "Dense".to_owned(),
            summary: "Dense source.".to_owned(),
            entities: Vec::new(),
            concepts: Vec::new(),
        };
        for index in 0..9 {
            analysis.entities.push(EntityAnalysis {
                name: format!("Entity {index}"),
                aliases: Vec::new(),
                summary: format!("Entity {index} is explicitly discussed."),
                confidence: Some(0.8),
                importance: Some(Importance::Supporting),
                evidence: Vec::new(),
            });
        }
        for index in 0..5 {
            analysis.concepts.push(ConceptAnalysis {
                name: format!("Concept {index}"),
                definition: format!("Concept {index} is explicitly discussed."),
                confidence: Some(0.7),
                importance: Some(Importance::Supporting),
                evidence: Vec::new(),
                aliases: Vec::new(),
            });
        }

        let normalized = normalize_analysis(&analysis, corpusbot_core::Template::Research);
        let candidates = normalized
            .entities
            .iter()
            .cloned()
            .map(AnalysisCandidate::Entity)
            .chain(
                normalized
                    .concepts
                    .iter()
                    .cloned()
                    .map(AnalysisCandidate::Concept),
            )
            .collect::<Vec<_>>();
        let batches = draft_candidate_batches(&candidates);

        assert_eq!(batches.len(), 4);
        assert_eq!(batches.iter().map(Vec::len).sum::<usize>(), 14);
    }

    #[tokio::test]
    async fn preserves_successful_batches_when_retrying_a_schema_failure() -> Result<()> {
        let mut analysis = SourceAnalysis {
            title: "Dense".to_owned(),
            summary: "Dense source.".to_owned(),
            entities: Vec::new(),
            concepts: Vec::new(),
        };
        for index in 0..9 {
            analysis.entities.push(EntityAnalysis {
                name: format!("Entity {index}"),
                aliases: Vec::new(),
                summary: format!("Entity {index} is explicitly discussed."),
                confidence: Some(0.8),
                importance: Some(Importance::Supporting),
                evidence: Vec::new(),
            });
        }
        let client = FakeLlmClient::new([
            r#"{"pages":[]}"#,
            r#"{"pages":[{"page_type":"entity","name":"Raft"}]}"#,
            r#"{"pages":[]}"#,
            r#"{"pages":[]}"#,
        ]);
        let agent = SourceAgent::new(client.clone());
        agent
            .generate_drafts("research", "Dense", "# Dense", &analysis, &[])
            .await?;

        let calls = client.calls();
        assert_eq!(calls.len(), 4);
        assert!(calls[0].prompt.contains("# Batch\n\n1 of 3"));
        assert!(calls[1].prompt.contains("# Batch\n\n2 of 3"));
        assert!(calls[2].prompt.contains("# Batch\n\n2 of 3"));
        assert!(calls[3].prompt.contains("# Batch\n\n3 of 3"));
        Ok(())
    }

    #[test]
    fn merges_repeated_batch_pages_and_evidence() {
        let evidence = CandidateEvidence {
            quote: "Raft elects a leader.".to_owned(),
            section: Some("Overview".to_owned()),
        };
        let mut plan = DraftPlan {
            source_summary: String::new(),
            entities: Vec::new(),
            concepts: Vec::new(),
        };
        let first = DraftPlan {
            source_summary: String::new(),
            entities: vec![EntityDraft {
                name: "Raft".to_owned(),
                aliases: vec!["Raft consensus".to_owned()],
                summary: "Raft is a consensus algorithm.".to_owned(),
                tags: Vec::new(),
                related: Vec::new(),
                sections: Vec::new(),
                confidence: Some(0.8),
                importance: Some(Importance::Core),
                evidence: vec![evidence.clone()],
            }],
            concepts: Vec::new(),
        };
        let second = DraftPlan {
            source_summary: String::new(),
            entities: vec![EntityDraft {
                name: "raft".to_owned(),
                aliases: vec!["Raft protocol".to_owned()],
                summary: "Raft coordinates replicated state.".to_owned(),
                tags: Vec::new(),
                related: Vec::new(),
                sections: Vec::new(),
                confidence: Some(0.9),
                importance: Some(Importance::Core),
                evidence: vec![CandidateEvidence {
                    quote: "Raft uses a majority vote.".to_owned(),
                    section: Some("Election".to_owned()),
                }],
            }],
            concepts: Vec::new(),
        };

        merge_draft_plan(&mut plan, first, corpusbot_core::Template::Research);
        merge_draft_plan(&mut plan, second, corpusbot_core::Template::Research);

        assert_eq!(plan.entities.len(), 1);
        assert_eq!(plan.entities[0].aliases.len(), 2);
        assert_eq!(plan.entities[0].evidence.len(), 2);
    }

    #[tokio::test]
    async fn rejects_schema_mismatch() {
        let client = FakeLlmClient::new([r#"{"wrong":true}"#]);
        let agent = SourceAgent::new(client);
        assert!(agent.analyze_source("Raft", "# Raft").await.is_err());
    }

    #[test]
    fn parses_json_after_reasoning_block() -> Result<()> {
        let response = LlmResponse {
            text: r#"<think>
Need JSON.
</think>
```json
{"title":"Raft","summary":"Consensus"}
```"#
                .to_owned(),
            provider: "fake".to_owned(),
            model: "fake-model".to_owned(),
            prompt_tokens: 0,
            completion_tokens: 0,
            response_id: None,
            finish_reason: None,
        };
        let analysis = parse_json::<SourceAnalysis>(&response)?;
        assert_eq!(analysis.title, "Raft");
        Ok(())
    }

    #[test]
    fn repairs_unescaped_quotes_in_json_strings() -> Result<()> {
        let response = LlmResponse {
            text: r#"{"title":"Raft","summary":"强调"证据"优先。"}"#.to_owned(),
            provider: "fake".to_owned(),
            model: "fake-model".to_owned(),
            prompt_tokens: 0,
            completion_tokens: 0,
            response_id: None,
            finish_reason: None,
        };
        let analysis = parse_json::<SourceAnalysis>(&response)?;
        assert_eq!(analysis.summary, "强调\"证据\"优先。");
        Ok(())
    }

    #[test]
    fn accepts_concept_summary_as_definition_for_provider_compatibility() -> Result<()> {
        let response = LlmResponse {
            text: r#"{"title":"Raft","summary":"Consensus","concepts":[{"name":"Leader Election","summary":"Leaders are elected."}]}"#.to_owned(),
            provider: "fake".to_owned(),
            model: "fake-model".to_owned(),
            prompt_tokens: 0,
            completion_tokens: 0,
            response_id: None,
            finish_reason: None,
        };
        let analysis = parse_json::<SourceAnalysis>(&response)?;
        assert_eq!(analysis.concepts[0].definition, "Leaders are elected.");
        assert_eq!(
            repair_missing_section_values("{\"paragraph\"}".to_owned()),
            "{\"paragraphs\":[]}"
        );
        Ok(())
    }

    #[test]
    fn repairs_malformed_empty_section_objects() -> Result<()> {
        let response = LlmResponse {
            text: r#"{"title":"Raft","summary":"Consensus","concepts":[{"name":"Election","summary":"Leaders are elected.","sections":[{"paragraph"}]}]}"#.to_owned(),
            provider: "fake".to_owned(),
            model: "fake-model".to_owned(),
            prompt_tokens: 0,
            completion_tokens: 0,
            response_id: None,
            finish_reason: None,
        };
        let analysis = parse_json::<SourceAnalysis>(&response)?;
        assert_eq!(analysis.concepts[0].definition, "Leaders are elected.");
        assert_eq!(
            repair_missing_section_values("{\"paragraph\"}".to_owned()),
            "{\"paragraphs\":[]}"
        );
        Ok(())
    }

    #[test]
    fn repairs_page_level_bullets_in_batch_pages() -> Result<()> {
        let response = LlmResponse {
            text: r#"{"pages":[
                {"page_type":"entity","name":"Raft","aliases":[],"summary":"Consensus.","tags":[],"related":[],"sections":[{"heading":"Overview","paragraphs":["Raft agrees."],"bullets":[]}],"bullets":["Existing"],"confidence":1.0,"importance":"supporting","evidence":[]},
                {"page_type":"entity","name":"Log","aliases":[],"summary":"A log.","tags":[],"related":[],"sections":[{"heading":"Overview","paragraphs":["Log stores events."]},{"bullets":["Moved"],"confidence":1.0,"importance":"supporting","evidence":[]}]
            }"#
                .to_owned(),
            provider: "fake".to_owned(),
            model: "fake-model".to_owned(),
            prompt_tokens: 0,
            completion_tokens: 0,
            response_id: None,
            finish_reason: None,
        };
        let raw = parse_json::<serde_json::Value>(&response)?;
        let plan = draft_plan_from_value(raw)?;

        assert_eq!(
            plan.entities[0].sections[0].bullets,
            vec!["Existing".to_owned()]
        );
        assert_eq!(
            plan.entities[1].sections[0].bullets,
            vec!["Moved".to_owned()]
        );
        Ok(())
    }

    #[test]
    fn draft_provider_schema_forbids_page_level_bullets() {
        let output = draft_response_format();
        let items = output
            .schema
            .pointer("/properties/pages/items")
            .expect("pages items schema");
        let entity_properties = &items["anyOf"][0]["properties"];

        assert!(entity_properties.get("bullets").is_none());
        assert_eq!(items["anyOf"][0]["additionalProperties"], false);
        assert_eq!(
            entity_properties["sections"]["items"]["properties"]["bullets"]["type"],
            "array"
        );
    }

    #[test]
    fn tolerates_missing_aliases_and_scalar_paragraphs() -> Result<()> {
        let response = LlmResponse {
            text: r#"{"entities":[{"name":"tshark","summary":"Packet analyzer.","sections":[{"heading":"Usage","paragraphs":""}]}],"concepts":[{"name":"Handshake","definition":"Three-way handshake.","aliases":null,"sections":[{"heading":"Process","paragraphs":["It has three messages."],"bullets":[]}]}]}"#.to_owned(),
            provider: "fake".to_owned(),
            model: "fake-model".to_owned(),
            prompt_tokens: 0,
            completion_tokens: 0,
            response_id: None,
            finish_reason: None,
        };
        let analysis = parse_json::<DraftPlan>(&response)?;

        assert_eq!(
            analysis.entities[0].sections[0].paragraphs,
            Vec::<String>::new()
        );
        assert!(analysis.concepts[0].aliases.is_empty());
        assert_eq!(
            analysis.concepts[0].sections[0].paragraphs,
            vec!["It has three messages.".to_owned()]
        );
        Ok(())
    }

    #[test]
    fn parses_single_page_array_batches() -> Result<()> {
        let response = LlmResponse {
            text: r#"{"pages":[
                {"page_type":"entity","name":"tcpdump","aliases":[],"summary":"A packet capture tool.","tags":[],"related":[],"sections":[]},
                {"page_type":"concept","name":"Three-way handshake","aliases":[],"definition":"A TCP connection setup exchange.","tags":[],"related":[],"sections":[]}
            ]}"#
                .to_owned(),
            provider: "fake".to_owned(),
            model: "fake-model".to_owned(),
            prompt_tokens: 0,
            completion_tokens: 0,
            response_id: None,
            finish_reason: None,
        };
        let raw = parse_json::<serde_json::Value>(&response)?;
        let plan = draft_plan_from_value(raw)?;

        assert_eq!(plan.entities.len() + plan.concepts.len(), 2);
        assert_eq!(plan.entities[0].name, "tcpdump");
        assert_eq!(plan.concepts[0].name, "Three-way handshake");
        Ok(())
    }

    #[test]
    fn repairs_misnested_legacy_batch_pages() -> Result<()> {
        let response = LlmResponse {
            text: r#"{"entities":[
                {"name":"tcpdump","aliases":[],"summary":"A packet capture tool."},
                {"concepts":[{"name":"Three-way handshake","aliases":[],"definition":"A TCP connection setup exchange."}]}
            ]}"#
                .to_owned(),
            provider: "fake".to_owned(),
            model: "fake-model".to_owned(),
            prompt_tokens: 0,
            completion_tokens: 0,
            response_id: None,
            finish_reason: None,
        };
        let raw = parse_json::<serde_json::Value>(&response)?;
        let plan = draft_plan_from_value(raw)?;

        assert_eq!(plan.entities.len(), 1);
        assert_eq!(plan.entities[0].name, "tcpdump");
        assert_eq!(plan.concepts.len(), 1);
        assert_eq!(plan.concepts[0].name, "Three-way handshake");
        Ok(())
    }

    #[test]
    fn accepts_one_rich_supporting_section() {
        let section = DraftSection {
            heading: "Usage".to_owned(),
            paragraphs: vec!["Wireshark opens pcap files captured by tcpdump.".to_owned()],
            bullets: Vec::new(),
        };

        assert!(page_sections_acceptable(
            Some(Importance::Supporting),
            std::slice::from_ref(&section)
        ));
        assert!(!page_sections_acceptable(
            Some(Importance::Core),
            &[section]
        ));
    }

    #[tokio::test]
    async fn audited_analysis_persists_attempt_metadata_and_artifacts() -> Result<()> {
        let root = tempfile::tempdir()?;
        let client = FakeLlmClient::new([r#"{"title":"Raft","summary":"Consensus"}"#]);
        let agent = SourceAgent::new(client);
        let analysis = agent
            .analyze_source_audited(
                root.path(),
                "audit-analysis",
                "manifest-1",
                "Raft",
                "# Raft",
            )
            .await?;
        assert_eq!(analysis.title, "Raft");

        let ledger = std::fs::read_to_string(
            root.path()
                .join(".wiki-db/audit/audit-analysis/events.jsonl"),
        )?;
        let events = ledger
            .lines()
            .map(serde_json::from_str::<crate::WorkflowAuditEvent>)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        assert_eq!(events.len(), 4);
        assert!(events.iter().any(|event| {
            event.node == WorkflowNode::Analyze
                && event.prompt_template_id.as_deref() == Some(ANALYZE_PROMPT_ID)
                && event
                    .prompt_hash
                    .as_deref()
                    .is_some_and(|hash| hash.len() == 64)
                && event.provider.as_deref() == Some("fake")
                && event.model.as_deref() == Some("fake-model")
                && event
                    .output_ref
                    .as_deref()
                    .is_some_and(|reference| reference.starts_with("audit/audit-analysis/"))
        }));
        assert!(
            root.path()
                .join(".wiki-db/audit/audit-analysis/analyze-1-request.json")
                .exists()
        );
        assert!(
            root.path()
                .join(".wiki-db/audit/audit-analysis/analyze-1-response.json")
                .exists()
        );
        Ok(())
    }

    #[tokio::test]
    async fn audited_analysis_retries_invalid_schema() -> Result<()> {
        let root = tempfile::tempdir()?;
        let client = FakeLlmClient::new([
            r#"{"wrong":true}"#,
            r#"{"title":"Raft","summary":"Consensus"}"#,
        ]);
        let agent = SourceAgent::new(client);
        let analysis = agent
            .analyze_source_audited(root.path(), "audit-retry", "manifest", "Raft", "# Raft")
            .await?;
        assert_eq!(analysis.title, "Raft");

        let ledger =
            std::fs::read_to_string(root.path().join(".wiki-db/audit/audit-retry/events.jsonl"))?;
        assert!(ledger.contains(r#""status":"validator_rejected""#));
        assert!(ledger.contains(r#""attempt":2"#));
        Ok(())
    }

    #[tokio::test]
    async fn audited_analysis_stops_after_bounded_attempts() -> Result<()> {
        let root = tempfile::tempdir()?;
        let client = FakeLlmClient::new([r#"{"wrong":true}"#, r#"{"wrong":true}"#]);
        let agent = SourceAgent::new(client);
        let error = agent
            .analyze_source_audited(
                root.path(),
                "audit-exhaustion",
                "manifest",
                "Raft",
                "# Raft",
            )
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            AgentError::AttemptsExhausted { attempts: 2, .. }
        ));
        let ledger = std::fs::read_to_string(
            root.path()
                .join(".wiki-db/audit/audit-exhaustion/events.jsonl"),
        )?;
        assert!(ledger.contains(r#""status":"attempts_exhausted""#));
        Ok(())
    }

    #[test]
    fn drops_citations_with_unknown_or_mismatched_quotes() {
        let revision = corpusbot_core::Revision::from_content(b"raft");
        let context = [QueryContextPage {
            path: "wiki/entities/Raft.md".to_owned(),
            title: "Raft".to_owned(),
            page_type: "entity".to_owned(),
            revision: revision.clone(),
            revision_manifest_id: "manifest".to_owned(),
            markdown: "Raft elects a leader before replication.".to_owned(),
        }];

        let valid = SourceAgent::<FakeLlmClient>::validate_answer(
            RawQueryAnswer {
                answer: "Raft elects a leader [1].".to_owned(),
                citations: vec![RawCitation {
                    path: context[0].path.clone(),
                    quote: "elects a leader".to_owned(),
                    revision: revision.clone(),
                }],
            },
            &context,
        );
        assert_eq!(valid.citations.len(), 1);
        assert!(!valid.insufficient_evidence);

        let invalid = SourceAgent::<FakeLlmClient>::validate_answer(
            RawQueryAnswer {
                answer: "Unsupported claim [1].".to_owned(),
                citations: vec![RawCitation {
                    path: context[0].path.clone(),
                    quote: "invented fact".to_owned(),
                    revision,
                }],
            },
            &context,
        );
        assert!(invalid.citations.is_empty());
        assert!(invalid.insufficient_evidence);
    }
}
