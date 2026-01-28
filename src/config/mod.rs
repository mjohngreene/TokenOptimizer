//! Configuration management for TokenOptimizer
//!
//! Supports configuration via:
//! 1. Config file (~/.config/token-optimizer/config.toml)
//! 2. Environment variables (VENICE_API_KEY, ANTHROPIC_API_KEY, etc.)
//! 3. CLI arguments (override file/env settings)

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Config file not found: {0}")]
    NotFound(PathBuf),

    #[error("Failed to read config: {0}")]
    ReadError(#[from] std::io::Error),

    #[error("Failed to parse config: {0}")]
    ParseError(#[from] toml::de::Error),

    #[error("Failed to serialize config: {0}")]
    SerializeError(#[from] toml::ser::Error),

    #[error("Missing required configuration: {0}")]
    MissingRequired(String),
}

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Venice.ai configuration
    pub venice: VeniceSettings,

    /// Claude/Anthropic configuration
    pub claude: ClaudeSettings,

    /// OpenAI configuration (optional)
    pub openai: Option<OpenAISettings>,

    /// Local LLM (Ollama) configuration
    pub local: LocalLLMSettings,

    /// Orchestration settings
    pub orchestrator: OrchestratorSettings,

    /// Optimization settings
    pub optimization: OptimizationSettings,

    /// Cache settings
    pub cache: CacheSettings,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            venice: VeniceSettings::default(),
            claude: ClaudeSettings::default(),
            openai: None,
            local: LocalLLMSettings::default(),
            orchestrator: OrchestratorSettings::default(),
            optimization: OptimizationSettings::default(),
            cache: CacheSettings::default(),
        }
    }
}

/// Venice.ai settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VeniceSettings {
    /// API key (can also use VENICE_API_KEY env var)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// Base URL for Venice API
    pub base_url: String,

    /// Default model to use
    pub model: String,

    /// Minimum USD balance before fallback
    pub min_balance_usd: f64,

    /// Minimum Diem balance before fallback
    pub min_balance_diem: f64,

    /// Maximum tokens for responses
    pub max_tokens: u32,

    /// Temperature for generation
    pub temperature: f32,

    /// Whether Venice is enabled
    pub enabled: bool,
}

impl Default for VeniceSettings {
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: "https://api.venice.ai/api/v1".to_string(),
            model: "llama-3.3-70b".to_string(),
            min_balance_usd: 0.10,
            min_balance_diem: 0.10,
            max_tokens: 4096,
            temperature: 0.7,
            enabled: true,
        }
    }
}

/// Claude/Anthropic settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ClaudeSettings {
    /// API key (can also use ANTHROPIC_API_KEY env var)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// Base URL for Anthropic API
    pub base_url: String,

    /// Default model to use
    pub model: String,

    /// Maximum tokens for responses
    pub max_tokens: u32,

    /// Temperature for generation
    pub temperature: f32,

    /// Whether to use Claude Code CLI as fallback
    pub use_cli_fallback: bool,

    /// Path to Claude Code CLI (if not in PATH)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli_path: Option<String>,

    /// Whether Claude is enabled as fallback
    pub enabled: bool,
}

impl Default for ClaudeSettings {
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: "https://api.anthropic.com/v1".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: 4096,
            temperature: 0.7,
            use_cli_fallback: true,
            cli_path: None,
            enabled: true,
        }
    }
}

/// OpenAI settings (optional)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenAISettings {
    /// API key (can also use OPENAI_API_KEY env var)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// Base URL for OpenAI API
    pub base_url: String,

    /// Default model to use
    pub model: String,

    /// Maximum tokens for responses
    pub max_tokens: u32,

    /// Temperature for generation
    pub temperature: f32,
}

impl Default for OpenAISettings {
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: "https://api.openai.com/v1".to_string(),
            model: "gpt-4".to_string(),
            max_tokens: 4096,
            temperature: 0.7,
        }
    }
}

/// Local LLM (Ollama) settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalLLMSettings {
    /// Ollama server URL
    pub url: String,

    /// Model to use for preprocessing
    pub model: String,

    /// Whether local LLM is enabled
    pub enabled: bool,

    /// Maximum tokens for compressed context
    pub max_compressed_tokens: usize,

    /// Relevance threshold for filtering (0.0 - 1.0)
    pub relevance_threshold: f32,

    /// Enable aggressive compression
    pub aggressive_compression: bool,
}

impl Default for LocalLLMSettings {
    fn default() -> Self {
        Self {
            url: "http://localhost:11434".to_string(),
            model: "llama3.2".to_string(),
            enabled: true,
            max_compressed_tokens: 2000,
            relevance_threshold: 0.3,
            aggressive_compression: false,
        }
    }
}

/// Orchestrator settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OrchestratorSettings {
    /// Primary provider (venice, claude, openai)
    pub primary_provider: String,

    /// Fallback provider (claude, openai, none)
    pub fallback_provider: String,

    /// Maximum retries before fallback
    pub max_retries: u32,

    /// Preserve context during handoff
    pub preserve_context: bool,

    /// Allow primary after fallback triggered
    pub allow_primary_after_fallback: bool,

    /// Session timeout in seconds
    pub session_timeout_secs: u64,

    /// Maximum conversation history to preserve
    pub max_history: usize,
}

impl Default for OrchestratorSettings {
    fn default() -> Self {
        Self {
            primary_provider: "venice".to_string(),
            fallback_provider: "claude".to_string(),
            max_retries: 2,
            preserve_context: true,
            allow_primary_after_fallback: false,
            session_timeout_secs: 3600,
            max_history: 20,
        }
    }
}

/// Optimization settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OptimizationSettings {
    /// Target token budget
    pub target_tokens: usize,

    /// Strategies to apply (in order)
    pub strategies: Vec<String>,

    /// Preserve code blocks during optimization
    pub preserve_code_blocks: bool,

    /// Use local LLM for optimization
    pub use_local_llm: bool,
}

impl Default for OptimizationSettings {
    fn default() -> Self {
        Self {
            target_tokens: 4000,
            strategies: vec![
                "strip_whitespace".to_string(),
                "remove_comments".to_string(),
                "relevance_filter".to_string(),
            ],
            preserve_code_blocks: true,
            use_local_llm: true,
        }
    }
}

/// Cache settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CacheSettings {
    /// Minimum tokens for caching
    pub min_cache_tokens: usize,

    /// Maximum cache breakpoints
    pub max_breakpoints: usize,

    /// Auto-reorder content for optimal caching
    pub auto_reorder: bool,

    /// Enable cache tracking
    pub track_cache: bool,
}

impl Default for CacheSettings {
    fn default() -> Self {
        Self {
            min_cache_tokens: 1024,
            max_breakpoints: 4,
            auto_reorder: true,
            track_cache: true,
        }
    }
}

impl Config {
    /// Get default config file path
    pub fn default_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("token-optimizer")
            .join("config.toml")
    }

    /// Load config from default location
    pub fn load() -> Result<Self, ConfigError> {
        Self::load_from(Self::default_path())
    }

    /// Load config from specific path
    pub fn load_from(path: PathBuf) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Ok(Self::default().with_env_overrides());
        }

        let content = std::fs::read_to_string(&path)?;
        let mut config: Config = toml::from_str(&content)?;

        // Apply environment variable overrides
        config = config.with_env_overrides();

        Ok(config)
    }

    /// Apply environment variable overrides
    pub fn with_env_overrides(mut self) -> Self {
        // Venice
        if let Ok(key) = std::env::var("VENICE_API_KEY") {
            self.venice.api_key = Some(key);
        }
        if let Ok(url) = std::env::var("VENICE_BASE_URL") {
            self.venice.base_url = url;
        }
        if let Ok(model) = std::env::var("VENICE_MODEL") {
            self.venice.model = model;
        }

        // Claude/Anthropic
        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            self.claude.api_key = Some(key);
        }
        if let Ok(url) = std::env::var("ANTHROPIC_BASE_URL") {
            self.claude.base_url = url;
        }
        if let Ok(model) = std::env::var("ANTHROPIC_MODEL") {
            self.claude.model = model;
        }

        // OpenAI
        if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            if self.openai.is_none() {
                self.openai = Some(OpenAISettings::default());
            }
            if let Some(ref mut openai) = self.openai {
                openai.api_key = Some(key);
            }
        }

        // Local LLM
        if let Ok(url) = std::env::var("OLLAMA_URL") {
            self.local.url = url;
        }
        if let Ok(model) = std::env::var("OLLAMA_MODEL") {
            self.local.model = model;
        }

        self
    }

    /// Save config to default location
    pub fn save(&self) -> Result<(), ConfigError> {
        self.save_to(Self::default_path())
    }

    /// Save config to specific path
    pub fn save_to(&self, path: PathBuf) -> Result<(), ConfigError> {
        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;

        Ok(())
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Check if at least one provider is configured
        let venice_configured = self.venice.enabled
            && (self.venice.api_key.is_some()
                || std::env::var("VENICE_API_KEY").is_ok());

        let claude_configured = self.claude.enabled
            && (self.claude.api_key.is_some()
                || std::env::var("ANTHROPIC_API_KEY").is_ok()
                || self.claude.use_cli_fallback);

        if !venice_configured && !claude_configured {
            return Err(ConfigError::MissingRequired(
                "At least one provider must be configured (VENICE_API_KEY or ANTHROPIC_API_KEY)".to_string()
            ));
        }

        Ok(())
    }

    /// Get Venice API key (from config or env)
    pub fn venice_api_key(&self) -> Option<String> {
        self.venice
            .api_key
            .clone()
            .or_else(|| std::env::var("VENICE_API_KEY").ok())
    }

    /// Get Claude API key (from config or env)
    pub fn claude_api_key(&self) -> Option<String> {
        self.claude
            .api_key
            .clone()
            .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
    }

    /// Generate example config content
    pub fn example() -> String {
        let example = Config::default();
        toml::to_string_pretty(&example).unwrap_or_default()
    }
}

/// Builder for creating Config programmatically
pub struct ConfigBuilder {
    config: Config,
}

impl ConfigBuilder {
    pub fn new() -> Self {
        Self {
            config: Config::default(),
        }
    }

    pub fn venice_api_key(mut self, key: impl Into<String>) -> Self {
        self.config.venice.api_key = Some(key.into());
        self
    }

    pub fn venice_model(mut self, model: impl Into<String>) -> Self {
        self.config.venice.model = model.into();
        self
    }

    pub fn claude_api_key(mut self, key: impl Into<String>) -> Self {
        self.config.claude.api_key = Some(key.into());
        self
    }

    pub fn claude_model(mut self, model: impl Into<String>) -> Self {
        self.config.claude.model = model.into();
        self
    }

    pub fn local_llm_url(mut self, url: impl Into<String>) -> Self {
        self.config.local.url = url.into();
        self
    }

    pub fn local_llm_model(mut self, model: impl Into<String>) -> Self {
        self.config.local.model = model.into();
        self
    }

    pub fn primary_provider(mut self, provider: impl Into<String>) -> Self {
        self.config.orchestrator.primary_provider = provider.into();
        self
    }

    pub fn fallback_provider(mut self, provider: impl Into<String>) -> Self {
        self.config.orchestrator.fallback_provider = provider.into();
        self
    }

    pub fn target_tokens(mut self, tokens: usize) -> Self {
        self.config.optimization.target_tokens = tokens;
        self
    }

    pub fn build(self) -> Config {
        self.config
    }
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.venice.model, "llama-3.3-70b");
        assert_eq!(config.orchestrator.primary_provider, "venice");
    }

    #[test]
    fn test_config_builder() {
        let config = ConfigBuilder::new()
            .venice_api_key("test-key")
            .venice_model("deepseek-coder-v2")
            .target_tokens(8000)
            .build();

        assert_eq!(config.venice.api_key, Some("test-key".to_string()));
        assert_eq!(config.venice.model, "deepseek-coder-v2");
        assert_eq!(config.optimization.target_tokens, 8000);
    }

    #[test]
    fn test_example_config() {
        let example = Config::example();
        assert!(example.contains("[venice]"));
        assert!(example.contains("[claude]"));
    }
}
