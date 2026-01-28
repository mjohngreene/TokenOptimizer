//! Local LLM agent implementation using Ollama

use super::{LocalAgentError, LocalTask, LocalTaskResult, PreprocessingAgent};
use crate::api::{ApiRequest, ContextItem};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Configuration for local LLM agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalAgentConfig {
    /// Ollama server URL
    pub ollama_url: String,
    /// Model to use (e.g., "llama3.2", "qwen2.5-coder", "deepseek-coder")
    pub model: String,
    /// Maximum tokens for context compression
    pub max_compressed_tokens: usize,
    /// Relevance threshold for including context (0.0 - 1.0)
    pub relevance_threshold: f32,
    /// Enable aggressive compression
    pub aggressive_compression: bool,
}

impl Default for LocalAgentConfig {
    fn default() -> Self {
        Self {
            ollama_url: "http://localhost:11434".to_string(),
            model: "llama3.2".to_string(),
            max_compressed_tokens: 2000,
            relevance_threshold: 0.3,
            aggressive_compression: false,
        }
    }
}

/// Local LLM agent using Ollama
pub struct LocalAgent {
    config: LocalAgentConfig,
    client: Client,
}

impl LocalAgent {
    pub fn new(config: LocalAgentConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }

    /// Send a prompt to the local LLM
    async fn query(&self, prompt: &str, system: Option<&str>) -> Result<String, LocalAgentError> {
        let mut body = json!({
            "model": self.config.model,
            "prompt": prompt,
            "stream": false,
            "options": {
                "temperature": 0.1,
                "num_predict": 1024
            }
        });

        if let Some(sys) = system {
            body["system"] = json!(sys);
        }

        let response = self
            .client
            .post(format!("{}/api/generate", self.config.ollama_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| LocalAgentError::Connection(e.to_string()))?;

        if !response.status().is_success() {
            return Err(LocalAgentError::Inference(format!(
                "Ollama returned status: {}",
                response.status()
            )));
        }

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| LocalAgentError::Inference(e.to_string()))?;

        json["response"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| LocalAgentError::Inference("No response field".to_string()))
    }

    async fn compress_context(
        &self,
        items: Vec<ContextItem>,
    ) -> Result<Vec<ContextItem>, LocalAgentError> {
        let mut compressed = Vec::new();

        for item in items {
            let prompt = format!(
                "Compress the following code/text while preserving all essential information needed for coding tasks. \
                Keep function signatures, key logic, imports, and important comments. \
                Remove redundant whitespace and verbose comments.\n\n\
                Content:\n{}\n\n\
                Compressed version:",
                item.content
            );

            let system = "You are a code compression assistant. Output only the compressed code/text, nothing else.";

            let compressed_content = self.query(&prompt, Some(system)).await?;

            compressed.push(ContextItem {
                name: item.name,
                content: compressed_content,
                item_type: item.item_type,
                relevance: item.relevance,
                cache_control: item.cache_control,
                is_static: item.is_static,
            });
        }

        Ok(compressed)
    }

    async fn score_relevance(
        &self,
        task: &str,
        items: Vec<ContextItem>,
    ) -> Result<Vec<(String, f32)>, LocalAgentError> {
        let mut scores = Vec::new();

        for item in items {
            let prompt = format!(
                "Rate how relevant the following content is for this task on a scale of 0.0 to 1.0.\n\n\
                Task: {}\n\n\
                Content ({}):\n{}\n\n\
                Output only a number between 0.0 and 1.0:",
                task,
                item.name,
                // Take first 500 chars for relevance scoring to save local tokens
                &item.content[..item.content.len().min(500)]
            );

            let system = "You are a relevance scoring assistant. Output only a decimal number between 0.0 and 1.0.";

            let response = self.query(&prompt, Some(system)).await?;
            let score: f32 = response.trim().parse().unwrap_or(0.5);
            scores.push((item.name, score.clamp(0.0, 1.0)));
        }

        Ok(scores)
    }

    async fn optimize_prompt(&self, prompt: &str) -> Result<String, LocalAgentError> {
        let query = format!(
            "Rewrite the following prompt to be more concise while preserving all requirements and intent. \
            Remove filler words and redundant phrases.\n\n\
            Original prompt:\n{}\n\n\
            Concise version:",
            prompt
        );

        let system = "You are a prompt optimization assistant. Output only the optimized prompt, nothing else.";

        self.query(&query, Some(system)).await
    }

    async fn extract_key_info(
        &self,
        content: &str,
        file_type: &str,
    ) -> Result<String, LocalAgentError> {
        let prompt = format!(
            "Extract the key information from this {} file that would be most useful for a coding assistant. \
            Include: function/class signatures, imports, type definitions, key logic patterns.\n\n\
            Content:\n{}\n\n\
            Key information:",
            file_type, content
        );

        let system = "You are a code analysis assistant. Output only the extracted key information.";

        self.query(&prompt, Some(system)).await
    }

    async fn minimalize_task(
        &self,
        task: &str,
        context_summary: &str,
    ) -> Result<String, LocalAgentError> {
        let prompt = format!(
            "Given this context summary and task, create a minimal but complete task description.\n\n\
            Context: {}\n\n\
            Original task: {}\n\n\
            Minimal task description:",
            context_summary, task
        );

        let system = "You are a task optimization assistant. Create the most concise task description that preserves all requirements.";

        self.query(&prompt, Some(system)).await
    }
}

#[async_trait]
impl PreprocessingAgent for LocalAgent {
    async fn process(&self, task: LocalTask) -> Result<LocalTaskResult, LocalAgentError> {
        match task {
            LocalTask::CompressContext { items } => {
                let compressed = self.compress_context(items).await?;
                Ok(LocalTaskResult::CompressedContext(compressed))
            }
            LocalTask::ScoreRelevance { task, items } => {
                let scores = self.score_relevance(&task, items).await?;
                Ok(LocalTaskResult::RelevanceScores(scores))
            }
            LocalTask::OptimizePrompt { prompt } => {
                let optimized = self.optimize_prompt(&prompt).await?;
                Ok(LocalTaskResult::OptimizedPrompt(optimized))
            }
            LocalTask::ExtractKeyInfo { content, file_type } => {
                let info = self.extract_key_info(&content, &file_type).await?;
                Ok(LocalTaskResult::ExtractedInfo(info))
            }
            LocalTask::MinimalizeTask {
                task,
                context_summary,
            } => {
                let minimal = self.minimalize_task(&task, &context_summary).await?;
                Ok(LocalTaskResult::MinimalTask(minimal))
            }
        }
    }

    async fn optimize_request(&self, request: ApiRequest) -> Result<ApiRequest, LocalAgentError> {
        let mut optimized = request.clone();

        // Score relevance of context items
        if !request.context.is_empty() {
            let scores = self
                .score_relevance(&request.task, request.context.clone())
                .await?;

            // Filter to relevant items
            let mut relevant_items: Vec<_> = request
                .context
                .into_iter()
                .zip(scores.iter())
                .filter(|(_, (_, score))| *score >= self.config.relevance_threshold)
                .map(|(mut item, (_, score))| {
                    item.relevance = Some(*score);
                    item
                })
                .collect();

            // Sort by relevance (highest first)
            relevant_items.sort_by(|a, b| {
                b.relevance
                    .partial_cmp(&a.relevance)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            // Compress if enabled
            if self.config.aggressive_compression {
                relevant_items = self.compress_context(relevant_items).await?;
            }

            optimized.context = relevant_items;
        }

        // Optimize the task prompt
        if request.task.len() > 200 {
            let optimized_task = self.optimize_prompt(&request.task).await?;
            optimized.task = optimized_task;
        }

        Ok(optimized)
    }

    async fn is_available(&self) -> bool {
        self.client
            .get(format!("{}/api/tags", self.config.ollama_url))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }
}
