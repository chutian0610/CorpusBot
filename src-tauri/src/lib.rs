mod health;

use health::health_payload;

#[tauri::command]
fn health() -> serde_json::Value {
    health_payload()
}

pub fn run() {
    if let Err(error) = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![health])
        .run(tauri::generate_context!())
    {
        eprintln!("CorpusBot failed to start: {error}");
        std::process::exit(1);
    }
}
