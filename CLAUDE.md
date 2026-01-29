# CLAUDE.md - Project Context for Claude Code

## Project Overview

TokenOptimizer is a Rust library and CLI tool for coordinating code agents with minimal token consumption. The primary goal is to reduce API costs when working with LLM-based coding assistants. See `SKILL.md` for an agent-discoverable capability manifest.

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
│   ├── response.rs     # ApiResponse, TokenUsage with cache support
│   ├── sse.rs          # SSE parser (OpenAI, Anthropic, Ollama formats)
│   ├── streaming.rs    # StreamChunk, StreamingProvider trait
│   └── venice.rs       # Venice.ai provider with credit tracking
├── cache/              # Cache prompting optimization
│   ├── mod.rs          # CacheConfig, CacheControl types
│   ├── strategy.rs     # CacheOptimizer, content classification
│   └── tracker.rs      # CacheTracker, metrics
├── metrics/            # Token tracking
│   └── mod.rs          # TokenMetrics, MetricsTracker
├── optimization/       # Prompt optimization
│   ├── mod.rs          # OptimizationConfig, StrategyType enum
│   └── strategies.rs   # PromptOptimizer, strategy implementations
├── orchestrator/       # Agent coordination with fallback
│   ├── mod.rs          # Orchestrator, FallbackProvider trait
│   └── session.rs      # Session management for handoffs
└── tui/                # Interactive terminal UI
    ├── mod.rs          # InteractiveShell main loop and provider selection
    ├── theme.rs        # Color constants (prompt, assistant, error, etc.)
    ├── spinner.rs      # ThinkingSpinner with braille animation
    ├── renderer.rs     # TerminalRenderer with termimad markdown support
    ├── prompt.rs       # PromptHandler with input history
    └── commands.rs     # Slash command parser and help renderer
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
3. `TruncateContext` - Boundary-aware truncation using `tiktoken-rs` token counts and priority-based boundary detection (code structure > paragraph > sentence > line > word). Validates truncation against token budget and retries with tighter limits if >10% over.
4. `Abbreviate` - Common word abbreviations
5. `LlmCompress` - Use local LLM for compression
6. `RelevanceFilter` - Hybrid keyword + LLM relevance scoring. Works without a local LLM via keyword-only mode (term frequency with log-TF, blending coverage and density). When a local agent is available, uses position-aware blending controlled by `keyword_weight` config (default `0.4`).
7. `ExtractSignatures` - Keep only function/class signatures
8. `Deduplicate` - 3-stage deduplication pipeline: exact hash, whitespace-normalized hash, and line-set Jaccard similarity (threshold 0.8) with length-ratio pre-filter

### Cache Prompting
- Anthropic requires ~1024 tokens minimum for caching
- Static content must come first in prompts
- Cache breakpoints mark reusable boundaries
- Cached tokens are ~90% cheaper on subsequent requests

### Agent Orchestration (Primary → Fallback)
The orchestrator manages a workflow where:
1. **Local LLM** (Ollama) preprocesses and optimizes prompts
2. **Primary provider** (default: Venice.ai) receives optimized prompts
3. **Fallback provider** (default: Claude via CLI) is used when primary credits are exhausted

Fallback triggers:
- 429 error with "insufficient" / "quota" / "balance" message
- Balance headers below threshold (`x-venice-balance-usd`, `x-venice-balance-diem`)
- Manual force via `orchestrator.force_fallback()`

Session handoff preserves conversation context for continuity.

### Interactive Mode (TUI)
The `interactive` subcommand launches a Claude Code-style shell with:
- **Streaming responses** via SSE parsing (`StreamingProvider` trait, `StreamChunk` enum)
- **Multi-turn conversation** history passed to providers in request messages
- **Markdown rendering** using `termimad` (re-renders after streaming completes if content has markdown elements)
- **Thinking spinner** shown until the first token arrives
- **Slash commands**: `/help`, `/quit`, `/clear`, `/model [name]`, `/provider [name]`, `/stats`, `/status`, `/compact`, `/context add|remove|list|clear`
- **Provider auto-selection**: tries primary (Venice) -> fallback (Claude/OpenAI) -> local (Ollama)
- **Live provider/model switching** via `/provider` and `/model` commands

SSE formats supported (`src/api/sse.rs`):
- **OpenAI/Venice**: `data: {"choices":[{"delta":{"content":"..."}}]}`
- **Anthropic**: `event: content_block_delta` / `data: {"delta":{"text":"..."}}`
- **Ollama**: line-delimited JSON `{"message":{"content":"..."}}`

### Configuration Structure
The config uses a generic **primary/fallback** provider model:

- **`[primary]`** — Primary API provider (default: Venice.ai)
  - `provider`, `api_key`, `base_url`, `model`, `min_balance_usd`, `min_balance_diem`, `max_tokens`, `temperature`, `enabled`
  - Env vars: `VENICE_API_KEY`, `VENICE_BASE_URL`, `VENICE_MODEL`
- **`[fallback]`** — Fallback provider (default: Claude via CLI; also supports OpenAI)
  - `provider` ("claude", "openai", or "none"), `api_key`, `base_url`, `model`, `max_tokens`, `temperature`, `enabled`, `use_cli`, `cli_path`
  - Env vars: `ANTHROPIC_API_KEY` / `OPENAI_API_KEY`, `FALLBACK_BASE_URL`, `FALLBACK_MODEL`
- **`[local]`** — Local LLM (Ollama) for preprocessing
- **`[orchestrator]`** — Orchestration settings (retries, context preservation)
- **`[optimization]`** — Prompt optimization settings (`keyword_weight`: 0.0–1.0, default 0.4, controls keyword vs LLM blending in hybrid relevance)
- **`[cache]`** — Cache prompting settings

Legacy config sections (`[venice]`, `[claude]`, `[openai]`) are still accepted and automatically migrated to the new structure via `Config::migrate_legacy()`.

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

# Example: show config for a section
cargo run -- config show primary
cargo run -- config show fallback

# Example: set a config value
cargo run -- config set primary.model deepseek-coder-v2
cargo run -- config set fallback.provider openai

# Example: launch interactive mode
cargo run -- interactive
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
- `crossterm` - Terminal colors, cursor, styled output
- `termimad` - Markdown rendering in terminal
- `indicatif` - Spinner/progress indicators
- `tokio-stream` / `futures-util` - Streaming SSE support

## API Provider Notes

### Venice.ai (Default Primary Provider)
- Config section: `[primary]` with `provider = "venice"`
- Base URL: `https://api.venice.ai/api/v1`
- Uses `Authorization: Bearer` header
- OpenAI-compatible chat completions format
- Balance tracking via response headers:
  - `x-venice-balance-usd` - USD credit balance
  - `x-venice-balance-diem` - Diem token balance
- Rate limit headers: `x-ratelimit-remaining-requests`, `x-ratelimit-remaining-tokens`
- Balance endpoint: `/api_keys/rate_limits` (beta)
- Recommended models for code: `llama-3.3-70b`, `deepseek-coder-v2`, `qwen-2.5-coder-32b`

### Anthropic Claude (Default Fallback Provider)
- Config section: `[fallback]` with `provider = "claude"`
- Uses `x-api-key` header
- Supports `cache_control` blocks in system and messages
- Returns `cache_creation_input_tokens` and `cache_read_input_tokens`
- API version: `2023-06-01`
- Can use Claude Code CLI instead of API (`use_cli = true`)

### OpenAI (Alternative Fallback Provider)
- Config section: `[fallback]` with `provider = "openai"`
- Uses `Authorization: Bearer` header
- No cache prompting support
- Standard chat completions format

### Ollama (Local Preprocessing)
- Config section: `[local]`
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
