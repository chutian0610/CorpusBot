/// A raw markdown file split into its frontmatter and body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RawMarkdown {
    pub body: String,
    pub had_frontmatter: bool,
}

fn is_delimiter(line: &str, delimiter: &str) -> bool {
    line.trim_end() == delimiter
}

/// Splits raw markdown without applying workspace page schema. Raw imports are
/// allowed to omit frontmatter, use unrelated fields, or contain invalid YAML.
pub fn split_raw_markdown(markdown: &str) -> RawMarkdown {
    let mut lines = markdown.lines();
    if !lines.next().is_some_and(|line| is_delimiter(line, "---")) {
        return RawMarkdown {
            body: markdown.to_owned(),
            had_frontmatter: false,
        };
    }

    let mut body_start = None;
    for (offset, line) in markdown.lines().enumerate().skip(1) {
        if is_delimiter(line, "---") || is_delimiter(line, "...") {
            body_start = Some(offset + 1);
            break;
        }
    }

    let Some(body_start) = body_start else {
        return RawMarkdown {
            body: markdown.to_owned(),
            had_frontmatter: false,
        };
    };

    let body = markdown
        .lines()
        .skip(body_start)
        .collect::<Vec<_>>()
        .join("\n")
        .trim_start_matches('\n')
        .to_owned();

    RawMarkdown {
        body,
        had_frontmatter: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_generic_frontmatter_without_applying_page_schema() {
        let raw = split_raw_markdown("---\ntitle: TCP\ncustom:\n  - 1\n---\n\n# TCP");

        assert_eq!(raw.body, "# TCP");
        assert!(raw.had_frontmatter);
    }

    #[test]
    fn treats_markdown_without_frontmatter_as_body() {
        let markdown = "# Notes";
        let raw = split_raw_markdown(markdown);

        assert_eq!(raw.body, markdown);
        assert!(!raw.had_frontmatter);
    }

    #[test]
    fn reports_invalid_yaml_and_still_returns_the_body() {
        let raw = split_raw_markdown("---\ntitle: \"unclosed\n---\n\n# Notes");

        assert_eq!(raw.body, "# Notes");
        assert!(raw.had_frontmatter);
    }

    #[test]
    fn reports_unclosed_frontmatter() {
        let raw = split_raw_markdown("---\ntitle: TCP\n\n# TCP");

        assert_eq!(raw.body, "---\ntitle: TCP\n\n# TCP");
        assert!(!raw.had_frontmatter);
    }
}
