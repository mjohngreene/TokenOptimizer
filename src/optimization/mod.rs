//! Optimization strategies for reducing token consumption

mod strategies;

pub use strategies::{OptimizationStrategy, PromptOptimizer};

use serde::{Deserialize, Serialize};

/// Configuration for optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationConfig {
    /// Target token budget for the prompt
    pub target_tokens: Option<usize>,
    /// Strategies to apply (in order)
    pub strategies: Vec<StrategyType>,
    /// Whether to use local LLM for optimization
    pub use_local_llm: bool,
    /// Preserve code blocks exactly
    pub preserve_code_blocks: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategyType {
    /// Remove redundant whitespace
    StripWhitespace,
    /// Remove comments from code
    RemoveComments,
    /// Truncate long contexts to key sections
    TruncateContext,
    /// Use abbreviations for common patterns
    Abbreviate,
    /// Compress using local LLM
    LlmCompress,
    /// Filter by relevance using local LLM
    RelevanceFilter,
    /// Extract only function signatures from code
    ExtractSignatures,
    /// Deduplicate similar content
    Deduplicate,
}

impl Default for OptimizationConfig {
    fn default() -> Self {
        Self {
            target_tokens: Some(4000),
            strategies: vec![
                StrategyType::StripWhitespace,
                StrategyType::RemoveComments,
                StrategyType::RelevanceFilter,
            ],
            use_local_llm: true,
            preserve_code_blocks: true,
        }
    }
}

/// Statistics about optimization results
#[derive(Debug, Clone, Default)]
pub struct OptimizationStats {
    pub original_tokens: usize,
    pub optimized_tokens: usize,
    pub tokens_saved: usize,
    pub compression_ratio: f32,
    pub strategies_applied: Vec<String>,
}

impl OptimizationStats {
    pub fn new(original: usize, optimized: usize) -> Self {
        let saved = original.saturating_sub(optimized);
        let ratio = if original > 0 {
            optimized as f32 / original as f32
        } else {
            1.0
        };

        Self {
            original_tokens: original,
            optimized_tokens: optimized,
            tokens_saved: saved,
            compression_ratio: ratio,
            strategies_applied: Vec::new(),
        }
    }
}
