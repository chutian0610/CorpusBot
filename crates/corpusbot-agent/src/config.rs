use crate::error::{AgentError, Result};

pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
pub const DEFAULT_MODEL: &str = "gpt-4o-mini";
pub const CONFIG_FILE_NAME: &str = "provider.json";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
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
