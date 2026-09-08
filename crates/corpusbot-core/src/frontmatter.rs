use serde::Serialize;
use time::OffsetDateTime;

use crate::date::IsoDate;
use crate::error::{CoreError, Result};
use crate::identity::PageIdentity;
use crate::link::Wikilink;
use crate::page_type::PageType;
use crate::source::SourceRef;
use crate::template::Template;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Frontmatter {
    page_type: PageType,
    title: String,
    created: IsoDate,
    updated: IsoDate,
    tags: Vec<String>,
    aliases: Vec<String>,
    related: Vec<Wikilink>,
    sources: Vec<SourceRef>,
}

impl Frontmatter {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        page_type: PageType,
        title: impl Into<String>,
        created: IsoDate,
        updated: IsoDate,
        tags: Vec<String>,
        aliases: Vec<String>,
        related: Vec<Wikilink>,
        sources: Vec<SourceRef>,
    ) -> Result<Self> {
        let title = title.into();
        if title.trim().is_empty() {
            return Err(CoreError::Frontmatter("title is empty".into()));
        }
        let title = title.trim().to_owned();

        let mut clean_tags = Vec::with_capacity(tags.len());
        for tag in tags {
            let tag = tag.trim();
            if tag.is_empty() {
                return Err(CoreError::Frontmatter("tag is empty".into()));
            }
            clean_tags.push(tag.to_owned());
        }
        if clean_tags.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(CoreError::Frontmatter(
                "duplicate tags are not allowed".into(),
            ));
        }

        let mut clean_aliases = Vec::with_capacity(aliases.len());
        for alias in aliases {
            clean_aliases.push(PageIdentity::normalize(alias)?);
        }

        if updated.date() < created.date() {
            return Err(CoreError::Frontmatter(
                "updated date precedes created date".into(),
            ));
        }

        Ok(Self {
            page_type,
            title,
            created,
            updated,
            tags: clean_tags,
            aliases: clean_aliases,
            related,
            sources,
        })
    }

    pub fn imported_at_date() -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH
    }

    pub fn page_type(&self) -> PageType {
        self.page_type
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn created(&self) -> IsoDate {
        self.created
    }

    pub fn updated(&self) -> IsoDate {
        self.updated
    }

    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    pub fn aliases(&self) -> &[String] {
        &self.aliases
    }

    pub fn related(&self) -> &[Wikilink] {
        &self.related
    }

    pub fn sources(&self) -> &[SourceRef] {
        &self.sources
    }

    pub fn identity(&self, template: Template) -> Result<PageIdentity> {
        PageIdentity::new(template, self.page_type, &self.title)
    }

    pub fn resolve_identity(&self, template: Template) -> Result<PageIdentity> {
        PageIdentity::new(template, self.page_type, &self.title)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(value: &str) -> crate::Result<IsoDate> {
        IsoDate::parse(value)
    }

    #[test]
    fn requires_all_core_fields() -> crate::Result<()> {
        let frontmatter = Frontmatter::new(
            PageType::Entity,
            "Rust",
            date("2026-09-07")?,
            date("2026-09-08")?,
            vec!["language".to_owned()],
            vec![],
            vec![],
            vec![],
        )?;
        assert_eq!(
            frontmatter.identity(Template::Generic)?.key(),
            "generic/entity/rust"
        );
        assert_eq!(frontmatter.tags().len(), 1);
        assert_eq!(frontmatter.sources().len(), 0);
        Ok(())
    }

    #[test]
    fn rejects_dates_out_of_order() -> crate::Result<()> {
        assert!(
            Frontmatter::new(
                PageType::Entity,
                "Rust",
                date("2026-09-08")?,
                date("2026-09-07")?,
                vec![],
                vec![],
                vec![],
                vec![],
            )
            .is_err()
        );
        Ok(())
    }
}
