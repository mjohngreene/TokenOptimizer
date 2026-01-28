//! Agent orchestration with automatic fallback
//!
//! This module coordinates between multiple API providers, handling:
//! - Primary provider (Venice.ai) with credit tracking
//! - Automatic fallback to secondary provider (Claude) when credits exhausted
//! - Session handoff with context preservation

mod session;

pub use session::{Session, SessionConfig, SessionState};

use crate::api::{ApiError, ApiProvider, ApiRequest, ApiResponse, VeniceProvider};
use crate::cache::CacheTracker;
use crate::metrics::MetricsTracker;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Fallback provider trait for Claude Code integration
#[async_trait]
pub trait FallbackProvider: Send + Sync {
    /// Execute a request through the fallback provider
    async fn execute(&self, request: ApiRequest) -> Result<ApiResponse, ApiError>;

    /// Check if the fallback provider is available
    async fn is_available(&self) -> bool;

    /// Get provider name for logging
    fn name(&self) -> &str;
}

/// Orchestrator configuration
#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    /// Minimum Venice balance before preemptive fallback
    pub venice_min_balance: f64,
    /// Whether to continue Venice requests after fallback starts
    pub allow_venice_after_fallback: bool,
    /// Maximum retries before fallback
    pub max_retries: u32,
    /// Whether to preserve context during handoff
    pub preserve_context: bool,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            venice_min_balance: 0.10,
            allow_venice_after_fallback: false,
            max_retries: 2,
            preserve_context: true,
        }
    }
}

/// Current orchestrator state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrchestratorState {
    /// Using Venice as primary provider
    UsingVenice,
    /// Venice credits low, preparing for fallback
    VeniceLow,
    /// Transitioned to fallback provider
    UsingFallback,
    /// Both providers unavailable
    Unavailable,
}

/// Orchestrates requests between Venice.ai and Claude Code
pub struct Orchestrator<F: FallbackProvider> {
    config: OrchestratorConfig,
    venice: Arc<VeniceProvider>,
    fallback: Arc<F>,
    state: Arc<RwLock<OrchestratorState>>,
    metrics: Arc<MetricsTracker>,
    cache_tracker: Arc<CacheTracker>,
    /// Accumulated context for session handoff
    session_context: Arc<RwLock<Vec<String>>>,
}

impl<F: FallbackProvider> Orchestrator<F> {
    pub fn new(
        config: OrchestratorConfig,
        venice: VeniceProvider,
        fallback: F,
        metrics: MetricsTracker,
    ) -> Self {
        Self {
            config,
            venice: Arc::new(venice),
            fallback: Arc::new(fallback),
            state: Arc::new(RwLock::new(OrchestratorState::UsingVenice)),
            metrics: Arc::new(metrics),
            cache_tracker: Arc::new(CacheTracker::default()),
            session_context: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Get current orchestrator state
    pub async fn state(&self) -> OrchestratorState {
        self.state.read().await.clone()
    }

    /// Get Venice balance information
    pub async fn venice_balance(&self) -> crate::api::VeniceBalance {
        self.venice.get_balance().await
    }

    /// Execute a request with automatic fallback
    pub async fn execute(&self, request: ApiRequest) -> Result<ApiResponse, ApiError> {
        let current_state = self.state.read().await.clone();

        match current_state {
            OrchestratorState::UsingVenice | OrchestratorState::VeniceLow => {
                self.try_venice_with_fallback(request).await
            }
            OrchestratorState::UsingFallback => {
                self.execute_fallback(request).await
            }
            OrchestratorState::Unavailable => {
                Err(ApiError::Provider("No providers available".to_string()))
            }
        }
    }

    async fn try_venice_with_fallback(&self, request: ApiRequest) -> Result<ApiResponse, ApiError> {
        let mut retries = 0;

        loop {
            match self.venice.send_request(request.clone()).await {
                Ok(response) => {
                    // Check balance after successful request
                    let balance = self.venice.get_balance().await;
                    if balance.balance_usd < self.config.venice_min_balance
                        && balance.balance_diem < self.config.venice_min_balance
                    {
                        info!(
                            "Venice balance low (${:.2} USD, {:.2} Diem), preparing for fallback",
                            balance.balance_usd, balance.balance_diem
                        );
                        *self.state.write().await = OrchestratorState::VeniceLow;
                    }

                    // Track metrics
                    self.metrics.record_request(
                        response.usage.prompt_tokens,
                        response.usage.completion_tokens,
                        0,
                        response.usage.estimated_cost_usd,
                    );

                    // Store response in session context for potential handoff
                    if self.config.preserve_context {
                        self.session_context
                            .write()
                            .await
                            .push(response.content.clone());
                    }

                    return Ok(response);
                }
                Err(ApiError::Provider(msg)) if msg.contains("exhausted") => {
                    warn!("Venice credits exhausted, switching to fallback");
                    *self.state.write().await = OrchestratorState::UsingFallback;
                    return self.execute_fallback_with_handoff(request).await;
                }
                Err(ApiError::RateLimited { retry_after_secs }) => {
                    if retries < self.config.max_retries {
                        retries += 1;
                        info!(
                            "Venice rate limited, retry {}/{} after {}s",
                            retries, self.config.max_retries, retry_after_secs
                        );
                        tokio::time::sleep(tokio::time::Duration::from_secs(retry_after_secs)).await;
                        continue;
                    } else {
                        warn!("Venice rate limit retries exhausted, switching to fallback");
                        return self.execute_fallback(request).await;
                    }
                }
                Err(e) => {
                    if retries < self.config.max_retries {
                        retries += 1;
                        warn!("Venice error: {}, retry {}/{}", e, retries, self.config.max_retries);
                        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                        continue;
                    } else {
                        return Err(e);
                    }
                }
            }
        }
    }

    async fn execute_fallback(&self, request: ApiRequest) -> Result<ApiResponse, ApiError> {
        if !self.fallback.is_available().await {
            *self.state.write().await = OrchestratorState::Unavailable;
            return Err(ApiError::Provider(format!(
                "Fallback provider '{}' unavailable",
                self.fallback.name()
            )));
        }

        info!("Executing request via fallback provider: {}", self.fallback.name());
        self.fallback.execute(request).await
    }

    async fn execute_fallback_with_handoff(&self, request: ApiRequest) -> Result<ApiResponse, ApiError> {
        // Build handoff context
        let session_history = self.session_context.read().await.clone();

        let handoff_note = if self.config.preserve_context && !session_history.is_empty() {
            format!(
                "\n\n[Session handoff from Venice.ai - {} previous responses in context]\n",
                session_history.len()
            )
        } else {
            String::new()
        };

        // Modify request to include handoff context if needed
        let mut handoff_request = request;
        if !handoff_note.is_empty() {
            handoff_request.task = format!("{}{}", handoff_note, handoff_request.task);
        }

        self.execute_fallback(handoff_request).await
    }

    /// Force switch to fallback provider
    pub async fn force_fallback(&self) {
        *self.state.write().await = OrchestratorState::UsingFallback;
    }

    /// Reset to Venice (if credits become available again)
    pub async fn reset_to_venice(&self) {
        if !self.venice.is_exhausted() {
            *self.state.write().await = OrchestratorState::UsingVenice;
        }
    }

    /// Get metrics summary
    pub fn metrics_summary(&self) -> crate::metrics::MetricsSummary {
        self.metrics.summary()
    }

    /// Get cache summary
    pub fn cache_summary(&self) -> crate::cache::CacheSummary {
        self.cache_tracker.summary()
    }
}

/// Claude Code fallback provider implementation
pub struct ClaudeCodeFallback {
    /// Command to invoke Claude Code CLI
    command: String,
    /// Working directory for Claude Code
    working_dir: Option<String>,
}

impl ClaudeCodeFallback {
    pub fn new() -> Self {
        Self {
            command: "claude".to_string(),
            working_dir: None,
        }
    }

    pub fn with_command(mut self, command: String) -> Self {
        self.command = command;
        self
    }

    pub fn with_working_dir(mut self, dir: String) -> Self {
        self.working_dir = Some(dir);
        self
    }
}

impl Default for ClaudeCodeFallback {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FallbackProvider for ClaudeCodeFallback {
    async fn execute(&self, request: ApiRequest) -> Result<ApiResponse, ApiError> {
        use tokio::process::Command;

        // Build the prompt for Claude Code
        let mut prompt = String::new();

        // Add context
        if !request.context.is_empty() {
            prompt.push_str("Context:\n");
            for ctx in &request.context {
                prompt.push_str(&format!("### {}\n```\n{}\n```\n\n", ctx.name, ctx.content));
            }
        }

        // Add the task
        prompt.push_str(&format!("Task: {}", request.task));

        // Execute Claude Code CLI
        let mut cmd = Command::new(&self.command);
        cmd.arg("--print"); // Non-interactive mode
        cmd.arg("--prompt").arg(&prompt);

        if let Some(dir) = &self.working_dir {
            cmd.current_dir(dir);
        }

        let output = cmd.output().await.map_err(|e| {
            ApiError::Provider(format!("Failed to execute Claude Code: {}", e))
        })?;

        if output.status.success() {
            let content = String::from_utf8_lossy(&output.stdout).to_string();

            Ok(ApiResponse {
                content,
                usage: crate::api::TokenUsage::default(), // CLI doesn't report tokens
                model: "claude-code-cli".to_string(),
                truncated: false,
                stop_reason: None,
            })
        } else {
            let error = String::from_utf8_lossy(&output.stderr);
            Err(ApiError::Provider(format!("Claude Code error: {}", error)))
        }
    }

    async fn is_available(&self) -> bool {
        use tokio::process::Command;

        Command::new(&self.command)
            .arg("--version")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn name(&self) -> &str {
        "Claude Code CLI"
    }
}

/// API-based Claude fallback (for when CLI isn't available)
pub struct ClaudeApiFallback {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

impl ClaudeApiFallback {
    pub fn new(api_key: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            model: "claude-sonnet-4-20250514".to_string(),
        }
    }

    pub fn with_model(mut self, model: String) -> Self {
        self.model = model;
        self
    }
}

#[async_trait]
impl FallbackProvider for ClaudeApiFallback {
    async fn execute(&self, request: ApiRequest) -> Result<ApiResponse, ApiError> {
        let mut messages = Vec::new();

        // Add context
        if !request.context.is_empty() {
            let context_text: String = request
                .context
                .iter()
                .map(|c| format!("### {}\n```\n{}\n```", c.name, c.content))
                .collect::<Vec<_>>()
                .join("\n\n");

            messages.push(serde_json::json!({
                "role": "user",
                "content": format!("Context:\n{}", context_text)
            }));
        }

        // Add task
        messages.push(serde_json::json!({
            "role": "user",
            "content": request.task
        }));

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "max_tokens": 4096,
        });

        if let Some(system) = &request.system {
            body["system"] = serde_json::json!(system);
        }

        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if response.status().is_success() {
            let json: serde_json::Value = response.json().await?;

            let content = json["content"][0]["text"]
                .as_str()
                .unwrap_or("")
                .to_string();

            let usage = crate::api::TokenUsage::new(
                json["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32,
                json["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32,
            );

            Ok(ApiResponse {
                content,
                usage,
                model: json["model"].as_str().unwrap_or(&self.model).to_string(),
                truncated: json["stop_reason"].as_str() == Some("max_tokens"),
                stop_reason: None,
            })
        } else {
            let error = response.text().await.unwrap_or_default();
            Err(ApiError::Provider(format!("Claude API error: {}", error)))
        }
    }

    async fn is_available(&self) -> bool {
        !self.api_key.is_empty()
    }

    fn name(&self) -> &str {
        "Claude API"
    }
}
