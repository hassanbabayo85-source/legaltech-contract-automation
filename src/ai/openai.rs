//! OpenAI-compatible chat completions adapter.
//!
//! Speaks the same request/response shape as OpenAI's
//! `/v1/chat/completions` endpoint, which is also implemented by many
//! compatible services (Gemini's OpenAI-compatible routing, OpenRouter,
//! Ollama, vLLM, ...). The endpoint and model come from configuration,
//! so no code changes are needed to switch backends.

use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;

use crate::ai::prompt::SYSTEM_PROMPT;
use crate::ai::provider::{AiError, AiProvider};
use crate::ai::schema::AiAnalysis;
use crate::config::Config;

/// An `AiProvider` backed by an OpenAI-compatible HTTP endpoint.
#[derive(Debug)]
pub struct OpenAiProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
    model: String,
}

impl OpenAiProvider {
    /// Builds the provider from configuration.
    ///
    /// Construction never fails: an unset API key is stored as `None`
    /// and only surfaces as [`AiError::NotConfigured`] when an analysis
    /// is attempted. This lets the server start cleanly in environments
    /// that do not yet need AI (e.g. a health-check-only deployment).
    pub fn from_config(config: &Config) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.ai_timeout_seconds))
            .build()
            .expect("reqwest client construction must not fail");
        Self {
            client,
            base_url: config.ai_base_url.clone(),
            api_key: config.ai_api_key.clone(),
            model: config.ai_model.clone(),
        }
    }
}

#[async_trait]
impl AiProvider for OpenAiProvider {
    fn name(&self) -> &'static str {
        "openai"
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn analyze_contract(&self, raw_text: &str) -> Result<AiAnalysis, AiError> {
        let api_key = self.api_key.as_deref().ok_or(AiError::NotConfigured)?;

        // Build the chat-completions URL. Providers disagree on whether
        // their base URL already includes the `/v1` prefix:
        //
        //   * OpenAI-style  : https://api.openai.com/v1       → append /chat/completions
        //   * Groq          : https://api.groq.com/openai     → append /v1/chat/completions
        //   * Gemini        : https://.../v1beta/openai       → append /chat/completions
        //
        // We handle both by checking whether the base already ends with
        // `/v1`, `/v1beta`, or the full chat-completions path.
        let base = self.base_url.trim_end_matches('/');
        let url = if base.ends_with("/chat/completions") {
            base.to_string()
        } else if base.ends_with("/v1") || base.ends_with("/v1beta") {
            format!("{base}/chat/completions")
        } else {
            format!("{base}/v1/chat/completions")
        };

        let body = serde_json::json!({
            "model": self.model,
            "response_format": { "type": "json_object" },
            "temperature": 0,
            "messages": [
                { "role": "system", "content": SYSTEM_PROMPT },
                { "role": "user", "content": raw_text },
            ],
        });

        let response = self
            .client
            .post(&url)
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await
            .map_err(|err| {
                if err.is_timeout() {
                    AiError::Timeout
                } else if err.is_connect() || err.is_request() {
                    AiError::Transient
                } else {
                    AiError::Other
                }
            })?;

        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(AiError::RateLimited);
        }
        if status.is_server_error() {
            return Err(AiError::Transient);
        }
        if !status.is_success() {
            return Err(AiError::Permanent {
                status: status.as_u16(),
            });
        }

        let parsed: ChatResponse = response
            .json()
            .await
            .map_err(|_| AiError::MalformedResponse)?;

        let content = parsed
            .choices
            .into_iter()
            .next()
            .ok_or(AiError::MalformedResponse)?
            .message
            .content;

        let analysis: AiAnalysis =
            serde_json::from_str(&content).map_err(|_| AiError::MalformedResponse)?;

        Ok(analysis)
    }
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    content: String,
}
