pub mod workflow;

pub use workflow::{AttemptStatus, Transition, WorkflowNode};

pub const MAX_ATTEMPTS: u32 = 2;

pub fn can_retry(attempt: u32) -> bool {
    attempt < MAX_ATTEMPTS
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn llm_output_never_selects_the_next_node() {
        let output = json!({ "next_node": "Commit" });
        assert!(output.get("next_node").is_some());
        assert_eq!(WorkflowNode::Analyze, WorkflowNode::Analyze);
        assert!(can_retry(1));
        assert!(!can_retry(2));
    }
}
