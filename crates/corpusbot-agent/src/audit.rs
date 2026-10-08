use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AgentError, Result};

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EventId(pub String);

impl EventId {
    pub fn generate() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkflowAuditEvent {
    pub event_id: EventId,
    pub run_id: String,
    pub node: crate::workflow::WorkflowNode,
    pub attempt: u32,
    pub status: crate::workflow::AttemptStatus,
    pub input_manifest_id: Option<String>,
    pub output_ref: Option<String>,
    pub prompt_template_id: Option<String>,
    pub prompt_hash: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub latency_ms: Option<u64>,
    pub tokens_in: Option<u64>,
    pub tokens_out: Option<u64>,
    pub max_tokens: Option<u64>,
    pub finish_reason: Option<String>,
    #[serde(default)]
    pub truncated: bool,
    pub decision: Option<String>,
    pub error_code: Option<String>,
}

impl WorkflowAuditEvent {
    pub fn start(
        run_id: impl Into<String>,
        node: crate::workflow::WorkflowNode,
        attempt: u32,
    ) -> Self {
        Self {
            event_id: EventId::generate(),
            run_id: run_id.into(),
            node,
            attempt,
            status: crate::workflow::AttemptStatus::Started,
            input_manifest_id: None,
            output_ref: None,
            prompt_template_id: None,
            prompt_hash: None,
            provider: None,
            model: None,
            latency_ms: None,
            tokens_in: None,
            tokens_out: None,
            max_tokens: None,
            finish_reason: None,
            truncated: false,
            decision: None,
            error_code: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_legacy_events_without_token_limit_fields() {
        let event: WorkflowAuditEvent = serde_json::from_value(serde_json::json!({
            "event_id": "legacy",
            "run_id": "legacy-run",
            "node": "analyze",
            "attempt": 1,
            "status": "succeeded",
            "input_manifest_id": null,
            "output_ref": null,
            "prompt_template_id": null,
            "prompt_hash": null,
            "provider": null,
            "model": null,
            "latency_ms": null,
            "tokens_in": null,
            "tokens_out": null,
            "decision": null,
            "error_code": null
        }))
        .unwrap();

        assert_eq!(event.max_tokens, None);
        assert_eq!(event.finish_reason, None);
        assert!(!event.truncated);
    }
}

#[async_trait]
pub trait AuditSink: Send + Sync {
    async fn record(&self, event: WorkflowAuditEvent) -> Result<()>;
    async fn store_artifact(
        &self,
        run_id: &str,
        file_name: &str,
        contents: &[u8],
    ) -> Result<String>;
}

#[derive(Clone)]
pub struct FileAuditSink {
    root: Arc<PathBuf>,
}

impl FileAuditSink {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: Arc::new(root.as_ref().join(".wiki-db/audit")),
        }
    }

    fn path(&self, run_id: &str) -> Result<PathBuf> {
        if run_id.is_empty() || run_id.contains(['/', '\\', '\0']) {
            return Err(AgentError::Audit("invalid run id".to_owned()));
        }
        Ok(self.root.join(run_id).join("events.jsonl"))
    }
}

#[async_trait]
impl AuditSink for FileAuditSink {
    async fn record(&self, event: WorkflowAuditEvent) -> Result<()> {
        let path = self.path(&event.run_id)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut line = serde_json::to_string(&event)?;
        line.push('\n');
        use tokio::io::AsyncWriteExt;
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await?;
        file.write_all(line.as_bytes()).await?;
        file.flush().await?;
        Ok(())
    }

    async fn store_artifact(
        &self,
        run_id: &str,
        file_name: &str,
        contents: &[u8],
    ) -> Result<String> {
        if run_id.is_empty() || run_id.contains(['/', '\\', '\0']) {
            return Err(AgentError::Audit("invalid run id".to_owned()));
        }
        if file_name.contains(['/', '\\', '\0']) || file_name.trim().is_empty() {
            return Err(AgentError::Audit("invalid artifact name".to_owned()));
        }
        let path = self.root.join(run_id).join(file_name);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(path, contents).await?;
        Ok(format!("audit/{run_id}/{file_name}"))
    }
}
