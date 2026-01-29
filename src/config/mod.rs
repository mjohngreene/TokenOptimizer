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
    /// Primary provider configuration (Venice.ai)
    pub primary: PrimaryProviderSettings,

    /// Fallback provider configuration
    pub fallback: FallbackProviderSettings,

    /// Local LLM (Ollama) configuration for preprocessing
    pub local: LocalLLMSettings,

    /// Orchestration settings
    pub orchestrator: OrchestratorSettings,

    /// Optimization settings
    pub optimization: OptimizationSettings,

    /// Cache settings
    pub cache: CacheSettings,

    /// Legacy Venice settings (for backward compatibility)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub venice: Option<VeniceSettings>,

    /// Legacy Claude settings (for backward compatibility)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claude: Option<ClaudeSettings>,

    /// Legacy OpenAI settings (for backward compatibility)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub openai: Option<OpenAISettings>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            primary: PrimaryProviderSettings::default(),
            fallback: FallbackProviderSettings::default(),
            local: LocalLLMSettings::default(),
            orchestrator: OrchestratorSettings::default(),
            optimization: OptimizationSettings::default(),
            cache: CacheSettings::default(),
            // Legacy fields
            venice: None,
            claude: None,
            openai: None,
        }
    }
}

/// Primary provider settings (Venice.ai)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PrimaryProviderSettings {
    /// Provider type (currently only "venice" supported)
    pub provider: String,

    /// API key (can also use VENICE_API_KEY env var)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// Base URL for the API
    pub base_url: String,

    /// Model to use for code generation
    /// Venice options: llama-3.3-70b, deepseek-coder-v2, qwen-2.5-coder-32b, etc.
    pub model: String,

    /// Minimum USD balance before triggering fallback
    pub min_balance_usd: f64,

    /// Minimum Diem balance before triggering fallback
    pub min_balance_diem: f64,

    /// Maximum tokens for responses
    pub max_tokens: u32,

    /// Temperature for generation (0.0 - 1.0)
    pub temperature: f32,

    /// Whether this provider is enabled
    pub enabled: bool,
}

impl Default for PrimaryProviderSettings {
    fn default() -> Self {
        Self {
            provider: "venice".to_string(),
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

/// Fallback provider settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FallbackProviderSettings {
    /// Provider type: "claude", "openai", or "none"
    pub provider: String,

    /// API key (can also use ANTHROPIC_API_KEY or OPENAI_API_KEY env var)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// Base URL for the API
    pub base_url: String,

    /// Model to use for code generation
    /// Claude options: claude-sonnet-4-20250514, claude-opus-4-20250514
    /// OpenAI options: gpt-4, gpt-4-turbo, gpt-4o
    pub model: String,

    /// Maximum tokens for responses
    pub max_tokens: u32,

    /// Temperature for generation (0.0 - 1.0)
    pub temperature: f32,

    /// Whether this provider is enabled
    pub enabled: bool,

    /// Use Claude Code CLI instead of API (Claude only)
    pub use_cli: bool,

    /// Path to Claude Code CLI (if not in PATH)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli_path: Option<String>,
}

impl Default for FallbackProviderSettings {
    fn default() -> Self {
        Self {
            provider: "claude".to_string(),
            api_key: None,
            base_url: "https://api.anthropic.com/v1".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: 4096,
            temperature: 0.7,
            enabled: true,
            use_cli: true,
            cli_path: None,
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
        // Primary provider (Venice)
        if let Ok(key) = std::env::var("VENICE_API_KEY") {
            self.primary.api_key = Some(key);
        }
        if let Ok(url) = std::env::var("VENICE_BASE_URL") {
            self.primary.base_url = url;
        }
        if let Ok(model) = std::env::var("VENICE_MODEL") {
            self.primary.model = model;
        }

        // Fallback provider
        // Check for Claude first, then OpenAI
        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            if self.fallback.provider == "claude" {
                self.fallback.api_key = Some(key);
            }
        }
        if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            if self.fallback.provider == "openai" {
                self.fallback.api_key = Some(key);
            }
        }
        if let Ok(url) = std::env::var("FALLBACK_BASE_URL") {
            self.fallback.base_url = url;
        }
        if let Ok(model) = std::env::var("FALLBACK_MODEL") {
            self.fallback.model = model;
        }

        // Local LLM
        if let Ok(url) = std::env::var("OLLAMA_URL") {
            self.local.url = url;
        }
        if let Ok(model) = std::env::var("OLLAMA_MODEL") {
            self.local.model = model;
        }

        // Migrate legacy config sections if present
        self = self.migrate_legacy();

        self
    }

    /// Migrate legacy venice/claude sections to new primary/fallback structure
    fn migrate_legacy(mut self) -> Self {
        // Migrate legacy venice settings to primary
        if let Some(venice) = self.venice.take() {
            if venice.api_key.is_some() {
                self.primary.api_key = venice.api_key;
            }
            self.primary.base_url = venice.base_url;
            self.primary.model = venice.model;
            self.primary.min_balance_usd = venice.min_balance_usd;
            self.primary.min_balance_diem = venice.min_balance_diem;
            self.primary.max_tokens = venice.max_tokens;
            self.primary.temperature = venice.temperature;
            self.primary.enabled = venice.enabled;
        }

        // Migrate legacy claude settings to fallback
        if let Some(claude) = self.claude.take() {
            if claude.api_key.is_some() {
                self.fallback.api_key = claude.api_key;
            }
            self.fallback.provider = "claude".to_string();
            self.fallback.base_url = claude.base_url;
            self.fallback.model = claude.model;
            self.fallback.max_tokens = claude.max_tokens;
            self.fallback.temperature = claude.temperature;
            self.fallback.enabled = claude.enabled;
            self.fallback.use_cli = claude.use_cli_fallback;
            self.fallback.cli_path = claude.cli_path;
        }

        // Migrate legacy openai settings to fallback if no claude
        if let Some(openai) = self.openai.take() {
            if self.fallback.api_key.is_none() && openai.api_key.is_some() {
                self.fallback.provider = "openai".to_string();
                self.fallback.api_key = openai.api_key;
                self.fallback.base_url = openai.base_url;
                self.fallback.model = openai.model;
                self.fallback.max_tokens = openai.max_tokens;
                self.fallback.temperature = openai.temperature;
            }
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
        let primary_configured = self.primary.enabled
            && (self.primary.api_key.is_some()
                || std::env::var("VENICE_API_KEY").is_ok());

        let fallback_configured = self.fallback.enabled
            && (self.fallback.api_key.is_some()
                || self.fallback_api_key().is_some()
                || (self.fallback.provider == "claude" && self.fallback.use_cli));

        if !primary_configured && !fallback_configured {
            return Err(ConfigError::MissingRequired(
                "At least one provider must be configured (VENICE_API_KEY or ANTHROPIC_API_KEY/OPENAI_API_KEY)".to_string()
            ));
        }

        Ok(())
    }

    /// Get primary provider (Venice) API key (from config or env)
    pub fn primary_api_key(&self) -> Option<String> {
        self.primary
            .api_key
            .clone()
            .or_else(|| std::env::var("VENICE_API_KEY").ok())
    }

    /// Get fallback provider API key (from config or env)
    pub fn fallback_api_key(&self) -> Option<String> {
        self.fallback.api_key.clone().or_else(|| {
            match self.fallback.provider.as_str() {
                "claude" => std::env::var("ANTHROPIC_API_KEY").ok(),
                "openai" => std::env::var("OPENAI_API_KEY").ok(),
                _ => None,
            }
        })
    }

    /// Get Venice API key (legacy alias)
    pub fn venice_api_key(&self) -> Option<String> {
        self.primary_api_key()
    }

    /// Get Claude API key (legacy alias)
    pub fn claude_api_key(&self) -> Option<String> {
        if self.fallback.provider == "claude" {
            self.fallback_api_key()
        } else {
            std::env::var("ANTHROPIC_API_KEY").ok()
        }
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

    // Primary provider (Venice) settings
    pub fn primary_api_key(mut self, key: impl Into<String>) -> Self {
        self.config.primary.api_key = Some(key.into());
        self
    }

    pub fn primary_model(mut self, model: impl Into<String>) -> Self {
        self.config.primary.model = model.into();
        self
    }

    pub fn primary_base_url(mut self, url: impl Into<String>) -> Self {
        self.config.primary.base_url = url.into();
        self
    }

    pub fn primary_min_balance(mut self, usd: f64, diem: f64) -> Self {
        self.config.primary.min_balance_usd = usd;
        self.config.primary.min_balance_diem = diem;
        self
    }

    // Fallback provider settings
    pub fn fallback_provider(mut self, provider: impl Into<String>) -> Self {
        self.config.fallback.provider = provider.into();
        self
    }

    pub fn fallback_api_key(mut self, key: impl Into<String>) -> Self {
        self.config.fallback.api_key = Some(key.into());
        self
    }

    pub fn fallback_model(mut self, model: impl Into<String>) -> Self {
        self.config.fallback.model = model.into();
        self
    }

    pub fn fallback_base_url(mut self, url: impl Into<String>) -> Self {
        self.config.fallback.base_url = url.into();
        self
    }

    pub fn fallback_use_cli(mut self, use_cli: bool) -> Self {
        self.config.fallback.use_cli = use_cli;
        self
    }

    // Legacy aliases
    pub fn venice_api_key(self, key: impl Into<String>) -> Self {
        self.primary_api_key(key)
    }

    pub fn venice_model(self, model: impl Into<String>) -> Self {
        self.primary_model(model)
    }

    pub fn claude_api_key(mut self, key: impl Into<String>) -> Self {
        self.config.fallback.provider = "claude".to_string();
        self.config.fallback.api_key = Some(key.into());
        self
    }

    pub fn claude_model(mut self, model: impl Into<String>) -> Self {
        self.config.fallback.provider = "claude".to_string();
        self.config.fallback.model = model.into();
        self
    }

    // Local LLM settings
    pub fn local_llm_url(mut self, url: impl Into<String>) -> Self {
        self.config.local.url = url.into();
        self
    }

    pub fn local_llm_model(mut self, model: impl Into<String>) -> Self {
        self.config.local.model = model.into();
        self
    }

    // General settings
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
        assert_eq!(config.primary.model, "llama-3.3-70b");
        assert_eq!(config.primary.provider, "venice");
        assert_eq!(config.fallback.provider, "claude");
    }

    #[test]
    fn test_config_builder() {
        let config = ConfigBuilder::new()
            .primary_api_key("test-key")
            .primary_model("deepseek-coder-v2")
            .fallback_provider("claude")
            .fallback_model("claude-opus-4-20250514")
            .target_tokens(8000)
            .build();

        assert_eq!(config.primary.api_key, Some("test-key".to_string()));
        assert_eq!(config.primary.model, "deepseek-coder-v2");
        assert_eq!(config.fallback.model, "claude-opus-4-20250514");
        assert_eq!(config.optimization.target_tokens, 8000);
    }

    #[test]
    fn test_example_config() {
        let example = Config::example();
        assert!(example.contains("[primary]"));
        assert!(example.contains("[fallback]"));
    }
}
