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
                        self.hybrid_relevance_filter(optimized, agent).await?
                    } else {
                        applied_strategies.push("relevance_filter".to_string());
                        self.keyword_relevance_filter(optimized)
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
            total += count_tokens(system);
        }

        for ctx in &request.context {
            total += count_tokens(&ctx.name);
            total += count_tokens(&ctx.content);
        }

        total += count_tokens(&request.task);

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
        let num_items = request.context.len().max(1);
        let tokens_per_item = target / num_items;

        for item in &mut request.context {
            let item_tokens = count_tokens(&item.content);
            if item_tokens > tokens_per_item {
                // Convert token budget to approximate char budget (multiply by 4)
                let char_budget = tokens_per_item * 4;
                item.content = smart_truncate(&item.content, char_budget);
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

    /// Keyword-only relevance filter (used when no local LLM is available)
    fn keyword_relevance_filter(&self, mut request: ApiRequest) -> ApiRequest {
        if request.context.is_empty() {
            return request;
        }

        let keywords = extract_task_keywords(&request.task);
        if keywords.is_empty() {
            return request;
        }

        for item in &mut request.context {
            let score = keyword_relevance_score(&keywords, &item.content);
            item.relevance = Some(score);
        }

        request.context.retain(|item| item.relevance.unwrap_or(0.0) >= 0.3);
        request.context.sort_by(|a, b| {
            b.relevance
                .partial_cmp(&a.relevance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        request
    }

    /// Hybrid relevance filter combining keyword scoring with LLM scoring
    async fn hybrid_relevance_filter(
        &self,
        mut request: ApiRequest,
        agent: &LocalAgent,
    ) -> Result<ApiRequest, anyhow::Error> {
        if request.context.is_empty() {
            return Ok(request);
        }

        let keywords = extract_task_keywords(&request.task);

        let task = LocalTask::ScoreRelevance {
            task: request.task.clone(),
            items: request.context.clone(),
        };

        if let LocalTaskResult::RelevanceScores(scores) = agent.process(task).await? {
            let base_weight = self.config.keyword_weight;

            let mut scored_items: Vec<_> = request
                .context
                .into_iter()
                .zip(scores.into_iter())
                .map(|(mut item, (_, llm_score))| {
                    let kw_score = if keywords.is_empty() {
                        llm_score
                    } else {
                        let ks = keyword_relevance_score(&keywords, &item.content);
                        blend_scores(ks, llm_score, base_weight)
                    };
                    item.relevance = Some(kw_score);
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

        Ok(request)
    }

    fn extract_signatures(&self, mut request: ApiRequest) -> ApiRequest {
        for item in &mut request.context {
            item.content = extract_function_signatures(&item.content);
        }
        request
    }

    fn deduplicate(&self, mut request: ApiRequest) -> ApiRequest {
        // Stage 1: Exact hash dedup
        let mut seen_exact = HashSet::new();
        request.context.retain(|item| {
            let hash = simple_hash(&item.content);
            seen_exact.insert(hash)
        });

        // Stage 2: Normalized hash dedup (catches whitespace-only differences)
        let mut seen_normalized = HashSet::new();
        request.context.retain(|item| {
            let hash = normalized_hash(&item.content);
            seen_normalized.insert(hash)
        });

        // Stage 3: Line-set similarity dedup
        let mut kept: Vec<crate::api::ContextItem> = Vec::new();
        for item in request.context {
            let dominated = kept.iter().any(|existing| {
                // Length-ratio pre-filter: skip obviously different-sized items
                let len_a = item.content.len();
                let len_b = existing.content.len();
                if len_a == 0 || len_b == 0 {
                    return false;
                }
                let ratio = len_a as f64 / len_b as f64;
                if !(0.5..=2.0).contains(&ratio) {
                    return false;
                }
                is_similar(&item.content, &existing.content, 0.8)
            });
            if !dominated {
                kept.push(item);
            }
        }
        request.context = kept;

        request
    }
}

/// Strategy trait for custom strategies
pub trait OptimizationStrategy: Send + Sync {
    fn name(&self) -> &str;
    fn apply(&self, request: ApiRequest) -> ApiRequest;
}

// ─── Token counting ─────────────────────────────────────────────────────────

/// Count tokens using tiktoken cl100k_base; falls back to len()/4
fn count_tokens(text: &str) -> usize {
    tiktoken_rs::cl100k_base()
        .map(|bpe| bpe.encode_with_special_tokens(text).len())
        .unwrap_or_else(|_| text.len() / 4)
}

// ─── Boundary-aware truncation ───────────────────────────────────────────────

/// Find the best logical boundary position at or before `max_pos`.
///
/// Priority hierarchy (highest first):
/// 1. Code structure boundaries (`\n\nfn `, `\n\npub fn `, `\n\ndef `, `\n\nclass `, `\n\nimpl `)
/// 2. Paragraph break (`\n\n`)
/// 3. Sentence end (`. `, `? `, `! `)
/// 4. Line break (`\n`)
/// 5. Word break (space)
/// 6. Hard cut at max_pos (last resort)
fn find_best_boundary(text: &str, max_pos: usize) -> usize {
    let search_region = &text[..max_pos.min(text.len())];

    // Priority 1: Code structure boundaries
    let code_boundaries = [
        "\n\nfn ",
        "\n\npub fn ",
        "\n\npub async fn ",
        "\n\nasync fn ",
        "\n\ndef ",
        "\n\nasync def ",
        "\n\nclass ",
        "\n\nimpl ",
        "\n\npub struct ",
        "\n\nstruct ",
        "\n\nmod ",
        "\n\npub mod ",
    ];
    let mut best_code: Option<usize> = None;
    for boundary in code_boundaries {
        if let Some(pos) = search_region.rfind(boundary) {
            best_code = Some(best_code.map_or(pos, |prev: usize| prev.max(pos)));
        }
    }
    if let Some(pos) = best_code {
        if pos > max_pos / 4 {
            return pos;
        }
    }

    // Priority 2: Paragraph break
    if let Some(pos) = search_region.rfind("\n\n") {
        if pos > max_pos / 4 {
            return pos;
        }
    }

    // Priority 3: Sentence end
    for pattern in [". ", "? ", "! "] {
        if let Some(pos) = search_region.rfind(pattern) {
            if pos > max_pos / 4 {
                return pos + pattern.len() - 1; // include the punctuation
            }
        }
    }

    // Priority 4: Line break
    if let Some(pos) = search_region.rfind('\n') {
        return pos;
    }

    // Priority 5: Word break
    if let Some(pos) = search_region.rfind(' ') {
        return pos;
    }

    // Priority 6: Hard cut
    max_pos.min(text.len())
}

fn smart_truncate(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        return text.to_string();
    }

    let boundary = find_best_boundary(text, max_chars);
    let truncated = &text[..boundary];

    // Validate with token count: if boundary-based truncation overshoots
    // the token budget by >10%, retry with a tighter char limit
    let target_tokens = max_chars / 4; // approximate token budget
    let actual_tokens = count_tokens(truncated);
    if actual_tokens > target_tokens + target_tokens / 10 {
        // Retry with a proportionally tighter limit
        let tighter_chars = max_chars * target_tokens / actual_tokens.max(1);
        let tighter_boundary = find_best_boundary(text, tighter_chars);
        return format!("{}...[truncated]", &text[..tighter_boundary]);
    }

    format!("{}...[truncated]", truncated)
}

// ─── Whitespace / comment helpers ────────────────────────────────────────────

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

// ─── Signature extraction ────────────────────────────────────────────────────

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

// ─── Hashing & deduplication helpers ─────────────────────────────────────────

fn simple_hash(text: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Normalize text for deduplication: trim each line, drop empty lines, rejoin.
fn normalize_for_dedup(text: &str) -> String {
    text.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Hash of the normalized form (catches whitespace-only differences).
fn normalized_hash(text: &str) -> u64 {
    simple_hash(&normalize_for_dedup(text))
}

/// Line-set Jaccard similarity: |intersection| / |union| >= threshold.
fn is_similar(a: &str, b: &str, threshold: f64) -> bool {
    let set_a: HashSet<&str> = a.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    let set_b: HashSet<&str> = b.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();

    if set_a.is_empty() && set_b.is_empty() {
        return true;
    }

    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();

    if union == 0 {
        return true;
    }

    (intersection as f64 / union as f64) >= threshold
}

// ─── Keyword relevance helpers ───────────────────────────────────────────────

const STOP_WORDS: &[&str] = &[
    "the", "a", "an", "is", "are", "was", "were", "be", "been", "being",
    "have", "has", "had", "do", "does", "did", "will", "would", "could",
    "should", "may", "might", "shall", "can", "need", "must",
    "and", "but", "or", "nor", "not", "so", "yet",
    "in", "on", "at", "to", "for", "of", "with", "by", "from", "as",
    "into", "about", "between", "through", "after", "before",
    "this", "that", "these", "those", "it", "its",
    "i", "me", "my", "we", "our", "you", "your", "he", "she", "they",
    "if", "then", "else", "when", "where", "how", "what", "which", "who",
];

/// Extract meaningful keywords from a task description.
fn extract_task_keywords(task: &str) -> Vec<String> {
    let stop: HashSet<&str> = STOP_WORDS.iter().copied().collect();
    task.split(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|w| w.to_lowercase())
        .filter(|w| w.len() > 2 && !stop.contains(w.as_str()))
        .collect()
}

/// Score how relevant `content` is to the given `keywords`.
///
/// Blends coverage (what fraction of keywords appear) at 70% with
/// density (log-weighted term frequency relative to content size) at 30%.
fn keyword_relevance_score(keywords: &[String], content: &str) -> f32 {
    if keywords.is_empty() {
        return 0.0;
    }

    let lower = content.to_lowercase();
    let content_words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty())
        .collect();

    if content_words.is_empty() {
        return 0.0;
    }

    let mut hits = 0usize;
    let mut total_freq = 0usize;
    for kw in keywords {
        let freq = content_words.iter().filter(|w| **w == kw.as_str()).count();
        if freq > 0 {
            hits += 1;
            total_freq += freq;
        }
    }

    let coverage = hits as f32 / keywords.len() as f32;
    // log-TF density: log2(1 + freq) / log2(1 + content_len)
    let density = (1.0 + total_freq as f64).log2() / (1.0 + content_words.len() as f64).log2();

    coverage * 0.7 + density as f32 * 0.3
}

/// Blend keyword score and LLM score with position-aware weighting.
///
/// - When keyword_score > 0.7: boost keyword weight by 20% (strong keyword match = trust it)
/// - When keyword_score < 0.4: boost LLM weight by 20% (weak keyword match = lean on LLM)
/// - Otherwise: use base_weight as-is
fn blend_scores(keyword_score: f32, llm_score: f32, base_weight: f32) -> f32 {
    let kw_weight = if keyword_score > 0.7 {
        (base_weight + 0.2).min(1.0)
    } else if keyword_score < 0.4 {
        (base_weight - 0.2).max(0.0)
    } else {
        base_weight
    };

    keyword_score * kw_weight + llm_score * (1.0 - kw_weight)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── count_tokens ──

    #[test]
    fn count_tokens_nonempty() {
        let n = count_tokens("hello world");
        assert!(n > 0, "expected >0 tokens, got {n}");
    }

    #[test]
    fn count_tokens_empty() {
        assert_eq!(count_tokens(""), 0);
    }

    // ── find_best_boundary ──

    #[test]
    fn boundary_code_structure() {
        let text = "line one\n\nfn foo() {\n    body\n}\n\nfn bar() {\n    body2\n}";
        let pos = find_best_boundary(text, 40);
        // Should land at the `\n\nfn bar` boundary
        let snippet = &text[..pos];
        assert!(
            snippet.contains("fn foo"),
            "expected code boundary before fn bar, got: {snippet}"
        );
    }

    #[test]
    fn boundary_paragraph() {
        let text = "First paragraph here.\n\nSecond paragraph that is longer and goes on.";
        let pos = find_best_boundary(text, 30);
        assert_eq!(&text[..pos], "First paragraph here.");
    }

    #[test]
    fn boundary_sentence() {
        let text = "This is sentence one. This is sentence two that is really long.";
        // max_pos at 30 => search in "This is sentence one. This is"
        let pos = find_best_boundary(text, 30);
        // Should include the period
        let snippet = &text[..pos];
        assert!(
            snippet.ends_with('.'),
            "expected sentence boundary, got: {snippet}"
        );
    }

    #[test]
    fn boundary_line_break() {
        let text = "aaaa bbbb\ncccc dddd eeee ffff";
        let pos = find_best_boundary(text, 15);
        assert_eq!(&text[..pos], "aaaa bbbb");
    }

    #[test]
    fn boundary_word_break() {
        let text = "abcde fghij";
        let pos = find_best_boundary(text, 8);
        assert_eq!(&text[..pos], "abcde");
    }

    #[test]
    fn boundary_hard_cut() {
        let text = "abcdefghijklmnop";
        let pos = find_best_boundary(text, 10);
        assert_eq!(pos, 10);
    }

    // ── smart_truncate ──

    #[test]
    fn truncate_short_text_passthrough() {
        let text = "short";
        assert_eq!(smart_truncate(text, 100), "short");
    }

    #[test]
    fn truncate_respects_boundary() {
        let text = "First paragraph.\n\nSecond paragraph that is much longer than the budget allows.";
        let result = smart_truncate(text, 30);
        assert!(result.contains("...[truncated]"));
        assert!(result.contains("First paragraph."));
    }

    // ── normalize_for_dedup / normalized_hash ──

    #[test]
    fn normalize_whitespace_invariance() {
        let a = "  hello world  \n\n  foo bar  ";
        let b = "hello world\nfoo bar";
        assert_eq!(normalize_for_dedup(a), normalize_for_dedup(b));
        assert_eq!(normalized_hash(a), normalized_hash(b));
    }

    #[test]
    fn normalize_different_content() {
        let a = "hello world";
        let b = "goodbye world";
        assert_ne!(normalized_hash(a), normalized_hash(b));
    }

    // ── is_similar ──

    #[test]
    fn similar_identical() {
        let text = "line1\nline2\nline3";
        assert!(is_similar(text, text, 0.8));
    }

    #[test]
    fn similar_minor_edit() {
        let a = "line1\nline2\nline3\nline4\nline5";
        let b = "line1\nline2\nline3\nline4\nline6"; // 1 of 5 lines differs
        // Jaccard = 4/6 = 0.667, below 0.8
        assert!(!is_similar(a, b, 0.8));
        // But above 0.5
        assert!(is_similar(a, b, 0.5));
    }

    #[test]
    fn similar_very_different() {
        let a = "alpha\nbeta\ngamma";
        let b = "delta\nepsilon\nzeta";
        assert!(!is_similar(a, b, 0.1));
    }

    // ── extract_task_keywords ──

    #[test]
    fn keywords_filters_stop_words() {
        let kws = extract_task_keywords("Fix the bug in the authentication module");
        assert!(kws.contains(&"fix".to_string()));
        assert!(kws.contains(&"bug".to_string()));
        assert!(kws.contains(&"authentication".to_string()));
        assert!(kws.contains(&"module".to_string()));
        assert!(!kws.contains(&"the".to_string()));
        assert!(!kws.contains(&"in".to_string()));
    }

    #[test]
    fn keywords_short_words_filtered() {
        let kws = extract_task_keywords("do it on me");
        // All are stop words or <=2 chars
        assert!(kws.is_empty());
    }

    // ── keyword_relevance_score ──

    #[test]
    fn relevance_matching_content() {
        let kws = extract_task_keywords("Fix the authentication bug");
        let content = "fn authenticate(user: &str) -> bool { /* bug here */ true }";
        let score = keyword_relevance_score(&kws, content);
        assert!(score > 0.1, "expected positive score, got {score}");
    }

    #[test]
    fn relevance_nonmatching_content() {
        let kws = extract_task_keywords("Fix the authentication bug");
        let content = "fn render_ui(canvas: &Canvas) { draw_rect(0, 0, 100, 100); }";
        let score = keyword_relevance_score(&kws, content);
        assert!(score < 0.3, "expected low score, got {score}");
    }

    #[test]
    fn relevance_empty_keywords() {
        let score = keyword_relevance_score(&[], "some content");
        assert_eq!(score, 0.0);
    }

    // ── blend_scores ──

    #[test]
    fn blend_high_keyword_boosts_keyword() {
        let result = blend_scores(0.8, 0.5, 0.4);
        // keyword_score > 0.7 => kw_weight = 0.6
        let expected = 0.8 * 0.6 + 0.5 * 0.4;
        assert!((result - expected).abs() < 1e-5, "got {result}, expected {expected}");
    }

    #[test]
    fn blend_low_keyword_boosts_llm() {
        let result = blend_scores(0.2, 0.9, 0.4);
        // keyword_score < 0.4 => kw_weight = 0.2
        let expected = 0.2 * 0.2 + 0.9 * 0.8;
        assert!((result - expected).abs() < 1e-5, "got {result}, expected {expected}");
    }

    #[test]
    fn blend_mid_uses_base_weight() {
        let result = blend_scores(0.5, 0.5, 0.4);
        // 0.4 <= 0.5 <= 0.7 => kw_weight = 0.4 (base)
        let expected = 0.5 * 0.4 + 0.5 * 0.6;
        assert!((result - expected).abs() < 1e-5, "got {result}, expected {expected}");
    }
}
