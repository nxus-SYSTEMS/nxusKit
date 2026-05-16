# Migration Guides

This document contains migration guides for major version changes in nxusKit.

---

# Migration Guide: v0.3.x → v0.4.0

This guide helps you migrate your code from nxusKit v0.3.x to v0.4.0.

## Overview

v0.4.0 introduces **breaking changes** to support graceful parameter degradation and provider capabilities:

1. **LLMProvider trait**: New required method `get_capabilities()`
2. **ChatResponse**: New fields `warnings` and `logprobs`

## Breaking Changes

### 1. LLMProvider Trait Enhancement

If you have custom providers implementing `LLMProvider`, you must add `get_capabilities()`:

```rust
// Before (v0.3.x)
impl LLMProvider for MyCustomProvider {
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse> {
        // ...
    }

    async fn chat_stream(&self, request: &ChatRequest)
        -> Result<impl Stream<Item = Result<StreamChunk>>> {
        // ...
    }
}

// After (v0.4.0) - Add get_capabilities()
impl LLMProvider for MyCustomProvider {
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse> {
        // ...
    }

    async fn chat_stream(&self, request: &ChatRequest)
        -> Result<impl Stream<Item = Result<StreamChunk>>> {
        // ...
    }

    // NEW: Required method
    fn get_capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_system_messages: true,
            supports_streaming: true,
            supports_vision: false,
            max_stop_sequences: Some(4),
            supports_presence_penalty: true,
            supports_frequency_penalty: true,
            supports_seed: false,
            supports_logprobs: false,
            supports_json_mode: false,
            supports_json_schema: false,
            penalty_range: Some((-2.0, 2.0)),
            max_logprobs: None,
        }
    }
}
```

### 2. ChatResponse New Fields

If you construct `ChatResponse` directly, add the new fields:

```rust
// Before (v0.3.x)
let response = ChatResponse {
    content: "Hello".to_string(),
    model: "gpt-4".to_string(),
    usage: TokenUsage::default(),
    finish_reason: Some(FinishReason::Stop),
    metadata: HashMap::new(),
};

// After (v0.4.0) - Add warnings and logprobs
let response = ChatResponse {
    content: "Hello".to_string(),
    model: "gpt-4".to_string(),
    usage: TokenUsage::default(),
    finish_reason: Some(FinishReason::Stop),
    metadata: HashMap::new(),
    warnings: Vec::new(),        // NEW
    logprobs: None,              // NEW
};
```

## Non-Breaking Additions

### New ChatRequest Fields (Optional)

All new fields are `Option<T>` and default to `None`:

```rust
let mut request = ChatRequest::new("gpt-4")
    .with_message(Message::user("Hello"));

// NEW optional fields (all work with existing code)
request.stop = Some(vec!["END".to_string()]);
request.presence_penalty = Some(0.5);
request.frequency_penalty = Some(0.3);
request.seed = Some(12345);
request.logprobs = Some(true);
request.top_logprobs = Some(5);
request.response_format = Some(ResponseFormat::Json);
request.provider_options = Some(ProviderOptions::Ollama(OllamaOptions {
    num_ctx: Some(4096),
    ..Default::default()
}));
```

### Response Warnings

Check for parameter adaptation warnings in responses:

```rust
let response = provider.chat(&request).await?;

// Check warnings (new in v0.4.0)
for warning in &response.warnings {
    match warning.severity {
        WarningSeverity::Info => println!("Info: {}", warning.message),
        WarningSeverity::Warning => println!("Warning: {}", warning.message),
        WarningSeverity::Error => println!("Error: {}", warning.message),
    }
}
```

### Provider Capabilities

Query provider capabilities at runtime:

```rust
let caps = provider.get_capabilities();

if caps.supports_json_mode {
    request.response_format = Some(ResponseFormat::Json);
}

if let Some(max_stops) = caps.max_stop_sequences {
    // Truncate stop sequences if needed
    if request.stop.as_ref().map(|s| s.len()).unwrap_or(0) > max_stops {
        // The ParameterAdapter handles this automatically
    }
}
```

## New Types Reference

### ResponseFormat

```rust
pub enum ResponseFormat {
    Text,                                    // Default text output
    Json,                                    // JSON mode
    JsonSchema { schema: serde_json::Value }, // JSON with schema (OpenAI)
}
```

### ParameterWarning

```rust
pub struct ParameterWarning {
    pub parameter: String,       // e.g., "stop", "presence_penalty"
    pub message: String,         // Human-readable warning
    pub severity: WarningSeverity,
}

pub enum WarningSeverity {
    Info,    // FYI, no impact
    Warning, // Potential impact (e.g., truncated)
    Error,   // Significant issue
}
```

### ProviderCapabilities

```rust
pub struct ProviderCapabilities {
    pub supports_system_messages: bool,
    pub supports_streaming: bool,
    pub supports_vision: bool,
    pub max_stop_sequences: Option<usize>,
    pub supports_presence_penalty: bool,
    pub supports_frequency_penalty: bool,
    pub supports_seed: bool,
    pub supports_logprobs: bool,
    pub supports_json_mode: bool,
    pub supports_json_schema: bool,
    pub penalty_range: Option<(f32, f32)>,
    pub max_logprobs: Option<u8>,
}
```

## Migration Checklist

- [ ] Update `Cargo.toml` to `nxuskit-engine = "0.4.0"`
- [ ] Run `cargo build` to find compile errors
- [ ] For custom providers: Add `get_capabilities()` method
- [ ] For custom ChatResponse construction: Add `warnings` and `logprobs` fields
- [ ] (Optional) Use new parameters in ChatRequest
- [ ] (Optional) Handle warnings in responses
- [ ] Run tests: `cargo test`

## Timeline

- **v0.4.0**: Current release with breaking changes
- **v0.9.0+**: Beta release (open source)
- **v1.0.0**: Stable release (API freeze)

---

# Migration Guide: v1.x → v2.0.0 (Historical)

This guide helps you migrate your code from nxusKit v1.x to v2.0.0.

## Overview

v2.0.0 introduces a **breaking change** to the `LLMProvider::list_models()` method:

**v1.x**: `async fn list_models(&self) -> Result<Vec<String>>`
**v2.0**: `async fn list_models(&self) -> Result<Vec<ModelInfo>>`

This change provides structured model information (size, context window, descriptions, metadata) instead of just names.

## Migration Strategies

### Strategy 1: Minimal Change (Extract Names Only)

If you only need model names, extract them from ModelInfo:

```rust
// Before (v1.x)
let models: Vec<String> = provider.list_models().await?;
for model in models {
    println!("Model: {}", model);
}

// After (v2.0) - Minimal change
let model_info = provider.list_models().await?;
let models: Vec<String> = model_info.iter().map(|m| m.name.clone()).collect();
for model in models {
    println!("Model: {}", model);
}
```

**Time required**: 2-5 minutes per call site
**Benefits**: Quick migration, minimal code changes
**Drawbacks**: Doesn't leverage new functionality

### Strategy 2: Leverage ModelInfo (Recommended)

Use the full ModelInfo structure for better UX:

```rust
// Before (v1.x)
let models = provider.list_models().await?;
for model in models {
    println!("  - {}", model);
}

// After (v2.0) - Recommended
let models = provider.list_models().await?;
for model in models {
    let size = model.formatted_size()
        .map(|s| format!(" ({})", s))
        .unwrap_or_default();
    let ctx = model.formatted_context_window()
        .map(|c| format!(" [{}]", c))
        .unwrap_or_default();

    println!("  - {}{}{}", model.name, size, ctx);

    if let Some(desc) = &model.description {
        println!("    {}", desc);
    }
}
```

**Time required**: 10-15 minutes per call site
**Benefits**: Better UX, displays useful model info
**Drawbacks**: Slightly more code

### Strategy 3: JSON Output for Tools

For CLI tools or APIs, use JSON serialization:

```rust
// After (v2.0) - JSON output
let models = provider.list_models().await?;
let json = serde_json::to_string_pretty(&models)?;
println!("{}", json);
```

**Output**:
```json
[
  {
    "name": "gpt-4o",
    "context_window": 128000,
    "description": "Most capable GPT-4 model...",
    "metadata": {
      "version": "4",
      "family": "gpt-4"
    }
  }
]
```

## Provider-Specific Changes

### Ollama

**Before (v1.x)**:
```rust
let models = ollama.list_models().await?;
// Returns: ["llama3:70b", "mistral:7b"]
```

**After (v2.0)**:
```rust
let models = ollama.list_models().await?;
// Returns: Vec<ModelInfo> with:
// - name: "llama3:70b"
// - size_bytes: Some(41503975700)
// - metadata: { "digest": "sha256:...", "modified_at": "..." }
```

### Claude

**Before (v1.x)**:
```rust
let models = claude.list_models().await?;
// Returns: [] (not supported)
```

**After (v2.0)**:
```rust
let models = claude.list_models().await?;
// Returns: Vec<ModelInfo> with 5 Claude models
// - All have context_window: Some(200_000)
// - All have descriptions
// - All have metadata: { "version": "3" or "3.5", "family": "..." }
```

### OpenAI

**Before (v1.x)**:
```rust
let models = openai.list_models().await?;
// Returns: [] (not supported)
```

**After (v2.0)**:
```rust
let models = openai.list_models().await?;
// Returns: Vec<ModelInfo> with 5 OpenAI models
// - Various context windows (8K, 16K, 128K)
// - All have descriptions
// - All have metadata: { "version": "...", "family": "..." }
```

## CLI Changes

### Before (v1.x)
```bash
$ nxuskit-cli models --provider ollama
Available models:
  - llama3:70b
  - mistral:7b
```

### After (v2.0)
```bash
$ nxuskit-cli models --provider ollama
Available models:
  - llama3:70b (38.7 GB)
  - mistral:7b (4.1 GB)

# JSON output
$ nxuskit-cli models --provider ollama --format json
[
  {
    "name": "llama3:70b",
    "size_bytes": 41503975700,
    "metadata": {
      "digest": "sha256:...",
      "modified_at": "2024-01-15T10:30:00Z"
    }
  }
]
```

## Common Migration Patterns

### Pattern 1: Filter Models by Name

**Before (v1.x)**:
```rust
let models = provider.list_models().await?;
let gpt4_models: Vec<String> = models.into_iter()
    .filter(|m| m.starts_with("gpt-4"))
    .collect();
```

**After (v2.0)**:
```rust
let models = provider.list_models().await?;
let gpt4_models: Vec<&ModelInfo> = models.iter()
    .filter(|m| m.name.starts_with("gpt-4"))
    .collect();
```

### Pattern 2: Display Model List in UI

**Before (v1.x)**:
```rust
let models = provider.list_models().await?;
for model in models {
    ui.add_item(model);
}
```

**After (v2.0)**:
```rust
let models = provider.list_models().await?;
for model in models {
    ui.add_item_with_details(
        &model.name,
        model.description.as_deref(),
        model.formatted_size().as_deref(),
        model.formatted_context_window().as_deref(),
    );
}
```

### Pattern 3: Find Specific Model

**Before (v1.x)**:
```rust
let models = provider.list_models().await?;
if models.contains(&"gpt-4".to_string()) {
    // Model exists
}
```

**After (v2.0)**:
```rust
let models = provider.list_models().await?;
if models.iter().any(|m| m.name == "gpt-4") {
    // Model exists
}
```

## Testing Your Migration

### Step 1: Update Dependencies

```toml
[dependencies]
nxuskit-engine = "2.0.0-alpha"
```

### Step 2: Run Compiler

```bash
cargo build
```

The compiler will identify all call sites that need updating with error messages like:
```
expected struct `ModelInfo`, found `std::string::String`
```

### Step 3: Run Tests

```bash
cargo test
```

### Step 4: Manual Testing

Test with each provider you use:

```bash
# Test Ollama
nxuskit-cli models --provider ollama

# Test Claude (with API key)
ANTHROPIC_API_KEY=xxx nxuskit-cli models --provider claude

# Test OpenAI (with API key)
OPENAI_API_KEY=xxx nxuskit-cli models --provider openai
```

## Troubleshooting

### Error: "expected Vec<ModelInfo>, found Vec<String>"

**Cause**: You're trying to return Vec<String> from list_models()

**Fix**: Return Vec<ModelInfo> instead:
```rust
async fn list_models(&self) -> Result<Vec<ModelInfo>> {
    Ok(vec![
        ModelInfo::new("model-1"),
        ModelInfo::with_size("model-2", 1_000_000_000),
    ])
}
```

### Error: "no method named `formatted_size` found"

**Cause**: You're calling formatted_size() on String instead of ModelInfo

**Fix**: Update to use ModelInfo:
```rust
// Before
for model in models {
    println!("{}", model);  // model is String
}

// After
for model in models {
    println!("{}", model.name);  // model is ModelInfo
}
```

### Error: Type mismatch in tests

**Cause**: Test expectations use String instead of ModelInfo

**Fix**: Update test assertions:
```rust
// Before
assert_eq!(models[0], "model-name");

// After
assert_eq!(models[0].name, "model-name");
```

## Rollback Plan

If you encounter issues and need to rollback:

1. Revert dependency:
   ```toml
   [dependencies]
   nxuskit-engine = "1.0"
   ```

2. Rebuild:
   ```bash
   cargo clean
   cargo build
   ```

3. No data migration needed (nxusKit is stateless)

## FAQ

**Q: Do I need to change my database schema?**
A: No, nxusKit is stateless and doesn't manage data persistence.

**Q: Will my v1.x code continue working?**
A: No, this is a breaking change. You must update code that calls list_models().

**Q: How long does migration typically take?**
A: Simple apps: 5-10 minutes. Complex apps: 15-30 minutes. Large codebases: 1-2 hours.

**Q: Can I use both v1.x and v2.0 in the same project?**
A: No, they have incompatible APIs. Choose one version per project.

**Q: What if I only care about model names?**
A: Use Strategy 1 (extract names) - see examples above.

**Q: Are there more breaking changes planned?**
A: No breaking changes planned for v2.x series. Next would be v3.0 (if ever needed).

## Support

- **Issues**: https://github.com/yourusername/nxuskit-engine/issues
- **Discussions**: https://github.com/yourusername/nxuskit-engine/discussions
- **Docs**: https://docs.rs/nxuskit-engine

## Version Support

| Version | Status | Support Level |
|---------|--------|---------------|
| v1.x | Deprecated | Bug fixes only (6 months) |
| v2.0.0-alpha | Current | Full support |
| v2.0.0 | Planned | TBD |
