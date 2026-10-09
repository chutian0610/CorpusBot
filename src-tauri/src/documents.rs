use std::collections::HashMap;
use std::path::PathBuf;

use corpusbot_core::WikiDoc;
use serde::Serialize;
use tauri::command;

use crate::commands::workspace;
use crate::error::CommandError;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentPage {
    pub path: String,
    pub title: String,
    pub page_type: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSummary {
    pub source_page: String,
    pub source_version_id: String,
    pub raw_path: Option<String>,
    pub original_name: Option<String>,
    pub size: Option<u64>,
    pub title: String,
    pub updated_at: String,
    pub pages: Vec<DocumentPage>,
}

#[command]
#[allow(clippy::needless_pass_by_value)]
pub fn list_documents(root: PathBuf) -> Result<Vec<DocumentSummary>, CommandError> {
    let workspace = workspace(&root)?;
    let mut documents: HashMap<String, DocumentSummary> = HashMap::new();
    let mut extracted: HashMap<String, Vec<DocumentPage>> = HashMap::new();

    for entry in walkdir::WalkDir::new(workspace.paths().wiki_dir.clone()).sort_by_file_name() {
        let entry = entry.map_err(|error| CommandError(error.to_string()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(workspace.paths().root.as_path())
            .map_err(|error| CommandError(error.to_string()))?
            .to_string_lossy()
            .replace('\\', "/");
        if matches!(relative.as_str(), "wiki/index.md" | "wiki/log.md") {
            continue;
        }
        let Ok(wiki_path) = corpusbot_core::WikiPath::parse(&relative) else {
            continue;
        };
        let Ok(markdown) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let Ok((document, _)) = WikiDoc::parse_markdown(wiki_path, &markdown) else {
            continue;
        };
        let frontmatter = document.frontmatter();
        let page_type = frontmatter.page_type().as_str().to_owned();

        if page_type == "source" {
            if let Some(source) = frontmatter.sources().first() {
                documents.insert(
                    source.source_version_id().to_owned(),
                    DocumentSummary {
                        source_page: relative,
                        source_version_id: source.source_version_id().to_owned(),
                        raw_path: None,
                        original_name: None,
                        size: None,
                        title: frontmatter.title().to_owned(),
                        updated_at: frontmatter.updated().to_string(),
                        pages: Vec::new(),
                    },
                );
            }
            continue;
        }

        for source in frontmatter.sources() {
            extracted
                .entry(source.source_version_id().to_owned())
                .or_default()
                .push(DocumentPage {
                    path: relative.clone(),
                    title: frontmatter.title().to_owned(),
                    page_type: page_type.clone(),
                    updated_at: frontmatter.updated().to_string(),
                });
        }
    }

    let mut summaries: Vec<DocumentSummary> = documents
        .into_values()
        .map(|mut summary| {
            let mut pages = extracted
                .remove(&summary.source_version_id)
                .unwrap_or_default();
            pages.sort_by(|left, right| {
                left.page_type
                    .cmp(&right.page_type)
                    .then(left.title.to_lowercase().cmp(&right.title.to_lowercase()))
            });
            summary.pages = pages;
            summary
        })
        .collect();
    summaries.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then(left.title.cmp(&right.title))
    });
    for summary in &mut summaries {
        let source = workspace
            .source_by_version_id(&summary.source_version_id)
            .map_err(|error| CommandError(error.to_string()))?;
        if let Some(source) = source {
            let raw_path = format!("raw/{}/{}", source.sha256, source.original_name);
            if workspace.paths().root.join(&raw_path).exists() {
                summary.raw_path = Some(raw_path);
                summary.original_name = Some(source.original_name);
                summary.size = Some(source.size);
            }
        }
    }
    Ok(summaries)
}
