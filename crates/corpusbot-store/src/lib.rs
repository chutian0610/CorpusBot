pub mod error;
pub mod lock;
pub mod metadata;
pub mod workspace;

pub use error::{Result, StoreError};
pub use lock::{LockOwner, WorkspaceLock};
pub use metadata::{Metadata, PageRow, SourceRow};
pub use workspace::{
    IngestCommitRequest, IngestCommitResult, PageFile, SnapshotResult, SnapshotRow, Workspace,
    WorkspacePaths, WorkspaceStatus, WorkspaceSummary,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_paths_stay_engine_private() {
        let paths = WorkspacePaths::new("/tmp/demo");
        assert!(paths.database.starts_with("/tmp/demo/.wiki-db/"));
    }
}
