pub mod error;
pub mod repository;

pub use error::{Result, VcsError};
pub use repository::{RepositoryHandle, ScopedUpdate, SnapshotSummary};
