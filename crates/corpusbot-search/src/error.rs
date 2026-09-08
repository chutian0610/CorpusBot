use thiserror::Error;

pub type Result<T> = std::result::Result<T, SearchError>;

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("no search index generation has been built")]
    NoGeneration,
    #[error("search index generation is invalid: {0}")]
    InvalidGeneration(String),
    #[error("search field is invalid: {0}")]
    InvalidField(String),
    #[error(transparent)]
    Tantivy(#[from] tantivy::TantivyError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Persist(#[from] tempfile::PersistError),
}
