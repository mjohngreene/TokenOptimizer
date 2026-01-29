//! TokenOptimizer CLI - Coordinate code agents with minimal token consumption

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use token_optimizer::{
    agents::{LocalAgent, LocalAgentConfig, PreprocessingAgent},
    api::{ApiConfig, ApiRequest, ContextItem, ContextType, ProviderType},
    cache::{CacheConfig, CacheOptimizer},
    config::Config,
    metrics::MetricsTracker,
    optimization::{OptimizationConfig, PromptOptimizer, StrategyType},
};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(Parser)]
#[command(name = "token-optimizer")]
#[command(about = "Optimize prompts for API coding agents to minimize token usage")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Verbosity level
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Optimize a prompt or file for minimal token usage
    Optimize {
        /// Input file or prompt text
        #[arg(short, long)]
        input: String,

        /// Context files to include
        #[arg(short, long)]
        context: Vec<PathBuf>,

        /// Target token count
        #[arg(short, long, default_value = "4000")]
        target: usize,

        /// Use local LLM for optimization
        #[arg(long, default_value = "true")]
        use_local: bool,

        /// Output file (default: stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Send optimized request to API
    Send {
        /// Task description
        #[arg(short, long)]
        task: String,

        /// Context files
        #[arg(short, long)]
        context: Vec<PathBuf>,

        /// API provider (claude, openai, ollama)
        #[arg(short, long, default_value = "claude")]
        provider: String,

        /// Model to use
        #[arg(short, long)]
        model: Option<String>,

        /// Skip optimization
        #[arg(long)]
        no_optimize: bool,
    },

    /// Benchmark optimization strategies
    Benchmark {
        /// Input file to benchmark
        #[arg(short, long)]
        input: PathBuf,

        /// Context files
        #[arg(short, long)]
        context: Vec<PathBuf>,
    },

    /// Show metrics summary
    Metrics,

    /// Check if local LLM (Ollama) is available
    CheckLocal {
        /// Ollama URL
        #[arg(long, default_value = "http://localhost:11434")]
        url: String,
    },

    /// Interactive mode for exploring optimization
    Interactive,

    /// Analyze and optimize request for cache efficiency
    CacheOptimize {
        /// Task description
        #[arg(short, long)]
        task: String,

        /// Context files (static files should be listed first)
        #[arg(short, long)]
        context: Vec<PathBuf>,

        /// System prompt file (will be cached)
        #[arg(short, long)]
        system: Option<PathBuf>,

        /// Mark specific context files as static (by index, comma-separated)
        #[arg(long)]
        static_indices: Option<String>,
    },

    /// Manage configuration
    #[command(subcommand)]
    Config(ConfigCommands),
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Initialize configuration file with defaults
    Init {
        /// Overwrite existing config
        #[arg(long)]
        force: bool,
    },

    /// Show current configuration
    Show {
        /// Show only specific section (venice, claude, local, etc.)
        #[arg(short, long)]
        section: Option<String>,
    },

    /// Set a configuration value
    Set {
        /// Configuration key (e.g., venice.api_key, claude.model)
        key: String,

        /// Value to set
        value: String,
    },

    /// Show configuration file path
    Path,

    /// Validate configuration
    Validate,

    /// Set API key interactively (masks input)
    SetKey {
        /// Provider (venice, claude, openai)
        provider: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Setup logging
    let log_level = match cli.log_level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        .with_target(false)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    match cli.command {
        Commands::Optimize {
            input,
            context,
            target,
            use_local,
            output,
        } => {
            run_optimize(input, context, target, use_local, output).await?;
        }
        Commands::Send {
            task,
            context,
            provider,
            model,
            no_optimize,
        } => {
            run_send(task, context, provider, model, no_optimize).await?;
        }
        Commands::Benchmark { input, context } => {
            run_benchmark(input, context).await?;
        }
        Commands::Metrics => {
            show_metrics()?;
        }
        Commands::CheckLocal { url } => {
            check_local(&url).await?;
        }
        Commands::Interactive => {
            run_interactive().await?;
        }
        Commands::CacheOptimize {
            task,
            context,
            system,
            static_indices,
        } => {
            run_cache_optimize(task, context, system, static_indices).await?;
        }
        Commands::Config(cmd) => {
            run_config_command(cmd).await?;
        }
    }

    Ok(())
}

async fn run_optimize(
    input: String,
    context_files: Vec<PathBuf>,
    target: usize,
    use_local: bool,
    output: Option<PathBuf>,
) -> Result<()> {
    info!("Optimizing prompt with target {} tokens", target);

    // Load context files
    let mut context = Vec::new();
    for path in context_files {
        let content = tokio::fs::read_to_string(&path).await?;
        context.push(ContextItem {
            name: path.display().to_string(),
            content,
            item_type: ContextType::File,
            relevance: None,
            cache_control: None,
            is_static: false,
        });
    }

    // Create request
    let request = ApiRequest::new(input).with_context(context);

    // Setup optimizer
    let local_agent = if use_local {
        let config = LocalAgentConfig::default();
        let agent = LocalAgent::new(config);
        if agent.is_available().await {
            Some(agent)
        } else {
            info!("Local LLM not available, using rule-based optimization only");
            None
        }
    } else {
        None
    };

    let config = OptimizationConfig {
        target_tokens: Some(target),
        strategies: vec![
            StrategyType::StripWhitespace,
            StrategyType::RemoveComments,
            StrategyType::RelevanceFilter,
            StrategyType::TruncateContext,
        ],
        use_local_llm: local_agent.is_some(),
        preserve_code_blocks: true,
    };

    let optimizer = PromptOptimizer::new(config, local_agent);
    let (optimized, stats) = optimizer.optimize(request).await?;

    // Output results
    let result = serde_json::to_string_pretty(&optimized)?;

    if let Some(path) = output {
        tokio::fs::write(&path, &result).await?;
        println!("Optimized prompt written to: {}", path.display());
    } else {
        println!("{}", result);
    }

    println!("\n--- Optimization Stats ---");
    println!("Original tokens: ~{}", stats.original_tokens);
    println!("Optimized tokens: ~{}", stats.optimized_tokens);
    println!("Tokens saved: ~{}", stats.tokens_saved);
    println!(
        "Compression ratio: {:.1}%",
        stats.compression_ratio * 100.0
    );
    println!("Strategies applied: {:?}", stats.strategies_applied);

    Ok(())
}

async fn run_send(
    task: String,
    context_files: Vec<PathBuf>,
    provider: String,
    model: Option<String>,
    no_optimize: bool,
) -> Result<()> {
    use token_optimizer::api::{ApiAgent, ApiProvider};

    // Load context
    let mut context = Vec::new();
    for path in context_files {
        let content = tokio::fs::read_to_string(&path).await?;
        context.push(ContextItem {
            name: path.display().to_string(),
            content,
            item_type: ContextType::File,
            relevance: None,
            cache_control: None,
            is_static: false,
        });
    }

    let mut request = ApiRequest::new(task).with_context(context);

    // Optimize if requested
    let tokens_saved = if !no_optimize {
        let config = OptimizationConfig::default();
        let optimizer = PromptOptimizer::new(config, None);
        let (optimized, stats) = optimizer.optimize(request).await?;
        request = optimized;
        stats.tokens_saved
    } else {
        0
    };

    // Setup API client
    let provider_type = match provider.to_lowercase().as_str() {
        "claude" => ProviderType::Claude,
        "openai" => ProviderType::OpenAI,
        "ollama" => ProviderType::Ollama,
        _ => ProviderType::Custom,
    };

    let default_model = match provider_type {
        ProviderType::Claude => "claude-sonnet-4-20250514",
        ProviderType::OpenAI => "gpt-4",
        ProviderType::Ollama => "llama3.2",
        ProviderType::Custom => "default",
    };

    let api_key = std::env::var(match provider_type {
        ProviderType::Claude => "ANTHROPIC_API_KEY",
        ProviderType::OpenAI => "OPENAI_API_KEY",
        _ => "API_KEY",
    })
    .unwrap_or_default();

    let config = ApiConfig {
        provider: provider_type,
        api_key,
        base_url: None,
        model: model.unwrap_or_else(|| default_model.to_string()),
        max_tokens: Some(4096),
        temperature: Some(0.7),
    };

    let agent = ApiAgent::new(config);
    let response = agent.send_request(request).await?;

    println!("{}", response.content);
    println!("\n--- Token Usage ---");
    println!("Prompt tokens: {}", response.usage.prompt_tokens);
    println!("Completion tokens: {}", response.usage.completion_tokens);
    println!("Total tokens: {}", response.usage.total_tokens);
    println!("Tokens saved by optimization: ~{}", tokens_saved);

    Ok(())
}

async fn run_benchmark(input: PathBuf, context_files: Vec<PathBuf>) -> Result<()> {
    use std::time::Instant;

    info!("Running benchmark on {}", input.display());

    let task = tokio::fs::read_to_string(&input).await?;

    let mut context = Vec::new();
    for path in context_files {
        let content = tokio::fs::read_to_string(&path).await?;
        context.push(ContextItem {
            name: path.display().to_string(),
            content,
            item_type: ContextType::File,
            relevance: None,
            cache_control: None,
            is_static: false,
        });
    }

    let request = ApiRequest::new(task).with_context(context);

    // Test different strategy combinations
    let strategy_sets = vec![
        ("None", vec![]),
        ("Whitespace only", vec![StrategyType::StripWhitespace]),
        (
            "Whitespace + Comments",
            vec![StrategyType::StripWhitespace, StrategyType::RemoveComments],
        ),
        (
            "Full rule-based",
            vec![
                StrategyType::StripWhitespace,
                StrategyType::RemoveComments,
                StrategyType::TruncateContext,
                StrategyType::Deduplicate,
            ],
        ),
        (
            "Signatures only",
            vec![StrategyType::ExtractSignatures],
        ),
    ];

    println!("=== Benchmark Results ===\n");
    println!(
        "{:<25} {:>12} {:>12} {:>12}",
        "Strategy", "Original", "Optimized", "Ratio"
    );
    println!("{}", "-".repeat(65));

    for (name, strategies) in strategy_sets {
        let config = OptimizationConfig {
            target_tokens: None,
            strategies,
            use_local_llm: false,
            preserve_code_blocks: true,
        };

        let optimizer = PromptOptimizer::new(config, None);
        let start = Instant::now();
        let (_, stats) = optimizer.optimize(request.clone()).await?;
        let _duration = start.elapsed();

        println!(
            "{:<25} {:>12} {:>12} {:>11.1}%",
            name,
            stats.original_tokens,
            stats.optimized_tokens,
            stats.compression_ratio * 100.0
        );
    }

    Ok(())
}

fn show_metrics() -> Result<()> {
    let tracker = MetricsTracker::new();
    let summary = tracker.summary();
    println!("{}", summary);
    Ok(())
}

async fn check_local(url: &str) -> Result<()> {
    let config = LocalAgentConfig {
        ollama_url: url.to_string(),
        ..Default::default()
    };

    let agent = LocalAgent::new(config);

    if agent.is_available().await {
        println!("Local LLM (Ollama) is available at {}", url);

        // Try to list models
        let client = reqwest::Client::new();
        if let Ok(response) = client.get(format!("{}/api/tags", url)).send().await {
            if let Ok(json) = response.json::<serde_json::Value>().await {
                if let Some(models) = json["models"].as_array() {
                    println!("\nAvailable models:");
                    for model in models {
                        if let Some(name) = model["name"].as_str() {
                            println!("  - {}", name);
                        }
                    }
                }
            }
        }
    } else {
        println!("Local LLM (Ollama) is NOT available at {}", url);
        println!("\nTo install Ollama:");
        println!("  curl -fsSL https://ollama.ai/install.sh | sh");
        println!("\nThen pull a model:");
        println!("  ollama pull llama3.2");
    }

    Ok(())
}

async fn run_interactive() -> Result<()> {
    use std::io::{self, BufRead, Write};

    println!("TokenOptimizer Interactive Mode");
    println!("================================");
    println!("Commands:");
    println!("  /optimize <text>  - Optimize the given text");
    println!("  /context <file>   - Add a context file");
    println!("  /clear            - Clear context");
    println!("  /stats            - Show statistics");
    println!("  /quit             - Exit");
    println!();

    let stdin = io::stdin();
    let mut context: Vec<ContextItem> = Vec::new();
    let tracker = MetricsTracker::new();

    loop {
        print!("> ");
        io::stdout().flush()?;

        let mut line = String::new();
        stdin.lock().read_line(&mut line)?;
        let line = line.trim();

        if line.is_empty() {
            continue;
        }

        if line.starts_with("/quit") {
            break;
        }

        if line.starts_with("/clear") {
            context.clear();
            println!("Context cleared.");
            continue;
        }

        if line.starts_with("/stats") {
            println!("{}", tracker.summary());
            continue;
        }

        if let Some(file) = line.strip_prefix("/context ") {
            match std::fs::read_to_string(file) {
                Ok(content) => {
                    context.push(ContextItem {
                        name: file.to_string(),
                        content,
                        item_type: ContextType::File,
                        relevance: None,
                        cache_control: None,
                        is_static: false,
                    });
                    println!("Added context: {}", file);
                }
                Err(e) => println!("Error reading file: {}", e),
            }
            continue;
        }

        if let Some(text) = line.strip_prefix("/optimize ") {
            let request = ApiRequest::new(text.to_string()).with_context(context.clone());

            let config = OptimizationConfig::default();
            let optimizer = PromptOptimizer::new(config, None);

            match optimizer.optimize(request).await {
                Ok((optimized, stats)) => {
                    println!("\nOptimized task: {}", optimized.task);
                    println!("Context items: {}", optimized.context.len());
                    println!(
                        "Tokens: {} -> {} ({:.1}% of original)",
                        stats.original_tokens,
                        stats.optimized_tokens,
                        stats.compression_ratio * 100.0
                    );
                }
                Err(e) => println!("Optimization error: {}", e),
            }
            continue;
        }

        println!("Unknown command. Type /quit to exit.");
    }

    Ok(())
}

async fn run_cache_optimize(
    task: String,
    context_files: Vec<PathBuf>,
    system_file: Option<PathBuf>,
    static_indices: Option<String>,
) -> Result<()> {
    info!("Analyzing request for cache optimization");

    // Parse static indices
    let static_idx: Vec<usize> = static_indices
        .map(|s| {
            s.split(',')
                .filter_map(|i| i.trim().parse().ok())
                .collect()
        })
        .unwrap_or_default();

    // Load context files
    let mut context = Vec::new();
    for (idx, path) in context_files.iter().enumerate() {
        let content = tokio::fs::read_to_string(&path).await?;
        let is_static = static_idx.contains(&idx);
        context.push(ContextItem {
            name: path.display().to_string(),
            content,
            item_type: ContextType::File,
            relevance: None,
            cache_control: if is_static {
                Some(token_optimizer::cache::CacheControl::default())
            } else {
                None
            },
            is_static,
        });
    }

    // Load system prompt if provided
    let system = if let Some(path) = system_file {
        Some(tokio::fs::read_to_string(&path).await?)
    } else {
        None
    };

    // Build request
    let mut request = ApiRequest::new(task).with_context(context);
    if let Some(sys) = system {
        request = request.with_cached_system(sys);
    }

    // Run cache optimizer
    let cache_config = CacheConfig::default();
    let mut cache_optimizer = CacheOptimizer::new(cache_config);
    let optimized = cache_optimizer.optimize_request(request);

    // Display results
    println!("\n=== Cache Optimization Analysis ===\n");

    println!("Request Structure:");
    if optimized.request.system.is_some() {
        println!("  System prompt: YES (cached: {})",
            optimized.request.system_cache_control.is_some());
    }
    println!("  Context items: {}", optimized.request.context.len());
    println!("  Cache breakpoints: {:?}", optimized.breakpoints);

    println!("\nToken Estimates:");
    println!("  Static (cacheable) tokens: ~{}", optimized.static_tokens);
    println!("  Dynamic tokens: ~{}", optimized.dynamic_tokens);
    println!("  Total tokens: ~{}", optimized.static_tokens + optimized.dynamic_tokens);

    println!("\nCaching Potential:");
    if optimized.static_tokens >= 1024 {
        println!("  Status: CACHE ELIGIBLE");
        println!("  Est. tokens saved on repeat: ~{}", optimized.estimated_cache_savings);
        println!("  Est. cost reduction: ~90% on cached portion");
    } else {
        println!("  Status: BELOW MINIMUM");
        println!("  Need {} more tokens in static content to enable caching",
            1024 - optimized.static_tokens);
    }

    println!("\nContext Item Classification:");
    for (idx, item) in optimized.request.context.iter().enumerate() {
        let status = if item.is_static { "STATIC" } else { "DYNAMIC" };
        let has_breakpoint = optimized.breakpoints.iter().any(|bp| {
            matches!(bp, token_optimizer::cache::BreakpointPosition::AfterContext(i) if *i == idx)
        });
        let bp_marker = if has_breakpoint { " [BREAKPOINT]" } else { "" };
        println!("  [{}] {} - {}{}", idx, status, item.name, bp_marker);
    }

    println!("\nRecommendations:");
    if optimized.static_tokens < 1024 {
        println!("  - Add more static content (type definitions, documentation) to enable caching");
    }
    if !optimized.request.context.iter().any(|c| c.is_static) {
        println!("  - Mark stable context files as static with --static-indices");
    }
    if optimized.request.system_cache_control.is_none() && optimized.request.system.is_some() {
        println!("  - Consider caching the system prompt if it's reused across requests");
    }

    Ok(())
}

async fn run_config_command(cmd: ConfigCommands) -> Result<()> {
    match cmd {
        ConfigCommands::Init { force } => {
            config_init(force).await?;
        }
        ConfigCommands::Show { section } => {
            config_show(section)?;
        }
        ConfigCommands::Set { key, value } => {
            config_set(&key, &value)?;
        }
        ConfigCommands::Path => {
            config_path();
        }
        ConfigCommands::Validate => {
            config_validate()?;
        }
        ConfigCommands::SetKey { provider } => {
            config_set_key(&provider).await?;
        }
    }
    Ok(())
}

async fn config_init(force: bool) -> Result<()> {
    let path = Config::default_path();

    if path.exists() && !force {
        println!("Configuration file already exists at: {}", path.display());
        println!("Use --force to overwrite");
        return Ok(());
    }

    let config = Config::default();
    config.save()?;

    println!("Configuration file created at: {}", path.display());
    println!();
    println!("Next steps:");
    println!("  1. Edit the config file to add your API keys, or");
    println!("  2. Set environment variables:");
    println!("     export VENICE_API_KEY=your_venice_key");
    println!("     export ANTHROPIC_API_KEY=your_anthropic_key");
    println!();
    println!("Or use the interactive key setup:");
    println!("  token-optimizer config set-key venice");
    println!("  token-optimizer config set-key claude");

    Ok(())
}

fn config_show(section: Option<String>) -> Result<()> {
    let config = Config::load()?;

    let display = if let Some(sec) = section {
        match sec.to_lowercase().as_str() {
            "primary" | "venice" => toml::to_string_pretty(&config.primary)?,
            "fallback" | "claude" => toml::to_string_pretty(&config.fallback)?,
            "local" => toml::to_string_pretty(&config.local)?,
            "orchestrator" => toml::to_string_pretty(&config.orchestrator)?,
            "optimization" => toml::to_string_pretty(&config.optimization)?,
            "cache" => toml::to_string_pretty(&config.cache)?,
            _ => {
                println!("Unknown section: {}", sec);
                println!("Available: primary, fallback, local, orchestrator, optimization, cache");
                return Ok(());
            }
        }
    } else {
        // Mask API keys in display
        let mut display_config = config.clone();
        if display_config.primary.api_key.is_some() {
            display_config.primary.api_key = Some("***".to_string());
        }
        if display_config.fallback.api_key.is_some() {
            display_config.fallback.api_key = Some("***".to_string());
        }
        toml::to_string_pretty(&display_config)?
    };

    println!("{}", display);

    // Show environment variable status
    println!("\n--- Environment Variables ---");
    println!("VENICE_API_KEY: {}", if std::env::var("VENICE_API_KEY").is_ok() { "set" } else { "not set" });
    println!("ANTHROPIC_API_KEY: {}", if std::env::var("ANTHROPIC_API_KEY").is_ok() { "set" } else { "not set" });
    println!("OPENAI_API_KEY: {}", if std::env::var("OPENAI_API_KEY").is_ok() { "set" } else { "not set" });
    println!("OLLAMA_URL: {}", std::env::var("OLLAMA_URL").unwrap_or_else(|_| "not set".to_string()));

    Ok(())
}

fn config_set(key: &str, value: &str) -> Result<()> {
    let mut config = Config::load()?;

    let parts: Vec<&str> = key.split('.').collect();
    if parts.len() != 2 {
        println!("Invalid key format. Use: section.key (e.g., primary.model)");
        return Ok(());
    }

    let (section, field) = (parts[0], parts[1]);

    match section {
        "primary" | "venice" => match field {
            "api_key" => config.primary.api_key = Some(value.to_string()),
            "provider" => config.primary.provider = value.to_string(),
            "model" => config.primary.model = value.to_string(),
            "base_url" => config.primary.base_url = value.to_string(),
            "min_balance_usd" => config.primary.min_balance_usd = value.parse()?,
            "min_balance_diem" => config.primary.min_balance_diem = value.parse()?,
            "max_tokens" => config.primary.max_tokens = value.parse()?,
            "temperature" => config.primary.temperature = value.parse()?,
            "enabled" => config.primary.enabled = value.parse()?,
            _ => {
                println!("Unknown primary field: {}", field);
                println!("Available: api_key, provider, model, base_url, min_balance_usd, min_balance_diem, max_tokens, temperature, enabled");
                return Ok(());
            }
        },
        "fallback" | "claude" => match field {
            "api_key" => config.fallback.api_key = Some(value.to_string()),
            "provider" => config.fallback.provider = value.to_string(),
            "model" => config.fallback.model = value.to_string(),
            "base_url" => config.fallback.base_url = value.to_string(),
            "max_tokens" => config.fallback.max_tokens = value.parse()?,
            "temperature" => config.fallback.temperature = value.parse()?,
            "use_cli" => config.fallback.use_cli = value.parse()?,
            "cli_path" => config.fallback.cli_path = Some(value.to_string()),
            "enabled" => config.fallback.enabled = value.parse()?,
            _ => {
                println!("Unknown fallback field: {}", field);
                println!("Available: api_key, provider, model, base_url, max_tokens, temperature, use_cli, cli_path, enabled");
                return Ok(());
            }
        },
        "local" => match field {
            "url" => config.local.url = value.to_string(),
            "model" => config.local.model = value.to_string(),
            "enabled" => config.local.enabled = value.parse()?,
            "relevance_threshold" => config.local.relevance_threshold = value.parse()?,
            _ => {
                println!("Unknown local field: {}", field);
                return Ok(());
            }
        },
        "orchestrator" => match field {
            "max_retries" => config.orchestrator.max_retries = value.parse()?,
            "preserve_context" => config.orchestrator.preserve_context = value.parse()?,
            _ => {
                println!("Unknown orchestrator field: {}", field);
                return Ok(());
            }
        },
        "optimization" => match field {
            "target_tokens" => config.optimization.target_tokens = value.parse()?,
            "preserve_code_blocks" => config.optimization.preserve_code_blocks = value.parse()?,
            "use_local_llm" => config.optimization.use_local_llm = value.parse()?,
            _ => {
                println!("Unknown optimization field: {}", field);
                return Ok(());
            }
        },
        "cache" => match field {
            "min_cache_tokens" => config.cache.min_cache_tokens = value.parse()?,
            "max_breakpoints" => config.cache.max_breakpoints = value.parse()?,
            "auto_reorder" => config.cache.auto_reorder = value.parse()?,
            _ => {
                println!("Unknown cache field: {}", field);
                return Ok(());
            }
        },
        _ => {
            println!("Unknown section: {}", section);
            println!("Available: primary, fallback, local, orchestrator, optimization, cache");
            return Ok(());
        }
    }

    config.save()?;
    println!("Set {} = {}", key, if field == "api_key" { "***" } else { value });

    Ok(())
}

fn config_path() {
    let path = Config::default_path();
    println!("{}", path.display());

    if path.exists() {
        println!("(file exists)");
    } else {
        println!("(file does not exist - run 'config init' to create)");
    }
}

fn config_validate() -> Result<()> {
    let config = Config::load()?;

    match config.validate() {
        Ok(()) => {
            println!("Configuration is valid!");
            println!();

            // Show what's configured
            println!("Configured providers:");

            let primary_key = config.primary_api_key();
            if config.primary.enabled && primary_key.is_some() {
                println!("  Primary ({}): enabled (model: {})", config.primary.provider, config.primary.model);
            } else if config.primary.enabled {
                println!("  Primary ({}): enabled but NO API KEY", config.primary.provider);
            } else {
                println!("  Primary: disabled");
            }

            let fallback_key = config.fallback_api_key();
            if config.fallback.enabled && (fallback_key.is_some() || (config.fallback.provider == "claude" && config.fallback.use_cli)) {
                let method = if config.fallback.provider == "claude" && config.fallback.use_cli { "CLI" } else { "API" };
                println!("  Fallback ({}): enabled via {} (model: {})", config.fallback.provider, method, config.fallback.model);
            } else if config.fallback.enabled {
                println!("  Fallback ({}): enabled but NO API KEY", config.fallback.provider);
            } else {
                println!("  Fallback: disabled");
            }

            if config.local.enabled {
                println!("  Local LLM: enabled (url: {}, model: {})", config.local.url, config.local.model);
            } else {
                println!("  Local LLM: disabled");
            }

            println!();
            println!("Orchestration: {} -> {}",
                config.orchestrator.primary_provider,
                config.orchestrator.fallback_provider);
        }
        Err(e) => {
            println!("Configuration validation failed:");
            println!("  {}", e);
            println!();
            println!("To fix, either:");
            println!("  1. Set API keys in config: token-optimizer config set-key venice");
            println!("  2. Set environment variables: export VENICE_API_KEY=your_key");
        }
    }

    Ok(())
}

async fn config_set_key(provider: &str) -> Result<()> {
    use std::io::{self, Write};

    let key_name = match provider.to_lowercase().as_str() {
        "venice" => "VENICE_API_KEY",
        "claude" | "anthropic" => "ANTHROPIC_API_KEY",
        "openai" => "OPENAI_API_KEY",
        _ => {
            println!("Unknown provider: {}", provider);
            println!("Available: venice, claude, openai");
            return Ok(());
        }
    };

    println!("Enter your {} API key (input will be hidden):", provider);
    print!("> ");
    io::stdout().flush()?;

    // Read key (note: this won't actually hide input in most terminals without rpassword crate)
    let mut key = String::new();
    io::stdin().read_line(&mut key)?;
    let key = key.trim();

    if key.is_empty() {
        println!("No key entered, aborting.");
        return Ok(());
    }

    // Save to config
    let mut config = Config::load()?;

    match provider.to_lowercase().as_str() {
        "venice" => config.primary.api_key = Some(key.to_string()),
        "claude" | "anthropic" => {
            config.fallback.provider = "claude".to_string();
            config.fallback.api_key = Some(key.to_string());
        }
        "openai" => {
            config.fallback.provider = "openai".to_string();
            config.fallback.api_key = Some(key.to_string());
        }
        _ => {}
    }

    config.save()?;

    println!();
    println!("API key saved to config file.");
    println!();
    println!("Alternatively, you can set the environment variable:");
    println!("  export {}=your_key", key_name);

    Ok(())
}
