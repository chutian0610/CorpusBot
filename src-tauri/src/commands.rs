#![allow(clippy::needless_pass_by_value)]

use std::path::{Path, PathBuf};

use corpusbot_agent::{
    QueryContextPage, RigLlmClient, SettingsInput, SettingsSummary, SourceAgent, provider_config,
};
use corpusbot_core::{Revision, Template, WikiDoc, Wikilink};
use corpusbot_ingest::Ingestor;
use corpusbot_search::SearchIndex;
use corpusbot_store::Workspace;
use serde::Serialize;

use crate::error::CommandError;

#[derive(Debug, Serialize)]
pub struct WorkspacePage {
    pub path: String,
    pub title: String,
    pub page_type: String,
    pub tags: Vec<String>,
    pub related: Vec<String>,
    pub sources: Vec<String>,
    pub markdown: String,
}

#[derive(Debug, Serialize)]
pub struct RestoreResult {
    pub snapshot_id: String,
    pub restored: bool,
}

fn template_from_name(value: &str) -> Result<Template, CommandError> {
    Template::ALL
        .into_iter()
        .find(|template| template.as_str() == value)
        .ok_or_else(|| CommandError(format!("unknown template: {value}")))
}

fn workspace(root: &Path) -> Result<Workspace, CommandError> {
    Workspace::open(root, Template::default_template())
        .map_err(|error| CommandError(error.to_string()))
}

fn page_from_workspace(workspace: &Workspace, path: &str) -> Result<WorkspacePage, CommandError> {
    let wiki_path =
        corpusbot_core::WikiPath::parse(path).map_err(|error| CommandError(error.to_string()))?;
    let markdown = workspace
        .read_page(path)
        .map_err(|error| CommandError(error.to_string()))?;
    let (document, _) = WikiDoc::parse_markdown(wiki_path, &markdown)
        .map_err(|error| CommandError(error.to_string()))?;
    let frontmatter = document.frontmatter();

    Ok(WorkspacePage {
        path: path.to_owned(),
        title: frontmatter.title().to_owned(),
        page_type: frontmatter.page_type().as_str().to_owned(),
        tags: frontmatter.tags().to_vec(),
        related: frontmatter.related().iter().map(Wikilink::render).collect(),
        sources: frontmatter
            .sources()
            .iter()
            .map(|source| source.title().to_owned())
            .collect(),
        markdown,
    })
}

fn query_context(
    workspace: &Workspace,
    question: &str,
    limit: usize,
) -> Result<(String, Vec<QueryContextPage>), CommandError> {
    if workspace
        .status()
        .map_err(|error| CommandError(error.to_string()))?
        .recovery_pending
    {
        return Err(CommandError("workspace has pending recovery".to_owned()));
    }
    let manifest = workspace
        .revision_manifest()
        .map_err(|error| CommandError(error.to_string()))?;
    let manifest_id = manifest.manifest_id().to_owned();
    let index = SearchIndex::new(workspace.paths().search_index.clone());
    let hits = index
        .search(question, limit)
        .map_err(|error| CommandError(error.to_string()))?;

    let mut remaining = 24_000usize;
    let mut context = Vec::new();
    for hit in hits {
        let resource = corpusbot_core::ResourceId::new(&hit.path)
            .map_err(|error| CommandError(error.to_string()))?;
        let revision = manifest.expected(&resource);
        if matches!(revision, Revision::Absent) {
            continue;
        }
        let Ok(markdown) = workspace.read_page(&hit.path) else {
            continue;
        };
        let used = remaining.min(markdown.chars().count());
        context.push(QueryContextPage {
            path: hit.path,
            title: hit.title,
            page_type: hit.page_type,
            revision,
            revision_manifest_id: manifest_id.clone(),
            markdown: markdown.chars().take(used).collect(),
        });
        remaining -= used;
        if remaining == 0 {
            break;
        }
    }
    Ok((manifest_id, context))
}

#[tauri::command]
pub fn init_workspace(
    root: PathBuf,
    template: String,
) -> Result<corpusbot_store::WorkspaceSummary, CommandError> {
    Workspace::init(&root, template_from_name(&template)?)
        .map_err(|error| CommandError(error.to_string()))
}

#[tauri::command]
pub fn open_workspace(root: PathBuf) -> Result<corpusbot_store::WorkspaceSummary, CommandError> {
    workspace(&root)?
        .summary()
        .map_err(|error| CommandError(error.to_string()))
}

#[tauri::command]
pub fn workspace_status(root: PathBuf) -> Result<corpusbot_store::WorkspaceStatus, CommandError> {
    workspace(&root)?
        .status()
        .map_err(|error| CommandError(error.to_string()))
}

#[tauri::command]
pub fn list_wiki_pages(root: PathBuf) -> Result<Vec<corpusbot_store::PageRow>, CommandError> {
    let workspace = workspace(&root)?;
    workspace
        .refresh_pages()
        .map_err(|error| CommandError(error.to_string()))?;
    workspace
        .pages()
        .map_err(|error| CommandError(error.to_string()))
}

#[tauri::command]
pub fn read_wiki_page(root: PathBuf, path: String) -> Result<WorkspacePage, CommandError> {
    let workspace = workspace(&root)?;
    page_from_workspace(&workspace, &path)
}

#[tauri::command]
pub fn ingest_content(
    root: PathBuf,
    file_name: String,
    markdown: String,
) -> Result<corpusbot_ingest::IngestResult, CommandError> {
    let workspace = workspace(&root)?;
    let client = RigLlmClient::new(
        corpusbot_agent::provider_config().map_err(|error| CommandError(error.to_string()))?,
    )?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        Ingestor::new(client)
            .ingest_content(&workspace, &file_name, &markdown)
            .await
            .map_err(|error| CommandError(error.to_string()))
    })
}

#[tauri::command]
pub async fn query(
    root: PathBuf,
    question: String,
    limit: Option<usize>,
) -> Result<corpusbot_agent::QueryAnswer, CommandError> {
    let workspace = workspace(&root)?;
    let (manifest_id, context) = query_context(&workspace, &question, limit.unwrap_or(8))?;
    let client =
        RigLlmClient::new(provider_config().map_err(|error| CommandError(error.to_string()))?)?;
    let mut answer = SourceAgent::new(client)
        .answer_question(&question, &context)
        .await
        .map_err(|error| CommandError(error.to_string()))?;
    answer.revision_manifest_id = manifest_id;
    Ok(answer)
}

#[tauri::command]
pub fn run_lint(root: PathBuf) -> Result<corpusbot_lint::LintReport, CommandError> {
    let workspace = workspace(&root)?;
    if workspace
        .status()
        .map_err(|error| CommandError(error.to_string()))?
        .recovery_pending
    {
        return Err(CommandError("workspace has pending recovery".to_owned()));
    }
    let manifest = workspace
        .revision_manifest()
        .map_err(|error| CommandError(error.to_string()))?;
    corpusbot_lint::engine::run_lint(&root, workspace.template(), manifest.manifest_id())
        .map_err(|error| CommandError(error.to_string()))
}

#[tauri::command]
pub fn create_snapshot(
    root: PathBuf,
    message: String,
) -> Result<corpusbot_store::SnapshotResult, CommandError> {
    workspace(&root)?
        .snapshot(&message)
        .map_err(|error| CommandError(error.to_string()))
}

#[tauri::command]
pub fn list_snapshots(
    root: PathBuf,
    limit: Option<usize>,
) -> Result<Vec<corpusbot_store::SnapshotRow>, CommandError> {
    workspace(&root)?
        .history(limit.unwrap_or(50))
        .map_err(|error| CommandError(error.to_string()))
}

#[tauri::command]
pub fn restore_snapshot(
    root: PathBuf,
    snapshot_id: String,
    confirmed: bool,
) -> Result<RestoreResult, CommandError> {
    if !confirmed {
        return Err(CommandError("restore requires confirmation".to_owned()));
    }
    workspace(&root)?
        .restore(&snapshot_id)
        .map_err(|error| CommandError(error.to_string()))?;
    Ok(RestoreResult {
        snapshot_id,
        restored: true,
    })
}

#[tauri::command]
pub fn get_settings() -> Result<SettingsSummary, CommandError> {
    corpusbot_agent::load_settings().map_err(|error| CommandError(error.to_string()))
}

#[tauri::command]
pub fn save_settings(settings: SettingsInput) -> Result<SettingsSummary, CommandError> {
    corpusbot_agent::save_settings(settings).map_err(|error| CommandError(error.to_string()))
}
