//! Local agent module for prompt preprocessing and optimization
//!
//! This module provides integration with local LLMs (via Ollama or similar)
//! to preprocess and optimize prompts before sending to API agents.

mod local;

pub use local::{LocalAgent, LocalAgentConfig};

use async_trait::async_trait;
use thiserror::Error;

use crate::api::{ApiRequest, ContextItem};

#[derive(Error, Debug)]
pub enum LocalAgentError {
    #[error("Failed to connect to local LLM: {0}")]
    Connection(String),

    #[error("Local LLM error: {0}")]
    Inference(String),

    #[error("Configuration error: {0}")]
    Config(String),
}

/// Tasks that the local agent can perform
#[derive(Debug, Clone)]
pub enum LocalTask {
    /// Compress/summarize context while preserving essential information
    CompressContext { items: Vec<ContextItem> },

    /// Score relevance of context items for a given task
    ScoreRelevance {
        task: String,
        items: Vec<ContextItem>,
    },

    /// Rewrite a prompt to be more concise while preserving intent
    OptimizePrompt { prompt: String },

    /// Extract key information from a file for context
    ExtractKeyInfo { content: String, file_type: String },

    /// Generate a minimal prompt that captures the task requirements
    MinimalizeTask { task: String, context_summary: String },
}

/// Result of local agent processing
#[derive(Debug, Clone)]
pub enum LocalTaskResult {
    CompressedContext(Vec<ContextItem>),
    RelevanceScores(Vec<(String, f32)>),
    OptimizedPrompt(String),
    ExtractedInfo(String),
    MinimalTask(String),
}

/// Trait for local preprocessing agents
#[async_trait]
pub trait PreprocessingAgent: Send + Sync {
    /// Process a task using the local LLM
    async fn process(&self, task: LocalTask) -> Result<LocalTaskResult, LocalAgentError>;

    /// Optimize an API request to reduce token usage
    async fn optimize_request(&self, request: ApiRequest) -> Result<ApiRequest, LocalAgentError>;

    /// Check if the local agent is available
    async fn is_available(&self) -> bool;
}
