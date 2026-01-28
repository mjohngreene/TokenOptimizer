//! Cache optimization strategies for prompt structuring

use super::{CacheAnalysis, CacheConfig};
use crate::api::{ApiRequest, ContextItem, ContextType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Stability classification for content
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentStability {
    /// Content that never changes (system prompts, documentation)
    Static,
    /// Content that changes infrequently (project structure, type definitions)
    SemiStatic,
    /// Content that may change between requests (current file, error messages)
    Dynamic,
    /// Content that always changes (user query, timestamps)
    Volatile,
}

impl ContentStability {
    /// Returns a priority for sorting (lower = more cacheable, should come first)
    pub fn cache_priority(&self) -> u8 {
        match self {
            ContentStability::Static => 0,
            ContentStability::SemiStatic => 1,
            ContentStability::Dynamic => 2,
            ContentStability::Volatile => 3,
        }
    }
}

/// Content wrapper with caching metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheableContent {
    /// The actual content
    pub content: String,
    /// Stability classification
    pub stability: ContentStability,
    /// Optional identifier for tracking cache hits
    pub cache_key: Option<String>,
    /// Whether to insert a cache breakpoint after this content
    pub cache_breakpoint: bool,
    /// Estimated token count (cached for performance)
    #[serde(skip)]
    estimated_tokens: Option<usize>,
}

impl CacheableContent {
    pub fn new(content: String, stability: ContentStability) -> Self {
        Self {
            content,
            stability,
            cache_key: None,
            cache_breakpoint: false,
            estimated_tokens: None,
        }
    }

    pub fn with_cache_key(mut self, key: String) -> Self {
        self.cache_key = Some(key);
        self
    }

    pub fn with_breakpoint(mut self) -> Self {
        self.cache_breakpoint = true;
        self
    }

    pub fn estimate_tokens(&mut self, tokens_per_char: f32) -> usize {
        if self.estimated_tokens.is_none() {
            self.estimated_tokens = Some((self.content.len() as f32 * tokens_per_char) as usize);
        }
        self.estimated_tokens.unwrap()
    }
}

/// Optimizer for cache-aware prompt structuring
pub struct CacheOptimizer {
    config: CacheConfig,
    /// Cache of content hashes to track what's been sent before
    content_cache: HashMap<String, ContentFingerprint>,
}

#[derive(Debug, Clone)]
struct ContentFingerprint {
    hash: u64,
    token_count: usize,
    #[allow(dead_code)]
    last_used: std::time::Instant,
}

impl CacheOptimizer {
    pub fn new(config: CacheConfig) -> Self {
        Self {
            config,
            content_cache: HashMap::new(),
        }
    }

    /// Analyze content for caching potential
    pub fn analyze(&self, content: &str) -> CacheAnalysis {
        let estimated_tokens = (content.len() as f32 * self.config.tokens_per_char) as usize;
        let meets_minimum = estimated_tokens >= self.config.min_cache_tokens;

        let mut suggestions = Vec::new();

        if !meets_minimum {
            let needed = self.config.min_cache_tokens - estimated_tokens;
            suggestions.push(format!(
                "Content is ~{} tokens short of minimum cache size ({}). Consider combining with other static content.",
                needed, self.config.min_cache_tokens
            ));
        }

        // Calculate potential savings (cached tokens are ~90% cheaper)
        let potential_savings = if meets_minimum { 0.9 } else { 0.0 };

        // Find good breakpoint positions (after major sections)
        let breakpoint_positions = self.find_breakpoint_positions(content);

        CacheAnalysis {
            meets_minimum,
            estimated_tokens,
            breakpoint_positions,
            potential_savings,
            suggestions,
        }
    }

    /// Optimize an API request for cache efficiency
    pub fn optimize_request(&mut self, mut request: ApiRequest) -> CacheOptimizedRequest {
        let mut sections = Vec::new();
        let mut total_static_tokens = 0;
        let mut total_dynamic_tokens = 0;

        // Classify system prompt as static
        if let Some(system) = &request.system {
            let tokens = (system.len() as f32 * self.config.tokens_per_char) as usize;
            total_static_tokens += tokens;
            sections.push(CacheableContent::new(system.clone(), ContentStability::Static));
        }

        // Classify context items by type
        let mut static_context = Vec::new();
        let mut semi_static_context = Vec::new();
        let mut dynamic_context = Vec::new();

        for item in request.context.drain(..) {
            let stability = self.classify_context(&item);
            let tokens = (item.content.len() as f32 * self.config.tokens_per_char) as usize;

            match stability {
                ContentStability::Static => {
                    total_static_tokens += tokens;
                    static_context.push((item, stability));
                }
                ContentStability::SemiStatic => {
                    total_static_tokens += tokens;
                    semi_static_context.push((item, stability));
                }
                _ => {
                    total_dynamic_tokens += tokens;
                    dynamic_context.push((item, stability));
                }
            }
        }

        // Reorder if enabled: static first, then semi-static, then dynamic
        if self.config.auto_reorder {
            let mut reordered = Vec::new();
            reordered.extend(static_context);
            reordered.extend(semi_static_context);
            reordered.extend(dynamic_context);

            request.context = reordered.into_iter().map(|(item, _)| item).collect();
        }

        // Determine cache breakpoint positions
        let breakpoints = self.calculate_breakpoints(&request, total_static_tokens);

        // Task is always volatile
        let task_tokens = (request.task.len() as f32 * self.config.tokens_per_char) as usize;
        total_dynamic_tokens += task_tokens;

        CacheOptimizedRequest {
            request,
            breakpoints,
            static_tokens: total_static_tokens,
            dynamic_tokens: total_dynamic_tokens,
            estimated_cache_savings: if total_static_tokens >= self.config.min_cache_tokens {
                (total_static_tokens as f32 * 0.9) as usize
            } else {
                0
            },
        }
    }

    /// Classify a context item's stability
    fn classify_context(&self, item: &ContextItem) -> ContentStability {
        match item.item_type {
            // Documentation is typically static
            ContextType::Documentation => ContentStability::Static,

            // Files depend on their content patterns
            ContextType::File => {
                // Type definition files are semi-static
                if item.name.ends_with(".d.ts")
                    || item.name.ends_with("types.rs")
                    || item.name.ends_with("types.py")
                    || item.name.ends_with("schema.prisma")
                    || item.name.contains("interface")
                {
                    ContentStability::SemiStatic
                }
                // Config files are semi-static
                else if item.name.ends_with(".json")
                    || item.name.ends_with(".toml")
                    || item.name.ends_with(".yaml")
                    || item.name.ends_with(".yml")
                {
                    ContentStability::SemiStatic
                }
                // Regular code files are dynamic
                else {
                    ContentStability::Dynamic
                }
            }

            // Snippets are typically from current work - dynamic
            ContextType::Snippet => ContentStability::Dynamic,

            // Errors and output are volatile
            ContextType::Error | ContextType::Output => ContentStability::Volatile,
        }
    }

    /// Calculate optimal cache breakpoint positions
    fn calculate_breakpoints(&self, request: &ApiRequest, static_tokens: usize) -> Vec<BreakpointPosition> {
        let mut breakpoints = Vec::new();

        // Only add breakpoints if we have enough static content
        if static_tokens < self.config.min_cache_tokens {
            return breakpoints;
        }

        // Add breakpoint after system prompt if it's large enough
        if let Some(system) = &request.system {
            let system_tokens = (system.len() as f32 * self.config.tokens_per_char) as usize;
            if system_tokens >= self.config.min_cache_tokens {
                breakpoints.push(BreakpointPosition::AfterSystem);
            }
        }

        // Add breakpoint after static context if we haven't hit the limit
        if breakpoints.len() < self.config.max_breakpoints {
            let mut cumulative_tokens = 0;
            let mut last_static_idx = None;

            for (idx, item) in request.context.iter().enumerate() {
                let stability = self.classify_context(item);
                let tokens = (item.content.len() as f32 * self.config.tokens_per_char) as usize;
                cumulative_tokens += tokens;

                if stability == ContentStability::Static || stability == ContentStability::SemiStatic {
                    if cumulative_tokens >= self.config.min_cache_tokens {
                        last_static_idx = Some(idx);
                    }
                } else {
                    // We've hit dynamic content
                    break;
                }
            }

            if let Some(idx) = last_static_idx {
                breakpoints.push(BreakpointPosition::AfterContext(idx));
            }
        }

        breakpoints
    }

    /// Find natural breakpoint positions in text
    fn find_breakpoint_positions(&self, content: &str) -> Vec<usize> {
        let mut positions = Vec::new();
        let mut current_pos = 0;

        // Look for section markers
        let markers = ["\n## ", "\n# ", "\n---\n", "\n\n\n"];

        for marker in markers {
            for (idx, _) in content.match_indices(marker) {
                if idx > current_pos + 500 {
                    // At least 500 chars between breakpoints
                    positions.push(idx);
                    current_pos = idx;
                }
            }
        }

        positions
    }

    /// Register content as sent (for cache tracking)
    pub fn register_sent(&mut self, cache_key: &str, content: &str) {
        let hash = self.hash_content(content);
        let token_count = (content.len() as f32 * self.config.tokens_per_char) as usize;

        self.content_cache.insert(
            cache_key.to_string(),
            ContentFingerprint {
                hash,
                token_count,
                last_used: std::time::Instant::now(),
            },
        );
    }

    /// Check if content matches what was previously sent
    pub fn check_cache(&self, cache_key: &str, content: &str) -> CacheCheckResult {
        if let Some(fingerprint) = self.content_cache.get(cache_key) {
            let current_hash = self.hash_content(content);
            if current_hash == fingerprint.hash {
                CacheCheckResult::Hit {
                    tokens_saved: fingerprint.token_count,
                }
            } else {
                CacheCheckResult::Modified
            }
        } else {
            CacheCheckResult::Miss
        }
    }

    fn hash_content(&self, content: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        hasher.finish()
    }
}

/// Position for a cache breakpoint
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BreakpointPosition {
    /// Insert breakpoint after system prompt
    AfterSystem,
    /// Insert breakpoint after context item at index
    AfterContext(usize),
    /// Insert breakpoint after all context
    AfterAllContext,
}

/// Result of optimizing a request for caching
#[derive(Debug)]
pub struct CacheOptimizedRequest {
    /// The optimized request
    pub request: ApiRequest,
    /// Positions where cache breakpoints should be inserted
    pub breakpoints: Vec<BreakpointPosition>,
    /// Estimated cacheable (static) tokens
    pub static_tokens: usize,
    /// Estimated non-cacheable (dynamic) tokens
    pub dynamic_tokens: usize,
    /// Estimated token cost savings from caching
    pub estimated_cache_savings: usize,
}

/// Result of checking cache status
#[derive(Debug)]
pub enum CacheCheckResult {
    /// Content matches cached version
    Hit { tokens_saved: usize },
    /// Content has been modified since caching
    Modified,
    /// Content not in cache
    Miss,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_stability_priority() {
        assert!(ContentStability::Static.cache_priority() < ContentStability::Dynamic.cache_priority());
        assert!(ContentStability::SemiStatic.cache_priority() < ContentStability::Volatile.cache_priority());
    }

    #[test]
    fn test_cache_analysis_minimum() {
        let config = CacheConfig::default();
        let optimizer = CacheOptimizer::new(config);

        // Small content should not meet minimum
        let small = "fn main() {}";
        let analysis = optimizer.analyze(small);
        assert!(!analysis.meets_minimum);

        // Large content should meet minimum
        let large = "x".repeat(5000);
        let analysis = optimizer.analyze(&large);
        assert!(analysis.meets_minimum);
    }
}
