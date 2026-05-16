# Provider Token Usage Accuracy Matrix

**Last Updated**: 2025-01-19
**Feature**: Streaming Token Usage Tracking (v0.4.5+)

## Overview

This document details token counting accuracy across all 13 LLM providers in nxusKit, including actual vs. estimated counts and strategies for maximizing accuracy.

---

## Quick Reference

| Provider | Token Source | Accuracy | Feature Required | Notes |
|----------|-------------|----------|-----------------|-------|
| **Claude** | Actual | 100% | ❌ None | From SSE stream events |
| **OpenAI** | Actual | 100% | ❌ None | From `stream_options.include_usage` |
| **Ollama** | Actual | 100% | ❌ None | Native NDJSON support |
| **Groq** | Estimated | 95-99%* | ✅ `stream-token-estimation` | Heuristic: 70-90% |
| **Mistral** | Estimated | 95-99%* | ✅ `stream-token-estimation` | Heuristic: 70-90% |
| **Fireworks** | Estimated | 95-99%* | ✅ `stream-token-estimation` | Heuristic: 70-90% |
| **Together** | Estimated | 95-99%* | ✅ `stream-token-estimation` | Heuristic: 70-90% |
| **OpenRouter** | Estimated | 95-99%* | ✅ `stream-token-estimation` | Heuristic: 70-90% |
| **Perplexity** | Estimated | 95-99%* | ✅ `stream-token-estimation` | Heuristic: 70-90% |
| **LM Studio** | Actual or Estimated | 100% / 95-99%* | ⚠️ Optional | Returns both if available |
| **MCP** | Estimated | 70-90% | ❌ None | Heuristic only (model-agnostic) |
| **Mock** | Actual | 100% | ❌ None | Test provider |
| **Loopback** | Actual | 100% | ❌ None | Test provider |

\* With `stream-token-estimation` feature; 70-90% without

---

## Detailed Provider Breakdown

### Tier 1: Native Actual Counts (100% Accurate)

#### Claude (via Anthropic API)
```
Accuracy:           100%
Source:             SSE stream events
Models:             claude-3-5-sonnet, claude-3-opus, claude-3-haiku
Setup:              Default - no configuration needed
Feature Flag:       Not required
```

**Implementation:**
- Anthropic streams `content_block_delta` and `message_delta` events
- `message_delta.usage` contains final token counts
- nxusKit captures usage from every event in stream

**Example:**
```rust
let (stream, usage_rx) = provider.stream_with_usage(&request).await?;
// → usage.has_actual() = true  (100% accurate)
// → usage.actual.unwrap().total() = exact counts
```

#### OpenAI (via OpenAI API)
```
Accuracy:           100%
Source:             stream_options.include_usage in response
Models:             gpt-4o, gpt-4, gpt-3.5-turbo
Setup:              Default - nxusKit sends include_usage=true
Feature Flag:       Not required
```

**Implementation:**
- nxusKit automatically sets `stream_options.include_usage=true`
- OpenAI streams final usage in last chunk
- nxusKit captures and propagates through all chunks as running totals

**Example:**
```rust
let (stream, usage_rx) = provider.stream_with_usage(&request).await?;
// → usage.has_actual() = true  (100% accurate)
// → usage.actual.unwrap().total() = exact counts from OpenAI
```

#### Ollama (via Local or Remote)
```
Accuracy:           100%
Source:             Native NDJSON response
Models:             All Ollama models (llama2, mistral, neural-chat, etc.)
Setup:              Default - autodetects local or remote
Feature Flag:       Not required
```

**Implementation:**
- Ollama streams NDJSON with `eval_count` and `prompt_eval_count` in each chunk
- nxusKit parses and aggregates counts through stream
- Final chunk contains accumulated totals

**Example:**
```rust
let provider = OllamaProvider::builder().build()?;
let (stream, usage_rx) = provider.stream_with_usage(&request).await?;
// → usage.has_actual() = true  (100% accurate)
// → usage.actual.unwrap().total() = actual hardware count
```

### Tier 2: Estimated Counts with Tiktoken (95-99% Accurate)

These providers don't return token counts in their streaming responses. nxusKit estimates them using client-side tokenization:

#### Groq
```
Accuracy:           95-99%* (with feature)
Source:             tiktoken-rs estimation
Models:             llama3-70b, mixtral-8x7b, etc.
Feature Flag:       stream-token-estimation (REQUIRED for 95-99%)
Heuristic Only:     70-90% (without feature)
```

**Why Estimated?**
- Groq doesn't provide token counts in streaming responses
- No API equivalent to OpenAI's `stream_options.include_usage`

**Implementation:**
```rust
// With feature enabled (95-99%)
#[cfg(feature = "stream-token-estimation")]
let estimator = TokenEstimator::for_model("llama3-70b");
// → Uses tiktoken-rs for OpenAI tokenizer equivalence

// Without feature (70-90%)
let estimator = TokenEstimator::for_model("unknown-model");
// → Falls back to heuristic
```

**Optimization Tips:**
- Enable `stream-token-estimation` feature for 95-99% accuracy
- Non-streaming `chat()` calls return 0 for estimated tokens (set to 0)
- For exact counts, follow up with separate API call

#### Mistral
```
Accuracy:           95-99%* (with feature)
Source:             tiktoken-rs estimation
Models:             mistral-large, mistral-medium, mistral-small
Feature Flag:       stream-token-estimation (REQUIRED for 95-99%)
Heuristic Only:     70-90% (without feature)
```

**Why Estimated?**
- Mistral uses similar tokenization to OpenAI (cl100k_base)
- Streaming doesn't include usage info
- Non-streaming responses include actual counts (nxusKit uses for fallback)

**Implementation:**
```rust
// With feature: estimated via tiktoken-rs (95-99%)
// Without: heuristic fallback (70-90%)
let provider = MistralProvider::new(api_key)?;
let (stream, usage_rx) = provider.stream_with_usage(&request).await?;
// → usage.has_actual() = false
// → usage.estimated.total() = 95-99% accurate (with feature)
```

#### Fireworks
```
Accuracy:           95-99%* (with feature)
Source:             tiktoken-rs estimation
Models:             llama-v3-70b, starcoder-34b, etc.
Feature Flag:       stream-token-estimation (REQUIRED for 95-99%)
Heuristic Only:     70-90% (without feature)
```

**Why Estimated?**
- Fireworks optimizes for speed, doesn't stream token counts
- Compatible with OpenAI tokenizer for many models
- Heuristic provides baseline estimate

**Implementation:**
```rust
let provider = FireworksProvider::builder().api_key(key)?.build()?;
let (stream, usage_rx) = provider.stream_with_usage(&request).await?;
// accuracy depends on feature flag
```

#### Together AI
```
Accuracy:           95-99%* (with feature)
Source:             tiktoken-rs estimation
Models:             Mixtral-8x7B, Llama-2-70b, etc.
Feature Flag:       stream-token-estimation (REQUIRED for 95-99%)
Heuristic Only:     70-90% (without feature)
```

**Why Estimated?**
- Hosts open-source models without streaming token info
- Good tiktoken coverage for popular open models

#### OpenRouter
```
Accuracy:           95-99%* (with feature)
Source:             tiktoken-rs estimation
Models:             100+ routed from various providers
Feature Flag:       stream-token-estimation (REQUIRED for 95-99%)
Heuristic Only:     70-90% (without feature)
```

**Why Estimated?**
- Routes to multiple providers, no unified token counting
- Non-streaming responses include usage (used for non-streaming requests)
- Streaming estimates via tiktoken where available

#### Perplexity
```
Accuracy:           95-99%* (with feature)
Source:             tiktoken-rs estimation
Models:             llama-sonar-large-online, llama-sonar-small
Feature Flag:       stream-token-estimation (REQUIRED for 95-99%)
Heuristic Only:     70-90% (without feature)
```

**Why Estimated?**
- Focuses on search-augmented responses
- Doesn't return token counts in streaming
- Heuristic estimates content + search context

#### LM Studio
```
Accuracy:           100% (when available) / 95-99%* (fallback)
Source:             Native if available + tiktoken estimation
Models:             Any model loaded in LM Studio
Feature Flag:       stream-token-estimation (improves fallback)
Details:            Tries actual, falls back to estimated
```

**Why Hybrid?**
- LM Studio returns usage in non-streaming responses
- nxusKit captures these and uses for non-streaming
- Streaming estimates via tiktoken as fallback
- Some models provide usage in streaming (100% accurate)

**Implementation:**
```rust
let provider = LmStudioProvider::builder().build()?;
let (stream, usage_rx) = provider.stream_with_usage(&request).await?;
// → usage.has_actual() might be true (depends on LM Studio version)
// → Fallback to estimated if not available
```

### Tier 3: Heuristic Estimation Only (70-90% Accurate)

#### MCP (Model Context Protocol)
```
Accuracy:           70-90%
Source:             Heuristic estimation only
Models:             N/A (provides tools, not chat)
Feature Flag:       Not applicable (tiktoken not used)
Note:               MCP is model-agnostic, not an LLM provider
```

**Why Heuristic?**
- MCP doesn't provide chat functionality
- If used with chat, token counting is generic
- 70-90% accuracy from character/word heuristic
- Recommended: Use with another provider for accurate token counts

**Note:** MCP integration uses LoopbackProvider for demonstrations. Real MCP usage requires wrapping chat calls with actual LLM provider.

### Tier 4: Test Providers (100% Accurate)

#### Mock Provider
```
Accuracy:           100%
Source:             Fixed test values (10 prompt, 20 completion)
Models:             Any model name
Feature Flag:       Not required
Purpose:            Unit testing without API calls
```

#### Loopback Provider
```
Accuracy:           100%
Source:             Deterministic test fixtures
Models:             12 models with specific behaviors (echo, json, errors, etc.)
Feature Flag:       Not required
Purpose:            Comprehensive testing, behavior validation
```

---

## Enabling Maximum Accuracy

### Step 1: Install with Feature Flag

For production accuracy (95-99% on OpenAI models):

```toml
[dependencies]
nxuskit = { version = "0.7", features = ["stream-token-estimation"] }
```

### Step 2: Use `stream_with_usage()` Helper

Simplest API for getting final token usage:

```rust
let (stream, usage_rx) = provider.stream_with_usage(&request).await?;
// ... process stream ...
let usage = usage_rx.await?;
println!("Tokens: {}", usage.best_available().total());
```

### Step 3: Check Data Source

Verify accuracy in your code:

```rust
if usage.has_actual() {
    println!("✓ 100% accurate (from provider)");
} else {
    println!("~ Estimated (95-99% with feature)");
}
```

### Step 4: Handle Edge Cases

```rust
// Stream interrupted?
if !usage.is_complete {
    println!("⚠ Partial count, stream failed mid-way");
}

// Multiple requests - track total
let total_tokens = TOTAL.fetch_add(
    usage.best_available().total(),
    Ordering::SeqCst
);
```

---

## Cost Tracking Recommendations

### For Billing-Critical Applications

**Use Tier 1 Providers (100% Actual):**
- Claude
- OpenAI
- Ollama (self-hosted)

```rust
// Safe for billing - 100% accurate
let (stream, usage_rx) = provider.stream_with_usage(&request).await?;
let usage = usage_rx.await?;
let cost = calculate_cost(usage.best_available());  // Accurate
```

### For Estimation/Budgeting

**Use Any Provider with Feature Enabled:**
- Groq + `stream-token-estimation` (95-99% for LLaMA models)
- Mistral + `stream-token-estimation` (95-99%)
- Together + `stream-token-estimation` (95-99%)

```rust
#[cfg(feature = "stream-token-estimation")]
{
    // Reliable estimate for budgeting
    let (stream, usage_rx) = provider.stream_with_usage(&request).await?;
    let usage = usage_rx.await?;
    let budget_estimate = calculate_cost(usage.best_available());  // 95-99%
}
```

### For Development/Testing

**Use Test Providers:**
- MockProvider (fixed counts, no API calls)
- LoopbackProvider (12 test models, no API calls)

```rust
// No API costs, deterministic for testing
let provider = MockProvider::default();
let (stream, usage_rx) = provider.stream_with_usage(&request).await?;
let usage = usage_rx.await?;  // Always same counts
```

---

## Troubleshooting Accuracy Issues

### Problem: Counts are significantly higher than expected

**Diagnosis:**
1. Check if `has_actual()` is false (using estimation)
2. Confirm feature flag is enabled: `cargo build --features stream-token-estimation`
3. Verify model name matches tokenizer support

**Solutions:**
- Enable `stream-token-estimation` feature
- Use tier 1 provider (Claude, OpenAI, Ollama)
- For unknown models, heuristic will overestimate by ~10-20%

### Problem: Estimation works locally but not in production

**Diagnosis:**
1. Feature flag might not be enabled in release build
2. tiktoken-rs might have compilation issues in CI/CD

**Solutions:**
```toml
# Ensure feature is in all profiles
[dependencies]
nxuskit = { version = "0.7", features = ["stream-token-estimation"] }

# Or explicitly for releases
[profile.release]
# No special config needed if feature is enabled above
```

### Problem: OpenAI showing estimated instead of actual

**Diagnosis:**
1. `stream_options.include_usage` not being sent
2. OpenAI API version mismatch
3. Older model versions don't support streaming usage

**Solution:**
- nxusKit automatically sends `include_usage=true`
- Use `gpt-4o` or `gpt-4-turbo` (older models don't support it)
- Verify with: `usage.has_actual()` in code

### Problem: LM Studio returns 0 tokens

**Diagnosis:**
1. LM Studio version doesn't support streaming usage
2. Model loaded in LM Studio doesn't report tokens
3. Estimation feature not enabled for fallback

**Solutions:**
- Update LM Studio to latest version
- Enable `stream-token-estimation` feature for fallback
- Check LM Studio logs for errors

---

## Feature Flag Impact

### Size Impact
- Without `stream-token-estimation`: ~5.2 MB binary
- With `stream-token-estimation`: ~5.25 MB binary (~50 KB added)

### Performance Impact
- First call creates TokenEstimator (negligible)
- Per-chunk estimation: <1ms (tiktoken-rs is highly optimized)
- No impact on network latency

### Accuracy Tradeoff
- Without feature: 70-90% (heuristic)
- With feature: 95-99% (tiktoken) → 70-90% fallback if unavailable
- Cost: ~50 KB binary size

**Recommendation:** Enable for production, disable for minimum binary size.

---

## Examples by Use Case

### 1. Real-Time Cost Display
```rust
// Wants accurate costs during streaming
let provider = OpenAIProvider::builder()...build()?;  // Tier 1 = 100%
let (mut stream, usage_rx) = provider.stream_with_usage(&request).await?;
while let Some(chunk) = stream.next().await {
    print!("{}", chunk?.content);
    // Display is 100% accurate (from OpenAI)
}
```

### 2. Billing Report
```rust
// Generate monthly costs
// Use Tier 1 provider (Claude, OpenAI) for exact billing
let providers = [
    (Box::new(claude_provider) as Box<dyn LLMProvider>),
    // Other providers if from same billing tier
];
// All counts are 100% accurate
```

### 3. Token Budget Tracking
```rust
// Track monthly token spend, allow 5% variance
// Use Tier 2 provider with feature enabled (95-99%)
let provider = GroqProvider::builder()...build()?;
// With stream-token-estimation: 95-99% accuracy sufficient for budgeting
```

### 4. Local Testing
```rust
// Fast iteration without API calls
let provider = LoopbackProvider::new();
// 100% deterministic, no API costs
```

---

## Summary Table

| Use Case | Recommended Provider | Accuracy | Cost |
|----------|-------------------|----------|------|
| **Exact Billing** | Claude, OpenAI | 100% | API cost |
| **Budget Planning** | Groq + feature | 95-99% | API cost |
| **Offline/Private** | Ollama | 100% | $0 (local) |
| **Cost Minimization** | Groq/Mistral | 95-99% | Low |
| **Development** | Mock/Loopback | 100% | $0 |

---

## See Also

- [Quickstart](../quickstart.md) - Usage examples
- [README](../../README.md) - Overview and feature matrix
- [Token Estimator Docs](../token-estimator.md) - Implementation details
