use crate::error::{AgentError, Result};
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use time::OffsetDateTime;

pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
pub const DEFAULT_MODEL: &str = "gpt-4o-mini";
pub const DAEMON_DB_FILE_NAME: &str = "daemon.db";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SettingsRecord {
    base_url: Option<String>,
    model: Option<String>,
    api_key: Option<String>,
    git_author_name: Option<String>,
    git_author_email: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSummary {
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub has_api_key: bool,
    pub git_author_name: Option<String>,
    pub git_author_email: Option<String>,
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsInput {
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub api_key: Option<String>,
    pub git_author_name: Option<String>,
    pub git_author_email: Option<String>,
}

pub fn load_settings() -> Result<SettingsSummary> {
    let database = daemon_db_path()?;
    let record = load_settings_at(&database)?;
    Ok(settings_summary(&record))
}

pub fn git_identity() -> Result<Option<(String, String)>> {
    let settings = load_settings()?;
    if settings.git_author_name.is_none() && settings.git_author_email.is_none() {
        return Ok(None);
    }

    Ok(Some((
        settings
            .git_author_name
            .unwrap_or_else(|| "CorpusBot".to_owned()),
        settings
            .git_author_email
            .unwrap_or_else(|| "corpusbot@local.invalid".to_owned()),
    )))
}

pub fn save_settings(input: SettingsInput) -> Result<SettingsSummary> {
    let database = daemon_db_path()?;
    save_settings_at(&database, input)
}

pub(crate) fn load_settings_at(database_path: &std::path::Path) -> Result<SettingsRecord> {
    let connection = open_daemon_database(database_path)?;
    let record = read_settings_record(&connection)?;
    Ok(record.unwrap_or_default())
}

pub(crate) fn save_settings_at(
    database_path: &std::path::Path,
    input: SettingsInput,
) -> Result<SettingsSummary> {
    let connection = open_daemon_database(database_path)?;
    let existing = read_settings_record(&connection)?;
    let mut record = existing.unwrap_or_default();

    record.base_url = input
        .base_url
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    record.model = input
        .model
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    if let Some(api_key) = input.api_key {
        record.api_key = if api_key.trim().is_empty() {
            None
        } else {
            Some(api_key.trim().to_owned())
        };
    }
    record.git_author_name = input
        .git_author_name
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    record.git_author_email = input
        .git_author_email
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());

    let updated_at = OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|_error| AgentError::ConfigurationSource(std::fmt::Error))?;
    let transaction = connection.unchecked_transaction()?;
    transaction.execute(
        "INSERT INTO app_settings(
                id, base_url, model, api_key, git_author_name, git_author_email, updated_at
             ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
               base_url = excluded.base_url,
               model = excluded.model,
               api_key = excluded.api_key,
               git_author_name = excluded.git_author_name,
               git_author_email = excluded.git_author_email,
               updated_at = excluded.updated_at",
        rusqlite::params![
            record.base_url,
            record.model,
            record.api_key,
            record.git_author_name,
            record.git_author_email,
            updated_at
        ],
    )?;
    transaction.commit()?;

    Ok(settings_summary(&record))
}

pub fn provider_config() -> Result<ProviderConfig> {
    let database = daemon_db_path()?;
    let record = load_settings_at(&database)?;
    let base_url = record
        .base_url
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_owned());
    let api_key = record.api_key.unwrap_or_default();
    let model = record.model.unwrap_or_else(|| DEFAULT_MODEL.to_owned());
    ProviderConfig::new(base_url, api_key, model)
}

pub fn provider_config_for_settings(input: &SettingsInput) -> Result<ProviderConfig> {
    let database = daemon_db_path()?;
    let record = load_settings_at(&database)?;
    let non_empty = |value: Option<&String>| -> Option<String> {
        value
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let base_url = non_empty(input.base_url.as_ref())
        .or(record.base_url)
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_owned());
    let api_key = non_empty(input.api_key.as_ref())
        .or(record.api_key)
        .unwrap_or_default();
    let model = non_empty(input.model.as_ref())
        .or(record.model)
        .unwrap_or_else(|| DEFAULT_MODEL.to_owned());
    ProviderConfig::new(base_url, api_key, model)
}

impl ProviderConfig {
    pub fn load() -> Result<Self> {
        let database = daemon_db_path()?;
        let record = load_settings_at(&database)?;
        let base_url = record
            .base_url
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_owned());
        let api_key = record.api_key.unwrap_or_default();
        let model = record.model.unwrap_or_else(|| DEFAULT_MODEL.to_owned());
        Self::new(base_url, api_key, model)
    }

    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Result<Self> {
        let base_url = normalize_base_url(base_url.into());
        let api_key = api_key.into().trim().to_owned();
        let model = model.into().trim().to_owned();
        if api_key.is_empty() {
            return Err(AgentError::Configuration("API key is empty".to_owned()));
        }
        if model.is_empty() {
            return Err(AgentError::Configuration("model is empty".to_owned()));
        }
        Ok(Self {
            base_url,
            api_key,
            model,
        })
    }
}

fn normalize_base_url(mut value: String) -> String {
    while value.ends_with('/') {
        value.pop();
    }
    value
}

pub(crate) fn daemon_db_path() -> Result<std::path::PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Err(AgentError::Configuration(
            "home directory is unavailable".to_owned(),
        ));
    };
    Ok(home.join(".corpusbot").join(DAEMON_DB_FILE_NAME))
}

fn settings_summary(record: &SettingsRecord) -> SettingsSummary {
    SettingsSummary {
        base_url: record.base_url.clone(),
        model: record.model.clone(),
        has_api_key: record
            .api_key
            .as_ref()
            .is_some_and(|key| !key.trim().is_empty()),
        git_author_name: record.git_author_name.clone(),
        git_author_email: record.git_author_email.clone(),
    }
}

fn open_daemon_database(path: &std::path::Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(parent)?.permissions();
            permissions.set_mode(0o700);
            std::fs::set_permissions(parent, permissions)?;
        }
    }

    let connection = Connection::open(path)?;
    connection.busy_timeout(std::time::Duration::from_millis(5_000))?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS app_settings (
          id INTEGER PRIMARY KEY CHECK (id = 1),
          base_url TEXT,
          model TEXT,
          api_key TEXT,
          git_author_name TEXT,
          git_author_email TEXT,
          updated_at TEXT NOT NULL
        )",
    )?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(path)?.permissions();
        permissions.set_mode(0o600);
        std::fs::set_permissions(path, permissions)?;
    }

    Ok(connection)
}

fn read_settings_record(connection: &Connection) -> Result<Option<SettingsRecord>> {
    let record = connection
        .query_row(
            "SELECT base_url, model, api_key, git_author_name, git_author_email
             FROM app_settings WHERE id = 1",
            [],
            |row| {
                Ok(SettingsRecord {
                    base_url: row.get(0)?,
                    model: row.get(1)?,
                    api_key: row.get(2)?,
                    git_author_name: row.get(3)?,
                    git_author_email: row.get(4)?,
                })
            },
        )
        .optional()?;
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_and_rejects_empty_values() -> Result<()> {
        let config = ProviderConfig::new("https://example.com/v1/", " secret ", " model ")?;
        assert_eq!(config.base_url, "https://example.com/v1");
        assert_eq!(config.api_key, "secret");
        assert_eq!(config.model, "model");
        assert!(ProviderConfig::new("https://example.com", "", "model").is_err());
        Ok(())
    }

    #[test]
    fn saves_loads_and_updates_daemon_settings() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let database = directory.path().join("daemon.db");

        let initial = load_settings_at(&database)?;
        assert_eq!(initial.base_url, None);
        assert!(!settings_summary(&initial).has_api_key);

        let saved = save_settings_at(
            &database,
            SettingsInput {
                base_url: Some(" https://example.com/v1/ ".to_owned()),
                model: Some(" test-model ".to_owned()),
                api_key: Some(" secret ".to_owned()),
                git_author_name: Some(" Test User ".to_owned()),
                git_author_email: Some(" test@example.com ".to_owned()),
            },
        )?;
        assert!(saved.has_api_key);
        assert_eq!(saved.git_author_name.as_deref(), Some("Test User"));

        let updated = save_settings_at(
            &database,
            SettingsInput {
                base_url: None,
                model: None,
                api_key: Some(String::new()),
                git_author_name: None,
                git_author_email: None,
            },
        )?;
        assert_eq!(updated.base_url, None);
        assert_eq!(updated.model, None);
        assert!(!updated.has_api_key);
        assert_eq!(updated.git_author_name, None);
        Ok(())
    }
}
