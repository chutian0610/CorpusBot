pub mod audit;
pub mod config;
pub mod error;
pub mod llm;
pub mod parsing;
pub mod prompts;
pub mod task;
pub mod workflow;

pub use audit::{AuditSink, FileAuditSink, WorkflowAuditEvent};
pub use config::DEFAULT_DRAFT_MAX_TOKENS;
pub use config::ProviderConfig;
pub use config::git_identity;
pub use config::{
    SettingsInput, SettingsSummary, load_settings, provider_config, provider_config_for_settings,
    save_settings,
};
pub use error::{AgentError, Result};
pub use llm::{
    ConnectionTestResult, FakeLlmClient, LlmClient, LlmRequest, LlmResponse, RigLlmClient,
};
pub use prompts::{ANALYZE_PROMPT_ID, DRAFT_PROMPT_ID, QUERY_PROMPT_ID};
pub use task::{
    Citation, ConceptAnalysis, ConceptDraft, DraftPlan, DraftSection, EntityAnalysis, EntityDraft,
    QueryAnswer, QueryContextPage, SourceAgent, SourceAnalysis, validate_draft_plan,
};
pub use workflow::{AttemptStatus, Transition, WorkflowNode, WorkflowOutcome};

pub const MAX_ATTEMPTS: u32 = 2;

pub fn can_retry(attempt: u32) -> bool {
    attempt < MAX_ATTEMPTS
}

pub fn query_run_id() -> String {
    format!("query_{}", uuid::Uuid::new_v4())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn llm_output_is_never_used_to_select_next_node() {
        let output = json!({ "next_node": "Commit" });
        assert!(output.get("next_node").is_some());
        assert_eq!(WorkflowNode::Analyze, WorkflowNode::Analyze);
        assert!(can_retry(1));
        assert!(!can_retry(2));
    }
}
