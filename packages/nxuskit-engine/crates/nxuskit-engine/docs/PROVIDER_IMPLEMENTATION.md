# Provider Implementation Guide

This guide documents the requirements for implementing new LLM providers in nxusKit.

## Timeout Configuration Requirements

**CRITICAL**: All providers MUST properly apply timeout configurations to the HTTP client.

### The Problem (Fixed in v0.4.3)

Prior to v0.4.3, providers stored timeout configuration values but created HTTP clients with `reqwest::Client::new()`, which ignored all configured timeouts. This caused:

- Streaming requests to fail with premature timeouts
- User frustration when correctly configured timeouts didn't work
- Inability to process long-running requests

### The Solution

Use the shared `build_http_client()` helper function to create HTTP clients with properly applied timeouts.

### Required Pattern

```rust
use super::build_http_client;

impl ProviderBuilder {
    pub fn build(self) -> Result<Provider> {
        // 1. Calculate timeout values with fallback chain
        let connection_timeout = self.connection_timeout
            .or(self.timeout)
            .unwrap_or(DEFAULT_CONNECTION_TIMEOUT);

        let stream_read_timeout = self.stream_read_timeout
            .or(self.timeout)
            .unwrap_or(DEFAULT_STREAM_READ_TIMEOUT);

        let total_timeout = self.total_timeout
            .or(self.timeout)
            .unwrap_or(DEFAULT_TOTAL_TIMEOUT);

        // 2. Build HTTP client with timeouts applied
        let client = build_http_client(
            connection_timeout,
            stream_read_timeout,
            total_timeout,
        )?;

        // 3. Return provider with configured client
        Ok(Provider {
            client,
            connection_timeout,
            stream_read_timeout,
            total_timeout,
            // ... other fields
        })
    }
}
```

### Timeout Types

| Timeout | Purpose | Default (Cloud) | Default (Local) |
|---------|---------|-----------------|-----------------|
| `connection_timeout` | TCP connection establishment | 60s | 120s |
| `stream_read_timeout` | Time between response chunks | 120s | 180s |
| `total_timeout` | Entire request duration | 60s | 120s |

- **Cloud providers** (Claude, OpenAI, etc.): Use shorter defaults
- **Local providers** (Ollama): Use longer defaults to accommodate variable hardware

### Testing Requirements

All providers MUST be tested with the `verify_provider_respects_timeout` helper:

```rust
#[tokio::test]
async fn test_new_provider_respects_timeout() {
    let mock_server = MockServer::start().await;

    // Set up mock with delay longer than configured timeout
    Mock::given(method("POST"))
        .and(path("/your/endpoint"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(5)))
        .mount(&mock_server)
        .await;

    let provider = NewProvider::builder()
        .total_timeout(Duration::from_secs(2))
        .build()
        .expect("Failed to build provider");

    verify_provider_respects_timeout(provider, 2).await;
}
```

### Checklist for New Providers

- [ ] Uses `build_http_client()` to create the HTTP client
- [ ] Supports all three timeout configuration methods:
  - `connection_timeout(Duration)`
  - `stream_read_timeout(Duration)`
  - `total_timeout(Duration)`
  - `timeout(Duration)` (general fallback)
- [ ] Has appropriate default timeout values
- [ ] Includes timeout verification tests
- [ ] Documents timeout behavior in rustdoc

### Anti-Patterns to Avoid

```rust
// BAD: Creates client with default timeouts, ignoring configuration
client: reqwest::Client::new(),

// BAD: Only sets some timeouts
let client = reqwest::Client::builder()
    .timeout(total_timeout)
    .build()?;

// GOOD: Uses helper function with all timeouts
let client = build_http_client(connection_timeout, stream_read_timeout, total_timeout)?;
```
