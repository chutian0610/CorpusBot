use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use git2::build::CheckoutBuilder;
use git2::{RepositoryInitOptions, Signature};

use crate::error::{Result, VcsError};

pub const MAIN_BRANCH: &str = "refs/heads/main";
const TRACKED_DIRECTORIES: [&str; 2] = ["wiki", "raw"];

pub struct ScopedUpdate {
    path: String,
    content: Option<Vec<u8>>,
}

impl ScopedUpdate {
    pub fn put(path: impl Into<String>, content: impl Into<Vec<u8>>) -> Self {
        Self {
            path: path.into(),
            content: Some(content.into()),
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn content(&self) -> Option<&[u8]> {
        self.content.as_deref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotSummary {
    pub commit_id: String,
    pub message: String,
    pub created_at: i64,
}

pub struct RepositoryHandle {
    root: PathBuf,
    repository: git2::Repository,
}

impl RepositoryHandle {
    pub fn init(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref();
        if root.join(".git").exists() {
            return Err(VcsError::RepositoryExists);
        }

        let mut options = RepositoryInitOptions::new();
        options
            .bare(false)
            .external_template(false)
            .mkdir(true)
            .mkpath(true)
            .initial_head("main");
        let repository = git2::Repository::init_opts(root, &options)?;
        set_default_identity(&repository)?;

        Ok(Self {
            root: root.to_path_buf(),
            repository,
        })
    }

    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref();
        let repository = git2::Repository::open(root)?;
        if repository.workdir().map(Path::to_path_buf).as_ref() != Some(&root.canonicalize()?) {
            return Err(VcsError::NoRepository);
        }

        Ok(Self {
            root: root.to_path_buf(),
            repository,
        })
    }

    pub fn set_identity(&self, name: &str, email: &str) -> Result<()> {
        let mut config = self.repository.config()?;
        config.set_str("user.name", name)?;
        config.set_str("user.email", email)?;
        Ok(())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn ensure_main(&self) -> Result<()> {
        if self.repository.head()?.shorthand() != Some("main") {
            return Err(VcsError::NotMain);
        }
        Ok(())
    }

    pub fn unsafe_state(&self) -> Option<String> {
        if self.repository.state() == git2::RepositoryState::Clean {
            None
        } else {
            Some(format!("{:?}", self.repository.state()))
        }
    }

    pub fn dirty_paths(&self) -> Result<Vec<String>> {
        let mut options = git2::StatusOptions::new();
        options
            .include_untracked(true)
            .recurse_untracked_dirs(true)
            .include_ignored(false);
        let statuses = self.repository.statuses(Some(&mut options))?;
        Ok(statuses
            .iter()
            .filter_map(|entry| {
                entry
                    .path()
                    .filter(|path| is_scoped(path))
                    .map(str::to_owned)
            })
            .collect())
    }

    pub fn is_dirty(&self) -> Result<bool> {
        Ok(!self.dirty_paths()?.is_empty())
    }

    pub fn head_id(&self) -> Result<Option<String>> {
        let Ok(head) = self.repository.head() else {
            return Ok(None);
        };
        let target = head
            .target()
            .ok_or_else(|| VcsError::Invalid("HEAD does not point to a commit".to_owned()))?;
        Ok(Some(target.to_string()))
    }

    pub fn snapshot_scoped(
        &self,
        message: &str,
        trailers: &[(&str, &str)],
    ) -> Result<SnapshotSummary> {
        let updates = capture_scope(&self.root)?;
        let commit_id = self.commit_tree(message, &updates, trailers)?;
        let created_at = self.commit_time(&commit_id)?;
        Ok(SnapshotSummary {
            commit_id,
            message: message.to_owned(),
            created_at,
        })
    }

    pub fn commit_scoped(
        &self,
        message: &str,
        updates: &[ScopedUpdate],
        trailers: &[(&str, &str)],
    ) -> Result<SnapshotSummary> {
        let commit_id = self.commit_tree(message, updates, trailers)?;
        let created_at = self.commit_time(&commit_id)?;
        Ok(SnapshotSummary {
            commit_id,
            message: message.to_owned(),
            created_at,
        })
    }

    pub fn checkout_head(&self) -> Result<()> {
        let mut checkout = CheckoutBuilder::new();
        checkout
            .force()
            .remove_untracked(true)
            .remove_ignored(true)
            .update_index(true);
        self.repository.checkout_head(Some(&mut checkout))?;
        Ok(())
    }

    pub fn scoped_updates(&self) -> Result<Vec<ScopedUpdate>> {
        capture_scope(&self.root)
    }

    pub fn history(&self, limit: usize) -> Result<Vec<SnapshotSummary>> {
        let mut revwalk = self.repository.revwalk()?;
        revwalk.push_head()?;
        revwalk.set_sorting(git2::Sort::TIME)?;

        let mut history = Vec::new();
        for id in revwalk.take(limit) {
            let id = id?;
            let commit = self.repository.find_commit(id)?;
            history.push(SnapshotSummary {
                commit_id: id.to_string(),
                message: commit.summary().unwrap_or_default().to_owned(),
                created_at: commit.time().seconds(),
            });
        }
        Ok(history)
    }

    pub fn scoped_content(&self, commit_id: &str) -> Result<BTreeMap<String, Vec<u8>>> {
        let oid = oid(commit_id)?;
        let commit = self.repository.find_commit(oid)?;
        let tree = commit.tree()?;
        let mut content = BTreeMap::new();
        collect_tree(&self.repository, &tree, "", &mut content)?;
        Ok(content)
    }

    fn commit_tree(
        &self,
        message: &str,
        updates: &[ScopedUpdate],
        trailers: &[(&str, &str)],
    ) -> Result<String> {
        let tree = self
            .repository
            .find_tree(build_tree(&self.repository, updates)?)?;
        let (name, email) = identity(&self.repository)?;
        let signature = Signature::now(&name, &email)?;
        let parent = match self.repository.head() {
            Ok(head) => Some(head.peel_to_commit()?),
            Err(error)
                if error.code() == git2::ErrorCode::NotFound
                    || error.code() == git2::ErrorCode::UnbornBranch =>
            {
                None
            }
            Err(error) => return Err(error.into()),
        };
        let has_parent = parent.is_some();

        let full_message = if trailers.is_empty() {
            message.to_owned()
        } else {
            let suffix = trailers
                .iter()
                .map(|(key, value)| format!("{key}: {value}"))
                .collect::<Vec<_>>()
                .join("\n");
            format!("{message}\n\n{suffix}")
        };

        let commit_id = match parent {
            Some(parent) => self.repository.commit(
                Some("HEAD"),
                &signature,
                &signature,
                &full_message,
                &tree,
                &[&parent],
            )?,
            None => {
                self.repository
                    .commit(None, &signature, &signature, &full_message, &tree, &[])?
            }
        };

        if !has_parent {
            self.repository
                .reference(MAIN_BRANCH, commit_id, true, &full_message)?;
            self.repository.set_head(MAIN_BRANCH)?;
        }

        let mut index = self.repository.index()?;
        index.read_tree(&tree)?;
        for update in updates {
            let Some(content) = update.content() else {
                continue;
            };
            let disk_path = self.root.join(update.path());
            if disk_path.is_file() && std::fs::read(disk_path)? == content {
                index.add_path(Path::new(update.path()))?;
            }
        }
        index.write()?;

        Ok(commit_id.to_string())
    }

    fn commit_time(&self, commit_id: &str) -> Result<i64> {
        Ok(self
            .repository
            .find_commit(oid(commit_id)?)?
            .time()
            .seconds())
    }
}

fn set_default_identity(repository: &git2::Repository) -> Result<()> {
    let mut config = repository.config()?;
    if config.get_string("user.name").is_err() {
        config.set_str("user.name", "CorpusBot")?;
    }
    if config.get_string("user.email").is_err() {
        config.set_str("user.email", "corpusbot@local.invalid")?;
    }
    Ok(())
}

fn identity(repository: &git2::Repository) -> Result<(String, String)> {
    let config = repository.config()?;
    let name = config
        .get_string("user.name")
        .unwrap_or_else(|_| "CorpusBot".to_owned());
    let email = config
        .get_string("user.email")
        .unwrap_or_else(|_| "corpusbot@local.invalid".to_owned());
    Ok((name, email))
}

fn capture_scope(root: &Path) -> Result<Vec<ScopedUpdate>> {
    let mut updates = Vec::new();
    for directory in TRACKED_DIRECTORIES {
        let full = root.join(directory);
        if !full.exists() {
            continue;
        }
        for entry in walkdir::WalkDir::new(full).sort_by_file_name() {
            let entry = entry.map_err(|error| VcsError::Other(error.to_string()))?;
            if !entry.file_type().is_file() {
                continue;
            }
            let relative = entry
                .path()
                .strip_prefix(root)
                .map_err(|error| VcsError::Other(error.to_string()))?
                .to_string_lossy()
                .replace('\\', "/");
            updates.push(ScopedUpdate::put(relative, std::fs::read(entry.path())?));
        }
    }

    let gitignore = root.join(".gitignore");
    if gitignore.exists() {
        updates.push(ScopedUpdate::put(".gitignore", std::fs::read(gitignore)?));
    }
    Ok(updates)
}

fn is_scoped(path: &str) -> bool {
    path == ".gitignore"
        || TRACKED_DIRECTORIES
            .iter()
            .any(|directory| path == *directory || path.starts_with(&format!("{directory}/")))
}

fn build_tree(repository: &git2::Repository, updates: &[ScopedUpdate]) -> Result<git2::Oid> {
    let mut blobs = BTreeMap::new();
    for update in updates {
        if update.path().is_empty() || !is_scoped(update.path()) || update.path().contains("..") {
            return Err(VcsError::Invalid(update.path().to_owned()));
        }
        let Some(content) = update.content() else {
            continue;
        };
        blobs.insert(update.path().to_owned(), repository.blob(content)?);
    }
    write_tree(repository, &blobs, "")
}

fn write_tree(
    repository: &git2::Repository,
    blobs: &BTreeMap<String, git2::Oid>,
    prefix: &str,
) -> Result<git2::Oid> {
    let mut builder = repository.treebuilder(None)?;
    let mut directories = BTreeMap::new();
    let prefix_pattern = if prefix.is_empty() {
        String::new()
    } else {
        format!("{prefix}/")
    };

    for (path, blob_id) in blobs {
        let relative = path
            .strip_prefix(&prefix_pattern)
            .ok_or_else(|| VcsError::Invalid(format!("path {path} is outside {prefix:?}")))?;
        if let Some((directory, _)) = relative.split_once('/') {
            directories
                .entry(directory.to_owned())
                .or_insert_with(Vec::new)
                .push((path.to_owned(), *blob_id));
        } else {
            builder.insert(relative, *blob_id, 0o100_644)?;
        }
    }

    for (directory, entries) in directories {
        let nested_prefix = if prefix.is_empty() {
            directory.clone()
        } else {
            format!("{prefix}/{directory}")
        };
        let nested = entries.into_iter().collect();
        let tree_id = write_tree(repository, &nested, &nested_prefix)?;
        builder.insert(&directory, tree_id, 0o040_000)?;
    }

    Ok(builder.write()?)
}

fn collect_tree(
    repository: &git2::Repository,
    tree: &git2::Tree,
    prefix: &str,
    output: &mut BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    for entry in tree.iter() {
        let name = entry.name().unwrap_or_default();
        let path = if prefix.is_empty() {
            name.to_owned()
        } else {
            format!("{prefix}/{name}")
        };
        match entry.kind() {
            Some(git2::ObjectType::Tree) => {
                let object = entry.to_object(repository)?;
                let nested = object
                    .as_tree()
                    .ok_or_else(|| VcsError::Invalid(format!("{path} is not a tree")))?;
                collect_tree(repository, nested, &path, output)?;
            }
            Some(git2::ObjectType::Blob) => {
                let object = entry.to_object(repository)?;
                let blob = object
                    .as_blob()
                    .ok_or_else(|| VcsError::Invalid(format!("{path} is not a blob")))?;
                output.insert(path, blob.content().to_vec());
            }
            _ => {}
        }
    }
    Ok(())
}

fn oid(value: &str) -> Result<git2::Oid> {
    git2::Oid::from_str(value).map_err(|error| VcsError::Invalid(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_workspace_scope_is_committed() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let repository = RepositoryHandle::init(temp.path())?;
        std::fs::write(temp.path().join("unrelated.txt"), b"private")?;
        std::fs::write(temp.path().join(".gitignore"), b".wiki-db/\n")?;
        std::fs::create_dir_all(temp.path().join("wiki"))?;
        std::fs::write(temp.path().join("wiki/one.md"), b"one")?;

        let summary = repository.snapshot_scoped("init", &[])?;
        assert_eq!(
            repository
                .scoped_content(&summary.commit_id)?
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            vec![".gitignore", "wiki/one.md"]
        );
        assert!(!repository.is_dirty()?);
        Ok(())
    }
}
