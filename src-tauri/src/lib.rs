mod commands;
mod error;
mod health;

use commands::{
    create_snapshot, get_settings, ingest_content, init_workspace, list_snapshots, list_wiki_pages,
    open_workspace, query, read_wiki_page, restore_snapshot, run_lint, save_settings,
    workspace_status,
};
use health::health_payload;

#[tauri::command]
fn health() -> serde_json::Value {
    health_payload()
}

pub fn run() {
    if let Err(error) = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![health])
        .invoke_handler(tauri::generate_handler![
            health,
            init_workspace,
            open_workspace,
            workspace_status,
            list_wiki_pages,
            read_wiki_page,
            ingest_content,
            query,
            run_lint,
            create_snapshot,
            list_snapshots,
            restore_snapshot,
            get_settings,
            save_settings,
        ])
        .run(tauri::generate_context!())
    {
        eprintln!("CorpusBot failed to start: {error}");
        std::process::exit(1);
    }
}
