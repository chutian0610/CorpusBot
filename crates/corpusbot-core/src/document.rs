use serde::Serialize;

use crate::error::{CoreError, Result};
use crate::frontmatter::Frontmatter;
use crate::link::{Wikilink, extract_wikilinks};
use crate::path::WikiPath;
use crate::template::Template;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WikiDoc {
    path: WikiPath,
    frontmatter: Frontmatter,
    body: String,
}

impl WikiDoc {
    pub fn new(path: WikiPath, frontmatter: Frontmatter, body: impl Into<String>) -> Result<Self> {
        let doc = Self {
            path,
            frontmatter,
            body: body.into(),
        };
        doc.validate()?;
        Ok(doc)
    }

    pub fn parse_markdown(path: WikiPath, markdown: &str) -> Result<(Self, Vec<Wikilink>)> {
        let (frontmatter, body) = split_frontmatter(markdown)?;
        let links = extract_wikilinks(&body);
        Ok((Self::new(path, frontmatter, body)?, links))
    }

    pub fn validate(&self) -> Result<()> {
        if self.path.is_reserved() {
            return Err(CoreError::Template(format!(
                "{} is a generated workspace file, not a Wiki Page",
                self.path
            )));
        }
        if self.frontmatter.title().trim().is_empty() {
            return Err(CoreError::Template("page title is empty".into()));
        }
        Ok(())
    }

    pub fn validate_for(&self, template: Template) -> Result<crate::identity::PageIdentity> {
        self.validate()?;
        template
            .definition()
            .validate_doc(&self.path, &self.frontmatter)
    }

    pub fn path(&self) -> &WikiPath {
        &self.path
    }

    pub fn frontmatter(&self) -> &Frontmatter {
        &self.frontmatter
    }

    pub fn body(&self) -> &str {
        &self.body
    }
}

#[derive(serde::Deserialize)]
struct RawFrontmatter {
    #[serde(rename = "type")]
    page_type: crate::page_type::PageType,
    title: String,
    created: crate::date::IsoDate,
    updated: crate::date::IsoDate,
    tags: Vec<String>,
    #[serde(default)]
    aliases: Vec<String>,
    related: Vec<crate::link::Wikilink>,
    sources: Vec<crate::source::SourceRef>,
}

fn split_frontmatter(markdown: &str) -> Result<(Frontmatter, String)> {
    let mut lines = markdown.lines();
    if !lines.next().is_some_and(|line| line.trim_end() == "---") {
        return Err(CoreError::Markdown(
            "frontmatter must start with ---".into(),
        ));
    }

    let mut yaml_lines = Vec::new();
    let mut body_start = 0;
    let mut closed = false;
    for (offset, line) in markdown.lines().enumerate() {
        if offset == 0 {
            continue;
        }
        if line.trim_end() == "---" || line.trim_end() == "..." {
            closed = true;
            body_start = offset + 1;
            break;
        }
        yaml_lines.push(line);
    }

    if !closed {
        return Err(CoreError::Markdown("frontmatter is not closed".into()));
    }

    let raw = serde_yaml::from_str::<RawFrontmatter>(&yaml_lines.join("\n"))?;
    let frontmatter = Frontmatter::new(
        raw.page_type,
        raw.title,
        raw.created,
        raw.updated,
        raw.tags,
        raw.aliases,
        raw.related,
        raw.sources,
    )?;
    let body = markdown
        .lines()
        .skip(body_start)
        .collect::<Vec<_>>()
        .join("\n");
    let body = body.trim_start_matches('\n').to_owned();
    Ok((frontmatter, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MARKDOWN: &str = r#"---
type: entity
title: Raft
created: 2026-09-07
updated: 2026-09-07
tags: [distributed-systems]
related: []
sources: []
---

Raft relates to [[Leader Election]].
"#;

    #[test]
    fn parses_markdown_and_wikilinks() -> crate::Result<()> {
        let path = WikiPath::parse("wiki/entities/Raft.md")?;
        let (doc, links) = WikiDoc::parse_markdown(path, MARKDOWN)?;
        assert_eq!(doc.frontmatter().title(), "Raft");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target(), "Leader Election");
        assert!(doc.body().starts_with("Raft relates to"));
        Ok(())
    }

    #[test]
    fn rejects_unclosed_frontmatter() -> std::result::Result<(), Box<dyn std::error::Error>> {
        assert!(
            WikiDoc::parse_markdown(
                WikiPath::parse("wiki/entities/Raft.md")?,
                "---\ntype: entity",
            )
            .is_err()
        );
        Ok(())
    }
}
