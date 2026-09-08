use thiserror::Error;

pub type Result<T> = std::result::Result<T, VcsError>;

#[derive(Debug, Error)]
pub enum VcsError {
    #[error("the selected root is already a Git repository")]
    RepositoryExists,
    #[error("no Git repository was found at the workspace root")]
    NoRepository,
    #[error("the workspace repository is not on the main branch")]
    NotMain,
    #[error("the workspace repository is in an unsafe Git state: {0}")]
    UnsafeState(String),
    #[error("snapshot {0} was not found")]
    SnapshotNotFound(String),
    #[error("invalid Git object or path: {0}")]
    Invalid(String),
    #[error("{0}")]
    Other(String),
    #[error(transparent)]
    Git(#[from] git2::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
