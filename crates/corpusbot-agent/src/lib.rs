pub mod audit;
pub mod config;
pub mod error;
pub mod llm;
pub mod task;
pub mod workflow;

pub use audit::{AuditSink, FileAuditSink, WorkflowAuditEvent};
pub use config::ProviderConfig;
pub use error::{AgentError, Result};
pub use llm::{FakeLlmClient, LlmClient, LlmRequest, LlmResponse, RigLlmClient};
pub use task::{
    Citation, ConceptAnalysis, DraftPlan, EntityAnalysis, QueryAnswer, QueryContextPage,
    SourceAgent, SourceAnalysis,
};
pub use workflow::{AttemptStatus, Transition, WorkflowNode, WorkflowOutcome};

pub const MAX_ATTEMPTS: u32 = 2;

pub fn can_retry(attempt: u32) -> bool {
    attempt < MAX_ATTEMPTS
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
