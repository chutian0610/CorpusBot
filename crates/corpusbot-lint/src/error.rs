use thiserror::Error;

pub type Result<T> = std::result::Result<T, LintError>;

#[derive(Debug, Error)]
pub enum LintError {
    #[error("workspace has pending recovery")]
    RecoveryPending,
    #[error(transparent)]
    Core(#[from] corpusbot_core::CoreError),
    #[error(transparent)]
    Yaml(#[from] serde_yaml::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    WalkDir(#[from] walkdir::Error),
}
