//! Venice.ai API provider with credit tracking and fallback support

use super::sse::{parse_sse_line, SseFormat};
use super::streaming::{StreamChunk, StreamingProvider};
use super::{ApiError, ApiProvider, ApiRequest, ApiResponse, ProviderType, TokenUsage};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::{Client, Response};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

/// Venice.ai specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VeniceConfig {
    /// Venice API key
    pub api_key: String,
    /// Model to use (e.g., "llama-3.3-70b", "deepseek-coder-v2")
    pub model: String,
    /// Base URL (default: https://api.venice.ai/api/v1)
    pub base_url: Option<String>,
    /// Minimum USD balance before triggering fallback
    pub min_balance_usd: f64,
    /// Minimum Diem balance before triggering fallback
    pub min_balance_diem: f64,
    /// Maximum tokens for response
    pub max_tokens: Option<u32>,
    /// Temperature for generation
    pub temperature: Option<f32>,
}

impl Default for VeniceConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            model: "llama-3.3-70b".to_string(),
            base_url: None,
            min_balance_usd: 0.10, // Trigger fallback when below $0.10
            min_balance_diem: 0.10,
            max_tokens: Some(4096),
            temperature: Some(0.7),
        }
    }
}

/// Credit balance tracking for Venice.ai
#[derive(Debug, Clone, Default)]
pub struct VeniceBalance {
    /// USD credit balance
    pub balance_usd: f64,
    /// Diem token balance
    pub balance_diem: f64,
    /// Whether credits are exhausted
    pub exhausted: bool,
    /// Last update timestamp
    pub last_updated: Option<std::time::Instant>,
}

/// Venice.ai API provider with credit tracking
pub struct VeniceProvider {
    config: VeniceConfig,
    client: Client,
    balance: Arc<RwLock<VeniceBalance>>,
    credits_exhausted: Arc<AtomicBool>,
}

impl VeniceProvider {
    pub fn new(config: VeniceConfig) -> Self {
        Self {
            config,
            client: Client::new(),
            balance: Arc::new(RwLock::new(VeniceBalance::default())),
            credits_exhausted: Arc::new(AtomicBool::new(false)),
        }
    }

    fn base_url(&self) -> &str {
        self.config
            .base_url
            .as_deref()
            .unwrap_or("https://api.venice.ai/api/v1")
    }

    /// Check if credits are exhausted
    pub fn is_exhausted(&self) -> bool {
        self.credits_exhausted.load(Ordering::SeqCst)
    }

    /// Get current balance
    pub async fn get_balance(&self) -> VeniceBalance {
        self.balance.read().await.clone()
    }

    /// Fetch current rate limits and balance from Venice API
    pub async fn fetch_balance(&self) -> Result<VeniceBalance, ApiError> {
        let url = format!("{}/api_keys/rate_limits", self.base_url());

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .send()
            .await?;

        if response.status().is_success() {
            let json: Value = response.json().await?;

            let balance = VeniceBalance {
                balance_usd: json["balance_usd"].as_f64().unwrap_or(0.0),
                balance_diem: json["balance_diem"].as_f64().unwrap_or(0.0),
                exhausted: false,
                last_updated: Some(std::time::Instant::now()),
            };

            // Update stored balance
            *self.balance.write().await = balance.clone();

            // Check if below threshold
            if balance.balance_usd < self.config.min_balance_usd
                && balance.balance_diem < self.config.min_balance_diem
            {
                self.credits_exhausted.store(true, Ordering::SeqCst);
            }

            Ok(balance)
        } else {
            Err(ApiError::Provider(format!(
                "Failed to fetch balance: {}",
                response.status()
            )))
        }
    }

    /// Update balance from response headers
    async fn update_balance_from_headers(&self, response: &Response) {
        let mut balance = self.balance.write().await;

        if let Some(usd) = response
            .headers()
            .get("x-venice-balance-usd")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<f64>().ok())
        {
            balance.balance_usd = usd;
        }

        if let Some(diem) = response
            .headers()
            .get("x-venice-balance-diem")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<f64>().ok())
        {
            balance.balance_diem = diem;
        }

        balance.last_updated = Some(std::time::Instant::now());

        // Check if below threshold
        if balance.balance_usd < self.config.min_balance_usd
            && balance.balance_diem < self.config.min_balance_diem
        {
            balance.exhausted = true;
            self.credits_exhausted.store(true, Ordering::SeqCst);
        }
    }

    fn build_request(&self, request: &ApiRequest) -> Value {
        let mut messages = Vec::new();

        // Add system prompt if present
        if let Some(system) = &request.system {
            messages.push(json!({
                "role": "system",
                "content": system
            }));
        }

        // Add context as user message
        if !request.context.is_empty() {
            let context_text = request
                .context
                .iter()
                .map(|c| format!("### {}\n```\n{}\n```", c.name, c.content))
                .collect::<Vec<_>>()
                .join("\n\n");

            messages.push(json!({
                "role": "user",
                "content": format!("Context:\n{}", context_text)
            }));
        }

        // Insert conversation history between context and current task
        for msg in &request.messages {
            let role = match msg.role {
                super::Role::User => "user",
                super::Role::Assistant => "assistant",
                super::Role::System => "system",
            };
            messages.push(json!({
                "role": role,
                "content": msg.content
            }));
        }

        // Add the task
        messages.push(json!({
            "role": "user",
            "content": request.task
        }));

        let mut body = json!({
            "model": self.config.model,
            "messages": messages,
        });

        if let Some(max_tokens) = self.config.max_tokens {
            body["max_tokens"] = json!(max_tokens);
        }

        if let Some(temp) = self.config.temperature {
            body["temperature"] = json!(temp);
        }

        body
    }

    fn parse_response(&self, json: Value) -> Result<ApiResponse, ApiError> {
        let content = json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let usage = TokenUsage::new(
            json["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as u32,
            json["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32,
        );

        Ok(ApiResponse {
            content,
            usage,
            model: json["model"].as_str().unwrap_or(&self.config.model).to_string(),
            truncated: json["choices"][0]["finish_reason"].as_str() == Some("length"),
            stop_reason: None,
        })
    }
}

#[async_trait]
impl ApiProvider for VeniceProvider {
    async fn send_request(&self, request: ApiRequest) -> Result<ApiResponse, ApiError> {
        // Check if already exhausted
        if self.is_exhausted() {
            return Err(ApiError::Provider(
                "Venice credits exhausted - fallback required".to_string(),
            ));
        }

        let url = format!("{}/chat/completions", self.base_url());
        let body = self.build_request(&request);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        // Update balance from headers before consuming response
        self.update_balance_from_headers(&response).await;

        let status = response.status();

        if status.is_success() {
            let json: Value = response.json().await?;
            self.parse_response(json)
        } else if status.as_u16() == 429 {
            let error_text = response.text().await.unwrap_or_default();

            // Check if it's a quota issue vs rate limit
            if error_text.contains("insufficient")
                || error_text.contains("quota")
                || error_text.contains("balance")
            {
                self.credits_exhausted.store(true, Ordering::SeqCst);
                Err(ApiError::Provider(
                    "Venice credits exhausted - fallback required".to_string(),
                ))
            } else {
                Err(ApiError::RateLimited {
                    retry_after_secs: 60,
                })
            }
        } else {
            let error_text = response.text().await.unwrap_or_default();
            Err(ApiError::Provider(format!("{}: {}", status, error_text)))
        }
    }

    fn estimate_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }

    fn provider_type(&self) -> ProviderType {
        ProviderType::Custom // Venice is a custom provider
    }
}

#[async_trait]
impl StreamingProvider for VeniceProvider {
    async fn send_streaming(
        &self,
        request: ApiRequest,
    ) -> Result<mpsc::Receiver<StreamChunk>, ApiError> {
        if self.is_exhausted() {
            return Err(ApiError::Provider(
                "Venice credits exhausted - fallback required".to_string(),
            ));
        }

        let url = format!("{}/chat/completions", self.base_url());
        let mut body = self.build_request(&request);
        body["stream"] = json!(true);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        // Update balance from headers before consuming stream body
        self.update_balance_from_headers(&response).await;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            if status.as_u16() == 429
                && (error_text.contains("insufficient")
                    || error_text.contains("quota")
                    || error_text.contains("balance"))
            {
                self.credits_exhausted.store(true, Ordering::SeqCst);
                return Err(ApiError::Provider(
                    "Venice credits exhausted - fallback required".to_string(),
                ));
            }
            return Err(ApiError::Provider(format!("{}: {}", status, error_text)));
        }

        let (tx, rx) = mpsc::channel(64);

        tokio::spawn(async move {
            let mut stream = response.bytes_stream();
            let mut buffer = String::new();

            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(bytes) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));

                        while let Some(newline_pos) = buffer.find('\n') {
                            let line = buffer[..newline_pos].to_string();
                            buffer = buffer[newline_pos + 1..].to_string();

                            if let Some(chunk) = parse_sse_line(&line, SseFormat::OpenAI) {
                                let is_done = matches!(chunk, StreamChunk::Done(_));
                                let is_error = matches!(chunk, StreamChunk::Error(_));
                                if tx.send(chunk).await.is_err() {
                                    return;
                                }
                                if is_done || is_error {
                                    return;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx
                            .send(StreamChunk::Error(format!("Stream error: {}", e)))
                            .await;
                        return;
                    }
                }
            }

            let _ = tx.send(StreamChunk::Done(TokenUsage::default())).await;
        });

        Ok(rx)
    }
}

/// Available Venice.ai models for code tasks
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VeniceModel {
    /// Llama 3.3 70B - Good balance of capability and cost
    Llama3_3_70B,
    /// DeepSeek Coder V2 - Optimized for code
    DeepSeekCoderV2,
    /// Qwen 2.5 Coder 32B - Strong code model
    Qwen25Coder32B,
    /// Venice Small - Budget option
    VeniceSmall,
    /// Grok Code Fast - Fast code completion
    GrokCodeFast,
}

impl VeniceModel {
    pub fn model_id(&self) -> &'static str {
        match self {
            VeniceModel::Llama3_3_70B => "llama-3.3-70b",
            VeniceModel::DeepSeekCoderV2 => "deepseek-coder-v2",
            VeniceModel::Qwen25Coder32B => "qwen-2.5-coder-32b",
            VeniceModel::VeniceSmall => "venice-small",
            VeniceModel::GrokCodeFast => "grok-code-fast-1",
        }
    }

    /// Cost per 1M tokens (input, output)
    pub fn pricing(&self) -> (f64, f64) {
        match self {
            VeniceModel::Llama3_3_70B => (0.70, 2.80),
            VeniceModel::DeepSeekCoderV2 => (0.50, 2.00),
            VeniceModel::Qwen25Coder32B => (0.40, 1.60),
            VeniceModel::VeniceSmall => (0.05, 0.15),
            VeniceModel::GrokCodeFast => (0.25, 1.87),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_venice_model_ids() {
        assert_eq!(VeniceModel::Llama3_3_70B.model_id(), "llama-3.3-70b");
        assert_eq!(VeniceModel::VeniceSmall.model_id(), "venice-small");
    }

    #[test]
    fn test_default_config() {
        let config = VeniceConfig::default();
        assert_eq!(config.min_balance_usd, 0.10);
        assert_eq!(config.model, "llama-3.3-70b");
    }
}
