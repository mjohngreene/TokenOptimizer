//! API response structures

use serde::{Deserialize, Serialize};

/// Response from an API coding agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse {
    /// The generated content
    pub content: String,

    /// Token usage statistics
    pub usage: TokenUsage,

    /// Model that generated the response
    pub model: String,

    /// Whether the response was truncated
    pub truncated: bool,

    /// Stop reason
    pub stop_reason: Option<StopReason>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    /// Tokens in the prompt
    pub prompt_tokens: u32,
    /// Tokens in the response
    pub completion_tokens: u32,
    /// Total tokens used
    pub total_tokens: u32,
    /// Estimated cost in USD (if available)
    pub estimated_cost_usd: Option<f64>,
    /// Tokens written to cache (Anthropic)
    pub cache_creation_tokens: Option<u32>,
    /// Tokens read from cache (Anthropic)
    pub cache_read_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    MaxTokens,
    StopSequence,
    ToolUse,
}

impl TokenUsage {
    pub fn new(prompt_tokens: u32, completion_tokens: u32) -> Self {
        Self {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
            estimated_cost_usd: None,
            cache_creation_tokens: None,
            cache_read_tokens: None,
        }
    }

    /// Create with cache information (for Anthropic responses)
    pub fn with_cache(
        prompt_tokens: u32,
        completion_tokens: u32,
        cache_creation: Option<u32>,
        cache_read: Option<u32>,
    ) -> Self {
        Self {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
            estimated_cost_usd: None,
            cache_creation_tokens: cache_creation,
            cache_read_tokens: cache_read,
        }
    }

    pub fn with_cost(mut self, cost_per_1k_input: f64, cost_per_1k_output: f64) -> Self {
        let input_cost = (self.prompt_tokens as f64 / 1000.0) * cost_per_1k_input;
        let output_cost = (self.completion_tokens as f64 / 1000.0) * cost_per_1k_output;
        self.estimated_cost_usd = Some(input_cost + output_cost);
        self
    }

    /// Calculate cost with cache pricing (Anthropic)
    /// - cache_creation: 25% more than base input price
    /// - cache_read: 90% less than base input price
    pub fn with_cache_cost(
        mut self,
        cost_per_1k_input: f64,
        cost_per_1k_output: f64,
    ) -> Self {
        let base_input_cost = (self.prompt_tokens as f64 / 1000.0) * cost_per_1k_input;
        let output_cost = (self.completion_tokens as f64 / 1000.0) * cost_per_1k_output;

        // Cache creation costs 25% more
        let cache_write_cost = self.cache_creation_tokens
            .map(|t| (t as f64 / 1000.0) * cost_per_1k_input * 1.25)
            .unwrap_or(0.0);

        // Cache read costs 90% less
        let cache_read_cost = self.cache_read_tokens
            .map(|t| (t as f64 / 1000.0) * cost_per_1k_input * 0.10)
            .unwrap_or(0.0);

        self.estimated_cost_usd = Some(base_input_cost + output_cost + cache_write_cost + cache_read_cost);
        self
    }

    /// Calculate tokens saved from cache
    pub fn cache_savings(&self) -> u32 {
        self.cache_read_tokens.unwrap_or(0)
    }

    /// Check if any caching occurred
    pub fn has_cache_activity(&self) -> bool {
        self.cache_creation_tokens.is_some() || self.cache_read_tokens.is_some()
    }
}
