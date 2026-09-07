use serde_json::json;

pub fn health_payload() -> serde_json::Value {
    json!({
        "status": "ok",
        "version": corpusbot_core::library_version(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_reports_health() {
        let payload = health_payload();
        assert_eq!(payload["status"], "ok");
        assert_eq!(payload["version"], "0.1.0");
    }
}
