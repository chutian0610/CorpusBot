use crate::error::{AgentError, Result};

pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
pub const DEFAULT_MODEL: &str = "gpt-4o-mini";
pub const CONFIG_FILE_NAME: &str = "provider.json";
pub const SETTINGS_FILE_NAME: &str = "settings.json";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
struct SettingsFile {
    base_url: Option<String>,
    model: Option<String>,
    api_key: Option<String>,
    git_author_name: Option<String>,
    git_author_email: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSummary {
    pub base_url: String,
    pub model: String,
    pub has_api_key: bool,
    pub git_author_name: Option<String>,
    pub git_author_email: Option<String>,
}

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsInput {
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub git_author_name: Option<String>,
    pub git_author_email: Option<String>,
}

pub fn settings_path() -> Result<std::path::PathBuf> {
    let Some(config_dir) = dirs::config_dir() else {
        return Err(AgentError::Configuration(
            "user configuration directory is unavailable".to_owned(),
        ));
    };
    Ok(config_dir.join("CorpusBot").join(SETTINGS_FILE_NAME))
}

pub fn load_settings() -> Result<SettingsSummary> {
    let file = read_settings()?;
    let base_url = file.base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_owned());
    let model = file.model.unwrap_or_else(|| DEFAULT_MODEL.to_owned());
    Ok(SettingsSummary {
        base_url,
        model,
        has_api_key: file.api_key.is_some_and(|key| !key.trim().is_empty()),
        git_author_name: file.git_author_name,
        git_author_email: file.git_author_email,
    })
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
    let path = settings_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = read_settings()?;
    file.base_url = Some(input.base_url.trim().to_owned());
    file.model = Some(input.model.trim().to_owned());
    if let Some(api_key) = input.api_key {
        file.api_key = if api_key.trim().is_empty() {
            None
        } else {
            Some(api_key.trim().to_owned())
        };
    }
    file.git_author_name = input
        .git_author_name
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    file.git_author_email = input
        .git_author_email
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());

    if file.base_url.as_deref().is_none_or(str::is_empty) {
        return Err(AgentError::Configuration("base URL is empty".to_owned()));
    }
    if file.model.as_deref().is_none_or(str::is_empty) {
        return Err(AgentError::Configuration("model is empty".to_owned()));
    }

    let raw = serde_json::to_vec_pretty(&file)?;
    let mut tempfile = tempfile::NamedTempFile::new_in(
        path.parent()
            .ok_or_else(|| AgentError::Configuration("invalid settings path".to_owned()))?,
    )?;
    use std::io::Write;
    tempfile.write_all(&raw)?;
    tempfile.as_file().sync_all()?;
    tempfile.persist(&path)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&path)?.permissions();
        permissions.set_mode(0o600);
        std::fs::set_permissions(&path, permissions)?;
    }

    load_settings()
}

fn read_settings() -> Result<SettingsFile> {
    let path = settings_path()?;
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(SettingsFile::default());
        }
        Err(error) => return Err(error.into()),
    };
    serde_json::from_str(&raw).map_err(Into::into)
}

pub fn provider_config() -> Result<ProviderConfig> {
    let file = read_settings()?;
    let base_url = std::env::var("OPENAI_BASE_URL")
        .ok()
        .or(file.base_url)
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_owned());
    let api_key = std::env::var("OPENAI_API_KEY")
        .ok()
        .or(file.api_key)
        .unwrap_or_default();
    let model = std::env::var("CORPUSBOT_MODEL")
        .ok()
        .or(file.model)
        .unwrap_or_else(|| DEFAULT_MODEL.to_owned());
    ProviderConfig::new(base_url, api_key, model)
}

impl ProviderConfig {
    pub fn load() -> Result<Self> {
        let file = Self::from_config_file()?;
        let base_url = std::env::var("OPENAI_BASE_URL")
            .ok()
            .or(file.base_url)
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_owned());
        let api_key = std::env::var("OPENAI_API_KEY")
            .ok()
            .or(file.api_key)
            .unwrap_or_default();
        let model = std::env::var("CORPUSBOT_MODEL")
            .ok()
            .or(file.model)
            .unwrap_or_else(|| DEFAULT_MODEL.to_owned());
        Self::new(base_url, api_key, model)
    }

    fn from_config_file() -> Result<ProviderFileConfig> {
        let Some(config_dir) = dirs::config_dir() else {
            return Ok(ProviderFileConfig::default());
        };
        let path = config_dir.join("CorpusBot").join(CONFIG_FILE_NAME);
        let raw = match std::fs::read_to_string(path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(ProviderFileConfig::default());
            }
            Err(error) => return Err(error.into()),
        };
        serde_json::from_str(&raw).map_err(Into::into)
    }

    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| AgentError::Configuration("OPENAI_API_KEY is not set".to_owned()))?;
        let base_url =
            std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_owned());
        let model = std::env::var("CORPUSBOT_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_owned());
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

#[derive(Default, serde::Deserialize)]
struct ProviderFileConfig {
    base_url: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
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
}
