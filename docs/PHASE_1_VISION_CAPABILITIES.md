# Phase 1: Vision Capability Detection

## Overview

Phase 1 introduces a provider-agnostic capability detection system to nxusKit. Applications can now query model capabilities (vision support, streaming) without relying on brittle pattern matching or hardcoded lists.

## What's New

### 1. CapabilityDetector Trait

A new `CapabilityDetector` trait provides a unified interface for capability queries across all providers:

```rust
#[async_trait]
pub trait CapabilityDetector: Send + Sync {
    async fn get_model_capabilities(&self, model_name: &str) -> Result<ModelCapabilities>;
}
```

### 2. ModelCapabilities Struct

The `ModelCapabilities` struct exposes model features:

```rust
pub struct ModelCapabilities {
    pub supports_vision: bool,
    pub supports_streaming: bool,
}
```

### 3. OllamaProvider Implementation

`OllamaProvider` now implements `CapabilityDetector`. Capability detection uses Ollama's `/api/show` endpoint with fallback to name-based heuristics.

## Usage Examples

### Rust API

```rust
use nxuskit-engine::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = OllamaProvider::builder().build()?;

    // Get capabilities for a model
    let caps = provider.get_model_capabilities("llava:latest").await?;

    if caps.supports_vision {
        println!("This model supports vision!");
    }

    Ok(())
}
```

### CLI

```bash
# Query model capabilities
cargo run --example capability_detection

# Text output (default)
cargo run -- capabilities --provider ollama llava:latest

# JSON output
cargo run -- capabilities --provider ollama llava:latest --format json
```

## Implementation Details

### Vision Detection Strategy

For Ollama models, detection happens in this order:

1. **API Query**: Calls `/api/show` endpoint to check `capabilities` array
2. **Name Heuristics**: Falls back to checking model name for "vision", "llava", or "bakllava"
3. **Graceful Degradation**: Returns `false` if detection fails (conservative default)

### Streaming Support

All Ollama models are assumed to support streaming (may be refined in Phase 2).

## Known Models

### Vision-Capable (Confirmed)

- `llava:*` - Multi-image capable
- `llama3.2-vision:90b` - Multi-image capable
- `llama3.2-vision:latest` - Multi-image capable
- `qwen2.5vl:*` - Multi-image capable
- `minicpm-v:*` - Multi-image capable

### Text-Only (Confirmed)

- `llama3:*`
- `mistral:*`
- `neural-chat:*`

## Use Cases

### Model Selection UI

```rust
// Filter models that support vision
let vision_models: Vec<_> = models.iter()
    .filter(|m| {
        provider.get_model_capabilities(&m.name)
            .await
            .map(|c| c.supports_vision)
            .unwrap_or(false)
    })
    .collect();
```

### Pre-execution Validation

```rust
// Validate model capability before running pipeline
let caps = provider.get_model_capabilities(&selected_model).await?;
if !caps.supports_vision && requires_vision {
    return Err("Selected model does not support vision".into());
}
```

### Dynamic Feature Enablement

```rust
// Enable image inputs only if model supports vision
if provider.get_model_capabilities(&model).await?.supports_vision {
    // Show image upload UI
    enable_image_upload();
}
```

## Testing

Run the example to test capability detection:

```bash
# Basic example with multiple models
cargo run --example capability_detection
```

## Future Enhancements (Phase 2)

- Multi-image vs single-image distinction via `VisionMode` enum
- Capability caching with TTL to reduce API calls
- Extended metadata (max_images, supported_formats)
- Support for Claude and OpenAI providers
- Runtime learning from API responses

## Migration Guide

### For peeler-alpha

Replace pattern matching code:

```rust
// Old pattern matching approach
fn detect_vision_from_name(model_name: &str) -> (String, String) {
    if model_name.to_lowercase().contains("llava") {
        ("multi".to_string(), "👁👁".to_string())
    } else {
        ("none".to_string(), "".to_string())
    }
}

// New API-based approach
let caps = provider.get_model_capabilities(model_name).await?;
if caps.supports_vision {
    // Vision is supported
}
```

## Troubleshooting

### "Failed to detect capabilities"

**Issue**: See `DEBUG: Failed to detect capabilities...` messages

**Solution**: Ensure Ollama server is running and accessible at the configured base URL

```rust
let provider = OllamaProvider::builder()
    .base_url("http://localhost:11434")  // Verify this is correct
    .build()?;
```

### Incorrect capability detection

**Issue**: Model shows no vision support when it should have it

**Solution**: Check Ollama version supports `/api/show` capabilities endpoint. As fallback, capability detection still uses name-based heuristics.

## API Reference

### CapabilityDetector Trait

```rust
#[async_trait]
pub trait CapabilityDetector: Send + Sync {
    /// Get capabilities for a specific model
    async fn get_model_capabilities(&self, model_name: &str)
        -> Result<ModelCapabilities>;
}
```

### ModelCapabilities Struct

```rust
pub struct ModelCapabilities {
    /// Whether the model supports vision inputs
    pub supports_vision: bool,

    /// Whether the model supports streaming responses
    pub supports_streaming: bool,
}

impl ModelCapabilities {
    pub fn new() -> Self { /* ... */ }
    pub fn with_vision(self, supports_vision: bool) -> Self { /* ... */ }
    pub fn with_streaming(self, supports_streaming: bool) -> Self { /* ... */ }
}
```

## Performance Considerations

- Each `get_model_capabilities()` call makes an API request to `/api/show`
- For applications querying multiple models, consider batching or caching (Phase 2 feature)
- Vision detection can be disabled via `OLLAMA_DETECT_VISION=0` environment variable to skip API calls

## Contributing

To add capability detection for Claude or OpenAI providers:

1. Implement `CapabilityDetector` for the provider
2. Add test cases in `src/providers/{provider}.rs`
3. Update CLI to support the provider in the `capabilities` command
4. Add documentation with examples

See Phase 2 for roadmap updates.
