use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use corpusbot_agent::LlmClient;
use corpusbot_ingest::Ingestor;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use crate::error::CommandError;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestJob {
    pub job_id: String,
    pub root: String,
    pub file_name: String,
    pub status: String,
    pub stage: Option<String>,
    pub run_id: Option<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub result: Option<corpusbot_ingest::IngestResult>,
    pub error: Option<String>,
}

#[derive(Clone, Default)]
pub struct IngestJobStore {
    jobs: Arc<Mutex<HashMap<String, IngestJob>>>,
    ingest_lock: Arc<Mutex<()>>,
}

impl IngestJobStore {
    pub(crate) fn get(&self, job_id: &str) -> Option<IngestJob> {
        self.jobs
            .lock()
            .ok()
            .and_then(|jobs| jobs.get(job_id).cloned())
    }

    fn update(&self, mut job: IngestJob, app: Option<&AppHandle>) {
        job.updated_at_ms = now_ms();
        if let Ok(mut jobs) = self.jobs.lock() {
            jobs.insert(job.job_id.clone(), job.clone());
        }
        if let Some(app) = app {
            let _ = app.emit("ingest-job-updated", &job);
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

#[allow(clippy::too_many_lines)]
pub(crate) async fn start_ingest_job(
    app: Option<AppHandle>,
    store: IngestJobStore,
    root: PathBuf,
    file_name: String,
    markdown: String,
    max_draft_tokens: u64,
    llm_client: impl LlmClient + 'static,
) -> Result<IngestJob, CommandError> {
    let job_id = Uuid::new_v4().to_string();
    let now = now_ms();
    let job = IngestJob {
        job_id: job_id.clone(),
        root: root.to_string_lossy().to_string(),
        file_name: file_name.clone(),
        status: "queued".to_owned(),
        stage: None,
        run_id: None,
        created_at_ms: now,
        updated_at_ms: now,
        result: None,
        error: None,
    };
    store.update(job.clone(), app.as_ref());
    tracing::info!(
        job_id = %job.job_id,
        root = %root.display(),
        file_name = %file_name,
        "ingest job queued"
    );

    let ingest_lock = store.ingest_lock.clone();
    let progress_store = store.clone();
    let progress_job_id = job_id.clone();
    let progress: corpusbot_ingest::IngestProgressCallback = Arc::new(move |update| {
        if let Ok(mut jobs) = progress_store.jobs.lock()
            && let Some(job) = jobs.get_mut(&progress_job_id)
        {
            job.stage = Some(update.stage.as_str().to_owned());
            job.run_id = Some(update.run_id.clone());
            job.updated_at_ms = now_ms();
            tracing::debug!(
                job_id = %progress_job_id,
                stage = update.stage.as_str(),
                run_id = %update.run_id,
                "ingest progress"
            );
        }
    });
    let queued_job = job.clone();
    let response_job = job.clone();
    tokio::task::spawn_blocking(move || {
        store.update(
            IngestJob {
                status: "running".to_owned(),
                stage: Some("prepare".to_owned()),
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
            let workspace = crate::commands::workspace(&root).map_err(|error| error.to_string())?;
            runtime
                .block_on(async {
                    Ingestor::new(llm_client)
                        .with_max_draft_tokens(max_draft_tokens)
                        .with_progress(progress.clone())
                        .ingest_content(&workspace, &file_name, &markdown)
                        .await
                })
                .map_err(|error| error.to_string())
        })();

        let (stage, run_id) = store
            .jobs
            .lock()
            .ok()
            .and_then(|jobs| {
                jobs.get(&job_id)
                    .map(|job| (job.stage.clone(), job.run_id.clone()))
            })
            .unwrap_or_else(|| (job.stage.clone(), job.run_id.clone()));
        let finished = match outcome {
            Ok(result) => IngestJob {
                status: "succeeded".to_owned(),
                result: Some(result),
                error: None,
                stage,
                run_id,
                ..job.clone()
            },
            Err(error) => IngestJob {
                status: "failed".to_owned(),
                error: Some(error),
                result: None,
                stage,
                run_id,
                ..job.clone()
            },
        };
        store.update(finished, app.as_ref());
        let final_status = store
            .jobs
            .lock()
            .ok()
            .and_then(|jobs| {
                jobs.get(&job_id)
                    .map(|job| (job.status.clone(), job.error.clone()))
            })
            .unwrap_or_default();
        if final_status.0 == "failed" {
            tracing::error!(
                job_id = %job_id,
                error = final_status.1.as_deref().unwrap_or_default(),
                "ingest job failed"
            );
        } else {
            tracing::info!(
                job_id = %job_id,
                status = %final_status.0,
                "ingest job finished"
            );
        }
    });

    Ok(response_job)
}

pub(crate) fn list_visible_ingest_jobs(
    store: &IngestJobStore,
    root: &std::path::Path,
) -> Vec<IngestJob> {
    let root = root.to_string_lossy();
    let mut jobs = store
        .jobs
        .lock()
        .map(|jobs| {
            jobs.values()
                .filter(|job| {
                    job.root == root
                        && ["queued", "running", "failed"].contains(&job.status.as_str())
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    jobs.sort_by_key(|job| job.created_at_ms);
    jobs
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn lists_only_active_jobs_for_selected_workspace() {
        let store = IngestJobStore::default();
        for (job_id, root, status) in [
            ("running", "/tmp/one", "running"),
            ("succeeded", "/tmp/one", "succeeded"),
            ("other-root", "/tmp/two", "running"),
        ] {
            let job = IngestJob {
                job_id: job_id.to_owned(),
                root: root.to_owned(),
                file_name: format!("{job_id}.md"),
                status: status.to_owned(),
                stage: None,
                run_id: None,
                created_at_ms: 1,
                updated_at_ms: 1,
                result: None,
                error: None,
            };
            store.update(job, None);
            assert!(store.get(job_id).is_some());
        }

        let jobs = list_visible_ingest_jobs(&store, Path::new("/tmp/one"));
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].job_id, "running");
    }
}
