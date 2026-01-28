//! API abstraction layer for various coding agent providers

mod client;
mod request;
mod response;
mod venice;

pub use client::ApiAgent;
pub use request::{ApiRequest, ContextItem, ContextType, Message, RequestConstraints, Role};
pub use response::{ApiResponse, StopReason, TokenUsage};
pub use venice::{VeniceBalance, VeniceConfig, VeniceModel, VeniceProvider};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Authentication failed: {0}")]
    Auth(String),

    #[error("Rate limited: retry after {retry_after_secs} seconds")]
    RateLimited { retry_after_secs: u64 },

    #[error("Provider error: {0}")]
    Provider(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Configuration for API providers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    pub provider: ProviderType,
    pub api_key: String,
    pub base_url: Option<String>,
    pub model: String,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderType {
    Claude,
    OpenAI,
    Ollama,
    Custom,
}

/// Trait for API providers
#[async_trait]
pub trait ApiProvider: Send + Sync {
    async fn send_request(&self, request: ApiRequest) -> Result<ApiResponse, ApiError>;
    fn estimate_tokens(&self, text: &str) -> usize;
    fn provider_type(&self) -> ProviderType;
}
