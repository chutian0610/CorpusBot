use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowNode {
    Analyze,
    ValidateAnalysis,
    RepairAnalysis,
    RetrieveContext,
    GenerateDraft,
    ValidateDraft,
    RepairDraft,
    SelfAudit,
    Commit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptStatus {
    Started,
    Succeeded,
    SchemaRejected,
    ValidatorRejected,
    AttemptsExhausted,
    Failed,
}

#[derive(Clone, Debug)]
pub enum Transition<S> {
    Next { state: S, node: WorkflowNode },
    Retry { state: S, reason: String },
    Reject { reason: String },
    Done { state: S },
}

impl<S> Transition<S> {
    pub fn is_retry(&self) -> bool {
        matches!(self, Self::Retry { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transitions_are_explicit_and_typed() {
        let transition = Transition::Next {
            state: 7,
            node: WorkflowNode::GenerateDraft,
        };

        assert!(!transition.is_retry());
    }
}
