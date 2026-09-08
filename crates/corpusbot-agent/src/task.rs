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
}
