# Integration Patterns Guide

This guide covers common integration patterns for using nxusKit in various contexts.

## Polymorphic Provider Usage

When building systems that work with multiple providers, use the `ModelLister` trait for polymorphic access to model discovery.

### Provider Registry Pattern

```rust
use nxuskit::provider::ModelLister;
use nxuskit::providers::{OllamaProvider, LmStudioProvider, MockProvider};
use std::collections::HashMap;

struct ProviderRegistry {
    providers: HashMap<String, Box<dyn ModelLister>>,
}

impl ProviderRegistry {
    fn new() -> Self {
        Self { providers: HashMap::new() }
    }

    fn register(&mut self, name: &str, provider: impl ModelLister + 'static) {
        self.providers.insert(name.to_string(), Box::new(provider));
    }

    async fn discover_all_models(&self) -> Vec<(String, Vec<nxuskit::ModelInfo>)> {
        let mut results = Vec::new();
        for (name, provider) in &self.providers {
            if let Ok(models) = provider.list_available_models().await {
                results.push((name.clone(), models));
            }
        }
        results
    }
}

// Usage
let mut registry = ProviderRegistry::new();
registry.register("ollama", OllamaProvider::new());
registry.register("lmstudio", LmStudioProvider::new());

let all_models = registry.discover_all_models().await;
```

### Why ModelLister?

The `ModelLister` trait is separate from `LLMProvider` to ensure correct vtable dispatch when using trait objects. This allows `Box<dyn ModelLister>` to correctly call the provider's implementation.

## Deterministic Evaluation

For CI/CD pipelines and reproducible test results, use `fresh_session()` to ensure each test run starts with a clean provider state.

### Testing Pattern

```rust
use nxuskit::providers::ClipsProvider;
use nxuskit::LLMProvider;

#[tokio::test]
async fn test_inference_determinism() {
    let provider = ClipsProvider::builder()
        .rules_directory("./rules")
        .build()
        .unwrap();

    let request = create_test_request();

    // Run 1: Fresh session
    let result1 = provider.fresh_session().chat(&request).await.unwrap();

    // Run 2: Another fresh session - should produce identical results
    let result2 = provider.fresh_session().chat(&request).await.unwrap();

    assert_eq!(result1.content, result2.content);
}
```

### Provider-Specific Behavior

| Provider | fresh_session() Behavior |
|----------|-------------------------|
| ClipsProvider | Clears environment cache, returns fresh instance |
| MockProvider | Returns new instance with same configuration |
| OllamaProvider | Returns clone (stateless) |
| LmStudioProvider | Returns clone (stateless) |
| API Providers | Returns clone (stateless) |

## Synchronous Contexts

For applications that cannot use async/await (e.g., immediate-mode GUIs, simple scripts), use `BlockingProvider`.

### Basic Usage

```rust
use nxuskit::blocking::BlockingProvider;
use nxuskit::providers::MockProvider;
use nxuskit::types::{ChatRequest, Message};
use nxuskit::LLMProvider;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mock = MockProvider::builder()
        .with_response("Hello!")
        .build()?;

    // Wrap for synchronous use
    let blocking = BlockingProvider::new(mock)?;

    let request = ChatRequest::new("model")
        .with_message(Message::user("Hi"));

    // Blocking call - no async/await needed
    let response = blocking.chat(&request)?;
    println!("{}", response.content);

    Ok(())
}
```

### Model Listing

If the wrapped provider implements `ModelLister`, you can list models synchronously:

```rust
use nxuskit::blocking::BlockingProvider;
use nxuskit::providers::OllamaProvider;

let ollama = OllamaProvider::new();
let blocking = BlockingProvider::new(ollama)?;

// Synchronous model listing
let models = blocking.list_models()?;
for model in models {
    println!("- {}", model.name);
}
```

### Feature Flag

`BlockingProvider` requires the `blocking-api` feature:

```toml
[dependencies]
nxuskit = { version = "0.7", features = ["blocking-api"] }
```

## Response Metadata

All providers populate `inference_metadata` in responses for consistent access to execution details:

```rust
let response = provider.chat(&request).await?;

// Common fields available across all providers
println!("Complete: {}", response.inference_metadata.is_complete);
println!("Finish reason: {:?}", response.inference_metadata.finish_reason);

// Provider-specific metadata
if let Some(meta) = &response.inference_metadata.provider_metadata {
    println!("Provider details: {}", meta);
}

// For CLIPS, access inference steps (rule firings)
if let Some(steps) = &response.inference_metadata.inference_steps {
    for step in steps {
        println!("Rule fired: {} (salience: {})",
            step.identifier,
            step.details.as_ref()
                .and_then(|d| d.get("salience"))
                .unwrap_or(&serde_json::Value::Null)
        );
    }
}
```

## Error Handling Patterns

nxusKit uses a unified error type `LlmError` across all providers. This section covers common error handling patterns.

### Basic Error Handling

```rust
use nxuskit::error::LlmError;
use nxuskit::LLMProvider;

async fn handle_chat(provider: &impl LLMProvider, request: &ChatRequest) {
    match provider.chat(request).await {
        Ok(response) => println!("Response: {}", response.content),
        Err(LlmError::RateLimit { retry_after }) => {
            if let Some(duration) = retry_after {
                println!("Rate limited, retry after {:?}", duration);
                tokio::time::sleep(duration).await;
                // Retry...
            }
        }
        Err(LlmError::AuthenticationError(msg)) => {
            eprintln!("API key invalid: {}", msg);
        }
        Err(LlmError::ModelNotFound(model)) => {
            eprintln!("Model '{}' not found", model);
        }
        Err(e) => {
            eprintln!("Unexpected error: {}", e);
        }
    }
}
```

### Provider-Specific Errors

Different providers may return different error types for similar conditions:

| Error Type | When It Occurs |
|------------|----------------|
| `RateLimit { retry_after }` | API rate limit exceeded (HTTP 429) |
| `AuthenticationError` | Invalid or missing API key |
| `ModelNotFound` | Requested model doesn't exist |
| `RequestTimeout` | Request exceeded timeout |
| `ApiError` | Provider-specific API error |
| `InvalidRequest` | Malformed request parameters |

### CLIPS-Specific Errors

The CLIPS provider has additional error cases:

```rust
use nxuskit::providers::ClipsProvider;

match clips.chat(&request).await {
    Ok(response) => { /* process response */ }
    Err(LlmError::ModelNotFound(path)) => {
        // Rule file not found at path
        eprintln!("Rule file not found: {}", path);
    }
    Err(LlmError::InvalidRequest(msg)) => {
        // JSON input parsing failed or invalid template
        eprintln!("Invalid CLIPS input: {}", msg);
    }
    Err(e) => {
        eprintln!("CLIPS error: {}", e);
    }
}
```

### Retry Pattern with Exponential Backoff

```rust
use std::time::Duration;
use nxuskit::error::LlmError;

async fn chat_with_retry<P: LLMProvider>(
    provider: &P,
    request: &ChatRequest,
    max_retries: u32,
) -> Result<ChatResponse, LlmError> {
    let mut attempt = 0;
    let base_delay = Duration::from_millis(100);

    loop {
        match provider.chat(request).await {
            Ok(response) => return Ok(response),
            Err(LlmError::RateLimit { retry_after }) => {
                attempt += 1;
                if attempt > max_retries {
                    return Err(LlmError::RateLimit { retry_after });
                }

                let delay = retry_after.unwrap_or(base_delay * 2u32.pow(attempt - 1));
                tokio::time::sleep(delay).await;
            }
            Err(e) => return Err(e),
        }
    }
}
```

### fresh_session() Error Handling

Some providers' `fresh_session()` returns `Result`:

```rust
// Providers returning Self directly
let mock_fresh = mock.fresh_session(); // MockProvider
let loopback_fresh = loopback.fresh_session(); // LoopbackProvider

// Providers returning Result<Self>
let ollama_fresh = ollama.fresh_session()?; // OllamaProvider
let clips_fresh = clips.fresh_session()?; // ClipsProvider (feature = "clips")
let lmstudio_fresh = lmstudio.fresh_session()?; // LmStudioProvider
```

## CLIPS Ordering Guarantees

When working with CLIPS, the output is deterministically ordered for reproducibility:

### Conclusion Ordering

Conclusions (derived facts) are sorted by `fact_index`, ensuring consistent output regardless of rule firing order:

```rust
// Conclusions are always sorted by fact_index
if let Some(output) = response.as_clips_output() {
    // Conclusions are guaranteed to be in fact_index order
    for conclusion in &output.conclusions {
        println!("Fact {}: {}", conclusion.fact_index, conclusion.template);
    }
}
```

### Rule Firing Ordering

When trace is enabled, rules are sorted alphabetically by name:

```rust
if let Some(trace) = &output.trace {
    // Rules are guaranteed to be sorted by name
    for rule in &trace.rules_fired {
        println!("{}: fired {} times", rule.rule_name, rule.fire_count);
    }
}
```

### Conflict Strategy

The conflict resolution strategy is recorded in `provider_metadata`:

```rust
if let Some(meta) = &response.inference_metadata.provider_metadata {
    if let Some(strategy) = meta.get("conflict_strategy") {
        println!("Strategy used: {}", strategy);
    }
}
```

Available strategies: `depth` (default), `breadth`, `random`, `complexity`, `simplicity`, `lex`, `mea`

### Streaming Mode Ordering

When using CLIPS streaming (via `stream_mode` in request config):

| Mode | Chunk Content | Ordering |
|------|---------------|----------|
| `Default` | Single chunk with all results | N/A |
| `Fact` | One chunk per derived fact | By fact_index |
| `Rule` | One chunk per rule firing | By rule firing sequence |

For deterministic testing, use `fresh_session()` to ensure clean state between runs.
