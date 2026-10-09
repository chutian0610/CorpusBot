#![allow(clippy::needless_pass_by_value)]

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use time::OffsetDateTime;

use corpusbot_agent::{
    ConnectionTestResult, LlmClient, QueryContextPage, RigLlmClient, SettingsInput,
    SettingsSummary, SourceAgent, WorkflowAuditEvent, WorkflowNode, git_identity, provider_config,
    provider_config_for_settings,
};
use corpusbot_core::{Revision, Template, WikiDoc, Wikilink, split_raw_markdown};
use corpusbot_search::SearchIndex;
use corpusbot_store::GitIdentity;
use corpusbot_store::Workspace;
use serde::Serialize;
use tauri::{AppHandle, State};

use crate::error::CommandError;
use crate::ingest_jobs::{IngestJob, IngestJobStore, list_visible_ingest_jobs, start_ingest_job};

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
    pub raw: Option<corpusbot_core::RawReference>,
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
    pub body: String,
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
    pub max_tokens: Option<u64>,
    pub finish_reason: Option<String>,
    pub truncated: bool,
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
    pub events: Vec<IngestAuditEvent>,
}

fn template_from_name(value: &str) -> Result<Template, CommandError> {
    Template::ALL
        .into_iter()
        .find(|template| template.as_str() == value)
        .ok_or_else(|| CommandError(format!("unknown template: {value}")))
}

pub(crate) fn workspace(root: &Path) -> Result<Workspace, CommandError> {
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
        raw: frontmatter.raw().cloned(),
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
        max_tokens: event.max_tokens,
        finish_reason: event.finish_reason,
        truncated: event.truncated,
        decision: event.decision,
        error_code: event.error_code,
    }
}

fn read_ingest_events(root: &Path, run_id: &str) -> Vec<IngestAuditEvent> {
    fn workflow_node_file_name(node: WorkflowNode) -> &'static str {
        match node {
            WorkflowNode::Analyze => "analyze",
            WorkflowNode::ValidateAnalysis => "validate-analysis",
            WorkflowNode::RepairAnalysis => "repair-analysis",
            WorkflowNode::RetrieveContext => "retrieve-context",
            WorkflowNode::GenerateDraft => "generate-draft",
            WorkflowNode::ValidateDraft => "validate-draft",
            WorkflowNode::RepairDraft => "repair-draft",
            WorkflowNode::SelfAudit => "self-audit",
            WorkflowNode::Commit => "commit",
        }
    }

    fn legacy_request_max_tokens(
        root: &Path,
        run_id: &str,
        event: &WorkflowAuditEvent,
    ) -> Option<u64> {
        let request_path = root.join(".wiki-db/audit").join(run_id).join(format!(
            "{}-{}-request.json",
            workflow_node_file_name(event.node),
            event.attempt
        ));
        let request =
            serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(request_path).ok()?)
                .ok()?;
        request["max_tokens"].as_u64()
    }

    fn normalize_legacy_audit_event(
        mut event: WorkflowAuditEvent,
        root: &Path,
        run_id: &str,
    ) -> WorkflowAuditEvent {
        if event.finish_reason.is_some() {
            return event;
        }

        event.max_tokens = event
            .max_tokens
            .or_else(|| legacy_request_max_tokens(root, run_id, &event));

        // Older audit events did not persist finish_reason. A schema rejection
        // at the configured output cap is the legacy truncation signature.
        let at_output_limit = event
            .max_tokens
            .zip(event.tokens_out)
            .is_some_and(|(max_tokens, tokens_out)| tokens_out >= max_tokens);
        let schema_rejected = event
            .decision
            .as_deref()
            .is_some_and(|decision| decision.contains("schema rejected"));
        if at_output_limit && schema_rejected {
            event.finish_reason = Some("length".to_owned());
            event.truncated = true;
        }

        event
    }

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
        .map(|event| audit_event(normalize_legacy_audit_event(event, root, run_id)))
        .collect()
}

fn filesystem_timestamp(path: &Path) -> String {
    let metadata = std::fs::metadata(path).ok();
    let system_time = metadata
        .as_ref()
        .and_then(|metadata| metadata.created().ok())
        .or_else(|| metadata.and_then(|metadata| metadata.modified().ok()))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    OffsetDateTime::from(system_time)
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_owned())
}

fn failed_audit_run(
    root: &Path,
    run_id: &str,
    events: &[IngestAuditEvent],
) -> Option<corpusbot_store::IngestRunRow> {
    if run_id.is_empty()
        || events.is_empty()
        || !events
            .iter()
            .any(|event| matches!(event.status.as_str(), "failed" | "attempts_exhausted"))
    {
        return None;
    }

    let directory = root.join(".wiki-db/audit").join(run_id);
    let timestamp = filesystem_timestamp(&directory);
    let manifest_id = events
        .iter()
        .find_map(|event| event.input_manifest_id.clone())
        .unwrap_or_default();

    Some(corpusbot_store::IngestRunRow {
        run_id: run_id.to_owned(),
        source_id: format!("audit_{run_id}"),
        status: "failed".to_owned(),
        baseline_snapshot_id: "unavailable".to_owned(),
        baseline_manifest_id: manifest_id,
        touched_resources: Vec::new(),
        created_at: timestamp.clone(),
        finished_at: Some(timestamp),
        original_name: None,
        source_version_id: None,
        sha256: None,
        size: None,
    })
}

fn failed_audit_ingest_runs(
    root: &Path,
    known_run_ids: &HashSet<String>,
    failed_jobs: &[IngestJob],
    limit: usize,
) -> Vec<corpusbot_store::IngestRunRow> {
    let audit_dir = root.join(".wiki-db/audit");
    let Ok(entries) = std::fs::read_dir(audit_dir) else {
        return Vec::new();
    };

    let mut runs = entries
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter_map(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .filter(|run_id| run_id.starts_with("ingest_") && !known_run_ids.contains(run_id))
        .filter_map(|run_id| {
            let events = read_ingest_events(root, &run_id);
            failed_audit_run(root, &run_id, &events).map(|mut run| {
                run.original_name = failed_jobs
                    .iter()
                    .find(|job| job.run_id.as_deref() == Some(run_id.as_str()))
                    .map(|job| job.file_name.clone());
                run.created_at = filesystem_timestamp(
                    &root
                        .join(".wiki-db/audit")
                        .join(&run_id)
                        .join("events.jsonl"),
                );
                run.finished_at = Some(run.created_at.clone());
                run
            })
        })
        .collect::<Vec<_>>();
    runs.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    runs.truncate(limit);
    runs
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
    let raw = split_raw_markdown(&markdown);

    Ok(Some(RawSource {
        source_version_id: source.source_version_id,
        path,
        original_name: source.original_name,
        size: source.size,
        body: raw.body,
    }))
}

#[tauri::command]
pub async fn start_ingest_content(
    app: AppHandle,
    state: State<'_, IngestJobStore>,
    root: PathBuf,
    file_name: String,
    markdown: String,
) -> Result<IngestJob, CommandError> {
    let provider_config = provider_config().map_err(|error| CommandError(error.to_string()))?;
    let client = RigLlmClient::new(provider_config.clone())
        .map_err(|error| CommandError(error.to_string()))?;
    start_ingest_job(
        Some(app),
        state.inner().clone(),
        root,
        file_name,
        markdown,
        provider_config.max_draft_tokens,
        client,
    )
    .await
}

#[tauri::command]
pub async fn get_ingest_job(
    state: State<'_, IngestJobStore>,
    job_id: String,
) -> Result<Option<IngestJob>, CommandError> {
    Ok(state.get(&job_id))
}

#[tauri::command]
pub fn list_ingest_jobs(state: State<'_, IngestJobStore>, root: PathBuf) -> Vec<IngestJob> {
    list_visible_ingest_jobs(state.inner(), &root)
}

#[tauri::command]
pub fn list_ingest_runs(
    state: State<'_, IngestJobStore>,
    root: PathBuf,
    limit: Option<usize>,
) -> Result<Vec<corpusbot_store::IngestRunRow>, CommandError> {
    list_ingest_runs_for_store(state.inner(), root, limit)
}

pub(crate) fn list_ingest_runs_for_store(
    store: &IngestJobStore,
    root: PathBuf,
    limit: Option<usize>,
) -> Result<Vec<corpusbot_store::IngestRunRow>, CommandError> {
    let limit = limit.unwrap_or(50);
    let workspace = workspace(&root)?;
    let mut runs = workspace
        .ingest_runs(limit)
        .map_err(|error| CommandError(error.to_string()))?;
    let known_run_ids = runs
        .iter()
        .map(|run| run.run_id.clone())
        .collect::<HashSet<_>>();
    let failed_jobs = list_visible_ingest_jobs(store, &root)
        .into_iter()
        .filter(|job| job.status == "failed")
        .collect::<Vec<_>>();
    runs.extend(failed_audit_ingest_runs(
        &root,
        &known_run_ids,
        failed_jobs.as_slice(),
        limit,
    ));
    runs.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    runs.truncate(limit);
    Ok(runs)
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
        let events = read_ingest_events(root.as_path(), &run_id);
        let Some(run) = failed_audit_run(root.as_path(), &run_id, &events) else {
            return Ok(None);
        };
        return Ok(Some(IngestRunDetail {
            events,
            run,
            source_page: None,
            source_page_markdown: None,
        }));
    };

    let source_page = run
        .source_version_id
        .as_ref()
        .map(|version| format!("wiki/sources/{version}.md"));
    let source_page_markdown = source_page
        .as_ref()
        .and_then(|path| workspace.read_page(path).ok());

    Ok(Some(IngestRunDetail {
        events: read_ingest_events(workspace.paths().root.as_path(), &run_id),
        run,
        source_page,
        source_page_markdown,
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
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        llm_client.test_connection(),
    )
    .await
    .map_err(|_| CommandError("provider request timed out after 30000ms".to_owned()))?
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
    use corpusbot_agent::AttemptStatus;
    use corpusbot_agent::Citation;
    use corpusbot_agent::WorkflowNode;
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
            raw: None,
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
            events: vec![],
        })?;
        assert_eq!(detail["runId"], "run");
        assert_eq!(detail["originalName"], "raft.md");
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
            max_draft_tokens: Some(12000),
        })?;
        assert_eq!(settings["baseUrl"], "https://example.com/v1");
        assert_eq!(settings["model"], "mvp-mock");
        assert_eq!(settings["hasApiKey"], true);
        assert_eq!(settings["gitAuthorName"], "CorpusBot");
        assert_eq!(settings["maxDraftTokens"], 12000);

        let input: SettingsInput = serde_json::from_value(json!({
            "baseUrl": "https://example.com/v1",
            "model": "mvp-mock",
            "apiKey": "secret",
            "gitAuthorName": "CorpusBot",
            "gitAuthorEmail": "corpusbot@local.invalid"
            ,"maxDraftTokens": 15000
        }))?;
        assert_eq!(input.base_url.as_deref(), Some("https://example.com/v1"));
        assert_eq!(input.model.as_deref(), Some("mvp-mock"));
        assert_eq!(input.git_author_name.as_deref(), Some("CorpusBot"));
        assert_eq!(input.max_draft_tokens, Some(15000));

        let empty: SettingsInput = serde_json::from_value(json!({}))?;
        assert_eq!(empty.base_url, None);
        assert_eq!(empty.model, None);
        assert_eq!(empty.max_draft_tokens, None);
        Ok(())
    }

    #[test]
    fn normalizes_legacy_output_limit_audit_events()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let root = tempfile::tempdir()?;
        let run_id = "legacy-run";
        let audit_dir = root.path().join(".wiki-db/audit").join(run_id);
        std::fs::create_dir_all(&audit_dir)?;

        let mut event = WorkflowAuditEvent::start(run_id, WorkflowNode::GenerateDraft, 1);
        event.status = AttemptStatus::ValidatorRejected;
        event.tokens_out = Some(6000);
        event.decision = Some("schema rejected".to_owned());
        std::fs::write(
            audit_dir.join("events.jsonl"),
            serde_json::to_string(&event)?,
        )?;
        std::fs::write(
            audit_dir.join("generate-draft-1-request.json"),
            r#"{"max_tokens":6000}"#,
        )?;

        let events = read_ingest_events(root.path(), run_id);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].max_tokens, Some(6000));
        assert_eq!(events[0].finish_reason.as_deref(), Some("length"));
        assert!(events[0].truncated);
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
