use thiserror::Error;

pub type Result<T> = std::result::Result<T, CoreError>;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("a core invariant was violated: {0}")]
    Invariant(String),
    #[error("invalid wiki path: {0}")]
    Path(String),
    #[error("invalid wikilink: {0}")]
    Wikilink(String),
    #[error("invalid frontmatter: {0}")]
    Frontmatter(String),
    #[error("invalid page identity: {0}")]
    PageIdentity(String),
    #[error("invalid resource revision: {0}")]
    Revision(String),
    #[error("revision conflict for {resource}: expected {expected}, found {current}")]
    RevisionConflict {
        resource: String,
        expected: String,
        current: String,
    },
    #[error("invalid revision manifest: {0}")]
    Manifest(String),
    #[error("template validation failed: {0}")]
    Template(String),
    #[error("invalid markdown document: {0}")]
    Markdown(String),
    #[error(transparent)]
    Yaml(#[from] serde_yaml::Error),
}
