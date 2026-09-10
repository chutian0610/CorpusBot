use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::audit::{AuditSink, WorkflowAuditEvent};
use crate::error::{AgentError, Result};
use crate::llm::LlmResponse;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkflowOutcome<S> {
    Completed { state: S },
    Rejected { reason: String },
}

#[derive(Clone, Debug)]
pub struct LlmCallTelemetry {
    pub prompt_template_id: String,
    pub prompt_hash: String,
    pub provider: String,
    pub model: String,
    pub latency_ms: u64,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub response_id: Option<String>,
}

pub struct WorkflowContext<S> {
    pub state: S,
    pub run_id: String,
    pub input_manifest_id: Option<String>,
    attempts: HashMap<WorkflowNode, u32>,
    audit_sink: Arc<dyn AuditSink>,
    last_llm_call: Option<LlmCallTelemetry>,
    last_response: Option<LlmResponse>,
    output_ref: Option<String>,
    decision: Option<String>,
}

impl<S> WorkflowContext<S> {
    pub fn new(
        state: S,
        run_id: impl Into<String>,
        input_manifest_id: Option<String>,
        audit_sink: Arc<dyn AuditSink>,
    ) -> Self {
        Self {
            state,
            run_id: run_id.into(),
            input_manifest_id,
            attempts: HashMap::new(),
            audit_sink,
            last_llm_call: None,
            last_response: None,
            output_ref: None,
            decision: None,
        }
    }

    pub fn attempt(&self, node: WorkflowNode) -> u32 {
        self.attempts.get(&node).copied().unwrap_or_default()
    }

    pub fn can_retry(&self, node: WorkflowNode) -> bool {
        self.attempt(node) < crate::MAX_ATTEMPTS
    }

    pub fn set_output_ref(&mut self, reference: impl Into<String>) {
        self.output_ref = Some(reference.into());
    }

    pub fn output_ref(&self) -> Option<&str> {
        self.output_ref.as_deref()
    }

    pub fn set_decision(&mut self, decision: impl Into<String>) {
        self.decision = Some(decision.into());
    }

    pub fn record_llm_response(
        &mut self,
        response: &LlmResponse,
        request: &crate::llm::LlmRequest,
        latency_ms: u64,
    ) {
        self.last_llm_call = Some(LlmCallTelemetry {
            prompt_template_id: request.prompt_template_id.clone(),
            prompt_hash: request.prompt_hash(),
            provider: response.provider.clone(),
            model: response.model.clone(),
            latency_ms,
            tokens_in: response.prompt_tokens,
            tokens_out: response.completion_tokens,
            response_id: response.response_id.clone(),
        });
        self.last_response = Some(response.clone());
    }

    pub fn last_response(&self) -> Option<&LlmResponse> {
        self.last_response.as_ref()
    }

    async fn audit(
        &self,
        node: WorkflowNode,
        attempt: u32,
        status: AttemptStatus,
        error_code: Option<String>,
    ) -> Result<()> {
        let mut event = WorkflowAuditEvent::start(self.run_id.clone(), node, attempt);
        event.status = status;
        event.input_manifest_id = self.input_manifest_id.clone();
        event.output_ref = self.output_ref.clone();
        event.decision = self.decision.clone();
        event.error_code = error_code;
        if let Some(call) = &self.last_llm_call {
            event.prompt_template_id = Some(call.prompt_template_id.clone());
            event.prompt_hash = Some(call.prompt_hash.clone());
            event.provider = Some(call.provider.clone());
            event.model = Some(call.model.clone());
            event.latency_ms = Some(call.latency_ms);
            event.tokens_in = Some(call.tokens_in);
            event.tokens_out = Some(call.tokens_out);
        }
        self.audit_sink.record(event).await
    }
}

#[async_trait]
pub trait WorkflowNodeHandler<S>: Send + Sync {
    async fn run(&self, context: &mut WorkflowContext<S>) -> Result<Transition<S>>;
}

pub struct WorkflowKernel<S> {
    handlers: HashMap<WorkflowNode, Arc<dyn WorkflowNodeHandler<S>>>,
    initial: WorkflowNode,
    max_steps: usize,
}

impl<S> WorkflowKernel<S>
where
    S: Send + 'static,
{
    pub fn new(initial: WorkflowNode) -> Self {
        Self {
            handlers: HashMap::new(),
            initial,
            max_steps: 32,
        }
    }

    pub fn max_steps(mut self, max_steps: usize) -> Self {
        self.max_steps = max_steps;
        self
    }

    pub fn handler(mut self, node: WorkflowNode, handler: Arc<dyn WorkflowNodeHandler<S>>) -> Self {
        self.handlers.insert(node, handler);
        self
    }

    pub async fn run(&self, mut context: WorkflowContext<S>) -> Result<WorkflowOutcome<S>> {
        let mut node = self.initial;
        let mut steps = 0usize;

        loop {
            steps += 1;
            if steps > self.max_steps {
                return Err(AgentError::MaxSteps {
                    max_steps: self.max_steps,
                });
            }

            let Some(handler) = self.handlers.get(&node) else {
                return Err(AgentError::NoHandler {
                    node: format!("{node:?}"),
                });
            };

            let attempt = context.attempt(node) + 1;
            let status_key = format!("{node:?}");
            context.attempts.insert(node, attempt);
            context.last_llm_call = None;
            context.output_ref = None;
            context.decision = None;
            context
                .audit(node, attempt, AttemptStatus::Started, None)
                .await?;

            let transition = match handler.run(&mut context).await {
                Ok(transition) => transition,
                Err(error) => {
                    context
                        .audit(node, attempt, AttemptStatus::Failed, Some(status_key))
                        .await?;
                    return Err(error);
                }
            };

            match transition {
                Transition::Next { state, node: next } => {
                    context.state = state;
                    context
                        .audit(node, attempt, AttemptStatus::Succeeded, None)
                        .await?;
                    node = next;
                }
                Transition::Retry { state, reason } => {
                    context.state = state;
                    if context.can_retry(node) {
                        context
                            .audit(
                                node,
                                attempt,
                                AttemptStatus::ValidatorRejected,
                                Some(reason.clone()),
                            )
                            .await?;
                    } else {
                        let error = AgentError::AttemptsExhausted {
                            node: format!("{node:?}"),
                            attempts: crate::MAX_ATTEMPTS,
                            reason: reason.clone(),
                        };
                        context
                            .audit(
                                node,
                                attempt,
                                AttemptStatus::AttemptsExhausted,
                                Some(reason),
                            )
                            .await?;
                        return Err(error);
                    }
                }
                Transition::Reject { reason } => {
                    context
                        .audit(node, attempt, AttemptStatus::Failed, Some(reason.clone()))
                        .await?;
                    return Ok(WorkflowOutcome::Rejected { reason });
                }
                Transition::Done { state } => {
                    context.state = state;
                    context
                        .audit(node, attempt, AttemptStatus::Succeeded, None)
                        .await?;
                    return Ok(WorkflowOutcome::Completed {
                        state: context.state,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::FileAuditSink;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct FixedHandler {
        transition: Transition<Vec<String>>,
    }

    #[async_trait]
    impl WorkflowNodeHandler<Vec<String>> for FixedHandler {
        async fn run(
            &self,
            context: &mut WorkflowContext<Vec<String>>,
        ) -> Result<Transition<Vec<String>>> {
            context.set_decision("fixed");
            Ok(match &self.transition {
                Transition::Next { state, node } => Transition::Next {
                    state: state.clone(),
                    node: *node,
                },
                Transition::Retry { state, reason } => Transition::Retry {
                    state: state.clone(),
                    reason: reason.clone(),
                },
                Transition::Reject { reason } => Transition::Reject {
                    reason: reason.clone(),
                },
                Transition::Done { state } => Transition::Done {
                    state: state.clone(),
                },
            })
        }
    }

    #[tokio::test]
    async fn runs_a_deterministic_transition_chain() -> Result<()> {
        let root = tempfile::tempdir()?;
        let kernel: WorkflowKernel<Vec<String>> = WorkflowKernel::new(WorkflowNode::Analyze)
            .handler(
                WorkflowNode::Analyze,
                Arc::new(FixedHandler {
                    transition: Transition::Next {
                        state: vec!["analyzed".to_owned()],
                        node: WorkflowNode::SelfAudit,
                    },
                }),
            )
            .handler(
                WorkflowNode::SelfAudit,
                Arc::new(FixedHandler {
                    transition: Transition::Done {
                        state: vec!["audited".to_owned()],
                    },
                }),
            );
        let context = WorkflowContext::new(
            Vec::new(),
            "run-success",
            Some("manifest".to_owned()),
            Arc::new(FileAuditSink::new(root.path())),
        );

        let outcome = kernel.run(context).await?;
        assert_eq!(
            outcome,
            WorkflowOutcome::Completed {
                state: vec!["audited".to_owned()]
            }
        );
        let events =
            std::fs::read_to_string(root.path().join(".wiki-db/audit/run-success/events.jsonl"))?;
        assert_eq!(events.lines().count(), 4);
        Ok(())
    }

    struct RetryHandler(Arc<AtomicU32>);

    #[async_trait]
    impl WorkflowNodeHandler<Vec<String>> for RetryHandler {
        async fn run(
            &self,
            context: &mut WorkflowContext<Vec<String>>,
        ) -> Result<Transition<Vec<String>>> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Ok(Transition::Retry {
                state: context.state.clone(),
                reason: "schema mismatch".to_owned(),
            })
        }
    }

    #[tokio::test]
    async fn retry_loops_are_bounded() -> Result<()> {
        let root = tempfile::tempdir()?;
        let calls = Arc::new(AtomicU32::new(0));
        let kernel: WorkflowKernel<Vec<String>> = WorkflowKernel::new(WorkflowNode::Analyze)
            .handler(WorkflowNode::Analyze, Arc::new(RetryHandler(calls.clone())));
        let context = WorkflowContext::new(
            Vec::new(),
            "run-retry",
            None,
            Arc::new(FileAuditSink::new(root.path())),
        );

        let error = kernel.run(context).await.unwrap_err();
        assert!(matches!(
            error,
            AgentError::AttemptsExhausted { attempts: 2, .. }
        ));
        assert_eq!(calls.load(Ordering::Relaxed), 2);
        Ok(())
    }
}
