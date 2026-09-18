#![allow(clippy::needless_pass_by_value)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use corpusbot_agent::{
    ConnectionTestResult, LlmClient, LlmRequest, QueryContextPage, RigLlmClient, SettingsInput,
    SettingsSummary, SourceAgent, WorkflowAuditEvent, git_identity, provider_config,
    provider_config_for_settings,
};
use corpusbot_core::{Revision, Template, WikiDoc, Wikilink};
use corpusbot_ingest::Ingestor;
use corpusbot_search::SearchIndex;
use corpusbot_store::GitIdentity;
use corpusbot_store::Workspace;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::error::CommandError;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacePage {
    pub path: String,
    pub title: String,
    pub page_type: String,
    pub created_at: String,
    pub updated_at: String,
    pub tags: Vec<String>,
    pub related: Vec<String>,
    pub aliases: Vec<String>,
    pub sources: Vec<String>,
    pub source_references: Vec<SourceReferenceSummary>,
    pub markdown: String,
    pub body: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceReferenceSummary {
    pub source_version_id: String,
    pub title: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RawSource {
    pub source_version_id: String,
    pub path: String,
    pub original_name: String,
    pub size: u64,
    pub markdown: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    pub snapshot_id: String,
    pub restored: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestAuditEvent {
    pub event_id: String,
    pub run_id: String,
    pub node: String,
    pub attempt: u32,
    pub status: String,
    pub input_manifest_id: Option<String>,
    pub output_ref: Option<String>,
    pub prompt_template_id: Option<String>,
    pub prompt_hash: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub latency_ms: Option<u64>,
    pub tokens_in: Option<u64>,
    pub tokens_out: Option<u64>,
    pub decision: Option<String>,
    pub error_code: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestRunDetail {
    #[serde(flatten)]
    pub run: corpusbot_store::IngestRunRow,
    pub source_page: Option<String>,
    pub source_page_markdown: Option<String>,
    pub original_markdown: Option<String>,
    pub original_markdown_truncated: bool,
    pub events: Vec<IngestAuditEvent>,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestJob {
    pub job_id: String,
    pub file_name: String,
    pub status: String,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub result: Option<corpusbot_ingest::IngestResult>,
    pub error: Option<String>,
}

#[derive(Clone, Default)]
pub struct IngestJobStore {
    pub(crate) jobs: Arc<Mutex<HashMap<String, IngestJob>>>,
    pub(crate) ingest_lock: Arc<Mutex<()>>,
}

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

fn template_from_name(value: &str) -> Result<Template, CommandError> {
    Template::ALL
        .into_iter()
        .find(|template| template.as_str() == value)
        .ok_or_else(|| CommandError(format!("unknown template: {value}")))
}

fn workspace(root: &Path) -> Result<Workspace, CommandError> {
    let identity = git_identity().map_err(|error| CommandError(error.to_string()))?;
    let identity = identity.map(|(name, email)| GitIdentity { name, email });
    Workspace::open_with_identity(root, Template::default_template(), identity)
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
        created_at: frontmatter.created().to_string(),
        updated_at: frontmatter.updated().to_string(),
        tags: frontmatter.tags().to_vec(),
        related: frontmatter.related().iter().map(Wikilink::render).collect(),
        aliases: frontmatter.aliases().to_vec(),
        sources: frontmatter
            .sources()
            .iter()
            .map(|source| source.title().to_owned())
            .collect(),
        source_references: frontmatter
            .sources()
            .iter()
            .map(|source| SourceReferenceSummary {
                source_version_id: source.source_version_id().to_owned(),
                title: source.title().to_owned(),
            })
            .collect(),
        markdown,
        body: document.body().to_owned(),
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

fn audit_event(event: WorkflowAuditEvent) -> IngestAuditEvent {
    let node = serde_json::to_value(event.node)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_owned());
    let status = serde_json::to_value(event.status)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_owned());
    IngestAuditEvent {
        event_id: event.event_id.0,
        run_id: event.run_id,
        node,
        attempt: event.attempt,
        status,
        input_manifest_id: event.input_manifest_id,
        output_ref: event.output_ref,
        prompt_template_id: event.prompt_template_id,
        prompt_hash: event.prompt_hash,
        provider: event.provider,
        model: event.model,
        latency_ms: event.latency_ms,
        tokens_in: event.tokens_in,
        tokens_out: event.tokens_out,
        decision: event.decision,
        error_code: event.error_code,
    }
}

fn read_ingest_events(root: &Path, run_id: &str) -> Vec<IngestAuditEvent> {
    if run_id.is_empty() || run_id.contains(['/', '\\', '\0']) {
        return Vec::new();
    }
    let Ok(raw) = std::fs::read_to_string(
        root.join(".wiki-db/audit")
            .join(run_id)
            .join("events.jsonl"),
    ) else {
        return Vec::new();
    };
    raw.lines()
        .filter_map(|line| serde_json::from_str::<WorkflowAuditEvent>(line).ok())
        .map(audit_event)
        .collect()
}

#[tauri::command]
pub fn init_workspace(
    root: PathBuf,
    template: String,
) -> Result<corpusbot_store::WorkspaceSummary, CommandError> {
    let identity = git_identity().map_err(|error| CommandError(error.to_string()))?;
    let identity = identity.map(|(name, email)| GitIdentity { name, email });
    Workspace::init_with_identity(&root, template_from_name(&template)?, identity)
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
pub fn read_raw_source(
    root: PathBuf,
    source_version_id: String,
) -> Result<Option<RawSource>, CommandError> {
    let workspace = workspace(&root)?;
    let Some(source) = workspace
        .source_by_version_id(&source_version_id)
        .map_err(|error| CommandError(error.to_string()))?
    else {
        return Ok(None);
    };
    let path = format!("raw/{}/{}", source.sha256, source.original_name);
    corpusbot_core::ResourceId::new(&path).map_err(|error| CommandError(error.to_string()))?;
    let markdown = std::fs::read_to_string(workspace.paths().root.join(&path))
        .map_err(|error| CommandError(error.to_string()))?;

    Ok(Some(RawSource {
        source_version_id: source.source_version_id,
        path,
        original_name: source.original_name,
        size: source.size,
        markdown,
    }))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

pub(crate) fn update_ingest_job(
    store: &IngestJobStore,
    mut job: IngestJob,
    app: Option<&AppHandle>,
) {
    job.updated_at_ms = now_ms();
    if let Ok(mut jobs) = store.jobs.lock() {
        jobs.insert(job.job_id.clone(), job.clone());
    }
    if let Some(app) = app {
        let _ = app.emit("ingest-job-updated", &job);
    }
}

pub(crate) async fn start_ingest_job(
    app: Option<AppHandle>,
    store: IngestJobStore,
    root: PathBuf,
    file_name: String,
    markdown: String,
    llm_client: impl LlmClient + 'static,
) -> Result<IngestJob, CommandError> {
    let job_id = Uuid::new_v4().to_string();
    let now = now_ms();
    let job = IngestJob {
        job_id: job_id.clone(),
        file_name: file_name.clone(),
        status: "queued".to_owned(),
        created_at_ms: now,
        updated_at_ms: now,
        result: None,
        error: None,
    };
    update_ingest_job(&store, job.clone(), app.as_ref());

    let ingest_lock = store.ingest_lock.clone();
    let queued_job = job.clone();
    let response_job = job.clone();
    tokio::task::spawn_blocking(move || {
        update_ingest_job(
            &store,
            IngestJob {
                status: "running".to_owned(),
                ..queued_job
            },
            None,
        );

        let _guard = ingest_lock.lock();
        let outcome = (|| -> Result<corpusbot_ingest::IngestResult, String> {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| error.to_string())?;
            let workspace = workspace(&root).map_err(|error| error.to_string())?;
            runtime
                .block_on(async {
                    Ingestor::new(llm_client)
                        .ingest_content(&workspace, &file_name, &markdown)
                        .await
                })
                .map_err(|error| error.to_string())
        })();

        let finished = match outcome {
            Ok(result) => IngestJob {
                status: "succeeded".to_owned(),
                result: Some(result),
                error: None,
                ..job.clone()
            },
            Err(error) => IngestJob {
                status: "failed".to_owned(),
                error: Some(error),
                result: None,
                ..job.clone()
            },
        };
        update_ingest_job(&store, finished, app.as_ref());
    });

    Ok(response_job)
}

#[tauri::command]
pub async fn start_ingest_content(
    app: AppHandle,
    state: State<'_, IngestJobStore>,
    root: PathBuf,
    file_name: String,
    markdown: String,
) -> Result<IngestJob, CommandError> {
    let client =
        RigLlmClient::new(provider_config().map_err(|error| CommandError(error.to_string()))?)?;
    start_ingest_job(
        Some(app),
        state.inner().clone(),
        root,
        file_name,
        markdown,
        client,
    )
    .await
}

#[tauri::command]
pub async fn get_ingest_job(
    state: State<'_, IngestJobStore>,
    job_id: String,
) -> Result<Option<IngestJob>, CommandError> {
    Ok(state
        .jobs
        .lock()
        .ok()
        .and_then(|jobs| jobs.get(&job_id).cloned()))
}

#[tauri::command]
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

#[tauri::command]
pub fn list_ingest_runs(
    root: PathBuf,
    limit: Option<usize>,
) -> Result<Vec<corpusbot_store::IngestRunRow>, CommandError> {
    workspace(&root)?
        .ingest_runs(limit.unwrap_or(50))
        .map_err(|error| CommandError(error.to_string()))
}

#[tauri::command]
pub fn read_ingest_run(
    root: PathBuf,
    run_id: String,
) -> Result<Option<IngestRunDetail>, CommandError> {
    let workspace = workspace(&root)?;
    let Some(run) = workspace
        .ingest_run(&run_id)
        .map_err(|error| CommandError(error.to_string()))?
    else {
        return Ok(None);
    };

    let source_page = run
        .source_version_id
        .as_ref()
        .map(|version| format!("wiki/sources/{version}.md"));
    let source_page_markdown = source_page
        .as_ref()
        .and_then(|path| workspace.read_page(path).ok());
    let original_markdown = run
        .sha256
        .as_ref()
        .zip(run.original_name.as_ref())
        .and_then(|(sha256, original_name)| {
            let raw_path = format!("raw/{sha256}/{original_name}");
            corpusbot_core::ResourceId::new(&raw_path).ok()?;
            std::fs::read_to_string(workspace.paths().root.join(raw_path)).ok()
        });
    let original_markdown_truncated = original_markdown
        .as_ref()
        .is_some_and(|markdown| markdown.chars().count() > 48_000);
    let original_markdown =
        original_markdown.map(|markdown| markdown.chars().take(48_000).collect::<String>());

    Ok(Some(IngestRunDetail {
        events: read_ingest_events(workspace.paths().root.as_path(), &run_id),
        run,
        source_page,
        source_page_markdown,
        original_markdown,
        original_markdown_truncated,
    }))
}

#[tauri::command]
pub async fn query(
    root: PathBuf,
    question: String,
    limit: Option<usize>,
) -> Result<corpusbot_agent::QueryAnswer, CommandError> {
    let client =
        RigLlmClient::new(provider_config().map_err(|error| CommandError(error.to_string()))?)?;
    query_with_client(root, question, limit, client).await
}

pub(crate) async fn query_with_client<LlmClientT>(
    root: PathBuf,
    question: String,
    limit: Option<usize>,
    llm_client: LlmClientT,
) -> Result<corpusbot_agent::QueryAnswer, CommandError>
where
    LlmClientT: LlmClient + 'static,
{
    let workspace = workspace(&root)?;
    let (manifest_id, context) = query_context(&workspace, &question, limit.unwrap_or(8))?;
    let mut answer = SourceAgent::new(llm_client)
        .answer_question_audited(
            root.as_path(),
            &corpusbot_agent::query_run_id(),
            &manifest_id,
            &question,
            &context,
        )
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

#[tauri::command]
pub async fn test_llm_connection(
    settings: SettingsInput,
) -> Result<ConnectionTestResult, CommandError> {
    let client = RigLlmClient::new(
        provider_config_for_settings(&settings).map_err(|error| CommandError(error.to_string()))?,
    )?;
    test_connection_with_client(client).await
}

pub(crate) async fn test_connection_with_client<LlmClientT>(
    llm_client: LlmClientT,
) -> Result<ConnectionTestResult, CommandError>
where
    LlmClientT: LlmClient + 'static,
{
    let started_at = std::time::Instant::now();
    let request = LlmRequest {
        operation: "connection-test".to_owned(),
        system: String::new(),
        prompt: "Reply with OK.".to_owned(),
        prompt_template_id: "connection-test-v1".to_owned(),
        temperature: None,
        max_tokens: Some(1),
    };
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        llm_client.complete(request),
    )
    .await
    .map_err(|_| CommandError("provider request timed out after 15000ms".to_owned()))?
    .map_err(|error| CommandError(error.to_string()))?;
    Ok(ConnectionTestResult {
        provider: response.provider,
        model: response.model,
        latency_ms: u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX),
        response_id: response.response_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use corpusbot_agent::Citation;
    use corpusbot_core::Revision;
    use corpusbot_ingest::IngestResult;
    use corpusbot_lint::{LintIssue, LintReport, LintSummary, Severity};
    use corpusbot_store::{IngestRunRow, PageRow, SnapshotRow, TouchedResource, WorkspaceStatus};
    use serde_json::{Value, json};

    #[test]
    fn page_commands_use_camel_case() -> Result<(), serde_json::Error> {
        let page = serde_json::to_value(WorkspacePage {
            path: "wiki/entities/raft.md".to_owned(),
            title: "Raft".to_owned(),
            page_type: "entity".to_owned(),
            created_at: "2026-09-17".to_owned(),
            updated_at: "2026-09-17".to_owned(),
            tags: vec![],
            related: vec![],
            aliases: vec![],
            sources: vec![],
            source_references: vec![],
            markdown: "Raft".to_owned(),
            body: "Raft".to_owned(),
        })?;
        assert_eq!(page["pageType"], "entity");
        assert_eq!(page["createdAt"], "2026-09-17");
        assert_eq!(page["sourceReferences"], json!([]));

        let restore = serde_json::to_value(RestoreResult {
            snapshot_id: "snapshot".to_owned(),
            restored: true,
        })?;
        assert_eq!(restore["snapshotId"], "snapshot");
        Ok(())
    }

    #[test]
    fn workspace_commands_use_camel_case() -> Result<(), serde_json::Error> {
        let summary = serde_json::to_value(corpusbot_store::WorkspaceSummary {
            root: "/tmp/workspace".to_owned(),
            template: "research".to_owned(),
            head_snapshot_id: Some("head".to_owned()),
        })?;
        assert_eq!(summary["headSnapshotId"], "head");

        let status = serde_json::to_value(WorkspaceStatus {
            root: "/tmp/workspace".to_owned(),
            template: "research".to_owned(),
            head_snapshot_id: Some("head".to_owned()),
            dirty_paths: vec!["wiki/index.md".to_owned()],
            unsafe_state: None,
            recovery_pending: false,
            page_count: 1,
        })?;
        assert_eq!(status["dirtyPaths"], json!(["wiki/index.md"]));
        assert_eq!(status["recoveryPending"], false);
        assert_eq!(status["pageCount"], 1);
        Ok(())
    }

    #[test]
    fn page_and_snapshot_rows_use_camel_case() -> Result<(), serde_json::Error> {
        let page_row = serde_json::to_value(PageRow {
            path: "wiki/entities/raft.md".to_owned(),
            title: "Raft".to_owned(),
            page_type: "entity".to_owned(),
            sha256: "sha".to_owned(),
            updated_at: "2026-09-15".to_owned(),
        })?;
        assert_eq!(page_row["pageType"], "entity");
        assert_eq!(page_row["updatedAt"], "2026-09-15");

        let snapshot_row = serde_json::to_value(SnapshotRow {
            snapshot_id: "snapshot".to_owned(),
            message: "baseline".to_owned(),
            created_at: 0,
        })?;
        assert_eq!(snapshot_row["snapshotId"], "snapshot");
        assert_eq!(snapshot_row["createdAt"], 0);
        Ok(())
    }

    #[test]
    fn ingest_result_uses_camel_case() -> Result<(), serde_json::Error> {
        let ingest = serde_json::to_value(IngestResult::Committed {
            run_id: "run".to_owned(),
            source_id: "source".to_owned(),
            source_version_id: "version".to_owned(),
            source_page: "wiki/sources/source.md".to_owned(),
            created_paths: vec![],
            updated_paths: vec![],
            snapshot_id: "snapshot".to_owned(),
            manifest_id: "manifest".to_owned(),
        })?;
        assert_eq!(ingest["status"], "committed");
        assert_eq!(ingest["runId"], "run");
        assert_eq!(ingest["sourceVersionId"], "version");
        assert_eq!(ingest["manifestId"], "manifest");
        Ok(())
    }

    #[test]
    fn ingest_run_detail_uses_camel_case() -> Result<(), serde_json::Error> {
        let detail = serde_json::to_value(IngestRunDetail {
            run: IngestRunRow {
                run_id: "run".to_owned(),
                source_id: "source".to_owned(),
                status: "committed".to_owned(),
                baseline_snapshot_id: "snapshot".to_owned(),
                baseline_manifest_id: "manifest".to_owned(),
                touched_resources: vec![TouchedResource {
                    path: "wiki/entities/raft.md".to_owned(),
                    revision_kind: "absent".to_owned(),
                    sha256: None,
                }],
                created_at: "2026-09-17T00:00:00Z".to_owned(),
                finished_at: None,
                original_name: Some("raft.md".to_owned()),
                source_version_id: Some("version".to_owned()),
                sha256: Some("sha".to_owned()),
                size: Some(128),
            },
            source_page: Some("wiki/sources/version.md".to_owned()),
            source_page_markdown: Some("# Source".to_owned()),
            original_markdown: Some("# Raft".to_owned()),
            original_markdown_truncated: false,
            events: vec![],
        })?;
        assert_eq!(detail["runId"], "run");
        assert_eq!(detail["originalName"], "raft.md");
        assert_eq!(detail["originalMarkdownTruncated"], false);
        assert_eq!(detail["touchedResources"][0]["revisionKind"], "absent");
        Ok(())
    }

    #[test]
    fn query_answer_uses_camel_case() -> Result<(), serde_json::Error> {
        let query = serde_json::to_value(corpusbot_agent::QueryAnswer {
            answer: "Raft".to_owned(),
            citations: vec![Citation {
                number: 1,
                path: "wiki/entities/raft.md".to_owned(),
                title: "Raft".to_owned(),
                quote: "Raft".to_owned(),
                resource_revision: Revision::from_content(b"Raft"),
            }],
            revision_manifest_id: "manifest".to_owned(),
            warnings: vec![],
            insufficient_evidence: false,
        })?;
        assert_eq!(query["revisionManifestId"], "manifest");
        assert_eq!(query["insufficientEvidence"], false);
        assert_eq!(query["citations"][0]["resourceRevision"]["kind"], "content");
        Ok(())
    }

    #[test]
    fn lint_report_uses_camel_case() -> Result<(), serde_json::Error> {
        let lint = serde_json::to_value(LintReport {
            generated_at: "2026-09-15T00:00:00Z".to_owned(),
            template: "research".to_owned(),
            revision_manifest_id: "manifest".to_owned(),
            summary: LintSummary {
                pages: 1,
                errors: 0,
                warnings: 0,
            },
            issues: vec![LintIssue {
                code: "ORPHAN_PAGE".to_owned(),
                severity: Severity::Warning,
                path: "wiki/entities/raft.md".to_owned(),
                message: "orphan".to_owned(),
                fix_hint: "link it".to_owned(),
            }],
        })?;
        assert_eq!(lint["generatedAt"], "2026-09-15T00:00:00Z");
        assert_eq!(lint["revisionManifestId"], "manifest");
        assert_eq!(lint["issues"][0]["fixHint"], "link it");
        Ok(())
    }

    #[test]
    fn settings_contract_uses_camel_case() -> Result<(), serde_json::Error> {
        let settings = serde_json::to_value(SettingsSummary {
            base_url: Some("https://example.com/v1".to_owned()),
            model: Some("mvp-mock".to_owned()),
            has_api_key: true,
            git_author_name: Some("CorpusBot".to_owned()),
            git_author_email: Some("corpusbot@local.invalid".to_owned()),
        })?;
        assert_eq!(settings["baseUrl"], "https://example.com/v1");
        assert_eq!(settings["model"], "mvp-mock");
        assert_eq!(settings["hasApiKey"], true);
        assert_eq!(settings["gitAuthorName"], "CorpusBot");

        let input: SettingsInput = serde_json::from_value(json!({
            "baseUrl": "https://example.com/v1",
            "model": "mvp-mock",
            "apiKey": "secret",
            "gitAuthorName": "CorpusBot",
            "gitAuthorEmail": "corpusbot@local.invalid"
        }))?;
        assert_eq!(input.base_url.as_deref(), Some("https://example.com/v1"));
        assert_eq!(input.model.as_deref(), Some("mvp-mock"));
        assert_eq!(input.git_author_name.as_deref(), Some("CorpusBot"));

        let empty: SettingsInput = serde_json::from_value(json!({}))?;
        assert_eq!(empty.base_url, None);
        assert_eq!(empty.model, None);
        Ok(())
    }

    #[test]
    fn connection_test_result_uses_camel_case() -> Result<(), serde_json::Error> {
        let result = serde_json::to_value(ConnectionTestResult {
            provider: "openai-compatible".to_owned(),
            model: "test-model".to_owned(),
            latency_ms: 42,
            response_id: Some("response".to_owned()),
        })?;
        assert_eq!(result["model"], "test-model");
        assert_eq!(result["latencyMs"], 42);
        assert_eq!(result["responseId"], "response");
        Ok(())
    }

    #[test]
    fn desktop_errors_serialize_as_messages() -> Result<(), serde_json::Error> {
        let error = serde_json::to_value(CommandError("workspace is busy".to_owned()))?;
        assert_eq!(error, Value::String("workspace is busy".to_owned()));
        Ok(())
    }
}
