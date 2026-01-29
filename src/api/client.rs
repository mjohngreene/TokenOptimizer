//! Generic API client for coding agents

use super::sse::{parse_sse_line, SseFormat};
use super::streaming::{StreamChunk, StreamingProvider};
use super::{ApiConfig, ApiError, ApiProvider, ApiRequest, ApiResponse, ProviderType, TokenUsage};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use tokio::sync::mpsc;

/// Generic API agent that can work with multiple providers
pub struct ApiAgent {
    config: ApiConfig,
    client: Client,
}

impl ApiAgent {
    pub fn new(config: ApiConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }

    fn build_claude_request(&self, request: &ApiRequest) -> Value {
        let mut messages = Vec::new();

        // Build context with cache control support
        // Static context items come first for optimal caching
        if !request.context.is_empty() {
            let mut content_blocks: Vec<Value> = Vec::new();

            // Group context items, adding cache breakpoints where specified
            for (idx, ctx) in request.context.iter().enumerate() {
                let block_text = format!("### {}\n```\n{}\n```", ctx.name, ctx.content);

                // Check if this item has cache control or is at a breakpoint
                let has_breakpoint = request.cache_breakpoints.contains(&idx)
                    || ctx.cache_control.is_some();

                if has_breakpoint {
                    // Add with cache_control
                    content_blocks.push(json!({
                        "type": "text",
                        "text": block_text,
                        "cache_control": { "type": "ephemeral" }
                    }));
                } else {
                    content_blocks.push(json!({
                        "type": "text",
                        "text": block_text
                    }));
                }
            }

            messages.push(json!({
                "role": "user",
                "content": content_blocks
            }));
        }

        // Insert conversation history between context and current task
        for msg in &request.messages {
            let role = match msg.role {
                super::Role::User => "user",
                super::Role::Assistant => "assistant",
                super::Role::System => continue, // System messages handled separately
            };
            messages.push(json!({
                "role": role,
                "content": msg.content
            }));
        }

        // Add the task (always dynamic, no caching)
        messages.push(json!({
            "role": "user",
            "content": request.task
        }));

        let mut body = json!({
            "model": self.config.model,
            "messages": messages,
            "max_tokens": self.config.max_tokens.unwrap_or(4096),
        });

        // Handle system prompt with optional caching
        if let Some(system) = &request.system {
            if request.system_cache_control.is_some() {
                // Use array format with cache_control for cacheable system prompt
                body["system"] = json!([
                    {
                        "type": "text",
                        "text": system,
                        "cache_control": { "type": "ephemeral" }
                    }
                ]);
            } else {
                body["system"] = json!(system);
            }
        }

        if let Some(temp) = self.config.temperature {
            body["temperature"] = json!(temp);
        }

        body
    }

    fn build_openai_request(&self, request: &ApiRequest) -> Value {
        let mut messages = Vec::new();

        if let Some(system) = &request.system {
            messages.push(json!({
                "role": "system",
                "content": system
            }));
        }

        // Add context
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

    fn parse_claude_response(&self, response: Value) -> Result<ApiResponse, ApiError> {
        let content = response["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();

        // Extract cache-related token counts
        let cache_creation = response["usage"]["cache_creation_input_tokens"]
            .as_u64()
            .map(|t| t as u32);
        let cache_read = response["usage"]["cache_read_input_tokens"]
            .as_u64()
            .map(|t| t as u32);

        let usage = TokenUsage::with_cache(
            response["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32,
            response["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32,
            cache_creation,
            cache_read,
        );

        Ok(ApiResponse {
            content,
            usage,
            model: response["model"].as_str().unwrap_or("").to_string(),
            truncated: response["stop_reason"].as_str() == Some("max_tokens"),
            stop_reason: None,
        })
    }

    fn parse_openai_response(&self, response: Value) -> Result<ApiResponse, ApiError> {
        let content = response["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let usage = TokenUsage::new(
            response["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as u32,
            response["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32,
        );

        Ok(ApiResponse {
            content,
            usage,
            model: response["model"].as_str().unwrap_or("").to_string(),
            truncated: response["choices"][0]["finish_reason"].as_str() == Some("length"),
            stop_reason: None,
        })
    }
}

#[async_trait]
impl ApiProvider for ApiAgent {
    async fn send_request(&self, request: ApiRequest) -> Result<ApiResponse, ApiError> {
        let (url, body, auth_header) = match self.config.provider {
            ProviderType::Claude => {
                let url = self
                    .config
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "https://api.anthropic.com/v1/messages".to_string());
                let body = self.build_claude_request(&request);
                (url, body, ("x-api-key", self.config.api_key.clone()))
            }
            ProviderType::OpenAI => {
                let url = self
                    .config
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "https://api.openai.com/v1/chat/completions".to_string());
                let body = self.build_openai_request(&request);
                (
                    url,
                    body,
                    ("Authorization", format!("Bearer {}", self.config.api_key)),
                )
            }
            ProviderType::Ollama => {
                let url = self
                    .config
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "http://localhost:11434/api/chat".to_string());
                let body = self.build_openai_request(&request); // Ollama uses OpenAI-compatible format
                (url, body, ("Authorization", String::new()))
            }
            ProviderType::Custom => {
                let url = self
                    .config
                    .base_url
                    .clone()
                    .ok_or_else(|| ApiError::Provider("Custom provider requires base_url".into()))?;
                let body = self.build_openai_request(&request);
                (
                    url,
                    body,
                    ("Authorization", format!("Bearer {}", self.config.api_key)),
                )
            }
        };

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("anthropic-version", "2023-06-01") // For Claude
            .header(auth_header.0, auth_header.1)
            .json(&body)
            .send()
            .await?;

        if response.status().is_success() {
            let json: Value = response.json().await?;
            match self.config.provider {
                ProviderType::Claude => self.parse_claude_response(json),
                _ => self.parse_openai_response(json),
            }
        } else {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            Err(ApiError::Provider(format!("{}: {}", status, error_text)))
        }
    }

    fn estimate_tokens(&self, text: &str) -> usize {
        // Simple estimation: ~4 characters per token on average
        // For accurate counts, use tiktoken
        text.len() / 4
    }

    fn provider_type(&self) -> ProviderType {
        self.config.provider.clone()
    }
}

#[async_trait]
impl StreamingProvider for ApiAgent {
    async fn send_streaming(
        &self,
        request: ApiRequest,
    ) -> Result<mpsc::Receiver<StreamChunk>, ApiError> {
        let (sse_format, url, mut body, auth_header) = match self.config.provider {
            ProviderType::Claude => {
                let url = self
                    .config
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "https://api.anthropic.com/v1/messages".to_string());
                let body = self.build_claude_request(&request);
                (
                    SseFormat::Anthropic,
                    url,
                    body,
                    ("x-api-key".to_string(), self.config.api_key.clone()),
                )
            }
            ProviderType::OpenAI => {
                let url = self
                    .config
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "https://api.openai.com/v1/chat/completions".to_string());
                let body = self.build_openai_request(&request);
                (
                    SseFormat::OpenAI,
                    url,
                    body,
                    (
                        "Authorization".to_string(),
                        format!("Bearer {}", self.config.api_key),
                    ),
                )
            }
            ProviderType::Ollama => {
                let url = self
                    .config
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "http://localhost:11434/api/chat".to_string());
                let body = self.build_openai_request(&request);
                (
                    SseFormat::Ollama,
                    url,
                    body,
                    ("Authorization".to_string(), String::new()),
                )
            }
            ProviderType::Custom => {
                let url = self
                    .config
                    .base_url
                    .clone()
                    .ok_or_else(|| ApiError::Provider("Custom provider requires base_url".into()))?;
                let body = self.build_openai_request(&request);
                (
                    SseFormat::OpenAI,
                    url,
                    body,
                    (
                        "Authorization".to_string(),
                        format!("Bearer {}", self.config.api_key),
                    ),
                )
            }
        };

        // Enable streaming in the request body
        body["stream"] = json!(true);

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("anthropic-version", "2023-06-01")
            .header(&auth_header.0, &auth_header.1)
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
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

                        // Process complete lines
                        while let Some(newline_pos) = buffer.find('\n') {
                            let line = buffer[..newline_pos].to_string();
                            buffer = buffer[newline_pos + 1..].to_string();

                            if let Some(chunk) = parse_sse_line(&line, sse_format) {
                                let is_done = matches!(chunk, StreamChunk::Done(_));
                                let is_error = matches!(chunk, StreamChunk::Error(_));
                                if tx.send(chunk).await.is_err() {
                                    return; // Receiver dropped
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

            // If stream ends without a Done chunk, send one
            let _ = tx.send(StreamChunk::Done(TokenUsage::default())).await;
        });

        Ok(rx)
    }
}
