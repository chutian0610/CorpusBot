use thiserror::Error;

pub type Result<T> = std::result::Result<T, StoreError>;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("workspace root already contains files")]
    RootNotEmpty,
    #[error("workspace root is not a directory")]
    RootNotDirectory,
    #[error("workspace lock is held: {owner}")]
    Locked { owner: String },
    #[error("workspace has pending recovery")]
    RecoveryPending,
    #[error("workspace tracked content is dirty: {paths:?}")]
    WorkspaceDirty { paths: Vec<String> },
    #[error("invalid ingest run id")]
    InvalidRunId,
    #[error(
        "restore conflict: workspace changed after capture (expected {expected}, current {current})"
    )]
    RestoreConflict { expected: String, current: String },
    #[error("snapshot {0} was not found")]
    SnapshotNotFound(String),
    #[error(transparent)]
    Core(#[from] corpusbot_core::CoreError),
    #[error(transparent)]
    Vcs(#[from] corpusbot_vcs::VcsError),
    #[error(transparent)]
    Search(#[from] corpusbot_search::SearchError),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    WalkDir(#[from] walkdir::Error),
    #[error(transparent)]
    Persist(#[from] tempfile::PersistError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
