//! Streaming response support for API providers

use super::{ApiError, ApiRequest, TokenUsage};
use async_trait::async_trait;
use tokio::sync::mpsc;

/// A chunk of a streaming response
#[derive(Debug, Clone)]
pub enum StreamChunk {
    /// A text delta (partial content)
    TextDelta(String),
    /// Stream completed with final usage stats
    Done(TokenUsage),
    /// An error occurred during streaming
    Error(String),
}

/// Trait for providers that support streaming responses
#[async_trait]
pub trait StreamingProvider: Send + Sync {
    /// Send a request and return a channel of streaming chunks.
    /// The receiver yields TextDelta chunks as they arrive, followed by a Done chunk.
    async fn send_streaming(
        &self,
        request: ApiRequest,
    ) -> Result<mpsc::Receiver<StreamChunk>, ApiError>;
}
