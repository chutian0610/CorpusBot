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

#[derive(Clone, Debug)]
pub struct PageFile {
    pub path: String,
    pub markdown: String,
    pub title: String,
    pub page_type: String,
    pub updated_at: String,
}

#[derive(Clone, Debug)]
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

        Ok(Self {
            paths,
            template,
            repository,
            metadata,
        })
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

    pub fn commit_ingest(&self, request: &IngestCommitRequest) -> Result<IngestCommitResult> {
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
        let lock = WorkspaceLock::acquire(&self.paths.root, "snapshot")?;
        let head_before = self.repository.head_id()?;
        if !self.repository.is_dirty()? {
            let head = head_before.ok_or_else(|| StoreError::SnapshotNotFound("HEAD".into()))?;
            let manifest_id = capture_manifest(&self.paths.root, head.as_str())?;
            drop(lock);
            return Ok(SnapshotResult {
                result: "already_clean".to_owned(),
                snapshot_id: head,
                manifest_id,
                workspace_changed_after_capture: false,
            });
        }

        let snapshot = self.repository.snapshot_scoped(message, &[])?;
        let manifest_id = capture_manifest(&self.paths.root, &snapshot.commit_id)?;
        drop(lock);
        Ok(SnapshotResult {
            result: "created".to_owned(),
            snapshot_id: snapshot.commit_id,
            manifest_id,
            workspace_changed_after_capture: false,
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
        let lock = WorkspaceLock::acquire(&self.paths.root, "restore")?;
        let selected = self.repository.scoped_content(snapshot_id)?;
        let pre_restore = self
            .repository
            .snapshot_scoped(&format!("pre-restore {snapshot_id}"), &[])?;
        let updates = selected
            .iter()
            .map(|(path, content)| ScopedUpdate::put(path.clone(), content.clone()))
            .collect::<Vec<_>>();
        self.repository
            .commit_scoped(&format!("restore {snapshot_id}"), &updates, &[])?;
        self.repository.checkout_head()?;
        drop(lock);
        self.rebuild_search_index()?;
        let _ = pre_restore;
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

    fn rebuild_search_index(&self) -> Result<()> {
        let documents = self.search_documents()?;
        SearchIndex::new(self.paths.search_index.clone()).rebuild(&documents)?;
        Ok(())
    }
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
}
