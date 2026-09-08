use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PageType {
    Entity,
    Concept,
    Source,
    Query,
    Comparison,
    Synthesis,
    Overview,
    Thesis,
    Methodology,
    Finding,
}

impl PageType {
    pub const BASE_TYPES: [Self; 7] = [
        Self::Entity,
        Self::Concept,
        Self::Source,
        Self::Query,
        Self::Comparison,
        Self::Synthesis,
        Self::Overview,
    ];

    pub const RESEARCH_TYPES: [Self; 3] = [Self::Thesis, Self::Methodology, Self::Finding];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Entity => "entity",
            Self::Concept => "concept",
            Self::Source => "source",
            Self::Query => "query",
            Self::Comparison => "comparison",
            Self::Synthesis => "synthesis",
            Self::Overview => "overview",
            Self::Thesis => "thesis",
            Self::Methodology => "methodology",
            Self::Finding => "finding",
        }
    }

    pub fn default_directory(self) -> &'static str {
        match self {
            Self::Entity => "wiki/entities",
            Self::Concept => "wiki/concepts",
            Self::Source => "wiki/sources",
            Self::Query => "wiki/queries",
            Self::Comparison => "wiki/comparisons",
            Self::Synthesis => "wiki/syntheses",
            Self::Overview => "wiki/overviews",
            Self::Thesis => "wiki/thesis",
            Self::Methodology => "wiki/methodologies",
            Self::Finding => "wiki/findings",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn research_types_are_separate_from_base_types() {
        for page_type in PageType::RESEARCH_TYPES {
            assert!(!PageType::BASE_TYPES.contains(&page_type));
        }
    }
}
