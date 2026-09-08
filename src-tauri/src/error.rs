use corpusbot_agent::AgentError;
use corpusbot_store::StoreError;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct CommandError(pub String);

impl std::fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for CommandError {}

impl From<AgentError> for CommandError {
    fn from(error: AgentError) -> Self {
        Self(error.to_string())
    }
}

impl From<StoreError> for CommandError {
    fn from(error: StoreError) -> Self {
        Self(error.to_string())
    }
}

impl From<std::io::Error> for CommandError {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}
