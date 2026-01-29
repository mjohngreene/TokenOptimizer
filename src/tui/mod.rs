//! Interactive terminal UI for TokenOptimizer
//!
//! Provides a Claude Code-style interactive shell with streaming responses,
//! colored output, markdown rendering, and multi-turn conversation support.

pub mod commands;
pub mod prompt;
pub mod renderer;
pub mod spinner;
pub mod theme;

use crate::api::{
    ApiConfig, ApiAgent, ApiProvider, ApiRequest, ContextItem, ContextType, Message,
    ProviderType, Role, StreamChunk, StreamingProvider, TokenUsage,
    VeniceConfig, VeniceProvider,
};
use crate::config::Config;
use crate::metrics::MetricsTracker;

use commands::{parse_command, render_help, ContextAction, SlashCommand};
use prompt::PromptHandler;
use renderer::TerminalRenderer;
use spinner::ThinkingSpinner;

use anyhow::Result;
use crossterm::style::Stylize;
use std::sync::Arc;

/// The active provider used for requests
enum ActiveProvider {
    Venice(Arc<VeniceProvider>),
    Api(Arc<ApiAgent>),
}

impl ActiveProvider {
    fn name(&self) -> &str {
        match self {
            ActiveProvider::Venice(_) => "Venice.ai",
            ActiveProvider::Api(agent) => match agent.provider_type() {
                ProviderType::Claude => "Claude",
                ProviderType::OpenAI => "OpenAI",
                ProviderType::Ollama => "Ollama",
                ProviderType::Custom => "Custom",
            },
        }
    }
}

/// Interactive shell with streaming, markdown, and multi-turn support
pub struct InteractiveShell {
    config: Config,
    provider: ActiveProvider,
    model: String,
    renderer: TerminalRenderer,
    prompt_handler: PromptHandler,
    /// Conversation history (user + assistant messages)
    conversation: Vec<Message>,
    /// Context files attached to the session
    context: Vec<ContextItem>,
    /// Metrics tracker
    metrics: MetricsTracker,
    /// Total tokens in this session
    session_tokens: u64,
    /// Number of turns completed
    turn_count: usize,
}

impl InteractiveShell {
    /// Create a new interactive shell, selecting the best available provider
    pub async fn new(config: Config) -> Result<Self> {
        let (provider, model) = Self::select_provider(&config)?;

        Ok(Self {
            config,
            provider,
            model,
            renderer: TerminalRenderer::new(),
            prompt_handler: PromptHandler::new(),
            conversation: Vec::new(),
            context: Vec::new(),
            metrics: MetricsTracker::new(),
            session_tokens: 0,
            turn_count: 0,
        })
    }

    /// Select the best available provider based on config
    fn select_provider(config: &Config) -> Result<(ActiveProvider, String)> {
        // Try primary (Venice) first
        if config.primary.enabled {
            if let Some(api_key) = config.primary_api_key() {
                let venice_config = VeniceConfig {
                    api_key,
                    model: config.primary.model.clone(),
                    base_url: Some(config.primary.base_url.clone()),
                    min_balance_usd: config.primary.min_balance_usd,
                    min_balance_diem: config.primary.min_balance_diem,
                    max_tokens: Some(config.primary.max_tokens),
                    temperature: Some(config.primary.temperature),
                };
                let model = venice_config.model.clone();
                return Ok((
                    ActiveProvider::Venice(Arc::new(VeniceProvider::new(venice_config))),
                    model,
                ));
            }
        }

        // Try fallback
        if config.fallback.enabled {
            if let Some(api_key) = config.fallback_api_key() {
                let provider_type = match config.fallback.provider.as_str() {
                    "claude" => ProviderType::Claude,
                    "openai" => ProviderType::OpenAI,
                    _ => ProviderType::Custom,
                };
                let base_url = match provider_type {
                    ProviderType::Claude => {
                        Some(format!("{}/messages", config.fallback.base_url))
                    }
                    _ => Some(format!("{}/chat/completions", config.fallback.base_url)),
                };
                let api_config = ApiConfig {
                    provider: provider_type,
                    api_key,
                    base_url,
                    model: config.fallback.model.clone(),
                    max_tokens: Some(config.fallback.max_tokens),
                    temperature: Some(config.fallback.temperature),
                };
                let model = api_config.model.clone();
                return Ok((
                    ActiveProvider::Api(Arc::new(ApiAgent::new(api_config))),
                    model,
                ));
            }
        }

        // Try local Ollama
        if config.local.enabled {
            let api_config = ApiConfig {
                provider: ProviderType::Ollama,
                api_key: String::new(),
                base_url: Some(format!("{}/api/chat", config.local.url)),
                model: config.local.model.clone(),
                max_tokens: None,
                temperature: Some(0.7),
            };
            let model = api_config.model.clone();
            return Ok((
                ActiveProvider::Api(Arc::new(ApiAgent::new(api_config))),
                model,
            ));
        }

        anyhow::bail!(
            "No provider available. Set VENICE_API_KEY, ANTHROPIC_API_KEY, or OPENAI_API_KEY, \
             or ensure Ollama is running locally."
        );
    }

    /// Run the interactive shell main loop
    pub async fn run(&mut self) -> Result<()> {
        self.renderer.render_banner(
            env!("CARGO_PKG_VERSION"),
            self.provider.name(),
            &self.model,
        );

        loop {
            let input = match self.prompt_handler.read_line(self.renderer.prompt_color()) {
                Some(input) => input,
                None => {
                    // EOF (Ctrl+D)
                    self.render_session_summary();
                    break;
                }
            };

            if input.is_empty() {
                continue;
            }

            // Check for slash commands
            if let Some(cmd) = parse_command(&input) {
                match self.handle_command(cmd).await {
                    CommandResult::Continue => continue,
                    CommandResult::Quit => {
                        self.render_session_summary();
                        break;
                    }
                }
            } else {
                // Regular message
                self.process_message(&input).await;
            }
        }

        Ok(())
    }

    /// Handle a slash command
    async fn handle_command(&mut self, cmd: SlashCommand) -> CommandResult {
        match cmd {
            SlashCommand::Help => {
                render_help(&self.renderer);
            }
            SlashCommand::Quit => {
                return CommandResult::Quit;
            }
            SlashCommand::Clear => {
                self.conversation.clear();
                self.renderer.render_success("Conversation history cleared.");
            }
            SlashCommand::Model(name) => {
                if let Some(name) = name {
                    self.model = name.clone();
                    self.renderer
                        .render_success(&format!("Model set to: {}", name));
                    // Rebuild provider with new model
                    if let Err(e) = self.switch_model(&name) {
                        self.renderer.render_error(&format!(
                            "Failed to switch model: {}. Using model name for next request.",
                            e
                        ));
                    }
                } else {
                    self.renderer
                        .render_info(&format!("Current model: {}", self.model));
                }
            }
            SlashCommand::Provider(name) => {
                if let Some(name) = name {
                    match self.switch_provider(&name) {
                        Ok(()) => {
                            self.renderer
                                .render_success(&format!("Switched to provider: {}", self.provider.name()));
                        }
                        Err(e) => {
                            self.renderer.render_error(&format!("{}", e));
                        }
                    }
                } else {
                    self.renderer.render_info(&format!(
                        "Current provider: {} ({})",
                        self.provider.name(),
                        self.model
                    ));
                }
            }
            SlashCommand::Stats => {
                self.render_stats();
            }
            SlashCommand::Status => {
                self.render_status().await;
            }
            SlashCommand::Compact => {
                let before = self.conversation.len();
                self.compact_history();
                self.renderer.render_success(&format!(
                    "Compacted conversation: {} -> {} messages",
                    before,
                    self.conversation.len()
                ));
            }
            SlashCommand::Context(action) => {
                self.handle_context_action(action).await;
            }
        }
        CommandResult::Continue
    }

    /// Process a user message: build request, stream response, update history
    async fn process_message(&mut self, input: &str) {
        // Build the API request
        let mut request = ApiRequest::new(input.to_string());

        // Add context files
        if !self.context.is_empty() {
            request = request.with_context(self.context.clone());
        }

        // Add conversation history
        request.messages = self.conversation.clone();

        // Start spinner
        let mut spinner = ThinkingSpinner::new();
        spinner.start("Thinking...");

        // Send streaming request
        let stream_result = match &self.provider {
            ActiveProvider::Venice(provider) => provider.send_streaming(request).await,
            ActiveProvider::Api(agent) => agent.send_streaming(request).await,
        };

        let mut rx = match stream_result {
            Ok(rx) => rx,
            Err(e) => {
                spinner.stop();
                self.renderer.render_error(&format!("Request failed: {}", e));
                return;
            }
        };

        // Receive and display streaming chunks
        let mut full_response = String::new();
        let mut final_usage = TokenUsage::default();
        let mut first_token = true;

        while let Some(chunk) = rx.recv().await {
            match chunk {
                StreamChunk::TextDelta(text) => {
                    if first_token {
                        spinner.stop();
                        print!("\n");
                        first_token = false;
                    }
                    full_response.push_str(&text);
                    self.renderer.render_delta(&text);
                }
                StreamChunk::Done(usage) => {
                    spinner.stop();
                    final_usage = usage;
                    break;
                }
                StreamChunk::Error(msg) => {
                    spinner.stop();
                    if !full_response.is_empty() {
                        println!();
                    }
                    self.renderer.render_error(&format!("Stream error: {}", msg));
                    break;
                }
            }
        }

        // If we got no content at all
        if full_response.is_empty() {
            if first_token {
                spinner.stop();
            }
            self.renderer.render_error("No response received.");
            return;
        }

        // Re-render with markdown if applicable
        self.renderer.render_markdown(&full_response);

        // Show usage stats
        let cached = final_usage.has_cache_activity();
        self.renderer.render_usage_line(
            final_usage.prompt_tokens,
            final_usage.completion_tokens,
            &self.model,
            cached,
        );

        // Record in conversation history
        self.conversation.push(Message {
            role: Role::User,
            content: input.to_string(),
        });
        self.conversation.push(Message {
            role: Role::Assistant,
            content: full_response,
        });

        // Update metrics
        self.session_tokens += final_usage.total_tokens as u64;
        self.turn_count += 1;
        self.metrics.record_request(
            final_usage.prompt_tokens,
            final_usage.completion_tokens,
            0,
            final_usage.estimated_cost_usd,
        );
    }

    /// Handle context management actions
    async fn handle_context_action(&mut self, action: ContextAction) {
        match action {
            ContextAction::Add(path) => {
                match tokio::fs::read_to_string(&path).await {
                    Ok(content) => {
                        self.context.push(ContextItem {
                            name: path.clone(),
                            content,
                            item_type: ContextType::File,
                            relevance: None,
                            cache_control: None,
                            is_static: false,
                        });
                        self.renderer
                            .render_success(&format!("Added context: {}", path));
                    }
                    Err(e) => {
                        self.renderer
                            .render_error(&format!("Failed to read {}: {}", path, e));
                    }
                }
            }
            ContextAction::Remove(name) => {
                let before = self.context.len();
                self.context.retain(|c| c.name != name);
                if self.context.len() < before {
                    self.renderer
                        .render_success(&format!("Removed context: {}", name));
                } else {
                    self.renderer
                        .render_error(&format!("Context not found: {}", name));
                }
            }
            ContextAction::List => {
                if self.context.is_empty() {
                    self.renderer.render_info("No context files loaded.");
                } else {
                    self.renderer.render_system("Context files:");
                    for (i, ctx) in self.context.iter().enumerate() {
                        let size = ctx.content.len();
                        let tokens_est = size / 4;
                        println!(
                            "  {} {} {}",
                            format!("[{}]", i).with(self.renderer.dim_color()),
                            ctx.name.clone().with(self.renderer.command_color()),
                            format!("(~{} tokens)", tokens_est).with(self.renderer.dim_color()),
                        );
                    }
                }
            }
            ContextAction::Clear => {
                let count = self.context.len();
                self.context.clear();
                self.renderer
                    .render_success(&format!("Cleared {} context files.", count));
            }
        }
    }

    /// Switch to a different model (keeps same provider)
    fn switch_model(&mut self, model: &str) -> Result<()> {
        self.model = model.to_string();

        // Rebuild the provider with the new model
        match &self.provider {
            ActiveProvider::Venice(_) => {
                if let Some(api_key) = self.config.primary_api_key() {
                    let venice_config = VeniceConfig {
                        api_key,
                        model: model.to_string(),
                        base_url: Some(self.config.primary.base_url.clone()),
                        min_balance_usd: self.config.primary.min_balance_usd,
                        min_balance_diem: self.config.primary.min_balance_diem,
                        max_tokens: Some(self.config.primary.max_tokens),
                        temperature: Some(self.config.primary.temperature),
                    };
                    self.provider =
                        ActiveProvider::Venice(Arc::new(VeniceProvider::new(venice_config)));
                }
            }
            ActiveProvider::Api(agent) => {
                let provider_type = agent.provider_type();
                let (api_key, base_url) = match provider_type {
                    ProviderType::Claude => (
                        self.config.fallback_api_key().unwrap_or_default(),
                        Some(format!("{}/messages", self.config.fallback.base_url)),
                    ),
                    ProviderType::OpenAI => (
                        self.config.fallback_api_key().unwrap_or_default(),
                        Some(format!(
                            "{}/chat/completions",
                            self.config.fallback.base_url
                        )),
                    ),
                    ProviderType::Ollama => (
                        String::new(),
                        Some(format!("{}/api/chat", self.config.local.url)),
                    ),
                    ProviderType::Custom => (String::new(), None),
                };
                let api_config = ApiConfig {
                    provider: provider_type,
                    api_key,
                    base_url,
                    model: model.to_string(),
                    max_tokens: Some(self.config.fallback.max_tokens),
                    temperature: Some(self.config.fallback.temperature),
                };
                self.provider = ActiveProvider::Api(Arc::new(ApiAgent::new(api_config)));
            }
        }

        Ok(())
    }

    /// Switch to a different provider
    fn switch_provider(&mut self, name: &str) -> Result<()> {
        match name.to_lowercase().as_str() {
            "venice" => {
                let api_key = self
                    .config
                    .primary_api_key()
                    .ok_or_else(|| anyhow::anyhow!("No Venice API key configured"))?;
                let venice_config = VeniceConfig {
                    api_key,
                    model: self.config.primary.model.clone(),
                    base_url: Some(self.config.primary.base_url.clone()),
                    min_balance_usd: self.config.primary.min_balance_usd,
                    min_balance_diem: self.config.primary.min_balance_diem,
                    max_tokens: Some(self.config.primary.max_tokens),
                    temperature: Some(self.config.primary.temperature),
                };
                self.model = venice_config.model.clone();
                self.provider =
                    ActiveProvider::Venice(Arc::new(VeniceProvider::new(venice_config)));
            }
            "claude" | "anthropic" => {
                let api_key = self
                    .config
                    .claude_api_key()
                    .ok_or_else(|| anyhow::anyhow!("No Anthropic API key configured"))?;
                let api_config = ApiConfig {
                    provider: ProviderType::Claude,
                    api_key,
                    base_url: Some(format!("{}/messages", self.config.fallback.base_url)),
                    model: self.config.fallback.model.clone(),
                    max_tokens: Some(self.config.fallback.max_tokens),
                    temperature: Some(self.config.fallback.temperature),
                };
                self.model = api_config.model.clone();
                self.provider = ActiveProvider::Api(Arc::new(ApiAgent::new(api_config)));
            }
            "openai" => {
                let api_key = std::env::var("OPENAI_API_KEY")
                    .or_else(|_| {
                        self.config
                            .fallback
                            .api_key
                            .clone()
                            .ok_or(std::env::VarError::NotPresent)
                    })
                    .map_err(|_| anyhow::anyhow!("No OpenAI API key configured"))?;
                let api_config = ApiConfig {
                    provider: ProviderType::OpenAI,
                    api_key,
                    base_url: Some("https://api.openai.com/v1/chat/completions".to_string()),
                    model: "gpt-4".to_string(),
                    max_tokens: Some(4096),
                    temperature: Some(0.7),
                };
                self.model = api_config.model.clone();
                self.provider = ActiveProvider::Api(Arc::new(ApiAgent::new(api_config)));
            }
            "ollama" | "local" => {
                let api_config = ApiConfig {
                    provider: ProviderType::Ollama,
                    api_key: String::new(),
                    base_url: Some(format!("{}/api/chat", self.config.local.url)),
                    model: self.config.local.model.clone(),
                    max_tokens: None,
                    temperature: Some(0.7),
                };
                self.model = api_config.model.clone();
                self.provider = ActiveProvider::Api(Arc::new(ApiAgent::new(api_config)));
            }
            _ => {
                anyhow::bail!(
                    "Unknown provider: {}. Available: venice, claude, openai, ollama",
                    name
                );
            }
        }
        Ok(())
    }

    /// Compact conversation history to reduce token usage
    fn compact_history(&mut self) {
        if self.conversation.len() <= 4 {
            return;
        }

        // Keep the first exchange and the last 2 exchanges
        let mut compacted = Vec::new();

        // Keep first pair
        if self.conversation.len() >= 2 {
            compacted.push(self.conversation[0].clone());
            compacted.push(self.conversation[1].clone());
        }

        // Keep last 4 messages (2 exchanges)
        let start = self.conversation.len().saturating_sub(4);
        for msg in &self.conversation[start..] {
            // Avoid duplicating the first pair
            if compacted.len() < 2 || !compacted.iter().any(|m: &Message| m.content == msg.content)
            {
                compacted.push(msg.clone());
            }
        }

        self.conversation = compacted;
    }

    /// Render session statistics
    fn render_stats(&self) {
        println!();
        self.renderer.render_system("Session Statistics:");
        println!(
            "  {} {}",
            "Turns:".with(self.renderer.dim_color()),
            format!("{}", self.turn_count).with(self.renderer.stats_color()),
        );
        println!(
            "  {} {}",
            "Total tokens:".with(self.renderer.dim_color()),
            format!("{}", self.session_tokens).with(self.renderer.stats_color()),
        );
        println!(
            "  {} {}",
            "History messages:".with(self.renderer.dim_color()),
            format!("{}", self.conversation.len()).with(self.renderer.stats_color()),
        );
        println!(
            "  {} {}",
            "Context files:".with(self.renderer.dim_color()),
            format!("{}", self.context.len()).with(self.renderer.stats_color()),
        );
        println!();
    }

    /// Render current status
    async fn render_status(&self) {
        println!();
        self.renderer.render_system("Current Status:");
        println!(
            "  {} {}",
            "Provider:".with(self.renderer.dim_color()),
            self.provider.name().with(self.renderer.stats_color()),
        );
        println!(
            "  {} {}",
            "Model:".with(self.renderer.dim_color()),
            self.model.clone().with(self.renderer.stats_color()),
        );

        // Show Venice balance if applicable
        if let ActiveProvider::Venice(venice) = &self.provider {
            let balance = venice.get_balance().await;
            if balance.last_updated.is_some() {
                println!(
                    "  {} ${}",
                    "Venice USD:".with(self.renderer.dim_color()),
                    format!("{:.2}", balance.balance_usd).with(self.renderer.stats_color()),
                );
                println!(
                    "  {} {}",
                    "Venice Diem:".with(self.renderer.dim_color()),
                    format!("{:.2}", balance.balance_diem).with(self.renderer.stats_color()),
                );
            }
        }

        println!(
            "  {} {}",
            "Session turns:".with(self.renderer.dim_color()),
            format!("{}", self.turn_count).with(self.renderer.stats_color()),
        );
        println!();
    }

    /// Render session summary on exit
    fn render_session_summary(&self) {
        println!();
        self.renderer.render_system("Session Summary:");
        println!(
            "  {} turns, {} total tokens",
            format!("{}", self.turn_count).with(self.renderer.stats_color()),
            format!("{}", self.session_tokens).with(self.renderer.stats_color()),
        );
        self.renderer.render_info("Goodbye!");
        println!();
    }
}

/// Result of handling a slash command
enum CommandResult {
    Continue,
    Quit,
}
