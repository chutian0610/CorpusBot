use std::path::PathBuf;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use corpusbot_agent::{LlmClient, LlmRequest, LlmResponse};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::commands::{
    IngestJobStore, query_with_client, start_ingest_job, test_connection_with_client,
};

const E2E_ANALYSIS: &str = r#"{
  "title": "E2E import",
  "summary": "This page provides E2E evidence.",
  "entities": [
    {"name": "E2E import", "aliases": ["E2E evidence"], "summary": "This page provides E2E evidence."}
  ],
  "concepts": [
    {"name": "E2E workflow", "definition": "The deterministic E2E ingest workflow."}
  ]
}"#;

const E2E_DRAFTS: &str = r#"{
  "source_summary": "This page provides E2E evidence.",
  "entities": [
    {"name": "E2E import", "aliases": ["E2E evidence"], "summary": "This page provides E2E evidence."}
  ],
  "concepts": [
    {"name": "E2E workflow", "definition": "The deterministic E2E ingest workflow."}
  ]
}"#;

// The answer revision is derived from the prompt because it must exactly match
// the search-index page revision used for citation validation.
#[derive(Clone, Default)]
struct E2eFakeLlmClient;

#[async_trait::async_trait]
impl LlmClient for E2eFakeLlmClient {
    async fn complete(&self, request: LlmRequest) -> corpusbot_agent::Result<LlmResponse> {
        let text = match request.operation.as_str() {
            "analyze_source" => E2E_ANALYSIS.to_owned(),
            "generate_drafts" => E2E_DRAFTS.to_owned(),
            "answer_query" => {
                let revision = request
                    .prompt
                    .split("revision: ")
                    .nth(1)
                    .and_then(|values| values.lines().next())
                    .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
                    .unwrap_or(serde_json::Value::Null);
                format!(
                    r#"{{"answer":"The imported page answers: [1].","citations":[{{"path":"wiki/entities/e2e-import.md","quote":"This page provides E2E evidence","revision":{revision}}}]}}"#
                )
            }
            "connection-test" => r#"{"ok":true}"#.to_owned(),
            unsupported => {
                return Err(corpusbot_agent::AgentError::Schema(format!(
                    "unsupported E2E operation: {unsupported}"
                )));
            }
        };
        Ok(LlmResponse {
            text,
            provider: "e2e-fake".to_owned(),
            model: "fake-model".to_owned(),
            prompt_tokens: 24,
            completion_tokens: 32,
            response_id: Some(request.prompt_hash()),
        })
    }
}

#[derive(Clone)]
pub struct LocalServerState {
    ingest_jobs: IngestJobStore,
    e2e_fake_llm: bool,
    e2e_settings: std::sync::Arc<tokio::sync::Mutex<Option<corpusbot_agent::SettingsSummary>>>,
}

#[derive(Debug, Deserialize)]
struct InvokeRequest {
    command: String,
    #[serde(default)]
    args: Value,
}

struct ApiError(String);

impl From<corpusbot_agent::AgentError> for ApiError {
    fn from(error: corpusbot_agent::AgentError) -> Self {
        Self(error.to_string())
    }
}

impl From<crate::error::CommandError> for ApiError {
    fn from(error: crate::error::CommandError) -> Self {
        Self(error.0)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": self.0 })),
        )
            .into_response()
    }
}

fn argument<T: serde::de::DeserializeOwned>(args: &Value, name: &str) -> Result<T, ApiError> {
    serde_json::from_value(args.get(name).cloned().unwrap_or(Value::Null))
        .map_err(|error| ApiError(format!("invalid argument {name}: {error}")))
}

fn json_response<T: serde::Serialize>(value: T) -> Result<Json<Value>, ApiError> {
    serde_json::to_value(value)
        .map(Json)
        .map_err(|error| ApiError(error.to_string()))
}

#[allow(clippy::too_many_lines)]
async fn invoke(
    State(state): State<LocalServerState>,
    Json(request): Json<InvokeRequest>,
) -> Result<Json<Value>, ApiError> {
    let args = request.args;
    let command = request.command;
    match command.as_str() {
        "init_workspace" => {
            let root: PathBuf = argument(&args, "root")?;
            let template: String = argument(&args, "template")?;
            json_response(crate::commands::init_workspace(root, template)?)
        }
        "open_workspace" => {
            let root: PathBuf = argument(&args, "root")?;
            json_response(crate::commands::open_workspace(root)?)
        }
        "workspace_status" => {
            let root: PathBuf = argument(&args, "root")?;
            json_response(crate::commands::workspace_status(root)?)
        }
        "list_wiki_pages" => {
            let root: PathBuf = argument(&args, "root")?;
            json_response(crate::commands::list_wiki_pages(root)?)
        }
        "read_wiki_page" => {
            let root: PathBuf = argument(&args, "root")?;
            let path: String = argument(&args, "path")?;
            json_response(crate::commands::read_wiki_page(root, path)?)
        }
        "start_ingest_content" => {
            let root: PathBuf = argument(&args, "root")?;
            let file_name: String = argument(&args, "fileName")?;
            let markdown: String = argument(&args, "markdown")?;
            let result = if state.e2e_fake_llm {
                let llm_client = E2eFakeLlmClient;
                start_ingest_job(
                    None,
                    state.ingest_jobs.clone(),
                    root,
                    file_name,
                    markdown,
                    llm_client,
                )
                .await?
            } else {
                let llm_client = corpusbot_agent::RigLlmClient::new(
                    corpusbot_agent::provider_config()
                        .map_err(|error| ApiError(error.to_string()))?,
                )?;
                start_ingest_job(
                    None,
                    state.ingest_jobs.clone(),
                    root,
                    file_name,
                    markdown,
                    llm_client,
                )
                .await?
            };
            json_response(result)
        }
        "get_ingest_job" => {
            let job_id: String = argument(&args, "jobId")?;
            let job = state
                .ingest_jobs
                .jobs
                .lock()
                .ok()
                .and_then(|jobs| jobs.get(&job_id).cloned());
            json_response(job)
        }
        "list_documents" => {
            let root: PathBuf = argument(&args, "root")?;
            json_response(crate::commands::list_documents(root)?)
        }
        "list_ingest_runs" => {
            let root: PathBuf = argument(&args, "root")?;
            let limit: Option<usize> = argument(&args, "limit")?;
            json_response(crate::commands::list_ingest_runs(root, limit)?)
        }
        "read_ingest_run" => {
            let root: PathBuf = argument(&args, "root")?;
            let run_id: String = argument(&args, "runId")?;
            json_response(crate::commands::read_ingest_run(root, run_id)?)
        }
        "read_raw_source" => {
            let root: PathBuf = argument(&args, "root")?;
            let source_version_id: String = argument(&args, "sourceVersionId")?;
            json_response(crate::commands::read_raw_source(root, source_version_id)?)
        }
        "query" => {
            let root: PathBuf = argument(&args, "root")?;
            let question: String = argument(&args, "question")?;
            let limit: Option<usize> = argument(&args, "limit")?;
            if state.e2e_fake_llm {
                let llm_client = E2eFakeLlmClient;
                json_response(query_with_client(root, question, limit, llm_client).await?)
            } else {
                let llm_client = corpusbot_agent::RigLlmClient::new(
                    corpusbot_agent::provider_config()
                        .map_err(|error| ApiError(error.to_string()))?,
                )?;
                json_response(query_with_client(root, question, limit, llm_client).await?)
            }
        }
        "run_lint" => {
            let root: PathBuf = argument(&args, "root")?;
            json_response(crate::commands::run_lint(root)?)
        }
        "create_snapshot" => {
            let root: PathBuf = argument(&args, "root")?;
            let message: String = argument(&args, "message")?;
            json_response(crate::commands::create_snapshot(root, message)?)
        }
        "list_snapshots" => {
            let root: PathBuf = argument(&args, "root")?;
            let limit: Option<usize> = argument(&args, "limit")?;
            json_response(crate::commands::list_snapshots(root, limit)?)
        }
        "restore_snapshot" => {
            let root: PathBuf = argument(&args, "root")?;
            let snapshot_id: String = argument(&args, "snapshotId")?;
            let confirmed: bool = argument(&args, "confirmed")?;
            json_response(crate::commands::restore_snapshot(
                root,
                snapshot_id,
                confirmed,
            )?)
        }
        "get_settings" => {
            if state.e2e_fake_llm {
                let settings = state.e2e_settings.lock().await.clone().unwrap_or(
                    corpusbot_agent::SettingsSummary {
                        base_url: None,
                        model: None,
                        has_api_key: false,
                        git_author_name: Some("CorpusBot E2E".to_owned()),
                        git_author_email: Some("e2e@corpusbot.invalid".to_owned()),
                    },
                );
                json_response(settings)
            } else {
                json_response(crate::commands::get_settings()?)
            }
        }
        "save_settings" => {
            let settings: corpusbot_agent::SettingsInput = argument(&args, "settings")?;
            if state.e2e_fake_llm {
                let summary = corpusbot_agent::SettingsSummary {
                    base_url: settings.base_url.clone(),
                    model: settings.model.clone(),
                    has_api_key: settings
                        .api_key
                        .as_deref()
                        .is_some_and(|key| !key.trim().is_empty()),
                    git_author_name: settings.git_author_name.clone(),
                    git_author_email: settings.git_author_email.clone(),
                };
                *state.e2e_settings.lock().await = Some(summary.clone());
                json_response(summary)
            } else {
                json_response(crate::commands::save_settings(settings)?)
            }
        }
        "test_llm_connection" => {
            let settings: corpusbot_agent::SettingsInput = argument(&args, "settings")?;
            if state.e2e_fake_llm {
                if settings
                    .api_key
                    .as_deref()
                    .unwrap_or_default()
                    .trim()
                    .is_empty()
                {
                    return Err(ApiError(
                        "provider configuration is incomplete: API key is empty".to_owned(),
                    ));
                }
                let llm_client = E2eFakeLlmClient;
                json_response(test_connection_with_client(llm_client).await?)
            } else {
                let llm_client = corpusbot_agent::RigLlmClient::new(
                    corpusbot_agent::provider_config_for_settings(&settings)
                        .map_err(|error| ApiError(error.to_string()))?,
                )?;
                json_response(test_connection_with_client(llm_client).await?)
            }
        }
        unsupported => Err(ApiError(format!(
            "unsupported local command: {unsupported}"
        ))),
    }
}

async fn health() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "corpusbot-local-server",
    }))
}

/// Start the local HTTP backend.
///
/// # Errors
///
/// Returns an I/O error if the listener cannot bind or HTTP serving fails.
pub async fn serve(bind: std::net::SocketAddr) -> Result<(), std::io::Error> {
    let e2e_fake_llm = std::env::var("CORPUSBOT_E2E_LLM")
        .is_ok_and(|value| matches!(value.as_str(), "1" | "true"));
    let state = LocalServerState {
        ingest_jobs: IngestJobStore::default(),
        e2e_fake_llm,
        e2e_settings: std::sync::Arc::new(tokio::sync::Mutex::new(None)),
    };
    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/invoke", post(invoke))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(bind).await?;
    println!("CorpusBot local backend listening on http://{bind}");
    if e2e_fake_llm {
        println!("E2E fake LLM provider enabled");
    }
    axum::serve(listener, app).await
}

#[allow(dead_code)]
fn _assert_send<F: std::future::Future + Send>(_future: F) {}
