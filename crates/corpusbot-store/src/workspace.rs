use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use corpusbot_core::{ResourceId, ResourceRevision, RevisionManifest, Template, WikiDoc};
use corpusbot_search::{SearchDocument, SearchIndex};
use corpusbot_vcs::ScopedUpdate;
use serde::Serialize;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::error::{Result, StoreError};
use crate::lock::WorkspaceLock;
use crate::metadata::Metadata;

pub const GITIGNORE: &str = ".wiki-db/\n.DS_Store\nThumbs.db\n";
pub const INDEX_TEMPLATE: &str = "# Index\n\nA generated catalog of Wiki pages.\n";
pub const LOG_TEMPLATE: &str = "# Log\n\nAppend-only operation log.\n";
pub const RAW_KEEP: &str = "raw/.gitkeep";

#[derive(Clone, Debug)]
pub struct WorkspacePaths {
    pub root: PathBuf,
    pub gitignore: PathBuf,
    pub wiki_dir: PathBuf,
    pub raw_dir: PathBuf,
    pub engine_dir: PathBuf,
    pub database: PathBuf,
    pub drafts: PathBuf,
    pub search_index: PathBuf,
}

impl WorkspacePaths {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        Self {
            gitignore: root.join(".gitignore"),
            wiki_dir: root.join("wiki"),
            raw_dir: root.join("raw"),
            engine_dir: root.join(".wiki-db"),
            database: root.join(".wiki-db/corpusbot.sqlite3"),
            drafts: root.join(".wiki-db/drafts"),
            search_index: root.join(".wiki-db/tantivy"),
            root,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct WorkspaceSummary {
    pub root: String,
    pub template: String,
    pub head_snapshot_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct WorkspaceStatus {
    pub root: String,
    pub template: String,
    pub head_snapshot_id: Option<String>,
    pub dirty_paths: Vec<String>,
    pub unsafe_state: Option<String>,
    pub recovery_pending: bool,
    pub page_count: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct SnapshotResult {
    pub result: String,
    pub snapshot_id: String,
    pub manifest_id: String,
    pub workspace_changed_after_capture: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct SnapshotRow {
    pub snapshot_id: String,
    pub message: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct PageFile {
    pub path: String,
    pub markdown: String,
    pub title: String,
    pub page_type: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct IngestCommitRequest {
    pub run_id: String,
    pub message: String,
    pub source: Option<crate::SourceRow>,
    pub files: Vec<(String, Vec<u8>)>,
    pub pages: Vec<PageFile>,
    pub touched: Vec<ResourceRevision>,
    pub baseline_manifest: RevisionManifest,
}

#[derive(Clone, Debug, Serialize)]
pub struct IngestCommitResult {
    pub run_id: String,
    pub snapshot_id: String,
    pub manifest_id: String,
}

#[derive(Clone, Debug)]
pub struct GitIdentity {
    pub name: String,
    pub email: String,
}

enum SnapshotCapture {
    Clean {
        snapshot_id: String,
        manifest_id: String,
    },
    Dirty(CapturedSnapshot),
}

struct CapturedSnapshot {
    updates: Vec<corpusbot_vcs::ScopedUpdate>,
    manifest: RevisionManifest,
}

pub struct Workspace {
    paths: WorkspacePaths,
    template: Template,
    repository: corpusbot_vcs::RepositoryHandle,
    metadata: Metadata,
}

impl Workspace {
    pub fn init(root: impl AsRef<Path>, template: Template) -> Result<WorkspaceSummary> {
        Self::init_with_identity(root, template, None)
    }

    pub fn init_with_identity(
        root: impl AsRef<Path>,
        template: Template,
        identity: Option<GitIdentity>,
    ) -> Result<WorkspaceSummary> {
        let root = root.as_ref();
        if root.join(".git").exists() {
            return Err(corpusbot_vcs::VcsError::RepositoryExists.into());
        }
        if root.exists() && !root.is_dir() {
            return Err(StoreError::RootNotDirectory);
        }
        if root.exists() && root.read_dir()?.next().is_some() {
            return Err(StoreError::RootNotEmpty);
        }

        std::fs::create_dir_all(root)?;
        let repository = corpusbot_vcs::RepositoryHandle::init(root)?;
        if let Some(identity) = identity {
            repository.set_identity(&identity.name, &identity.email)?;
        }
        let paths = WorkspacePaths::new(root);
        std::fs::create_dir_all(&paths.wiki_dir)?;
        std::fs::create_dir_all(&paths.raw_dir)?;
        std::fs::create_dir_all(&paths.drafts)?;
        std::fs::create_dir_all(&paths.search_index)?;
        std::fs::write(&paths.gitignore, GITIGNORE)?;
        std::fs::write(paths.wiki_dir.join("index.md"), INDEX_TEMPLATE)?;
        std::fs::write(paths.wiki_dir.join("log.md"), LOG_TEMPLATE)?;
        std::fs::write(paths.raw_dir.join(".gitkeep"), "")?;

        let metadata = Metadata::open(&paths.database)?;
        metadata.set_meta("template", template.as_str())?;

        let updates = vec![
            ScopedUpdate::put(".gitignore", GITIGNORE),
            ScopedUpdate::put("wiki/index.md", INDEX_TEMPLATE),
            ScopedUpdate::put("wiki/log.md", LOG_TEMPLATE),
            ScopedUpdate::put(RAW_KEEP, ""),
        ];
        let snapshot =
            repository.commit_scoped(&format!("init {}", template.as_str()), &updates, &[])?;
        Ok(WorkspaceSummary {
            root: paths.root.display().to_string(),
            template: template.as_str().to_owned(),
            head_snapshot_id: Some(snapshot.commit_id),
        })
    }

    pub fn open(root: impl AsRef<Path>, template: Template) -> Result<Self> {
        Self::open_with_identity(root, template, None)
    }

    pub fn open_with_identity(
        root: impl AsRef<Path>,
        template: Template,
        identity: Option<GitIdentity>,
    ) -> Result<Self> {
        let paths = WorkspacePaths::new(root.as_ref());
        let repository = corpusbot_vcs::RepositoryHandle::open(&paths.root)?;
        if let Some(identity) = identity {
            repository.set_identity(&identity.name, &identity.email)?;
        }
        let metadata = Metadata::open(&paths.database)?;
        let stored_template = metadata
            .get_meta("template")?
            .unwrap_or_else(|| template.as_str().to_owned());
        let template = Template::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == stored_template)
            .unwrap_or(template);

        let workspace = Self {
            paths,
            template,
            repository,
            metadata,
        };
        workspace.reconcile_pending_restore()?;
        workspace.reconcile_pending_ingest()?;
        Ok(workspace)
    }

    pub fn paths(&self) -> &WorkspacePaths {
        &self.paths
    }

    pub fn template(&self) -> Template {
        self.template
    }

    pub fn summary(&self) -> Result<WorkspaceSummary> {
        Ok(WorkspaceSummary {
            root: self.paths.root.display().to_string(),
            template: self.template.as_str().to_owned(),
            head_snapshot_id: self.repository.head_id()?,
        })
    }

    pub fn refresh_pages(&self) -> Result<Vec<String>> {
        let mut current = BTreeMap::new();
        for entry in WalkDir::new(&self.paths.wiki_dir).sort_by_file_name() {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }
            let relative = entry
                .path()
                .strip_prefix(&self.paths.root)
                .map_err(|error| {
                    StoreError::Core(corpusbot_core::CoreError::Path(error.to_string()))
                })?
                .to_string_lossy()
                .replace('\\', "/");
            let content = std::fs::read(entry.path())?;
            let text = String::from_utf8_lossy(&content);
            let Ok((doc, _)) =
                WikiDoc::parse_markdown(corpusbot_core::WikiPath::parse(&relative)?, &text)
            else {
                continue;
            };
            let page = crate::metadata::PageRow {
                path: relative.clone(),
                title: doc.frontmatter().title().to_owned(),
                page_type: doc.frontmatter().page_type().as_str().to_owned(),
                sha256: hex(&content),
                updated_at: doc.frontmatter().updated().to_string(),
            };
            self.metadata.upsert_page(&page)?;
            current.insert(relative, ());
        }

        for path in self.metadata.page_paths()? {
            if path.starts_with("wiki/")
                && path != "wiki/index.md"
                && path != "wiki/log.md"
                && !current.contains_key(&path)
            {
                self.metadata.remove_page(&path)?;
            }
        }
        Ok(Vec::new())
    }

    pub fn pages(&self) -> Result<Vec<crate::metadata::PageRow>> {
        self.metadata.pages()
    }

    pub fn status(&self) -> Result<WorkspaceStatus> {
        self.refresh_pages()?;
        Ok(WorkspaceStatus {
            root: self.paths.root.display().to_string(),
            template: self.template.as_str().to_owned(),
            head_snapshot_id: self.repository.head_id()?,
            dirty_paths: self.repository.dirty_paths()?,
            unsafe_state: self.repository.unsafe_state(),
            recovery_pending: self.metadata.has_pending_recovery()?,
            page_count: self.metadata.pages()?.len(),
        })
    }

    pub fn revision_manifest(&self) -> Result<RevisionManifest> {
        capture_revision_manifest(
            &self.paths.root,
            &self.repository.head_id()?.unwrap_or_default(),
        )
    }

    pub fn read_page(&self, path: &str) -> Result<String> {
        corpusbot_core::WikiPath::parse(path)?;
        Ok(std::fs::read_to_string(self.paths.root.join(path))?)
    }

    pub fn source_by_sha(&self, sha256: &str) -> Result<Option<crate::SourceRow>> {
        self.metadata.source_by_sha(sha256)
    }

    pub fn write_draft(&self, run_id: &str, relative: &str, content: &[u8]) -> Result<PathBuf> {
        ResourceId::new(relative)?;
        let path = self.paths.drafts.join(run_id).join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file =
            tempfile::NamedTempFile::new_in(path.parent().expect("draft parent exists"))?;
        std::io::Write::write_all(&mut file, content)?;
        file.persist(&path)?;
        Ok(path)
    }

    pub fn write_raw_source(&self, relative: &str, content: &[u8]) -> Result<()> {
        ResourceId::new(relative)?;
        let full = self.paths.root.join(relative);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = tempfile::NamedTempFile::new_in(full.parent().expect("raw parent exists"))?;
        std::io::Write::write_all(&mut file, content)?;
        file.persist(full)?;
        Ok(())
    }

    pub fn begin_ingest(&self, request: &IngestCommitRequest) -> Result<()> {
        self.reconcile_pending_restore()?;
        self.reconcile_pending_ingest()?;
        validate_run_id(&request.run_id)?;
        let _lock = WorkspaceLock::acquire(&self.paths.root, "ingest-begin")?;
        if self.metadata.has_pending_recovery()? {
            return Err(StoreError::RecoveryPending);
        }
        if let Some(state) = self.repository.unsafe_state() {
            return Err(corpusbot_vcs::VcsError::UnsafeState(state).into());
        }
        if self.repository.is_dirty()? {
            return Err(StoreError::WorkspaceDirty {
                paths: self.repository.dirty_paths()?,
            });
        }

        let backup_dir = self.recovery_backup_dir(&request.run_id)?;
        self.backup_touched_paths(request, &backup_dir)?;
        let relative_backup_dir = backup_dir
            .strip_prefix(&self.paths.root)
            .map_err(|error| StoreError::Core(corpusbot_core::CoreError::Path(error.to_string())))?
            .to_string_lossy();
        self.metadata.insert_pending_ingest(
            &request.run_id,
            request
                .source
                .as_ref()
                .map_or("", |source| source.source_id.as_str()),
            request.baseline_manifest.head_snapshot_id(),
            request.baseline_manifest.manifest_id(),
            &serde_json::to_string(&request.touched)?,
            &serde_json::to_string(request)?,
            &rfc3339_now(),
            &relative_backup_dir,
        )?;
        Ok(())
    }

    pub fn commit_ingest(&self, request: &IngestCommitRequest) -> Result<IngestCommitResult> {
        self.reconcile_pending_restore()?;
        self.reconcile_pending_ingest()?;
        let lock = WorkspaceLock::acquire(&self.paths.root, "ingest-commit")?;
        if self.metadata.has_pending_recovery()? {
            return Err(StoreError::RecoveryPending);
        }
        if let Some(state) = self.repository.unsafe_state() {
            return Err(corpusbot_vcs::VcsError::UnsafeState(state).into());
        }

        let head = self.repository.head_id()?.unwrap_or_default();
        let current = capture_revision_manifest(&self.paths.root, &head)?;
        for touched in &request.touched {
            let found = current.expected(touched.resource());
            if found != *touched.revision() {
                return Err(corpusbot_core::CoreError::RevisionConflict {
                    resource: touched.resource().path().to_owned(),
                    expected: touched.revision().key(),
                    current: found.key(),
                }
                .into());
            }
        }

        let mut old_content = BTreeMap::new();
        let mut updates = self.repository.scoped_updates()?;
        for (path, content) in &request.files {
            let full = self.paths.root.join(path);
            if full.exists() {
                old_content.insert(path.clone(), std::fs::read(&full)?);
            }
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut file =
                tempfile::NamedTempFile::new_in(full.parent().ok_or_else(|| {
                    StoreError::Core(corpusbot_core::CoreError::Path(path.clone()))
                })?)?;
            std::io::Write::write_all(&mut file, content)?;
            file.persist(&full)?;
            updates.retain(|update| update.path() != path);
            updates.push(ScopedUpdate::put(path.clone(), content.clone()));
        }

        let transaction = self.metadata.unchecked_transaction()?;
        if let Some(source) = &request.source {
            crate::metadata::Metadata::insert_source_tx(&transaction, source)?;
        }
        for page in &request.pages {
            crate::metadata::Metadata::upsert_page_tx(
                &transaction,
                &crate::metadata::PageRow {
                    path: page.path.clone(),
                    title: page.title.clone(),
                    page_type: page.page_type.clone(),
                    sha256: hex(page.markdown.as_bytes()),
                    updated_at: page.updated_at.clone(),
                },
            )?;
        }

        let snapshot = match self.repository.commit_scoped(
            &request.message,
            &updates,
            &[("CorpusBot-Run", request.run_id.as_str())],
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                for (path, content) in old_content {
                    let full = self.paths.root.join(path);
                    if let Some(parent) = full.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(full, content)?;
                }
                self.restore_baseline_touched(request)?;
                self.metadata
                    .mark_ingest_finished(&request.run_id, "failed")?;
                return Err(error.into());
            }
        };
        transaction.commit()?;
        let manifest = capture_revision_manifest(&self.paths.root, &snapshot.commit_id)?;
        drop(lock);

        Ok(IngestCommitResult {
            run_id: request.run_id.clone(),
            snapshot_id: snapshot.commit_id,
            manifest_id: manifest.manifest_id().to_owned(),
        })
    }

    pub fn snapshot(&self, message: &str) -> Result<SnapshotResult> {
        self.reconcile_pending_restore()?;
        self.reconcile_pending_ingest()?;
        let lock = WorkspaceLock::acquire(&self.paths.root, "snapshot")?;
        match self.capture_snapshot_locked(&lock)? {
            SnapshotCapture::Clean {
                snapshot_id,
                manifest_id,
            } => {
                drop(lock);
                Ok(SnapshotResult {
                    result: "already_clean".to_owned(),
                    snapshot_id,
                    manifest_id,
                    workspace_changed_after_capture: false,
                })
            }
            SnapshotCapture::Dirty(captured) => {
                let result = self.commit_snapshot_locked(&lock, message, captured)?;
                drop(lock);
                Ok(result)
            }
        }
    }

    fn capture_snapshot_locked(&self, _lock: &WorkspaceLock) -> Result<SnapshotCapture> {
        let head_before = self.repository.head_id()?;
        if !self.repository.is_dirty()? {
            let snapshot_id =
                head_before.ok_or_else(|| StoreError::SnapshotNotFound("HEAD".into()))?;
            let manifest_id = capture_manifest(&self.paths.root, &snapshot_id)?;
            return Ok(SnapshotCapture::Clean {
                snapshot_id,
                manifest_id,
            });
        }

        let updates = self.repository.scoped_updates()?;
        let manifest = manifest_from_scope(&updates, head_before.as_deref().unwrap_or_default())?;
        Ok(SnapshotCapture::Dirty(CapturedSnapshot {
            updates,
            manifest,
        }))
    }

    fn commit_snapshot_locked(
        &self,
        _lock: &WorkspaceLock,
        message: &str,
        captured: CapturedSnapshot,
    ) -> Result<SnapshotResult> {
        let CapturedSnapshot { updates, manifest } = captured;
        let snapshot = self.repository.commit_scoped(message, &updates, &[])?;
        let current_manifest = capture_revision_manifest(&self.paths.root, &snapshot.commit_id)?;
        Ok(SnapshotResult {
            result: "created".to_owned(),
            snapshot_id: snapshot.commit_id,
            manifest_id: manifest.manifest_id().to_owned(),
            workspace_changed_after_capture: current_manifest.manifest_id()
                != manifest.manifest_id(),
        })
    }

    pub fn history(&self, limit: usize) -> Result<Vec<SnapshotRow>> {
        Ok(self
            .repository
            .history(limit)?
            .into_iter()
            .map(|summary| SnapshotRow {
                snapshot_id: summary.commit_id,
                message: summary.message,
                created_at: summary.created_at,
            })
            .collect())
    }

    pub fn restore(&self, snapshot_id: &str) -> Result<()> {
        self.reconcile_pending_restore()?;
        self.reconcile_pending_ingest()?;
        let run_id = self.prepare_restore(snapshot_id)?;
        self.activate_restore(&run_id)?;
        Ok(())
    }

    pub fn reconcile_pending_restore(&self) -> Result<()> {
        if !self.metadata.has_pending_recovery()? {
            return Ok(());
        }
        let lock = WorkspaceLock::acquire(&self.paths.root, "restore-recovery")?;
        let Some(run) = self.metadata.pending_restore()? else {
            return Ok(());
        };
        let target_commit = self
            .repository
            .commit_id_with_trailer("CorpusBot-Restore-Run", &run.run_id)?;

        let Some(target_commit) = target_commit else {
            self.metadata
                .mark_restore_finished(&run.run_id, "aborted")?;
            drop(lock);
            self.cleanup_restore_staging(&run.run_id);
            return Ok(());
        };

        if self.repository.is_dirty()? {
            self.repository.snapshot_scoped(
                &format!("recovery backup for restore {}", run.run_id),
                &[("CorpusBot-Restore-Backup", run.run_id.as_str())],
            )?;
        }
        let selected = self.repository.scoped_content(&target_commit)?;
        let updates = scoped_updates(&selected);
        self.repository.commit_scoped(
            &format!("complete restore {}", run.target_snapshot_id),
            &updates,
            &[("CorpusBot-Restore-Run", run.run_id.as_str())],
        )?;
        self.write_scoped_worktree(&selected)?;
        self.refresh_pages()?;
        self.rebuild_search_index()?;
        self.metadata
            .mark_restore_finished(&run.run_id, "completed")?;
        drop(lock);
        self.cleanup_restore_staging(&run.run_id);
        Ok(())
    }

    fn prepare_restore(&self, snapshot_id: &str) -> Result<String> {
        let lock = WorkspaceLock::acquire(&self.paths.root, "restore-prepare")?;
        if self.metadata.has_pending_recovery()? {
            return Err(StoreError::RecoveryPending);
        }
        if let Some(state) = self.repository.unsafe_state() {
            return Err(corpusbot_vcs::VcsError::UnsafeState(state).into());
        }

        let selected = self.repository.scoped_content(snapshot_id)?;
        let expected_manifest = self.revision_manifest()?;
        let run_id = format!(
            "restore_{:x}",
            time::OffsetDateTime::now_utc().unix_timestamp_nanos()
        );
        let staging_dir = self.restore_staging_dir(&run_id)?;
        self.stage_scoped_content(&staging_dir, &selected)?;

        let pre_restore = self.repository.snapshot_scoped(
            &format!("pre-restore {snapshot_id}"),
            &[("CorpusBot-Restore-Baseline", run_id.as_str())],
        )?;
        self.metadata.insert_pending_restore(
            &run_id,
            snapshot_id,
            &pre_restore.commit_id,
            expected_manifest.manifest_id(),
            &rfc3339_now(),
        )?;

        let current_manifest = self.revision_manifest()?;
        if current_manifest.manifest_id() != expected_manifest.manifest_id() {
            self.metadata.mark_restore_finished(&run_id, "conflicted")?;
            drop(lock);
            self.cleanup_restore_staging(&run_id);
            return Err(StoreError::RestoreConflict {
                expected: expected_manifest.manifest_id().to_owned(),
                current: current_manifest.manifest_id().to_owned(),
            });
        }

        self.metadata.update_restore_phase(&run_id, "switching")?;
        Ok(run_id)
    }

    fn activate_restore(&self, run_id: &str) -> Result<()> {
        let lock = WorkspaceLock::acquire(&self.paths.root, "restore-switch")?;
        let run = self
            .metadata
            .pending_restore()?
            .filter(|run| run.run_id == run_id)
            .ok_or(StoreError::RecoveryPending)?;
        let current_manifest = self.revision_manifest()?;
        if current_manifest.manifest_id() != run.expected_manifest_id {
            self.metadata.mark_restore_finished(run_id, "conflicted")?;
            drop(lock);
            self.cleanup_restore_staging(run_id);
            return Err(StoreError::RestoreConflict {
                expected: run.expected_manifest_id.clone(),
                current: current_manifest.manifest_id().to_owned(),
            });
        }

        let selected = self.repository.scoped_content(&run.target_snapshot_id)?;
        let updates = scoped_updates(&selected);
        let committed = self.repository.commit_scoped(
            &format!("restore {}", run.target_snapshot_id),
            &updates,
            &[("CorpusBot-Restore-Run", run.run_id.as_str())],
        );
        let target_commit = match committed {
            Ok(summary) => summary.commit_id,
            Err(error) => {
                let Some(commit_id) = self
                    .repository
                    .commit_id_with_trailer("CorpusBot-Restore-Run", run_id)?
                else {
                    self.metadata.mark_restore_finished(run_id, "failed")?;
                    drop(lock);
                    self.cleanup_restore_staging(run_id);
                    return Err(error.into());
                };
                commit_id
            }
        };
        let committed_content = self.repository.scoped_content(&target_commit)?;
        self.write_scoped_worktree(&committed_content)?;
        self.refresh_pages()?;
        self.rebuild_search_index()?;
        self.metadata.mark_restore_finished(run_id, "completed")?;
        drop(lock);
        self.cleanup_restore_staging(run_id);
        Ok(())
    }

    fn restore_staging_dir(&self, run_id: &str) -> Result<PathBuf> {
        validate_run_id(run_id)?;
        Ok(self
            .paths
            .engine_dir
            .join("restore")
            .join(run_id)
            .join("target"))
    }

    fn stage_scoped_content(
        &self,
        staging_dir: &Path,
        content: &BTreeMap<String, Vec<u8>>,
    ) -> Result<()> {
        if staging_dir.exists() {
            std::fs::remove_dir_all(staging_dir)?;
        }
        std::fs::create_dir_all(staging_dir)?;
        for (path, bytes) in content {
            validate_scoped_path(path)?;
            let target = staging_dir.join(path);
            write_atomic(&target, bytes)?;
        }
        Ok(())
    }

    fn write_scoped_worktree(&self, content: &BTreeMap<String, Vec<u8>>) -> Result<()> {
        for path in content.keys() {
            validate_scoped_path(path)?;
        }
        for directory in [&self.paths.wiki_dir, &self.paths.raw_dir] {
            if directory.exists() {
                std::fs::remove_dir_all(directory)?;
            }
            std::fs::create_dir_all(directory)?;
        }
        for (path, bytes) in content {
            write_atomic(&self.paths.root.join(path), bytes)?;
        }
        Ok(())
    }

    fn cleanup_restore_staging(&self, run_id: &str) {
        if let Ok(directory) = self.restore_staging_dir(run_id) {
            let _ = std::fs::remove_dir_all(
                directory
                    .parent()
                    .map_or_else(|| directory.clone(), Path::to_path_buf),
            );
        }
    }

    pub fn reconcile_pending_ingest(&self) -> Result<()> {
        if !self.metadata.has_pending_recovery()? {
            return Ok(());
        };
        let lock = WorkspaceLock::acquire(&self.paths.root, "ingest-recovery")?;
        let Some(run) = self.metadata.pending_ingest()? else {
            return Ok(());
        };
        let request = run
            .request_json
            .as_deref()
            .ok_or(StoreError::RecoveryPending)?;
        let request: IngestCommitRequest = serde_json::from_str(request)?;
        let run_commit = self
            .repository
            .commit_id_with_trailer("CorpusBot-Run", &run.run_id)?;

        if run_commit.is_some() {
            let transaction = self.metadata.unchecked_transaction()?;
            if let Some(source) = &request.source {
                crate::metadata::Metadata::insert_source_tx(&transaction, source)?;
            }
            for page in &request.pages {
                crate::metadata::Metadata::upsert_page_tx(
                    &transaction,
                    &crate::metadata::PageRow {
                        path: page.path.clone(),
                        title: page.title.clone(),
                        page_type: page.page_type.clone(),
                        sha256: hex(page.markdown.as_bytes()),
                        updated_at: page.updated_at.clone(),
                    },
                )?;
            }
            transaction.commit()?;
        } else {
            self.restore_baseline_touched(&request)?;
        }

        self.rebuild_search_index()?;
        self.metadata.mark_ingest_finished(
            &run.run_id,
            if run_commit.is_some() {
                "committed"
            } else {
                "recovered"
            },
        )?;
        drop(lock);
        if run_commit.is_none() {
            let Some(relative) = run.current_backup_dir.as_deref() else {
                return Ok(());
            };
            let backup_root = Path::new(relative).parent().map_or_else(
                || self.paths.root.join(relative),
                |parent| self.paths.root.join(parent),
            );
            let _ = std::fs::remove_dir_all(backup_root);
        }
        Ok(())
    }

    pub fn rebuild_search_index(&self) -> Result<()> {
        let documents = self.search_documents()?;
        SearchIndex::new(self.paths.search_index.clone()).rebuild(&documents)?;
        Ok(())
    }

    fn recovery_backup_dir(&self, run_id: &str) -> Result<PathBuf> {
        Ok(self
            .paths
            .engine_dir
            .join("recovery")
            .join(run_id)
            .join("current"))
    }

    fn backup_touched_paths(&self, request: &IngestCommitRequest, backup_dir: &Path) -> Result<()> {
        if backup_dir.exists() {
            return Ok(());
        }
        std::fs::create_dir_all(backup_dir)?;
        for touched in &request.touched {
            let path = touched.resource().path();
            let full = self.paths.root.join(path);
            if full.is_file() {
                std::fs::create_dir_all(backup_dir.join(path).parent().ok_or_else(|| {
                    StoreError::Core(corpusbot_core::CoreError::Path(path.to_owned()))
                })?)?;
                std::fs::copy(full, backup_dir.join(path))?;
            }
        }
        Ok(())
    }

    fn restore_baseline_touched(&self, request: &IngestCommitRequest) -> Result<()> {
        let baseline = self
            .repository
            .scoped_content(request.baseline_manifest.head_snapshot_id())?;
        for touched in &request.touched {
            let path = touched.resource().path();
            let full = self.paths.root.join(path);
            if let Some(content) = baseline.get(path) {
                if let Some(parent) = full.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let mut file =
                    tempfile::NamedTempFile::new_in(full.parent().ok_or_else(|| {
                        StoreError::Core(corpusbot_core::CoreError::Path(path.to_owned()))
                    })?)?;
                std::io::Write::write_all(&mut file, content)?;
                file.persist(&full)?;
            } else if full.is_file() {
                std::fs::remove_file(&full)?;
            }
        }
        Ok(())
    }

    fn search_documents(&self) -> Result<Vec<SearchDocument>> {
        let mut documents = Vec::new();
        for entry in WalkDir::new(&self.paths.wiki_dir).sort_by_file_name() {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry
                .path()
                .strip_prefix(&self.paths.root)
                .map_err(|error| {
                    StoreError::Core(corpusbot_core::CoreError::Path(error.to_string()))
                })?
                .to_string_lossy()
                .replace('\\', "/");
            let markdown = std::fs::read_to_string(entry.path())?;
            if let Ok((document, _)) =
                WikiDoc::parse_markdown(corpusbot_core::WikiPath::parse(&path)?, &markdown)
            {
                documents.push(SearchDocument {
                    path,
                    title: document.frontmatter().title().to_owned(),
                    page_type: document.frontmatter().page_type().as_str().to_owned(),
                    tags: document.frontmatter().tags().to_vec(),
                    body: markdown,
                    updated_at: document.frontmatter().updated().to_string(),
                });
            }
        }
        Ok(documents)
    }
}

fn validate_run_id(run_id: &str) -> Result<()> {
    if run_id.is_empty() || run_id.contains(['/', '\\', '\0']) || matches!(run_id, "." | "..") {
        return Err(StoreError::InvalidRunId);
    }
    Ok(())
}

fn rfc3339_now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown-time".to_owned())
}

fn scoped_updates(content: &BTreeMap<String, Vec<u8>>) -> Vec<ScopedUpdate> {
    content
        .iter()
        .map(|(path, bytes)| ScopedUpdate::put(path.clone(), bytes.clone()))
        .collect()
}

fn validate_scoped_path(path: &str) -> Result<()> {
    ResourceId::new(path)?;
    if path != ".gitignore" && !path.starts_with("wiki/") && !path.starts_with("raw/") {
        return Err(StoreError::Core(corpusbot_core::CoreError::Path(
            path.to_owned(),
        )));
    }
    Ok(())
}

fn write_atomic(path: &Path, content: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = tempfile::NamedTempFile::new_in(path.parent().ok_or_else(|| {
        StoreError::Core(corpusbot_core::CoreError::Path(path.display().to_string()))
    })?)?;
    std::io::Write::write_all(&mut file, content)?;
    file.persist(path)?;
    Ok(())
}

pub fn capture_revision_manifest(root: &Path, head_snapshot_id: &str) -> Result<RevisionManifest> {
    let mut resources = Vec::new();
    for directory in ["wiki", "raw"] {
        let full = root.join(directory);
        if !full.exists() {
            continue;
        }
        for entry in WalkDir::new(full).sort_by_file_name() {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry
                .path()
                .strip_prefix(root)
                .map_err(|error| {
                    StoreError::Core(corpusbot_core::CoreError::Path(error.to_string()))
                })?
                .to_string_lossy()
                .replace('\\', "/");
            let content = std::fs::read(entry.path())?;
            resources.push(corpusbot_core::ResourceRevision::content(
                corpusbot_core::ResourceId::new(path)?,
                &content,
            ));
        }
    }
    let gitignore = std::fs::read(root.join(".gitignore"))?;
    resources.push(corpusbot_core::ResourceRevision::content(
        corpusbot_core::ResourceId::new(".gitignore")?,
        &gitignore,
    ));
    Ok(RevisionManifest::capture(resources, head_snapshot_id)?)
}

fn capture_manifest(root: &Path, head_snapshot_id: &str) -> Result<String> {
    Ok(capture_revision_manifest(root, head_snapshot_id)?
        .manifest_id()
        .to_owned())
}

fn manifest_from_scope(
    updates: &[corpusbot_vcs::ScopedUpdate],
    head_snapshot_id: &str,
) -> Result<RevisionManifest> {
    let mut resources = Vec::new();
    for update in updates {
        let Some(content) = update.content() else {
            continue;
        };
        let resource = ResourceId::new(update.path())?;
        resources.push(ResourceRevision::content(resource, content));
    }
    Ok(RevisionManifest::capture(resources, head_snapshot_id)?)
}

pub fn hex(content: &[u8]) -> String {
    format!("{:x}", Sha256::digest(content))
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    const PAGE: &str = r#"---
type: entity
title: Raft
created: 2026-09-07
updated: 2026-09-07
tags: [distributed-systems]
related: []
sources: []
---

Raft elects a leader.
"#;

    #[test]
    fn initializes_snapshots_and_restores() -> Result<()> {
        let root = tempfile::tempdir()?;
        let summary = Workspace::init(root.path(), Template::Research)?;
        assert_eq!(summary.template, "research");
        assert!(root.path().join(".gitignore").exists());
        assert!(root.path().join("wiki/index.md").exists());
        assert!(root.path().join("raw/.gitkeep").exists());

        let workspace = Workspace::open(root.path(), Template::Research)?;
        assert_eq!(workspace.status()?.page_count, 0);
        assert!(workspace.status()?.dirty_paths.is_empty());

        std::fs::create_dir_all(root.path().join("wiki/entities"))?;
        std::fs::write(root.path().join("wiki/entities/Raft.md"), PAGE)?;
        let status = workspace.status()?;
        assert_eq!(status.page_count, 1);
        assert_eq!(status.dirty_paths, vec!["wiki/entities/Raft.md"]);

        let first = workspace.snapshot("one source")?;
        assert_eq!(first.result, "created");
        std::fs::write(
            root.path().join("wiki/entities/Vector.md"),
            PAGE.replace("Raft", "Vector"),
        )?;
        let second = workspace.snapshot("two sources")?;
        assert_eq!(second.result, "created");

        workspace.restore(&first.snapshot_id)?;
        let status = workspace.status()?;
        assert_eq!(status.page_count, 1);
        assert!(!root.path().join("wiki/entities/Vector.md").exists());
        let index = SearchIndex::new(root.path().join(".wiki-db/tantivy"));
        let hits = index.search("Raft", 8)?;
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "wiki/entities/Raft.md");
        assert!(index.search("Vector", 8)?.is_empty());

        let history = workspace.history(10)?;
        assert!(history.len() >= 4);
        Ok(())
    }

    #[test]
    fn clean_snapshot_returns_head_without_a_new_commit() -> Result<()> {
        let root = tempfile::tempdir()?;
        Workspace::init(root.path(), Template::Research)?;
        let workspace = Workspace::open(root.path(), Template::Research)?;
        let head = workspace
            .summary()?
            .head_snapshot_id
            .expect("init snapshot");

        let snapshot = workspace.snapshot("clean")?;
        assert_eq!(snapshot.result, "already_clean");
        assert_eq!(snapshot.snapshot_id, head);
        assert!(!snapshot.workspace_changed_after_capture);
        assert_eq!(workspace.history(10)?.len(), 1);
        Ok(())
    }

    #[test]
    fn dirty_snapshot_freezes_capture_and_reports_later_changes() -> Result<()> {
        let root = tempfile::tempdir()?;
        Workspace::init(root.path(), Template::Research)?;
        let workspace = Workspace::open(root.path(), Template::Research)?;
        let path = root.path().join("wiki/entities/Raft.md");
        std::fs::create_dir_all(path.parent().expect("wiki parent"))?;
        std::fs::write(&path, PAGE)?;

        let lock = WorkspaceLock::acquire(root.path(), "snapshot-test")?;
        let captured = match workspace.capture_snapshot_locked(&lock)? {
            SnapshotCapture::Dirty(captured) => captured,
            SnapshotCapture::Clean { .. } => panic!("dirty workspace captured as clean"),
        };
        std::fs::write(&path, PAGE.replace("Raft elects", "The user changed Raft"))?;
        let snapshot = workspace.commit_snapshot_locked(&lock, "captured", captured)?;
        drop(lock);

        assert_eq!(snapshot.result, "created");
        assert!(snapshot.workspace_changed_after_capture);
        let committed = workspace.repository.scoped_content(&snapshot.snapshot_id)?;
        assert_eq!(
            committed.get("wiki/entities/Raft.md").map(Vec::as_slice),
            Some(PAGE.as_bytes())
        );
        assert_eq!(
            std::fs::read_to_string(&path)?,
            PAGE.replace("Raft elects", "The user changed Raft")
        );
        assert!(!workspace.status()?.dirty_paths.is_empty());
        Ok(())
    }

    #[test]
    fn restore_conflict_preserves_changes_after_capture() -> Result<()> {
        let root = tempfile::tempdir()?;
        Workspace::init(root.path(), Template::Research)?;
        let workspace = Workspace::open(root.path(), Template::Research)?;
        std::fs::create_dir_all(root.path().join("wiki/entities"))?;
        std::fs::write(root.path().join("wiki/entities/Raft.md"), PAGE)?;
        let first = workspace.snapshot("first")?;
        std::fs::write(
            root.path().join("wiki/entities/Vector.md"),
            PAGE.replace("Raft", "Vector"),
        )?;
        workspace.snapshot("second")?;

        let run_id = workspace.prepare_restore(&first.snapshot_id)?;
        std::fs::write(
            root.path().join("wiki/entities/Raft.md"),
            PAGE.replace("Raft elects", "The user changed how Raft elects"),
        )?;

        assert!(matches!(
            workspace.activate_restore(&run_id),
            Err(StoreError::RestoreConflict { .. })
        ));
        let status = workspace.status()?;
        assert!(!status.recovery_pending);
        assert_eq!(
            std::fs::read_to_string(root.path().join("wiki/entities/Raft.md"))?,
            PAGE.replace("Raft elects", "The user changed how Raft elects")
        );
        assert!(root.path().join("wiki/entities/Vector.md").exists());
        Ok(())
    }

    #[test]
    fn open_aborts_a_restore_prepared_before_git_switch() -> Result<()> {
        let root = tempfile::tempdir()?;
        Workspace::init(root.path(), Template::Research)?;
        let workspace = Workspace::open(root.path(), Template::Research)?;
        std::fs::create_dir_all(root.path().join("wiki/entities"))?;
        std::fs::write(root.path().join("wiki/entities/Raft.md"), PAGE)?;
        let first = workspace.snapshot("first")?;
        std::fs::write(
            root.path().join("wiki/entities/Vector.md"),
            PAGE.replace("Raft", "Vector"),
        )?;
        workspace.snapshot("second")?;
        std::fs::write(root.path().join("unrelated.txt"), "keep")?;

        let run_id = workspace.prepare_restore(&first.snapshot_id)?;
        assert!(workspace.status()?.recovery_pending);
        let reopened = Workspace::open(root.path(), Template::Research)?;
        let status = reopened.status()?;

        assert!(!status.recovery_pending);
        assert!(status.dirty_paths.is_empty());
        assert!(root.path().join("wiki/entities/Vector.md").exists());
        assert_eq!(
            std::fs::read_to_string(root.path().join("unrelated.txt"))?,
            "keep"
        );
        assert!(!root.path().join(".wiki-db/restore").join(&run_id).exists());
        Ok(())
    }

    #[test]
    fn open_completes_a_restore_that_crashed_after_git_switch() -> Result<()> {
        let root = tempfile::tempdir()?;
        Workspace::init(root.path(), Template::Research)?;
        let workspace = Workspace::open(root.path(), Template::Research)?;
        std::fs::create_dir_all(root.path().join("wiki/entities"))?;
        std::fs::write(root.path().join("wiki/entities/Raft.md"), PAGE)?;
        let first = workspace.snapshot("first")?;
        std::fs::write(
            root.path().join("wiki/entities/Vector.md"),
            PAGE.replace("Raft", "Vector"),
        )?;
        workspace.snapshot("second")?;
        std::fs::write(root.path().join("unrelated.txt"), "keep")?;

        let run_id = workspace.prepare_restore(&first.snapshot_id)?;
        let selected = workspace.repository.scoped_content(&first.snapshot_id)?;
        workspace.repository.commit_scoped(
            "interrupted restore",
            &scoped_updates(&selected),
            &[("CorpusBot-Restore-Run", run_id.as_str())],
        )?;
        std::fs::write(
            root.path().join("wiki/entities/Raft.md"),
            "partially restored",
        )?;
        assert!(workspace.status()?.recovery_pending);

        let reopened = Workspace::open(root.path(), Template::Research)?;
        let status = reopened.status()?;
        assert!(!status.recovery_pending);
        assert!(status.dirty_paths.is_empty());
        assert_eq!(reopened.read_page("wiki/entities/Raft.md")?, PAGE);
        assert!(!root.path().join("wiki/entities/Vector.md").exists());
        assert_eq!(
            std::fs::read_to_string(root.path().join("unrelated.txt"))?,
            "keep"
        );
        reopened.rebuild_search_index()?;
        let hits = SearchIndex::new(root.path().join(".wiki-db/tantivy")).search("Raft", 8)?;
        assert_eq!(hits[0].path, "wiki/entities/Raft.md");
        Ok(())
    }

    #[test]
    fn source_shas_are_deduplicated() -> Result<()> {
        let root = tempfile::tempdir()?;
        Workspace::init(root.path(), Template::Generic)?;
        let workspace = Workspace::open(root.path(), Template::Generic)?;
        let sha = "a".repeat(64);
        assert!(!workspace.metadata.source_exists_by_sha(&sha)?);
        workspace.metadata.insert_source(
            "source_a",
            "version_a",
            &sha,
            "paper.md",
            128,
            &rfc3339(datetime!(2026-09-07 12:00 UTC))?,
        )?;
        assert!(workspace.metadata.source_exists_by_sha(&sha)?);
        Ok(())
    }

    #[test]
    fn open_recovers_pending_apply_without_a_run_commit() -> Result<()> {
        let root = tempfile::tempdir()?;
        Workspace::init(root.path(), Template::Research)?;
        let workspace = Workspace::open(root.path(), Template::Research)?;
        let baseline = workspace.revision_manifest()?;
        let path = "wiki/entities/Interrupted.md";
        let request = test_request("interrupted-run", path, baseline, None);

        workspace.begin_ingest(&request)?;
        std::fs::create_dir_all(root.path().join("wiki/entities"))?;
        std::fs::write(root.path().join(path), PAGE)?;
        assert!(workspace.status()?.recovery_pending);

        let reopened = Workspace::open(root.path(), Template::Research)?;
        let status = reopened.status()?;
        assert!(!status.recovery_pending);
        assert!(status.dirty_paths.is_empty());
        assert!(!root.path().join(path).exists());
        Ok(())
    }

    #[test]
    fn open_reconciles_a_commit_that_reached_git_before_sqlite() -> Result<()> {
        let root = tempfile::tempdir()?;
        Workspace::init(root.path(), Template::Research)?;
        let workspace = Workspace::open(root.path(), Template::Research)?;
        let baseline = workspace.revision_manifest()?;
        let path = "wiki/entities/Reconciled.md";
        let source = crate::SourceRow {
            source_id: "source_reconcile".to_owned(),
            source_version_id: "version_reconcile".to_owned(),
            sha256: "f".repeat(64),
            original_name: "reconcile.md".to_owned(),
            size: 128,
            imported_at: rfc3339(time::OffsetDateTime::UNIX_EPOCH)?,
        };
        let request = test_request("reconciled-run", path, baseline, Some(source.clone()));

        workspace.begin_ingest(&request)?;
        std::fs::create_dir_all(root.path().join("wiki/entities"))?;
        std::fs::write(root.path().join(path), PAGE)?;
        let repository = corpusbot_vcs::RepositoryHandle::open(root.path())?;
        let updates = repository.scoped_updates()?;
        repository.commit_scoped(
            "ingest Reconciled",
            &updates,
            &[("CorpusBot-Run", request.run_id.as_str())],
        )?;
        assert!(workspace.status()?.recovery_pending);

        let reopened = Workspace::open(root.path(), Template::Research)?;
        let status = reopened.status()?;
        assert!(!status.recovery_pending);
        assert!(status.dirty_paths.is_empty());
        let stored = reopened
            .metadata
            .source_by_sha(&source.sha256)?
            .expect("reconciled source is retained");
        assert_eq!(stored.source_id, source.source_id);
        assert!(reopened.pages()?.iter().any(|page| page.path == path));
        reopened.rebuild_search_index()?;
        let hits = SearchIndex::new(root.path().join(".wiki-db/tantivy")).search("Raft", 8)?;
        assert_eq!(hits[0].path, path);
        Ok(())
    }

    #[test]
    fn workspace_lock_blocks_concurrent_mutations() -> Result<()> {
        let root = tempfile::tempdir()?;
        Workspace::init(root.path(), Template::Generic)?;
        let _guard = WorkspaceLock::acquire(root.path(), "test-one")?;
        let second = WorkspaceLock::acquire(root.path(), "test-two");
        assert!(matches!(second, Err(StoreError::Locked { .. })));
        Ok(())
    }

    #[test]
    fn init_rejects_nonempty_roots() -> Result<()> {
        let root = tempfile::tempdir()?;
        std::fs::write(root.path().join("unrelated.txt"), "keep")?;
        assert!(matches!(
            Workspace::init(root.path(), Template::Generic),
            Err(StoreError::RootNotEmpty)
        ));
        assert!(root.path().join("unrelated.txt").exists());
        Ok(())
    }

    #[test]
    fn init_rejects_existing_repositories() -> Result<()> {
        let root = tempfile::tempdir()?;
        std::fs::create_dir_all(root.path().join(".git"))?;
        assert!(matches!(
            Workspace::init(root.path(), Template::Generic),
            Err(StoreError::Vcs(corpusbot_vcs::VcsError::RepositoryExists))
        ));
        Ok(())
    }

    fn rfc3339(value: time::OffsetDateTime) -> Result<String> {
        value
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(|error| {
                StoreError::Core(corpusbot_core::CoreError::Frontmatter(error.to_string()))
            })
    }

    fn test_request(
        run_id: &str,
        path: &str,
        baseline: RevisionManifest,
        source: Option<crate::SourceRow>,
    ) -> IngestCommitRequest {
        IngestCommitRequest {
            run_id: run_id.to_owned(),
            message: "test ingest".to_owned(),
            source,
            files: vec![(path.to_owned(), PAGE.as_bytes().to_vec())],
            pages: vec![PageFile {
                path: path.to_owned(),
                markdown: PAGE.to_owned(),
                title: "Interrupted".to_owned(),
                page_type: "entity".to_owned(),
                updated_at: "2026-09-09".to_owned(),
            }],
            touched: vec![ResourceRevision::absent(
                ResourceId::new(path).expect("valid path"),
            )],
            baseline_manifest: baseline,
        }
    }
}
