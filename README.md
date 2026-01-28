# TokenOptimizer

A Rust library and CLI tool for coordinating code agents with a primary focus on **minimizing token consumption**. This project helps reduce API costs when working with LLM-based coding assistants by optimizing prompts and leveraging cache prompting.

## Features

### Agent Orchestration (Venice.ai → Claude Code)
- **Venice.ai as primary** - Use Venice credits for cost-effective API calls
- **Automatic fallback** - Switch to Claude Code when Venice credits exhausted
- **Session handoff** - Preserve conversation context during provider transitions
- **Credit tracking** - Monitor balance via response headers
- **Configurable thresholds** - Set minimum balance for preemptive fallback

### Prompt Optimization
- **Whitespace stripping** - Remove unnecessary whitespace while preserving code structure
- **Comment removal** - Strip comments from code context
- **Context truncation** - Smart truncation at logical boundaries (function/class definitions)
- **Signature extraction** - Extract only function/class signatures for minimal context
- **Deduplication** - Remove duplicate content
- **Relevance filtering** - Use local LLM to score and filter context by relevance

### Cache Prompting
Maximize cache hit rates with providers like Anthropic Claude:
- **Automatic content classification** - Categorize content as Static/SemiStatic/Dynamic/Volatile
- **Optimal prompt structuring** - Reorder content to put cacheable items first
- **Cache breakpoint management** - Strategic placement of cache breakpoints
- **Cache metrics tracking** - Monitor hit rates and cost savings

### Local LLM Preprocessing
Use small local models (via Ollama) to preprocess before expensive API calls:
- Context compression
- Relevance scoring
- Prompt optimization
- Key information extraction

### Metrics & Tracking
- Token usage tracking
- Cost estimation (with cache-aware pricing)
- Compression ratio statistics
- Per-session metrics

## Installation

### Prerequisites
- Rust 1.70+
- (Optional) [Ollama](https://ollama.ai/) for local LLM preprocessing

### Build from source
```bash
git clone https://github.com/YOUR_USERNAME/TokenOptimizer.git
cd TokenOptimizer
cargo build --release
```

## Configuration

TokenOptimizer uses a TOML configuration file with environment variable overrides.

### Quick Setup

```bash
# Initialize config file
token-optimizer config init

# Set API keys interactively
token-optimizer config set-key venice
token-optimizer config set-key claude

# Or use environment variables
export VENICE_API_KEY=your_venice_key
export ANTHROPIC_API_KEY=your_anthropic_key

# Validate configuration
token-optimizer config validate
```

### Config File Location

- **Linux/macOS**: `~/.config/token-optimizer/config.toml`
- **Windows**: `%APPDATA%\token-optimizer\config.toml`

### Configuration Options

```bash
# Show current configuration
token-optimizer config show

# Show specific section
token-optimizer config show --section venice

# Set individual values
token-optimizer config set venice.model deepseek-coder-v2
token-optimizer config set orchestrator.primary_provider venice
token-optimizer config set optimization.target_tokens 8000
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| `VENICE_API_KEY` | Venice.ai API key |
| `ANTHROPIC_API_KEY` | Anthropic/Claude API key |
| `OPENAI_API_KEY` | OpenAI API key (optional) |
| `OLLAMA_URL` | Ollama server URL |
| `OLLAMA_MODEL` | Ollama model for preprocessing |

Environment variables override config file values.

### Example Configuration

See [`config.example.toml`](config.example.toml) for a complete example with all options documented.

## Usage

### CLI Commands

#### Optimize a prompt
```bash
token-optimizer optimize \
  --input "Fix the authentication bug in the login handler" \
  --context src/auth.rs \
  --context src/handlers/login.rs \
  --target 4000
```

#### Analyze cache optimization potential
```bash
token-optimizer cache-optimize \
  --task "Implement the new feature" \
  --context types.d.ts \
  --context src/feature.ts \
  --system prompts/system.txt \
  --static-indices "0,1"
```

#### Benchmark optimization strategies
```bash
token-optimizer benchmark \
  --input task.txt \
  --context src/main.rs
```

#### Check local LLM availability
```bash
token-optimizer check-local --url http://localhost:11434
```

#### Interactive mode
```bash
token-optimizer interactive
```

### As a Library

```rust
use token_optimizer::{
    api::{ApiRequest, ContextItem, ContextType},
    cache::{CacheConfig, CacheOptimizer},
    optimization::{OptimizationConfig, PromptOptimizer, StrategyType},
};

// Create a request
let request = ApiRequest::new("Fix the bug in auth.rs".to_string())
    .with_cached_system("You are a coding assistant...".to_string())
    .with_context(vec![
        ContextItem {
            name: "types.rs".to_string(),
            content: "pub struct User { ... }".to_string(),
            item_type: ContextType::File,
            relevance: None,
            cache_control: None,
            is_static: true,  // Mark as cacheable
        },
    ]);

// Optimize for token reduction
let config = OptimizationConfig {
    target_tokens: Some(4000),
    strategies: vec![
        StrategyType::StripWhitespace,
        StrategyType::RemoveComments,
        StrategyType::TruncateContext,
    ],
    ..Default::default()
};
let optimizer = PromptOptimizer::new(config, None);
let (optimized, stats) = optimizer.optimize(request).await?;

println!("Tokens saved: {}", stats.tokens_saved);
```

### Cache Optimization

```rust
use token_optimizer::cache::{CacheConfig, CacheOptimizer};

let mut cache_optimizer = CacheOptimizer::new(CacheConfig::default());
let optimized = cache_optimizer.optimize_request(request);

println!("Static tokens: {}", optimized.static_tokens);
println!("Cache eligible: {}", optimized.static_tokens >= 1024);
println!("Est. savings: {}", optimized.estimated_cache_savings);
```

## Cache Prompting Strategy

To maximize cache efficiency with Anthropic's Claude:

1. **Structure prompts correctly**: Static content must come first
   - System prompt (cached)
   - Documentation/type definitions (cached)
   - Semi-static context (project structure)
   - Dynamic context (current file, errors)
   - User task (always dynamic)

2. **Meet minimum thresholds**: Claude requires ~1024 tokens minimum for caching

3. **Use cache breakpoints**: Mark boundaries where cache can be reused

4. **Track what's cached**: Use `CacheTracker` to monitor hit rates

## Configuration

### Optimization Config
```rust
OptimizationConfig {
    target_tokens: Some(4000),      // Target token budget
    strategies: vec![...],           // Strategies to apply
    use_local_llm: true,            // Use Ollama for preprocessing
    preserve_code_blocks: true,     // Don't strip code formatting
}
```

### Cache Config
```rust
CacheConfig {
    min_cache_tokens: 1024,         // Minimum for caching
    max_breakpoints: 4,             // Max cache breakpoints
    auto_reorder: true,             // Reorder for optimal caching
    pad_to_minimum: false,          // Pad small sections
    tokens_per_char: 0.25,          // Estimation ratio
}
```

### Agent Orchestration

```rust
use token_optimizer::{
    VeniceConfig, VeniceProvider,
    ClaudeCodeFallback, Orchestrator, OrchestratorConfig,
    metrics::MetricsTracker,
};

// Configure Venice.ai as primary provider
let venice_config = VeniceConfig {
    api_key: std::env::var("VENICE_API_KEY").unwrap(),
    model: "llama-3.3-70b".to_string(),
    min_balance_usd: 0.10,  // Trigger fallback below $0.10
    ..Default::default()
};
let venice = VeniceProvider::new(venice_config);

// Configure Claude Code as fallback
let fallback = ClaudeCodeFallback::new();

// Create orchestrator
let orchestrator = Orchestrator::new(
    OrchestratorConfig::default(),
    venice,
    fallback,
    MetricsTracker::new(),
);

// Execute request - automatically falls back if Venice exhausted
let response = orchestrator.execute(request).await?;

// Check current state
match orchestrator.state().await {
    OrchestratorState::UsingVenice => println!("Using Venice"),
    OrchestratorState::UsingFallback => println!("Switched to Claude Code"),
    _ => {}
}

// Check Venice balance
let balance = orchestrator.venice_balance().await;
println!("Venice balance: ${:.2}", balance.balance_usd);
```

## Supported Providers

- **Venice.ai** - Primary provider with credit tracking and automatic fallback
- **Anthropic Claude** - Full support including cache prompting (fallback)
- **OpenAI** - Basic support (no cache prompting)
- **Ollama** - Local models for preprocessing
- **Custom** - Any OpenAI-compatible API

## License

MIT

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
