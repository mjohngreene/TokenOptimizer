# Skills for TokenOptimizer Development

This file contains custom skills (slash commands) useful for developing and maintaining the TokenOptimizer project.

---

## /token-analyze

Analyze token usage and optimization potential for a given file or prompt.

```yaml
name: token-analyze
description: Analyze token usage and suggest optimizations
arguments:
  - name: target
    description: File path or prompt text to analyze
    required: true
```

### Implementation
1. Read the target file/text
2. Estimate token count (~4 chars per token)
3. Identify optimization opportunities:
   - Comment density
   - Whitespace ratio
   - Duplicate content
   - Function signature extractability
4. Suggest applicable strategies
5. Estimate potential savings

---

## /cache-check

Check if content is cache-eligible and suggest improvements.

```yaml
name: cache-check
description: Analyze content for cache prompting eligibility
arguments:
  - name: files
    description: Comma-separated list of context files
    required: true
```

### Implementation
1. Load and classify each file by stability
2. Calculate total static vs dynamic tokens
3. Check against 1024 token minimum
4. Suggest reordering if needed
5. Recommend which files to mark as static

---

## /optimize-prompt

Interactively optimize a prompt for minimal token usage.

```yaml
name: optimize-prompt
description: Optimize a prompt through multiple strategies
arguments:
  - name: prompt
    description: The prompt text to optimize
    required: true
  - name: target-tokens
    description: Target token count
    required: false
    default: "4000"
```

### Implementation
1. Show original token count
2. Apply strategies incrementally:
   - Strip whitespace → show savings
   - Remove comments → show savings
   - Truncate → show savings
3. Display final optimized prompt
4. Show total compression ratio

---

## /benchmark-strategies

Benchmark all optimization strategies on given content.

```yaml
name: benchmark-strategies
description: Compare optimization strategies on test content
arguments:
  - name: input
    description: Input file to benchmark
    required: true
```

### Implementation
1. Load input file
2. Run each strategy independently
3. Run combinations
4. Display comparison table:
   - Strategy name
   - Original tokens
   - Optimized tokens
   - Compression ratio
   - Processing time

---

## /add-strategy

Scaffold a new optimization strategy.

```yaml
name: add-strategy
description: Add a new optimization strategy to the codebase
arguments:
  - name: name
    description: Strategy name (e.g., "semantic-compress")
    required: true
  - name: description
    description: What the strategy does
    required: true
```

### Implementation
1. Add variant to `StrategyType` enum in `src/optimization/mod.rs`
2. Add match arm in `PromptOptimizer::optimize()`
3. Create helper function stub in `src/optimization/strategies.rs`
4. Add to default strategy list if appropriate
5. Update README.md with new strategy

---

## /test-api

Test API connectivity and token counting.

```yaml
name: test-api
description: Test connection to an API provider
arguments:
  - name: provider
    description: Provider name (claude, openai, ollama)
    required: true
```

### Implementation
1. Check for required env vars (ANTHROPIC_API_KEY, etc.)
2. Send minimal test request
3. Display response metadata:
   - Model used
   - Token counts
   - Cache information (if Claude)
   - Response time
4. Report any errors

---

## /estimate-cost

Estimate API costs for a request.

```yaml
name: estimate-cost
description: Estimate token costs for a request
arguments:
  - name: task
    description: Task description
    required: true
  - name: context
    description: Context files (comma-separated)
    required: false
```

### Implementation
1. Build request from inputs
2. Estimate tokens:
   - System prompt
   - Context items
   - Task
   - Expected response (~1000 tokens)
3. Calculate costs for each provider:
   - Claude Sonnet: $3/$15 per 1M tokens
   - Claude Opus: $15/$75 per 1M tokens
   - GPT-4: $30/$60 per 1M tokens
4. Show with/without cache comparison

---

## /local-llm-setup

Help set up local LLM for preprocessing.

```yaml
name: local-llm-setup
description: Guide through Ollama setup for local preprocessing
```

### Implementation
1. Check if Ollama is installed
2. Check if Ollama is running
3. List available models
4. Recommend models for preprocessing:
   - `llama3.2` - Good balance
   - `qwen2.5-coder` - Code-focused
   - `deepseek-coder` - Alternative
5. Provide pull commands
6. Test with sample compression task

---

## /profile-request

Profile a full request through the optimization pipeline.

```yaml
name: profile-request
description: Profile token usage through the full pipeline
arguments:
  - name: task
    description: Task to profile
    required: true
  - name: context
    description: Context files
    required: false
```

### Implementation
1. Build initial request
2. Track tokens at each stage:
   - Original
   - After whitespace strip
   - After comment removal
   - After relevance filter
   - After truncation
3. Show waterfall of savings
4. Identify most impactful strategies
5. Suggest optimal strategy combination

---

## /watch-metrics

Watch token metrics in real-time during a session.

```yaml
name: watch-metrics
description: Display live token usage metrics
```

### Implementation
1. Start metrics display (refreshing)
2. Show:
   - Total tokens used
   - Tokens saved
   - Cache hit rate
   - Estimated cost
   - Requests made
3. Update on each API call
4. Show session summary on exit

---

## /generate-system-prompt

Generate an optimized system prompt for a specific use case.

```yaml
name: generate-system-prompt
description: Generate a token-efficient system prompt
arguments:
  - name: use-case
    description: The coding task type (bug-fix, feature, refactor, review)
    required: true
  - name: language
    description: Primary programming language
    required: false
```

### Implementation
1. Select base template for use case
2. Customize for language if specified
3. Optimize for token efficiency:
   - Remove redundant instructions
   - Use concise phrasing
   - Include only essential constraints
4. Verify meets cache minimum (1024 tokens)
5. Output with token count and caching recommendation
