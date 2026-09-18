use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use corpusbot_core::{PageIdentity, PageType, Template, WikiPath, Wikilink, extract_wikilinks};
use serde_yaml::Value;
use time::OffsetDateTime;
use walkdir::WalkDir;

use crate::error::{LintError, Result};
use crate::is_reserved_page;
use crate::report::{LintIssue, LintReport, LintSummary, Severity};

const REQUIRED_FIELDS: [&str; 7] = [
    "type", "title", "created", "updated", "tags", "related", "sources",
];

#[derive(Clone, Debug)]
struct RawPage {
    path: String,
    markdown: String,
}

#[derive(Clone, Debug, Default)]
struct ParsedPage {
    path: String,
    title: Option<String>,
    page_type: Option<PageType>,
    created: Option<String>,
    updated: Option<String>,
    aliases: Vec<String>,
    links: Vec<Wikilink>,
    body: String,
}

pub fn run_lint(
    root: impl AsRef<Path>,
    template: Template,
    revision_manifest_id: impl Into<String>,
) -> Result<LintReport> {
    let root = root.as_ref();
    let mut raw_pages = Vec::new();
    let wiki_dir = root.join("wiki");
    for entry in WalkDir::new(&wiki_dir).sort_by_file_name() {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry
            .path()
            .strip_prefix(root)
            .map_err(|error| LintError::Core(corpusbot_core::CoreError::Path(error.to_string())))?
            .to_string_lossy()
            .replace('\\', "/");
        if WikiPath::parse(&path).is_err() {
            continue;
        }
        raw_pages.push(RawPage {
            path,
            markdown: std::fs::read_to_string(entry.path())?,
        });
    }

    run_lint_pages(&raw_pages, template, revision_manifest_id)
}

fn run_lint_pages(
    raw_pages: &[RawPage],
    template: Template,
    revision_manifest_id: impl Into<String>,
) -> Result<LintReport> {
    let mut issues = Vec::new();
    let mut parsed = Vec::new();
    let mut resolution = HashMap::new();

    for page in raw_pages {
        let parsed_page = parse_page(page, template, &mut issues)?;
        resolution.insert(normalize(&parsed_page.path), parsed_page.path.clone());
        if let Some(title) = &parsed_page.title {
            resolution.insert(normalize(title), parsed_page.path.clone());
        }
        for alias in &parsed_page.aliases {
            resolution.insert(normalize(alias), parsed_page.path.clone());
        }
        parsed.push(parsed_page);
    }

    let valid_pages: Vec<&ParsedPage> = parsed
        .iter()
        .filter(|page| page.page_type.is_some() && page.title.is_some())
        .collect();
    let mut inbound = HashSet::new();
    for page in &parsed {
        for link in &page.links {
            let Some(target) = resolve_link(link, &resolution) else {
                issues.push(LintIssue {
                    code: "DEAD_LINK".to_owned(),
                    severity: Severity::Error,
                    path: page.path.clone(),
                    message: format!("{} cannot be resolved", link.render()),
                    fix_hint: "Create the page or link to an existing entity".to_owned(),
                });
                continue;
            };
            inbound.insert(target);
        }
    }

    for page in &parsed {
        if matches!(page.path.as_str(), "wiki/index.md" | "wiki/log.md") {
            continue;
        }
        if !inbound.contains(&page.path) {
            issues.push(LintIssue {
                code: "ORPHAN_PAGE".to_owned(),
                severity: Severity::Warning,
                path: page.path.clone(),
                message: "page has no inbound wikilinks".to_owned(),
                fix_hint: "Link it from an overview or related page".to_owned(),
            });
        }

        let Some(page_type) = page.page_type else {
            continue;
        };
        if !template.allows(page_type) {
            issues.push(LintIssue {
                code: "INVALID_PAGE_TYPE".to_owned(),
                severity: Severity::Error,
                path: page.path.clone(),
                message: format!(
                    "{} is not allowed by the {} template",
                    page_type.as_str(),
                    template.as_str()
                ),
                fix_hint: "Move the page or use an allowed page type".to_owned(),
            });
        }
    }

    for left_index in 0..valid_pages.len() {
        for right_index in (left_index + 1)..valid_pages.len() {
            let left = valid_pages[left_index];
            let right = valid_pages[right_index];
            if left.page_type != right.page_type {
                continue;
            }
            let left_name = normalize(left.title.as_deref().unwrap_or_default());
            let right_name = normalize(right.title.as_deref().unwrap_or_default());
            eprintln!(
                "compare={left_name:?} {right_name:?} similar={}",
                similar(&left_name, &right_name)
            );
            if left_name != right_name && similar(&left_name, &right_name) {
                issues.push(LintIssue {
                    code: "POSSIBLE_DUPLICATE".to_owned(),
                    severity: Severity::Warning,
                    path: right.path.clone(),
                    message: format!("title may duplicate {}", left.path),
                    fix_hint: "Compare pages and merge or rename intentionally".to_owned(),
                });
            }
        }
    }

    issues.extend(index_drift(&parsed));
    issues.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.code.cmp(&right.code))
            .then(left.message.cmp(&right.message))
    });

    let errors = issues
        .iter()
        .filter(|issue| issue.severity == Severity::Error)
        .count();
    let warnings = issues.len() - errors;
    Ok(LintReport {
        generated_at: OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(|error| {
                LintError::Core(corpusbot_core::CoreError::Frontmatter(error.to_string()))
            })?,
        template: template.as_str().to_owned(),
        revision_manifest_id: revision_manifest_id.into(),
        summary: LintSummary {
            pages: parsed
                .iter()
                .filter(|page| !is_reserved_page(&page.path))
                .count(),
            errors,
            warnings,
        },
        issues,
    })
}

fn parse_page(
    page: &RawPage,
    template: Template,
    issues: &mut Vec<LintIssue>,
) -> Result<ParsedPage> {
    let mut result = ParsedPage {
        path: page.path.clone(),
        ..ParsedPage::default()
    };
    if is_reserved_page(&page.path) {
        result.links = extract_wikilinks(&page.markdown);
        return Ok(result);
    }
    let raw_frontmatter = if page.markdown.starts_with("---") {
        let mut lines = page.markdown.lines();
        lines.next();
        let mut yaml = Vec::new();
        let mut closed = false;
        let mut consumed = 1usize;
        for line in lines {
            consumed += 1;
            if line.trim_end() == "---" || line.trim_end() == "..." {
                closed = true;
                break;
            }
            yaml.push(line);
        }
        if !closed {
            issues.push(missing(page.path.clone(), "frontmatter terminator"));
            result.body = page.markdown.clone();
            return Ok(result);
        }
        let body = page
            .markdown
            .lines()
            .skip(consumed)
            .collect::<Vec<_>>()
            .join("\n");
        result.body = body;
        serde_yaml::from_str::<Value>(&yaml.join("\n"))?
    } else {
        issues.push(missing(page.path.clone(), "frontmatter"));
        Value::Mapping(serde_yaml::Mapping::new())
    };

    let fields = raw_frontmatter
        .as_mapping()
        .ok_or_else(|| LintError::Yaml(serde::de::Error::custom("frontmatter is not a mapping")))?;

    for field in REQUIRED_FIELDS {
        if !fields
            .iter()
            .any(|(key, value)| key.as_str() == Some(field) && !value.is_null())
        {
            issues.push(missing(page.path.clone(), field));
        }
    }

    if let Some(value) = fields.get(Value::String("title".to_owned())).or_else(|| {
        fields
            .iter()
            .find(|(key, _)| key.as_str() == Some("title"))
            .map(|(_, value)| value)
    }) {
        let title = value.as_str().map(ToOwned::to_owned);
        if let Some(title) = title.filter(|title| !title.trim().is_empty()) {
            result.title = Some(title);
        } else {
            issues.push(missing(page.path.clone(), "title"));
        }
    }

    if let Some(value) = lookup(fields, "type") {
        match serde_yaml::from_value::<PageType>(value.clone()) {
            Ok(page_type) => result.page_type = Some(page_type),
            Err(_) => issues.push(LintIssue {
                code: "INVALID_PAGE_TYPE".to_owned(),
                severity: Severity::Error,
                path: page.path.clone(),
                message: "page type is invalid".to_owned(),
                fix_hint: "Use a page type allowed by the template".to_owned(),
            }),
        }
    }

    for field in ["created", "updated"] {
        if let Some(value) = lookup(fields, field) {
            let raw = value.as_str().unwrap_or_default();
            match corpusbot_core::IsoDate::parse(raw) {
                Ok(date) if field == "created" => result.created = Some(date.to_string()),
                Ok(date) if field == "updated" => result.updated = Some(date.to_string()),
                Ok(_) => {}
                Err(_) => issues.push(LintIssue {
                    code: "INVALID_DATE".to_owned(),
                    severity: Severity::Error,
                    path: page.path.clone(),
                    message: format!("{field} is invalid"),
                    fix_hint: "Use an ISO 8601 calendar date".to_owned(),
                }),
            }
        }
    }

    if let Some(value) = lookup(fields, "aliases")
        && let Some(values) = value.as_sequence()
    {
        for alias in values {
            if let Some(alias) = alias.as_str()
                && let Ok(normalized) = PageIdentity::normalize(alias)
            {
                result.aliases.push(normalized);
            }
        }
    }

    result.links = extract_wikilinks(&result.body);
    if template.allows(result.page_type.unwrap_or(PageType::Entity)) {
        let _ = result.page_type;
    }
    Ok(result)
}

fn lookup<'a>(fields: &'a serde_yaml::Mapping, key: &str) -> Option<&'a Value> {
    fields
        .iter()
        .find(|(field_key, _)| field_key.as_str() == Some(key))
        .map(|(_, value)| value)
}

fn missing(path: String, field: &str) -> LintIssue {
    LintIssue {
        code: "MISSING_FRONTMATTER_FIELD".to_owned(),
        severity: Severity::Error,
        path,
        message: format!("{field} is missing or empty"),
        fix_hint: "Add the required frontmatter field".to_owned(),
    }
}

fn resolve_link(link: &Wikilink, resolution: &HashMap<String, String>) -> Option<String> {
    let target = link.target();
    resolution.get(&normalize(target)).cloned().or_else(|| {
        let stem = target.strip_suffix(".md").unwrap_or(target);
        resolution.get(&normalize(stem)).cloned()
    })
}

fn normalize(value: &str) -> String {
    PageIdentity::normalize(value).unwrap_or_default()
}

fn index_drift(pages: &[ParsedPage]) -> Vec<LintIssue> {
    let mut grouped: BTreeMap<String, Vec<&ParsedPage>> = BTreeMap::new();
    for page in pages {
        let Some(page_type) = page.page_type else {
            continue;
        };
        grouped
            .entry(page_type.default_directory().to_owned())
            .or_default()
            .push(page);
    }

    let mut expected = Vec::new();
    for (_, group) in grouped {
        let mut group = group;
        group.sort_by(|left, right| {
            let left = left.title.as_deref().unwrap_or_default().to_lowercase();
            let right = right.title.as_deref().unwrap_or_default().to_lowercase();
            left.cmp(&right)
        });
        for page in group {
            expected.push(page.path.clone());
        }
    }

    let Some(index) = pages.iter().find(|page| page.path == "wiki/index.md") else {
        return vec![LintIssue {
            code: "INDEX_DRIFT".to_owned(),
            severity: Severity::Warning,
            path: "wiki/index.md".to_owned(),
            message: "generated index is missing".to_owned(),
            fix_hint: "Regenerate the index during ingest".to_owned(),
        }];
    };

    let actual = index.links.iter().map(Wikilink::target).collect::<Vec<_>>();
    if actual == expected {
        return Vec::new();
    }

    vec![LintIssue {
        code: "INDEX_DRIFT".to_owned(),
        severity: Severity::Warning,
        path: "wiki/index.md".to_owned(),
        message: "index order or page set does not match canonical order".to_owned(),
        fix_hint: "Regenerate the index during ingest".to_owned(),
    }]
}

fn similar(left: &str, right: &str) -> bool {
    if left.len() < 4 || right.len() < 4 {
        return false;
    }
    let left = left.chars().collect::<Vec<_>>();
    let right = right.chars().collect::<Vec<_>>();
    let max = left.len().max(right.len());
    let distance = levenshtein(&left, &right);
    1.0 - (distance as f64 / max as f64) >= 0.82
}

fn levenshtein(left: &[char], right: &[char]) -> usize {
    let mut previous = (0..=right.len()).collect::<Vec<_>>();
    for (left_index, left_char) in left.iter().enumerate() {
        let mut current = vec![left_index + 1];
        for (right_index, right_char) in right.iter().enumerate() {
            let substitution = previous[right_index] + usize::from(left_char != right_char);
            let insertion = previous[right_index + 1] + 1;
            let deletion = current[right_index] + 1;
            current.push(substitution.min(insertion).min(deletion));
        }
        previous = current;
    }
    previous[right.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_page() -> RawPage {
        RawPage {
            path: "wiki/sources/RaftSource.md".to_owned(),
            markdown: r#"---
type: source
title: Raft Source
created: 2026-09-08
updated: 2026-09-08
tags: [research]
related: []
sources: []
---

Raft source notes.
"#
            .to_owned(),
        }
    }

    fn entity_page(body: &str, date: &str, page_type: &str) -> RawPage {
        RawPage {
            path: "wiki/entities/Raft.md".to_owned(),
            markdown: format!(
                r#"---
type: {page_type}
title: Raft
created: 2026-09-08
updated: {date}
tags: [distributed-systems]
related: []
sources: []
---

{body}
"#
            ),
        }
    }

    fn index_page(paths: &[&str]) -> RawPage {
        let links = paths
            .iter()
            .map(|path| format!("- [[{path}|page]]"))
            .collect::<Vec<_>>()
            .join("\n");
        RawPage {
            path: "wiki/index.md".to_owned(),
            markdown: format!("# Index\n\n{links}\n"),
        }
    }

    #[test]
    fn accepts_a_consistent_workspace() -> Result<()> {
        let pages = vec![
            source_page(),
            entity_page("Raft elects a leader.", "2026-09-08", "entity"),
            index_page(&["wiki/entities/Raft.md", "wiki/sources/RaftSource.md"]),
        ];
        let report = run_lint_pages(&pages, Template::Research, "manifest")?;
        assert_eq!(report.summary.pages, 2);
        assert_eq!(report.summary.errors, 0);
        assert_eq!(report.summary.warnings, 0);
        Ok(())
    }

    #[test]
    fn detects_dead_link_orphan_and_bad_frontmatter() -> Result<()> {
        let pages = vec![
            source_page(),
            entity_page("See [[Missing Page]].", "not-a-date", "entity"),
            RawPage {
                path: "wiki/entities/Broken.md".to_owned(),
                markdown: "No frontmatter here.".to_owned(),
            },
            index_page(&["wiki/entities/Raft.md"]),
        ];
        let report = run_lint_pages(&pages, Template::Research, "manifest")?;
        let codes = report
            .issues
            .iter()
            .map(|issue| issue.code.as_str())
            .collect::<HashSet<_>>();
        assert!(codes.contains("DEAD_LINK"));
        assert!(codes.contains("ORPHAN_PAGE"));
        assert!(codes.contains("MISSING_FRONTMATTER_FIELD"));
        assert!(codes.contains("INVALID_DATE"));
        assert!(codes.contains("INDEX_DRIFT"));
        assert!(report.summary.errors > 0);
        Ok(())
    }

    #[test]
    fn reserved_pages_are_not_counted_as_content_pages() -> Result<()> {
        let report = run_lint_pages(
            &[index_page(&["wiki/index.md", "wiki/log.md"])],
            Template::Research,
            "manifest",
        )?;
        assert_eq!(report.summary.pages, 0);
        Ok(())
    }

    #[test]
    fn detects_invalid_page_type() -> Result<()> {
        let pages = vec![
            source_page(),
            entity_page("Raft elects a leader.", "2026-09-08", "widget"),
            index_page(&["wiki/entities/Raft.md"]),
        ];
        let report = run_lint_pages(&pages, Template::Research, "manifest")?;
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.code == "INVALID_PAGE_TYPE")
        );
        Ok(())
    }

    #[test]
    fn detects_similar_titles_as_possible_duplicate() -> Result<()> {
        let mut pages = vec![
            source_page(),
            entity_page("Raft elects a leader.", "2026-09-08", "entity"),
        ];
        pages[1].markdown = pages[1]
            .markdown
            .replace("title: Raft\n", "title: Raft Algorithm\n");
        pages.push(RawPage {
            path: "wiki/entities/Raft Algorithm.md".to_owned(),
            markdown: r#"---
type: entity
title: Raft Algorythm
created: 2026-09-08
updated: 2026-09-08
tags: []
related: []
sources: []
---

Similar idea.
"#
            .to_owned(),
        });
        pages.push(index_page(&[
            "wiki/entities/Raft.md",
            "wiki/entities/Raft Algorithm.md",
            "wiki/sources/RaftSource.md",
        ]));
        let report = run_lint_pages(&pages, Template::Research, "manifest")?;
        assert!(
            report
                .issues
                .iter()
                .any(|issue| issue.code == "POSSIBLE_DUPLICATE")
        );
        Ok(())
    }
}
