//! TokenOptimizer - Coordinate code agents with minimal token consumption
//!
//! This library provides tools for optimizing prompts sent to API-based coding agents
//! by using local LLMs for preprocessing, compression, and context management.
//!
//! ## Key Features
//!
//! - **Prompt Optimization**: Reduce token usage through compression, relevance filtering
//! - **Cache Prompting**: Structure prompts to maximize cache hit rates with providers like Anthropic
//! - **Local LLM Preprocessing**: Use small local models to preprocess before expensive API calls
//! - **Metrics Tracking**: Monitor token usage, costs, and cache efficiency
//! - **Agent Orchestration**: Venice.ai primary with Claude Code fallback

pub mod agents;
pub mod api;
pub mod cache;
pub mod config;
pub mod metrics;
pub mod optimization;
pub mod orchestrator;

pub use agents::{LocalAgent, LocalAgentConfig, PreprocessingAgent};
pub use api::{ApiAgent, ApiRequest, ApiResponse, VeniceConfig, VeniceProvider};
pub use cache::{CacheConfig, CacheOptimizer, CacheTracker, CacheMetrics};
pub use config::{Config, ConfigBuilder, ConfigError};
pub use metrics::TokenMetrics;
pub use orchestrator::{
    ClaudeApiFallback, ClaudeCodeFallback, FallbackProvider, Orchestrator, OrchestratorConfig,
    OrchestratorState, Session, SessionConfig,
};
pub use optimization::{OptimizationStrategy, PromptOptimizer};
