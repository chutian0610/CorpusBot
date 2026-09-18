use async_trait::async_trait;
use rig_core::client::CompletionClient;
use rig_core::completion::message::AssistantContent;
use rig_core::completion::request::CompletionRequestBuilder;
use rig_core::providers::openai::completion::CompletionModel;
use sha2::{Digest, Sha256};

use crate::config::ProviderConfig;
use crate::error::{AgentError, Result};

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct LlmRequest {
    pub operation: String,
    pub system: String,
    pub prompt: String,
    pub prompt_template_id: String,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u64>,
}

impl LlmRequest {
    pub fn prompt_hash(&self) -> String {
        let digest = Sha256::digest(format!("{}\n{}", self.system, self.prompt).as_bytes());
        format!("{digest:x}")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LlmResponse {
    pub text: String,
    pub provider: String,
    pub model: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub response_id: Option<String>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTestResult {
    pub provider: String,
    pub model: String,
    pub latency_ms: u64,
    pub response_id: Option<String>,
}

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse>;
}

pub struct RigLlmClient {
    model: CompletionModel,
    provider: String,
    model_name: String,
}

impl RigLlmClient {
    pub fn new(config: ProviderConfig) -> Result<Self> {
        let builder = <rig_core::client::ClientBuilder<
            rig_core::providers::openai::OpenAICompletionsExtBuilder,
            rig_core::markers::Missing,
            rig_core::markers::Missing,
        > as Default>::default();
        let client = builder
            .api_key(config.api_key)
            .base_url(config.base_url)
            .build()?;
        let model_name = config.model;
        let model = client.completion_model(model_name.clone());

        Ok(Self {
            model,
            provider: "openai-compatible".to_owned(),
            model_name,
        })
    }

    pub async fn test_connection(&self) -> Result<ConnectionTestResult> {
        let started_at = std::time::Instant::now();
        let request = async {
            CompletionRequestBuilder::new(self.model.clone(), "Reply with OK.".to_owned())
                .max_tokens_opt(Some(1))
                .send()
                .await
        };
        let response = match tokio::time::timeout(std::time::Duration::from_secs(15), request).await
        {
            Ok(response) => response?,
            Err(_) => return Err(AgentError::Timeout { timeout_ms: 15_000 }),
        };

        Ok(ConnectionTestResult {
            provider: self.provider.clone(),
            model: self.model_name.clone(),
            latency_ms: started_at.elapsed().as_millis() as u64,
            response_id: response.response_id,
        })
    }
}

#[async_trait]
impl LlmClient for RigLlmClient {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        let mut builder = CompletionRequestBuilder::new(self.model.clone(), request.prompt.clone());
        if !request.system.is_empty() {
            builder = builder.preamble(request.system.clone());
        }
        let response = builder
            .temperature_opt(request.temperature)
            .max_tokens_opt(request.max_tokens)
            .send()
            .await?;

        let text = response
            .choice
            .iter()
            .filter_map(|content| match content {
                AssistantContent::Text(text) => Some(text.text().to_owned()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("");
        if text.trim().is_empty() {
            return Err(AgentError::EmptyResponse);
        }

        Ok(LlmResponse {
            text,
            provider: self.provider.clone(),
            model: self.model_name.clone(),
            prompt_tokens: response.usage.input_tokens,
            completion_tokens: response.usage.output_tokens,
            response_id: response.response_id,
        })
    }
}

#[derive(Default)]
pub struct FakeLlmClient {
    responses: std::sync::Mutex<std::collections::VecDeque<String>>,
    calls: std::sync::Mutex<Vec<LlmRequest>>,
}

impl FakeLlmClient {
    pub fn new(responses: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            responses: std::sync::Mutex::new(responses.into_iter().map(Into::into).collect()),
            calls: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub fn calls(&self) -> Vec<LlmRequest> {
        self.calls.lock().expect("calls mutex poisoned").clone()
    }
}

#[async_trait]
impl LlmClient for FakeLlmClient {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        self.calls
            .lock()
            .expect("calls mutex poisoned")
            .push(request.clone());
        let Some(text) = self
            .responses
            .lock()
            .expect("responses mutex poisoned")
            .pop_front()
        else {
            return Err(AgentError::EmptyResponse);
        };
        if text.trim().is_empty() {
            return Err(AgentError::EmptyResponse);
        }

        Ok(LlmResponse {
            text,
            provider: "fake".to_owned(),
            model: "fake-model".to_owned(),
            prompt_tokens: 10,
            completion_tokens: 20,
            response_id: Some("fake-response".to_owned()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fake_client_records_requests() -> Result<()> {
        let client = FakeLlmClient::new([r#"{"ok":true}"#]);
        let request = LlmRequest {
            operation: "analyze".to_owned(),
            system: "Return JSON".to_owned(),
            prompt: "source".to_owned(),
            prompt_template_id: "analyze-v1".to_owned(),
            temperature: Some(0.1),
            max_tokens: Some(100),
        };
        let response = client.complete(request.clone()).await?;
        assert_eq!(response.text, r#"{"ok":true}"#);
        assert_eq!(client.calls().len(), 1);
        assert_eq!(client.calls()[0].prompt_hash().len(), 64);
        assert!(!response.text.trim().is_empty());
        Ok(())
    }
}
