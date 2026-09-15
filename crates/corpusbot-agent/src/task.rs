use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use crate::audit::{AuditSink, FileAuditSink};
use crate::error::{AgentError, Result};
use crate::llm::{LlmClient, LlmRequest, LlmResponse};
use crate::workflow::{
    Transition, WorkflowContext, WorkflowKernel, WorkflowNode, WorkflowNodeHandler, WorkflowOutcome,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EntityAnalysis {
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub summary: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConceptAnalysis {
    pub name: String,
    pub definition: String,
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
    #[serde(default)]
    pub aliases: Vec<String>,
    pub summary: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConceptDraft {
    pub name: String,
    pub definition: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DraftPlan {
    pub source_summary: String,
    #[serde(default)]
    pub entities: Vec<EntityDraft>,
    #[serde(default)]
    pub concepts: Vec<ConceptDraft>,
}

pub const ANALYZE_PROMPT_ID: &str = "analyze-source-v1";
pub const DRAFT_PROMPT_ID: &str = "generate-drafts-v1";
pub const QUERY_PROMPT_ID: &str = "answer-query-v1";

pub struct SourceAgent<C> {
    client: Arc<C>,
}

impl<C> SourceAgent<C>
where
    C: LlmClient + 'static,
{
    pub fn new(client: C) -> Self {
        Self {
            client: Arc::new(client),
        }
    }

    pub async fn analyze_source(
        &self,
        source_title: &str,
        markdown: &str,
    ) -> Result<SourceAnalysis> {
        let request = analyze_request(source_title, markdown);
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
        let request = draft_request(
            template_name,
            source_title,
            source_markdown,
            analysis,
            related_pages,
        )?;
        let response = self.client.complete(request).await?;
        parse_json(&response)
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
        let handler = Arc::new(DraftHandler {
            client: self.client.clone(),
            audit: sink.clone(),
            template,
            source_title: source_title.to_owned(),
            source_markdown: source_markdown.to_owned(),
            analysis: analysis.clone(),
            related_pages: related_pages.to_vec(),
        });
        let kernel: WorkflowKernel<DraftState> = WorkflowKernel::new(WorkflowNode::GenerateDraft)
            .handler(WorkflowNode::GenerateDraft, handler)
            .handler(
                WorkflowNode::ValidateDraft,
                Arc::new(ValidateDraftHandler { template }),
            );
        let context = WorkflowContext::new(
            DraftState { plan: None },
            run_id,
            Some(manifest_id.to_owned()),
            sink,
        );
        match kernel.run(context).await? {
            WorkflowOutcome::Completed { state } => state
                .plan
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
}

#[derive(Clone, Debug, Default)]
struct QueryState {
    context: Vec<QueryContextPage>,
    answer: Option<QueryAnswer>,
}

struct AnalyzeHandler<C> {
    client: Arc<C>,
    audit: Arc<FileAuditSink>,
    source_title: String,
    markdown: String,
}

#[async_trait]
impl<C: LlmClient + 'static> WorkflowNodeHandler<AnalysisState> for AnalyzeHandler<C> {
    async fn run(
        &self,
        context: &mut WorkflowContext<AnalysisState>,
    ) -> Result<Transition<AnalysisState>> {
        let request = analyze_request(&self.source_title, &self.markdown);
        let (response_ref, decision, parsed) = complete_audited(
            self.client.as_ref(),
            &self.audit,
            WorkflowNode::Analyze,
            context,
            &request,
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
                reason: "analysis output did not match schema".to_owned(),
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
    template: corpusbot_core::Template,
    source_title: String,
    source_markdown: String,
    analysis: SourceAnalysis,
    related_pages: Vec<(String, String, String)>,
}

#[async_trait]
impl<C: LlmClient + 'static> WorkflowNodeHandler<DraftState> for DraftHandler<C> {
    async fn run(
        &self,
        context: &mut WorkflowContext<DraftState>,
    ) -> Result<Transition<DraftState>> {
        let request = draft_request(
            self.template.as_str(),
            &self.source_title,
            &self.source_markdown,
            &self.analysis,
            &self.related_pages,
        )?;
        let (response_ref, decision, parsed) = complete_audited(
            self.client.as_ref(),
            &self.audit,
            WorkflowNode::GenerateDraft,
            context,
            &request,
        )
        .await?;
        context.set_output_ref(response_ref);
        context.set_decision(decision);
        match parsed {
            Some(plan) => Ok(Transition::Next {
                state: DraftState { plan: Some(plan) },
                node: WorkflowNode::ValidateDraft,
            }),
            None => Ok(Transition::Retry {
                state: context.state.clone(),
                reason: "draft output did not match schema".to_owned(),
            }),
        }
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

fn analyze_request(source_title: &str, markdown: &str) -> LlmRequest {
    LlmRequest {
        operation: "analyze_source".to_owned(),
        system: ANALYZE_SYSTEM.to_owned(),
        prompt: format!("# Source title\n\n{source_title}\n\n# Source markdown\n\n{markdown}"),
        prompt_template_id: ANALYZE_PROMPT_ID.to_owned(),
        temperature: Some(0.1),
        max_tokens: Some(3000),
    }
}

fn draft_request(
    template_name: &str,
    source_title: &str,
    source_markdown: &str,
    analysis: &SourceAnalysis,
    related_pages: &[(String, String, String)],
) -> Result<LlmRequest> {
    let related = related_pages
        .iter()
        .map(|(path, title, excerpt)| format!("## {path}\n\nTitle: {title}\n\n{excerpt}"))
        .collect::<Vec<_>>()
        .join("\n\n");
    let analysis_json = serde_json::to_string_pretty(analysis)?;
    Ok(LlmRequest {
        operation: "generate_drafts".to_owned(),
        system: DRAFT_SYSTEM.replace("{template}", template_name),
        prompt: format!(
            "# Source title\n\n{source_title}\n\n# Source markdown\n\n{source_markdown}\n\n# Analysis JSON\n\n{analysis_json}\n\n# Existing related pages\n\n{related}"
        ),
        prompt_template_id: DRAFT_PROMPT_ID.to_owned(),
        temperature: Some(0.2),
        max_tokens: Some(4000),
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
    }
}

async fn complete_audited<C: LlmClient, S, T>(
    client: &C,
    audit: &FileAuditSink,
    node: WorkflowNode,
    context: &mut WorkflowContext<S>,
    request: &LlmRequest,
) -> Result<(String, &'static str, Option<T>)>
where
    T: serde::de::DeserializeOwned,
{
    let attempt = context.attempt(node);
    let request_json = serde_json::to_vec_pretty(request)?;
    let request_ref = audit
        .store_artifact(
            &context.run_id.clone(),
            &format!("{}-{attempt}-request.json", node_name(node)),
            &request_json,
        )
        .await?;
    context.set_output_ref(request_ref.clone());
    let started = Instant::now();
    match client.complete(request.clone()).await {
        Ok(response) => {
            let latency = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
            let response_ref = audit
                .store_artifact(
                    &context.run_id.clone(),
                    &format!("{}-{attempt}-response.json", node_name(node)),
                    &serde_json::to_vec_pretty(&serde_json::json!({
                        "provider": response.provider,
                        "model": response.model,
                        "response_id": response.response_id,
                        "text": response.text,
                    }))?,
                )
                .await?;
            context.record_llm_response(&response, request, latency);
            match parse_json::<T>(&response) {
                Ok(parsed) => Ok((response_ref, "llm output parsed", Some(parsed))),
                Err(_error) => {
                    context.set_decision("schema rejected");
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

fn validate_draft_plan(plan: &DraftPlan, template: corpusbot_core::Template) -> Option<String> {
    if plan.source_summary.trim().is_empty() {
        return Some("source summary is empty".to_owned());
    }
    if plan.source_summary.chars().count() > 4000 {
        return Some("source summary is too long".to_owned());
    }
    for entity in &plan.entities {
        if entity.name.trim().is_empty()
            || entity.summary.trim().chars().count() < 8
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
            || concept.definition.trim().chars().count() < 8
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
    let value = serde_json::from_str::<T>(&text[start..=end])?;
    Ok(value)
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
{"title":"string","summary":"string","entities":[{"name":"string","aliases":["string"],"summary":"string"}],"concepts":[{"name":"string","definition":"string"}]}
Use concise evidence-backed wording. Do not invent facts."#;

const DRAFT_SYSTEM: &str = r#"You are a wiki editor. Return only a JSON object, without Markdown fences.
For the template named {template}, produce concise source-attributed summaries:
{"source_summary":"string","entities":[{"name":"string","aliases":["string"],"summary":"string"}],"concepts":[{"name":"string","definition":"string"}]}
Do not invent facts or add links."#;

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
pub struct QueryAnswer {
    pub answer: String,
    pub citations: Vec<Citation>,
    pub revision_manifest_id: String,
    pub warnings: Vec<String>,
    pub insufficient_evidence: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
        };
        let analysis = parse_json::<SourceAnalysis>(&response)?;
        assert_eq!(analysis.title, "Raft");
        Ok(())
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
