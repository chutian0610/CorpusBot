use serde::{Deserialize, Serialize};

use crate::error::{AgentError, Result};
use crate::llm::{LlmClient, LlmRequest, LlmResponse};

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
    client: C,
}

impl<C> SourceAgent<C>
where
    C: LlmClient,
{
    pub fn new(client: C) -> Self {
        Self { client }
    }

    pub async fn analyze_source(
        &self,
        source_title: &str,
        markdown: &str,
    ) -> Result<SourceAnalysis> {
        let request = LlmRequest {
            operation: "analyze_source".to_owned(),
            system: ANALYZE_SYSTEM.to_owned(),
            prompt: format!("# Source title\n\n{source_title}\n\n# Source markdown\n\n{markdown}"),
            prompt_template_id: ANALYZE_PROMPT_ID.to_owned(),
            temperature: Some(0.1),
            max_tokens: Some(3000),
        };
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
        let related = related_pages
            .iter()
            .map(|(path, title, excerpt)| format!("## {path}\n\nTitle: {title}\n\n{excerpt}"))
            .collect::<Vec<_>>()
            .join("\n\n");
        let analysis_json = serde_json::to_string_pretty(analysis)?;
        let request = LlmRequest {
            operation: "generate_drafts".to_owned(),
            system: DRAFT_SYSTEM.replace("{template}", template_name),
            prompt: format!(
                "# Source title\n\n{source_title}\n\n# Source markdown\n\n{source_markdown}\n\n# Analysis JSON\n\n{analysis_json}\n\n# Existing related pages\n\n{related}"
            ),
            prompt_template_id: DRAFT_PROMPT_ID.to_owned(),
            temperature: Some(0.2),
            max_tokens: Some(4000),
        };
        let response = self.client.complete(request).await?;
        parse_json(&response)
    }

    pub async fn answer_question(
        &self,
        question: &str,
        context: &[QueryContextPage],
    ) -> Result<QueryAnswer> {
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
        let request = LlmRequest {
            operation: "answer_query".to_owned(),
            system: QUERY_SYSTEM.to_owned(),
            prompt: format!("# Question\n\n{question}\n\n# Evidence\n\n{evidence}"),
            prompt_template_id: QUERY_PROMPT_ID.to_owned(),
            temperature: Some(0.1),
            max_tokens: Some(1500),
        };
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
}

fn parse_json<T: for<'de> Deserialize<'de>>(response: &LlmResponse) -> Result<T> {
    let mut text = response.text.trim();
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

const ANALYZE_SYSTEM: &str = r#"You are a precise research analyst. Return only a JSON object, without Markdown fences.
Required shape:
{"title":"string","summary":"string","entities":[{"name":"string","aliases":["string"],"summary":"string"}],"concepts":[{"name":"string","definition":"string"}]}
Use concise evidence-backed wording. Do not invent facts."#;

const DRAFT_SYSTEM: &str = r#"You are a wiki editor. Return only a JSON object, without Markdown fences.
For the template named {template}, produce concise source-attributed summaries:
{"source_summary":"string","entities":[{"name":"string","aliases":["string"],"summary":"string"}],"concepts":[{"name":"string","definition":"string"}]}
Do not invent facts or add links."#;

const QUERY_SYSTEM: &str = r#"You are a wiki research assistant. Answer only from numbered evidence.
Return only JSON:
{"answer":"string with [number] citations","citations":[{"number":1,"path":"wiki/page.md","quote":"exact evidence quote","revision":{"kind":"content","value":{"sha256":"..."}}}]}
Quotes must be copied exactly from evidence. If evidence is insufficient, use an empty citations array."#;

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
