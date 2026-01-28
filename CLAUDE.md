# CLAUDE.md - Project Context for Claude Code

## Project Overview

TokenOptimizer is a Rust library and CLI tool for coordinating code agents with minimal token consumption. The primary goal is to reduce API costs when working with LLM-based coding assistants.

## Architecture

```
src/
├── lib.rs              # Library exports
├── main.rs             # CLI entry point (clap-based)
├── agents/             # Local LLM integration
│   ├── mod.rs          # PreprocessingAgent trait
│   └── local.rs        # Ollama-based local agent
├── api/                # API client layer
│   ├── mod.rs          # ApiProvider trait, error types
│   ├── client.rs       # Multi-provider client (Claude, OpenAI, Ollama)
│   ├── request.rs      # ApiRequest, ContextItem structures
│   └── response.rs     # ApiResponse, TokenUsage with cache support
├── cache/              # Cache prompting optimization
│   ├── mod.rs          # CacheConfig, CacheControl types
│   ├── strategy.rs     # CacheOptimizer, content classification
│   └── tracker.rs      # CacheTracker, metrics
├── metrics/            # Token tracking
│   └── mod.rs          # TokenMetrics, MetricsTracker
└── optimization/       # Prompt optimization
    ├── mod.rs          # OptimizationConfig, StrategyType enum
    └── strategies.rs   # PromptOptimizer, strategy implementations
```

## Key Concepts

### Content Stability (for caching)
- **Static**: Never changes (system prompts, documentation)
- **SemiStatic**: Changes infrequently (type definitions, configs)
- **Dynamic**: May change between requests (current file)
- **Volatile**: Always changes (user query, errors)

### Optimization Strategies
1. `StripWhitespace` - Remove unnecessary whitespace
2. `RemoveComments` - Strip code comments
3. `TruncateContext` - Smart truncation at logical boundaries
4. `Abbreviate` - Common word abbreviations
5. `LlmCompress` - Use local LLM for compression
6. `RelevanceFilter` - Filter by relevance score
7. `ExtractSignatures` - Keep only function/class signatures
8. `Deduplicate` - Remove duplicate content

### Cache Prompting
- Anthropic requires ~1024 tokens minimum for caching
- Static content must come first in prompts
- Cache breakpoints mark reusable boundaries
- Cached tokens are ~90% cheaper on subsequent requests

## Development Commands

```bash
# Check compilation
cargo check

# Run tests
cargo test

# Build release
cargo build --release

# Run CLI
cargo run -- <command>

# Example: optimize a prompt
cargo run -- optimize --input "Fix the bug" --context src/main.rs

# Example: analyze cache potential
cargo run -- cache-optimize --task "Add feature" --context types.rs --static-indices "0"
```

## Dependencies

- `tokio` - Async runtime
- `reqwest` - HTTP client (with rustls)
- `serde` / `serde_json` - Serialization
- `clap` - CLI framework
- `tiktoken-rs` - Token counting
- `async-trait` - Async trait support
- `anyhow` / `thiserror` - Error handling
- `tracing` - Logging

## API Provider Notes

### Anthropic Claude
- Uses `x-api-key` header
- Supports `cache_control` blocks in system and messages
- Returns `cache_creation_input_tokens` and `cache_read_input_tokens`
- API version: `2023-06-01`

### OpenAI
- Uses `Authorization: Bearer` header
- No cache prompting support
- Standard chat completions format

### Ollama
- Local server at `http://localhost:11434`
- OpenAI-compatible format
- Used for preprocessing (relevance scoring, compression)

## Testing

When adding new optimization strategies:
1. Add variant to `StrategyType` enum in `optimization/mod.rs`
2. Implement in `PromptOptimizer::optimize()` match arm
3. Add helper function if needed
4. Test with `cargo run -- benchmark`

## Code Style

- Use `anyhow::Result` for fallible functions in binaries
- Use `thiserror` for library error types
- Prefer `async` for I/O operations
- Document public APIs with `///` comments
- Keep functions focused and small
