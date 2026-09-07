use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct Wikilink {
    target: String,
    alias: Option<String>,
}

impl Wikilink {
    pub fn parse(value: impl AsRef<str>) -> Result<Self> {
        let raw = value.as_ref();
        if raw.is_empty() || raw.contains("]]") || raw.contains("[[") {
            return Err(CoreError::Wikilink(raw.to_owned()));
        }

        let (target, alias) = raw
            .split_once('|')
            .map_or((raw, None), |(target, alias)| (target, Some(alias)));
        let target = target.trim();
        let alias = alias.map(str::trim);

        if target.is_empty()
            || target.contains(['\n', '\r', '\0'])
            || alias.is_some_and(str::is_empty)
        {
            return Err(CoreError::Wikilink(raw.to_owned()));
        }

        Ok(Self {
            target: target.to_owned(),
            alias: alias.map(ToOwned::to_owned),
        })
    }

    pub fn new(target: impl Into<String>) -> Result<Self> {
        let target = target.into();
        Self::parse(target)
    }

    pub fn with_alias(target: impl Into<String>, alias: impl Into<String>) -> Result<Self> {
        Self::parse(format!("{}|{}", target.into(), alias.into()))
    }

    pub fn target(&self) -> &str {
        &self.target
    }

    pub fn alias(&self) -> Option<&str> {
        self.alias.as_deref()
    }

    pub fn render(&self) -> String {
        self.alias.as_deref().map_or_else(
            || format!("[[{}]]", self.target),
            |alias| format!("[[{}|{}]]", self.target, alias),
        )
    }
}

pub fn extract_wikilinks(markdown: &str) -> Vec<Wikilink> {
    let bytes = markdown.as_bytes();
    let mut links = Vec::new();
    let mut index = 0;

    while let Some(start) = markdown[index..].find("[[") {
        let start = index + start + 2;
        let Some(length) = bytes[start..].windows(2).position(|pair| pair == b"]]") else {
            break;
        };
        let end = start + length;
        if let Ok(link) = Wikilink::parse(&markdown[start..end]) {
            links.push(link);
        }
        index = end + 2;
    }

    links
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_target_and_alias() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let link = Wikilink::parse("Rust|The Rust Language")?;
        assert_eq!(link.target(), "Rust");
        assert_eq!(link.alias(), Some("The Rust Language"));
        assert_eq!(link.render(), "[[Rust|The Rust Language]]");
        Ok(())
    }

    #[test]
    fn rejects_empty_or_escaped_links() {
        for value in ["", "|alias", "target|", "[[nested]]", "target]]"] {
            assert!(Wikilink::parse(value).is_err(), "{value} should be invalid");
        }
    }

    #[test]
    fn extracts_only_complete_links() {
        let links = extract_wikilinks("See [[Raft]] and [[Leader Election|Paxos alternative]].");
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target(), "Raft");
        assert_eq!(links[1].target(), "Leader Election");
        assert!(extract_wikilinks("broken [[link").is_empty());
    }
}
