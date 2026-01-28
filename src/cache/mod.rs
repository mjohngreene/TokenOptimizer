//! Cache prompting support for minimizing token costs
//!
//! This module provides tools for structuring prompts to maximize cache hit rates
//! with API providers that support prompt caching (e.g., Anthropic's Claude).
//!
//! ## Cache Prompting Principles
//!
//! 1. **Static content first**: Place cacheable content at the beginning of prompts
//! 2. **Minimum size**: Ensure cacheable sections meet minimum token thresholds (~1024 for Claude)
//! 3. **Stability**: Content that changes invalidates the cache for everything after it
//! 4. **Explicit markers**: Use cache_control blocks to mark cache breakpoints

mod strategy;
mod tracker;

pub use strategy::{BreakpointPosition, CacheOptimizer, CacheOptimizedRequest, CacheableContent, ContentStability};
pub use tracker::{CacheMetrics, CacheSummary, CacheTracker};

use serde::{Deserialize, Serialize};

/// Minimum tokens required for caching (Anthropic requirement)
pub const MIN_CACHE_TOKENS: usize = 1024;

/// Cache control directive for API requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheControl {
    /// Type of cache control
    #[serde(rename = "type")]
    pub control_type: CacheControlType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheControlType {
    /// Mark this as an ephemeral cache breakpoint
    Ephemeral,
}

impl Default for CacheControl {
    fn default() -> Self {
        Self {
            control_type: CacheControlType::Ephemeral,
        }
    }
}

/// Configuration for cache optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Minimum tokens for a cacheable block
    pub min_cache_tokens: usize,
    /// Maximum number of cache breakpoints to use
    pub max_breakpoints: usize,
    /// Whether to automatically reorder content for optimal caching
    pub auto_reorder: bool,
    /// Whether to pad small cacheable sections to meet minimum
    pub pad_to_minimum: bool,
    /// Estimated tokens per character (for size calculations)
    pub tokens_per_char: f32,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            min_cache_tokens: MIN_CACHE_TOKENS,
            max_breakpoints: 4, // Anthropic supports up to 4 cache breakpoints
            auto_reorder: true,
            pad_to_minimum: false,
            tokens_per_char: 0.25, // ~4 chars per token
        }
    }
}

/// Represents the caching potential of content
#[derive(Debug, Clone)]
pub struct CacheAnalysis {
    /// Whether the content meets minimum size for caching
    pub meets_minimum: bool,
    /// Estimated token count
    pub estimated_tokens: usize,
    /// Recommended cache breakpoint positions
    pub breakpoint_positions: Vec<usize>,
    /// Potential savings if cached (as percentage)
    pub potential_savings: f32,
    /// Suggestions for improving cacheability
    pub suggestions: Vec<String>,
}
