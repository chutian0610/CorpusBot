use thiserror::Error;

pub type Result<T> = std::result::Result<T, AgentError>;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("provider configuration is incomplete: {0}")]
    Configuration(String),
    #[error("provider request timed out after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },
    #[error("provider returned an empty response")]
    EmptyResponse,
    #[error("provider output did not match the schema: {0}")]
    Schema(String),
    #[error("workflow node {node} has no handler")]
    NoHandler { node: String },
    #[error("workflow node {node} exhausted {attempts} attempts: {reason}")]
    AttemptsExhausted {
        node: String,
        attempts: u32,
        reason: String,
    },
    #[error("workflow exceeded {max_steps} transitions")]
    MaxSteps { max_steps: usize },
    #[error("audit persistence failed: {0}")]
    Audit(String),
    #[error(transparent)]
    Core(#[from] corpusbot_core::CoreError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Rig(#[from] rig_core::completion::CompletionError),
    #[error(transparent)]
    HttpClient(#[from] rig_core::http_client::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
    #[error(transparent)]
    Persist(#[from] tempfile::PersistError),
    #[error(transparent)]
    ConfigurationSource(#[from] std::fmt::Error),
}
