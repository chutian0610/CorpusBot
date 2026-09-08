use unicode_normalization::UnicodeNormalization;

use crate::error::{CoreError, Result};
use crate::page_type::PageType;
use crate::template::Template;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PageIdentity {
    template: Template,
    page_type: PageType,
    canonical_name: String,
}

impl PageIdentity {
    pub fn normalize(value: impl AsRef<str>) -> Result<String> {
        let value = value.as_ref();
        let normalized = value
            .nfkc()
            .flat_map(char::to_lowercase)
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let normalized = normalized.trim();
        if normalized.is_empty() {
            return Err(CoreError::PageIdentity("canonical name is empty".into()));
        }
        Ok(normalized.to_owned())
    }

    pub fn new(
        template: Template,
        page_type: PageType,
        canonical_name: impl AsRef<str>,
    ) -> Result<Self> {
        let canonical_name = Self::normalize(canonical_name)?;
        if !template.allows(page_type) {
            return Err(CoreError::PageIdentity(format!(
                "{} template does not allow {} pages",
                template.as_str(),
                page_type.as_str()
            )));
        }

        Ok(Self {
            template,
            page_type,
            canonical_name,
        })
    }

    pub fn resolve_alias(
        template: Template,
        page_type: PageType,
        alias: impl AsRef<str>,
    ) -> Result<Self> {
        Self::new(template, page_type, alias)
    }

    pub fn template(&self) -> Template {
        self.template
    }

    pub fn page_type(&self) -> PageType {
        self.page_type
    }

    pub fn canonical_name(&self) -> &str {
        &self.canonical_name
    }

    pub fn key(&self) -> String {
        format!(
            "{}/{}/{}",
            self.template.as_str(),
            self.page_type.as_str(),
            self.canonical_name
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_fullwidth_whitespace_and_case()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        assert_eq!(PageIdentity::normalize("  Ｒｕｓｔ   LANG  ")?, "rust lang");
        Ok(())
    }

    #[test]
    fn does_not_merge_equivalent_spelling_with_hyphen()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        assert_ne!(
            PageIdentity::normalize("GPT 5")?,
            PageIdentity::normalize("GPT-5")?
        );
        Ok(())
    }

    #[test]
    fn validates_template_page_type() {
        assert!(PageIdentity::new(Template::Generic, PageType::Thesis, "Test").is_err());
        assert!(PageIdentity::new(Template::Research, PageType::Thesis, "Test").is_ok());
    }
}
