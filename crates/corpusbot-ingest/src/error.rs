use thiserror::Error;

pub type Result<T> = std::result::Result<T, IngestError>;

#[derive(Debug, Error)]
pub enum IngestError {
    #[error("workspace tracked content is dirty: {paths:?}")]
    WorkspaceDirty { paths: Vec<String> },
    #[error("source file is not valid UTF-8 Markdown")]
    InvalidSourceEncoding,
    #[error("source filename is invalid")]
    InvalidSourceName,
    #[error("draft failed deterministic self audit: {0}")]
    SelfAudit(String),
    #[error(transparent)]
    Agent(#[from] corpusbot_agent::AgentError),
    #[error(transparent)]
    Core(#[from] corpusbot_core::CoreError),
    #[error(transparent)]
    Store(#[from] corpusbot_store::StoreError),
    #[error(transparent)]
    Vcs(#[from] corpusbot_vcs::VcsError),
    #[error(transparent)]
    SerdeYaml(#[from] serde_yaml::Error),
    #[error(transparent)]
    WalkDir(#[from] walkdir::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
