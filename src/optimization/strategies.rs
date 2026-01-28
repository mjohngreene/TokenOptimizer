//! Optimization strategy implementations

use super::{OptimizationConfig, OptimizationStats, StrategyType};
use crate::agents::{LocalAgent, LocalTask, LocalTaskResult, PreprocessingAgent};
use crate::api::ApiRequest;
use std::collections::HashSet;

/// Prompt optimizer that applies various strategies
pub struct PromptOptimizer {
    config: OptimizationConfig,
    local_agent: Option<LocalAgent>,
}

impl PromptOptimizer {
    pub fn new(config: OptimizationConfig, local_agent: Option<LocalAgent>) -> Self {
        Self {
            config,
            local_agent,
        }
    }

    /// Optimize an API request using configured strategies
    pub async fn optimize(
        &self,
        request: ApiRequest,
    ) -> Result<(ApiRequest, OptimizationStats), anyhow::Error> {
        let original_tokens = self.estimate_tokens(&request);
        let mut optimized = request;
        let mut applied_strategies = Vec::new();

        for strategy in &self.config.strategies {
            optimized = match strategy {
                StrategyType::StripWhitespace => {
                    applied_strategies.push("strip_whitespace".to_string());
                    self.strip_whitespace(optimized)
                }
                StrategyType::RemoveComments => {
                    applied_strategies.push("remove_comments".to_string());
                    self.remove_comments(optimized)
                }
                StrategyType::TruncateContext => {
                    applied_strategies.push("truncate_context".to_string());
                    self.truncate_context(optimized)
                }
                StrategyType::Abbreviate => {
                    applied_strategies.push("abbreviate".to_string());
                    self.abbreviate(optimized)
                }
                StrategyType::LlmCompress => {
                    if let Some(agent) = &self.local_agent {
                        applied_strategies.push("llm_compress".to_string());
                        self.llm_compress(optimized, agent).await?
                    } else {
                        optimized
                    }
                }
                StrategyType::RelevanceFilter => {
                    if let Some(agent) = &self.local_agent {
                        applied_strategies.push("relevance_filter".to_string());
                        self.relevance_filter(optimized, agent).await?
                    } else {
                        optimized
                    }
                }
                StrategyType::ExtractSignatures => {
                    applied_strategies.push("extract_signatures".to_string());
                    self.extract_signatures(optimized)
                }
                StrategyType::Deduplicate => {
                    applied_strategies.push("deduplicate".to_string());
                    self.deduplicate(optimized)
                }
            };

            // Check if we've hit target
            if let Some(target) = self.config.target_tokens {
                if self.estimate_tokens(&optimized) <= target {
                    break;
                }
            }
        }

        let optimized_tokens = self.estimate_tokens(&optimized);
        let mut stats = OptimizationStats::new(original_tokens, optimized_tokens);
        stats.strategies_applied = applied_strategies;

        Ok((optimized, stats))
    }

    fn estimate_tokens(&self, request: &ApiRequest) -> usize {
        let mut total = 0;

        if let Some(system) = &request.system {
            total += system.len() / 4;
        }

        for ctx in &request.context {
            total += ctx.name.len() / 4;
            total += ctx.content.len() / 4;
        }

        total += request.task.len() / 4;

        total
    }

    fn strip_whitespace(&self, mut request: ApiRequest) -> ApiRequest {
        // Strip from context items
        for item in &mut request.context {
            if !self.config.preserve_code_blocks {
                item.content = strip_whitespace(&item.content);
            } else {
                // Only strip whitespace outside code blocks
                item.content = strip_whitespace_preserve_code(&item.content);
            }
        }

        // Strip from task
        request.task = collapse_whitespace(&request.task);

        request
    }

    fn remove_comments(&self, mut request: ApiRequest) -> ApiRequest {
        for item in &mut request.context {
            item.content = remove_code_comments(&item.content);
        }
        request
    }

    fn truncate_context(&self, mut request: ApiRequest) -> ApiRequest {
        let target = self.config.target_tokens.unwrap_or(4000);
        let chars_per_item = (target * 4) / request.context.len().max(1);

        for item in &mut request.context {
            if item.content.len() > chars_per_item {
                // Try to truncate at a logical boundary
                item.content = smart_truncate(&item.content, chars_per_item);
            }
        }

        request
    }

    fn abbreviate(&self, mut request: ApiRequest) -> ApiRequest {
        // Common abbreviations for coding contexts
        let abbreviations = [
            ("function", "fn"),
            ("return", "ret"),
            ("string", "str"),
            ("number", "num"),
            ("boolean", "bool"),
            ("undefined", "undef"),
            ("parameter", "param"),
            ("argument", "arg"),
            ("configuration", "config"),
            ("implementation", "impl"),
            ("documentation", "docs"),
        ];

        // Only abbreviate in task description, not in code
        for (long, short) in abbreviations {
            request.task = request.task.replace(long, short);
        }

        request
    }

    async fn llm_compress(
        &self,
        mut request: ApiRequest,
        agent: &LocalAgent,
    ) -> Result<ApiRequest, anyhow::Error> {
        if !request.context.is_empty() {
            let task = LocalTask::CompressContext {
                items: request.context.clone(),
            };

            if let LocalTaskResult::CompressedContext(compressed) = agent.process(task).await? {
                request.context = compressed;
            }
        }

        Ok(request)
    }

    async fn relevance_filter(
        &self,
        mut request: ApiRequest,
        agent: &LocalAgent,
    ) -> Result<ApiRequest, anyhow::Error> {
        if !request.context.is_empty() {
            let task = LocalTask::ScoreRelevance {
                task: request.task.clone(),
                items: request.context.clone(),
            };

            if let LocalTaskResult::RelevanceScores(scores) = agent.process(task).await? {
                // Filter and reorder by relevance
                let mut scored_items: Vec<_> = request
                    .context
                    .into_iter()
                    .zip(scores.into_iter())
                    .map(|(mut item, (_, score))| {
                        item.relevance = Some(score);
                        item
                    })
                    .filter(|item| item.relevance.unwrap_or(0.0) >= 0.3)
                    .collect();

                scored_items.sort_by(|a, b| {
                    b.relevance
                        .partial_cmp(&a.relevance)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

                request.context = scored_items;
            }
        }

        Ok(request)
    }

    fn extract_signatures(&self, mut request: ApiRequest) -> ApiRequest {
        for item in &mut request.context {
            item.content = extract_function_signatures(&item.content);
        }
        request
    }

    fn deduplicate(&self, mut request: ApiRequest) -> ApiRequest {
        let mut seen = HashSet::new();
        request.context.retain(|item| {
            let hash = simple_hash(&item.content);
            seen.insert(hash)
        });
        request
    }
}

/// Strategy trait for custom strategies
pub trait OptimizationStrategy: Send + Sync {
    fn name(&self) -> &str;
    fn apply(&self, request: ApiRequest) -> ApiRequest;
}

// Helper functions

fn strip_whitespace(text: &str) -> String {
    text.lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn strip_whitespace_preserve_code(text: &str) -> String {
    // Simple approach: don't strip lines that look like code
    text.lines()
        .map(|line| {
            if line.trim().is_empty() {
                ""
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn collapse_whitespace(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut last_was_space = false;

    for c in text.chars() {
        if c.is_whitespace() {
            if !last_was_space {
                result.push(' ');
                last_was_space = true;
            }
        } else {
            result.push(c);
            last_was_space = false;
        }
    }

    result.trim().to_string()
}

fn remove_code_comments(code: &str) -> String {
    let mut result = String::new();
    let mut in_multiline_comment = false;
    let mut in_string = false;
    let mut string_char = '"';

    let chars: Vec<char> = code.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if in_multiline_comment {
            if i + 1 < len && chars[i] == '*' && chars[i + 1] == '/' {
                in_multiline_comment = false;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }

        if in_string {
            result.push(chars[i]);
            if chars[i] == string_char && (i == 0 || chars[i - 1] != '\\') {
                in_string = false;
            }
            i += 1;
            continue;
        }

        if chars[i] == '"' || chars[i] == '\'' {
            in_string = true;
            string_char = chars[i];
            result.push(chars[i]);
            i += 1;
            continue;
        }

        if i + 1 < len && chars[i] == '/' && chars[i + 1] == '/' {
            // Skip to end of line
            while i < len && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }

        if i + 1 < len && chars[i] == '/' && chars[i + 1] == '*' {
            in_multiline_comment = true;
            i += 2;
            continue;
        }

        if chars[i] == '#' && (i == 0 || chars[i - 1] == '\n') {
            // Python/shell style comment
            while i < len && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }

        result.push(chars[i]);
        i += 1;
    }

    result
}

fn smart_truncate(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        return text.to_string();
    }

    // Try to truncate at function/class boundary
    let truncation_points = ["\n\nfn ", "\n\nfunc ", "\n\ndef ", "\n\nclass ", "\n\nimpl "];

    for point in truncation_points {
        if let Some(pos) = text[..max_chars.min(text.len())].rfind(point) {
            if pos > max_chars / 2 {
                return format!("{}...[truncated]", &text[..pos]);
            }
        }
    }

    // Fallback to line boundary
    if let Some(pos) = text[..max_chars.min(text.len())].rfind('\n') {
        return format!("{}...[truncated]", &text[..pos]);
    }

    format!("{}...[truncated]", &text[..max_chars])
}

fn extract_function_signatures(code: &str) -> String {
    let mut signatures = Vec::new();

    for line in code.lines() {
        let trimmed = line.trim();

        // Rust functions
        if trimmed.starts_with("pub fn ")
            || trimmed.starts_with("fn ")
            || trimmed.starts_with("async fn ")
            || trimmed.starts_with("pub async fn ")
        {
            if let Some(end) = trimmed.find('{') {
                signatures.push(format!("{} {{ ... }}", &trimmed[..end].trim()));
            } else {
                signatures.push(trimmed.to_string());
            }
        }

        // Python functions
        if trimmed.starts_with("def ") || trimmed.starts_with("async def ") {
            signatures.push(trimmed.to_string());
        }

        // JavaScript/TypeScript functions
        if trimmed.starts_with("function ")
            || trimmed.contains("=> {")
            || trimmed.starts_with("export function ")
            || trimmed.starts_with("export const ")
        {
            signatures.push(trimmed.to_string());
        }

        // Struct/class definitions
        if trimmed.starts_with("struct ")
            || trimmed.starts_with("pub struct ")
            || trimmed.starts_with("class ")
            || trimmed.starts_with("export class ")
        {
            if let Some(end) = trimmed.find('{') {
                signatures.push(format!("{} {{ ... }}", &trimmed[..end].trim()));
            } else {
                signatures.push(trimmed.to_string());
            }
        }
    }

    signatures.join("\n")
}

fn simple_hash(text: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}
