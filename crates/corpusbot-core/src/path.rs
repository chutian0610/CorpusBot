use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

pub const RESERVED_PAGE_PATHS: [&str; 2] = ["wiki/index.md", "wiki/log.md"];

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct WikiPath(String);

impl WikiPath {
    pub fn parse(value: impl AsRef<str>) -> Result<Self> {
        let raw = value.as_ref();
        if raw.is_empty()
            || raw.starts_with('/')
            || raw.contains('\\')
            || raw.ends_with('/')
            || raw.contains("//")
        {
            return Err(CoreError::Path(raw.to_owned()));
        }

        let path = Path::new(raw);
        if path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        }) {
            return Err(CoreError::Path(raw.to_owned()));
        }

        let encoded = raw
            .split('/')
            .map(|segment| {
                if segment.is_empty()
                    || segment == "."
                    || segment == ".."
                    || segment.starts_with('.')
                {
                    Err(CoreError::Path(raw.to_owned()))
                } else {
                    Ok(())
                }
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        debug_assert_eq!(encoded.len(), raw.split('/').count());

        if path.extension() != Some("md".as_ref()) {
            return Err(CoreError::Path(raw.to_owned()));
        }

        if !raw.starts_with("wiki/") || raw == "wiki" {
            return Err(CoreError::Path(raw.to_owned()));
        }

        Ok(Self(raw.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_reserved(&self) -> bool {
        RESERVED_PAGE_PATHS.contains(&self.0.as_str())
    }

    pub fn directory(&self) -> &str {
        self.0.rsplit_once('/').map_or("wiki", |(parent, _)| parent)
    }
}

impl AsRef<str> for WikiPath {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for WikiPath {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_wiki_markdown_paths() -> std::result::Result<(), Box<dyn std::error::Error>> {
        assert_eq!(
            WikiPath::parse("wiki/entities/Rust.md")?.as_str(),
            "wiki/entities/Rust.md"
        );
        assert_eq!(WikiPath::parse("wiki/index.md")?.as_str(), "wiki/index.md");
        Ok(())
    }

    #[test]
    fn rejects_paths_outside_the_wiki_scope() {
        for value in [
            "",
            "raw/source.md",
            "/wiki/entities/rust.md",
            "wiki/../raw/source.md",
            "wiki/entities/./rust.md",
            "wiki/entities/rust.txt",
            "wiki/entities/",
            "wiki//rust.md",
            "wiki/entities/.hidden.md",
        ] {
            assert!(WikiPath::parse(value).is_err(), "{value} should be invalid");
        }
    }
}
