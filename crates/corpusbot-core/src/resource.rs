use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::error::{CoreError, Result};
use crate::path::WikiPath;

pub type ManifestId = String;
pub type SnapshotId = String;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ResourceId(String);

impl ResourceId {
    pub fn new(path: impl Into<String>) -> Result<Self> {
        let path = path.into();
        if path.is_empty()
            || path.starts_with('/')
            || path.ends_with('/')
            || path.contains('\\')
            || path.contains("//")
            || path
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err(CoreError::Revision("invalid resource path".into()));
        }
        Ok(Self(path))
    }

    pub fn page(path: WikiPath) -> Self {
        Self(path.as_str().to_owned())
    }

    pub fn index() -> Self {
        Self("wiki/index.md".to_owned())
    }

    pub fn log() -> Self {
        Self("wiki/log.md".to_owned())
    }

    pub fn metadata(component: &str) -> Result<Self> {
        Self::new(format!(".wiki-db/{component}"))
    }

    pub fn search_index() -> Self {
        Self(".wiki-db/tantivy/generation".to_owned())
    }

    pub fn path(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for ResourceId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Revision {
    Absent,
    Content { sha256: String },
    Generation { id: String, sequence: u64 },
}

impl Revision {
    pub fn absent() -> Self {
        Self::Absent
    }

    pub fn from_content(content: &[u8]) -> Self {
        let digest = Sha256::digest(content);
        Self::Content {
            sha256: format!("{digest:x}"),
        }
    }

    pub fn generation(id: impl Into<String>, sequence: u64) -> Self {
        Self::Generation {
            id: id.into(),
            sequence,
        }
    }

    pub fn key(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "invalid".to_owned())
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ResourceRevision {
    resource: ResourceId,
    revision: Revision,
}

impl ResourceRevision {
    pub fn new(resource: ResourceId, revision: Revision) -> Self {
        Self { resource, revision }
    }

    pub fn absent(resource: ResourceId) -> Self {
        Self {
            resource,
            revision: Revision::absent(),
        }
    }

    pub fn content(resource: ResourceId, content: &[u8]) -> Self {
        Self {
            resource,
            revision: Revision::from_content(content),
        }
    }

    pub fn resource(&self) -> &ResourceId {
        &self.resource
    }

    pub fn revision(&self) -> &Revision {
        &self.revision
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RevisionManifest {
    manifest_id: String,
    head_snapshot_id: String,
    resources: Vec<ResourceRevision>,
}

impl RevisionManifest {
    pub fn capture<I>(resources: I, head_snapshot_id: impl Into<String>) -> Result<Self>
    where
        I: IntoIterator<Item = ResourceRevision>,
    {
        let mut resources: Vec<ResourceRevision> = resources.into_iter().collect();
        resources.sort_by(|left, right| {
            left.resource()
                .path()
                .as_bytes()
                .cmp(right.resource().path().as_bytes())
        });
        if resources
            .windows(2)
            .any(|pair| pair[0].resource() == pair[1].resource())
        {
            return Err(CoreError::Manifest("duplicate resource revisions".into()));
        }

        Ok(Self {
            manifest_id: manifest_id(&resources)?,
            head_snapshot_id: head_snapshot_id.into(),
            resources,
        })
    }

    pub fn manifest_id(&self) -> &str {
        &self.manifest_id
    }

    pub fn head_snapshot_id(&self) -> &str {
        &self.head_snapshot_id
    }

    pub fn resources(&self) -> &[ResourceRevision] {
        &self.resources
    }

    pub fn expected(&self, resource: &ResourceId) -> Revision {
        self.resources
            .iter()
            .find(|entry| entry.resource() == resource)
            .map_or(Revision::Absent, |entry| entry.revision().clone())
    }

    pub fn verify_unchanged<I>(&self, current: I) -> Result<()>
    where
        I: IntoIterator<Item = ResourceRevision>,
    {
        let current = Self::capture(current, &self.head_snapshot_id)?;
        for expected in &self.resources {
            let found = current.expected(expected.resource());
            if found != expected.revision {
                return Err(CoreError::RevisionConflict {
                    resource: expected.resource().path().to_owned(),
                    expected: expected.revision.key(),
                    current: found.key(),
                });
            }
        }
        Ok(())
    }
}

fn manifest_id(resources: &[ResourceRevision]) -> Result<String> {
    let canonical = resources
        .iter()
        .map(|entry| {
            json!({
                "resource_path": entry.resource().path(),
                "resource_revision": entry.revision(),
            })
        })
        .collect::<Vec<_>>();
    let serialized =
        serde_json::to_vec(&canonical).map_err(|error| CoreError::Manifest(error.to_string()))?;
    let digest = Sha256::digest(serialized);
    Ok(format!("{digest:x}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> crate::Result<Vec<ResourceRevision>> {
        Ok(vec![
            ResourceRevision::content(
                ResourceId::page(WikiPath::parse("wiki/entities/Rust.md")?),
                b"rust",
            ),
            ResourceRevision::content(ResourceId::index(), b"index"),
            ResourceRevision::content(ResourceId::log(), b"log"),
        ])
    }

    #[test]
    fn manifest_ids_are_order_independent() -> crate::Result<()> {
        let mut resources = sample()?;
        resources.reverse();
        let first = RevisionManifest::capture(sample()?, "head")?;
        let second = RevisionManifest::capture(resources, "head")?;
        assert_eq!(first.manifest_id(), second.manifest_id());
        assert_eq!(first.resources(), second.resources());
        Ok(())
    }

    #[test]
    fn verifies_resource_revisions() -> crate::Result<()> {
        let baseline = RevisionManifest::capture(sample()?, "head")?;
        assert!(baseline.verify_unchanged(sample()?).is_ok());

        let mut current = sample()?;
        current[0] = ResourceRevision::content(current[0].resource().clone(), b"changed");
        assert!(baseline.verify_unchanged(current).is_err());
        Ok(())
    }

    #[test]
    fn absent_expectation_detects_a_new_resource() -> crate::Result<()> {
        let path = ResourceId::page(WikiPath::parse("wiki/entities/Vector.md")?);
        let baseline = RevisionManifest::capture([ResourceRevision::absent(path.clone())], "head")?;
        let current = [ResourceRevision::content(path, b"new")];
        assert!(baseline.verify_unchanged(current).is_err());
        Ok(())
    }
}
