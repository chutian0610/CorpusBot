mod commands;
mod error;
mod health;
pub mod local_server;

use commands::{
    IngestJobStore, create_snapshot, get_ingest_job, get_settings, init_workspace, list_documents,
    list_ingest_runs, list_snapshots, list_wiki_pages, open_workspace, query, read_ingest_run,
    read_raw_source, read_wiki_page, restore_snapshot, run_lint, save_settings,
    start_ingest_content, test_llm_connection, workspace_status,
};
use health::health_payload;

#[tauri::command]
fn health() -> serde_json::Value {
    health_payload()
}

pub fn run() {
    if let Err(error) = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(IngestJobStore::default())
        .invoke_handler(tauri::generate_handler![health])
        .invoke_handler(tauri::generate_handler![
            health,
            init_workspace,
            open_workspace,
            workspace_status,
            list_wiki_pages,
            read_wiki_page,
            read_raw_source,
            start_ingest_content,
            get_ingest_job,
            list_ingest_runs,
            read_ingest_run,
            list_documents,
            query,
            run_lint,
            create_snapshot,
            list_snapshots,
            restore_snapshot,
            get_settings,
            save_settings,
            test_llm_connection,
        ])
        .run(tauri::generate_context!())
    {
        eprintln!("CorpusBot failed to start: {error}");
        std::process::exit(1);
    }
}
