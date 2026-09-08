use serde::{Deserialize, Serialize};

use crate::date::DateTimeUtc;
use crate::error::{CoreError, Result};

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct SourceRef {
    source_version_id: String,
    title: String,
}

impl SourceRef {
    pub fn new(source_version_id: impl Into<String>, title: impl Into<String>) -> Result<Self> {
        let source_version_id = source_version_id.into();
        let title = title.into();
        if source_version_id.trim().is_empty() || title.trim().is_empty() {
            return Err(CoreError::Frontmatter(
                "source reference is incomplete".into(),
            ));
        }
        Ok(Self {
            source_version_id: source_version_id.trim().to_owned(),
            title: title.trim().to_owned(),
        })
    }

    pub fn source_version_id(&self) -> &str {
        &self.source_version_id
    }

    pub fn title(&self) -> &str {
        &self.title
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceRecord {
    source_id: String,
    source_version_id: String,
    original_name: String,
    sha256: String,
    size: u64,
    #[serde(with = "crate::date::datetime_utc")]
    imported_at: DateTimeUtc,
}

impl SourceRecord {
    pub fn new(
        source_id: impl Into<String>,
        source_version_id: impl Into<String>,
        original_name: impl Into<String>,
        sha256: impl Into<String>,
        size: u64,
        imported_at: DateTimeUtc,
    ) -> Result<Self> {
        let source_id = identifier(source_id)?;
        let source_version_id = identifier(source_version_id)?;
        let original_name = original_name.into();
        if original_name.is_empty() || original_name.contains(['/', '\\', '\0']) {
            return Err(CoreError::Frontmatter("invalid source file name".into()));
        }
        let sha256 = sha256.into();
        if !is_sha256(&sha256) {
            return Err(CoreError::Frontmatter("invalid source SHA-256".into()));
        }
        if sha256 != sha256.to_lowercase() {
            return Err(CoreError::Frontmatter("SHA-256 must be lowercase".into()));
        }

        Ok(Self {
            source_id,
            source_version_id,
            original_name,
            sha256,
            size,
            imported_at,
        })
    }

    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    pub fn source_version_id(&self) -> &str {
        &self.source_version_id
    }

    pub fn original_name(&self) -> &str {
        &self.original_name
    }

    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn imported_at(&self) -> DateTimeUtc {
        self.imported_at
    }
}

pub fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn identifier(value: impl Into<String>) -> Result<String> {
    let value = value.into();
    if value.trim().is_empty() {
        return Err(CoreError::Frontmatter("source identifier is empty".into()));
    }
    Ok(value.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::*;

    const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    #[test]
    fn builds_a_source_record() -> crate::Result<()> {
        let source = SourceRecord::new(
            "source",
            "version",
            "paper.md",
            SHA,
            128,
            datetime!(2026-09-07 12:00 UTC),
        )?;
        assert_eq!(source.source_version_id(), "version");
        assert_eq!(source.sha256(), SHA);
        Ok(())
    }

    #[test]
    fn rejects_bad_hashes() {
        assert!(
            SourceRecord::new(
                "source",
                "version",
                "paper.md",
                "bad",
                1,
                datetime!(2026-09-07 12:00 UTC)
            )
            .is_err()
        );
    }
}
