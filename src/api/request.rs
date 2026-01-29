//! API request structures

use crate::cache::CacheControl;
use serde::{Deserialize, Serialize};

/// A message in a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

/// Request to send to an API coding agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRequest {
    /// System prompt/instructions
    pub system: Option<String>,

    /// Cache control for system prompt (enables caching if set)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_cache_control: Option<CacheControl>,

    /// Conversation messages
    pub messages: Vec<Message>,

    /// Context files to include (path -> content)
    pub context: Vec<ContextItem>,

    /// The actual task/question
    pub task: String,

    /// Optional constraints for the response
    pub constraints: Option<RequestConstraints>,

    /// Positions where cache breakpoints should be inserted
    #[serde(skip)]
    pub cache_breakpoints: Vec<usize>,
}

/// A piece of context (file, snippet, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextItem {
    pub name: String,
    pub content: String,
    pub item_type: ContextType,
    /// Relevance score from local agent (0.0 - 1.0)
    pub relevance: Option<f32>,
    /// Cache control for this context item
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    /// Whether this item should be treated as static (cacheable)
    #[serde(default)]
    pub is_static: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextType {
    File,
    Snippet,
    Documentation,
    Error,
    Output,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestConstraints {
    /// Maximum tokens for context
    pub max_context_tokens: Option<u32>,
    /// Maximum tokens for response
    pub max_response_tokens: Option<u32>,
    /// Prefer concise responses
    pub prefer_concise: bool,
}

impl ApiRequest {
    pub fn new(task: String) -> Self {
        Self {
            system: None,
            system_cache_control: None,
            messages: Vec::new(),
            context: Vec::new(),
            task,
            constraints: None,
            cache_breakpoints: Vec::new(),
        }
    }

    pub fn with_system(mut self, system: String) -> Self {
        self.system = Some(system);
        self
    }

    /// Set system prompt with caching enabled
    pub fn with_cached_system(mut self, system: String) -> Self {
        self.system = Some(system);
        self.system_cache_control = Some(CacheControl::default());
        self
    }

    pub fn with_context(mut self, context: Vec<ContextItem>) -> Self {
        self.context = context;
        self
    }

    pub fn with_constraints(mut self, constraints: RequestConstraints) -> Self {
        self.constraints = Some(constraints);
        self
    }

    /// Add cache breakpoints at specified context indices
    pub fn with_cache_breakpoints(mut self, breakpoints: Vec<usize>) -> Self {
        self.cache_breakpoints = breakpoints;
        self
    }

    /// Enable caching for the system prompt
    pub fn enable_system_cache(&mut self) {
        self.system_cache_control = Some(CacheControl::default());
    }

    /// Mark a context item as cacheable (static)
    pub fn mark_context_static(&mut self, index: usize) {
        if let Some(item) = self.context.get_mut(index) {
            item.is_static = true;
            item.cache_control = Some(CacheControl::default());
        }
    }

    /// Reorder context to put static items first (for optimal caching)
    pub fn optimize_for_caching(&mut self) {
        // Sort by is_static (true first) while preserving relative order
        self.context.sort_by_key(|item| !item.is_static);

        // Add cache breakpoint after last static item
        if let Some(last_static_idx) = self.context.iter().rposition(|item| item.is_static) {
            self.cache_breakpoints = vec![last_static_idx];
        }
    }
}
