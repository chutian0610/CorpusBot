use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Template {
    Generic,
    Research,
}

impl Template {
    pub const ALL: [Self; 2] = [Self::Generic, Self::Research];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Generic => "generic",
            Self::Research => "research",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_mvp_templates_are_enabled() {
        assert_eq!(Template::ALL.map(Template::as_str), ["generic", "research"]);
    }
}
