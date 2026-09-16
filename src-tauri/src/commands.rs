#![allow(clippy::needless_pass_by_value)]

use std::path::{Path, PathBuf};

use corpusbot_agent::{
    QueryContextPage, RigLlmClient, SettingsInput, SettingsSummary, SourceAgent, git_identity,
    provider_config,
};
use corpusbot_core::{Revision, Template, WikiDoc, Wikilink};
use corpusbot_ingest::Ingestor;
use corpusbot_search::SearchIndex;
use corpusbot_store::GitIdentity;
use corpusbot_store::Workspace;
use serde::Serialize;

use crate::error::CommandError;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
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

#[cfg(test)]
mod tests {
    use super::*;
    use corpusbot_agent::Citation;
    use corpusbot_core::Revision;
    use corpusbot_ingest::IngestResult;
    use corpusbot_lint::{LintIssue, LintReport, LintSummary, Severity};
    use corpusbot_store::{PageRow, SnapshotRow, WorkspaceStatus};
    use serde_json::{Value, json};

    #[test]
    fn page_commands_use_camel_case() -> Result<(), serde_json::Error> {
        let page = serde_json::to_value(WorkspacePage {
            path: "wiki/entities/raft.md".to_owned(),
            title: "Raft".to_owned(),
            page_type: "entity".to_owned(),
            tags: vec![],
            related: vec![],
            sources: vec![],
            markdown: "Raft".to_owned(),
        })?;
        assert_eq!(page["pageType"], "entity");

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
    fn desktop_errors_serialize_as_messages() -> Result<(), serde_json::Error> {
        let error = serde_json::to_value(CommandError("workspace is busy".to_owned()))?;
        assert_eq!(error, Value::String("workspace is busy".to_owned()));
        Ok(())
    }
}
