use async_trait::async_trait;
use rig_core::client::CompletionClient;
use rig_core::client::VerifyClient;
use rig_core::completion::FinishReason;
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
    pub structured_output: Option<StructuredOutput>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct StructuredOutput {
    pub name: String,
    pub schema: serde_json::Value,
}

impl LlmRequest {
    pub fn prompt_hash(&self) -> String {
        let structured_output = serde_json::to_string(&self.structured_output).unwrap_or_default();
        let digest = Sha256::digest(
            format!("{}\n{}\n{}", self.system, self.prompt, structured_output).as_bytes(),
        );
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
    pub finish_reason: Option<String>,
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

    async fn test_connection(&self) -> Result<ConnectionTestResult> {
        let started_at = std::time::Instant::now();
        let response = self
            .complete(LlmRequest {
                operation: "connection-test".to_owned(),
                system: String::new(),
                prompt: "Reply with OK.".to_owned(),
                prompt_template_id: "connection-test-v1".to_owned(),
                temperature: None,
                max_tokens: Some(64),
                structured_output: None,
            })
            .await?;
        Ok(ConnectionTestResult {
            provider: response.provider,
            model: response.model,
            latency_ms: started_at.elapsed().as_millis() as u64,
            response_id: response.response_id,
        })
    }
}

pub struct RigLlmClient {
    client: rig_core::providers::openai::CompletionsClient,
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
            client,
            model,
            provider: "openai-compatible".to_owned(),
            model_name,
        })
    }

    pub async fn test_connection(&self) -> Result<ConnectionTestResult> {
        let started_at = std::time::Instant::now();
        self.client.verify().await.map_err(|error| {
            AgentError::Other(format!("provider verification failed: {error}").into())
        })?;

        Ok(ConnectionTestResult {
            provider: self.provider.clone(),
            model: self.model_name.clone(),
            latency_ms: started_at.elapsed().as_millis() as u64,
            response_id: None,
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
        if let Some(output) = &request.structured_output {
            builder = builder.additional_params(serde_json::json!({
                "response_format": {
                    "type": "json_schema",
                    "json_schema": {
                        "name": output.name,
                        "strict": true,
                        "schema": output.schema,
                    }
                }
            }));
        }
        let response = builder
            .temperature_opt(request.temperature)
            .max_tokens_opt(request.max_tokens)
            .send()
            .await?;

        let finish_reason = response.finish_reason().map(|reason| match reason {
            FinishReason::Stop => "stop".to_owned(),
            FinishReason::Length => "length".to_owned(),
            FinishReason::ToolCalls => "tool_calls".to_owned(),
            FinishReason::ContentFilter => "content_filter".to_owned(),
            FinishReason::Other(reason) => reason,
        });

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
            finish_reason,
        })
    }
}

#[derive(Default)]
pub struct FakeLlmClient {
    responses: std::sync::Mutex<std::collections::VecDeque<String>>,
    calls: std::sync::Arc<std::sync::Mutex<Vec<LlmRequest>>>,
}

impl Clone for FakeLlmClient {
    fn clone(&self) -> Self {
        Self {
            responses: std::sync::Mutex::new(
                self.responses.lock().expect("responses mutex").clone(),
            ),
            calls: std::sync::Arc::clone(&self.calls),
        }
    }
}

impl FakeLlmClient {
    pub fn new(responses: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            responses: std::sync::Mutex::new(responses.into_iter().map(Into::into).collect()),
            calls: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
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
            finish_reason: None,
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
            structured_output: None,
        };
        let response = client.complete(request.clone()).await?;
        assert_eq!(response.text, r#"{"ok":true}"#);
        assert_eq!(client.calls().len(), 1);
        assert_eq!(client.calls()[0].prompt_hash().len(), 64);
        assert!(!response.text.trim().is_empty());
        Ok(())
    }
}
