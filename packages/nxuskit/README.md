# nxuskit

Safe Rust wrapper for the nxusKit C ABI SDK.

Provides ergonomic Rust types and a safe API surface over the pre-built
`libnxuskit` shared (or static) library. All unsafe FFI calls are confined to
internal modules — consumers interact only with safe types.

The public SDK bundle includes nxusKit SDK Community Edition, which is free and
open source. Pro adds proprietary commercial capabilities such as solver-backed
workflows, ZEN decision tables, plugin loading, and trust-policy features.

## Prerequisites

1. **nxusKit SDK binary** for your platform:
   - Download from [GitHub Releases](https://github.com/nxus-SYSTEMS/nxusKit/releases)
   - Unpack to a known location (e.g., `/opt/nxuskit-sdk/` or `~/nxuskit-sdk/`)

2. **Environment variable** pointing to the SDK library directory:
   ```bash
   export NXUSKIT_LIB_DIR=/opt/nxuskit-sdk/lib
   # Or: export NXUSKIT_SDK_DIR=/opt/nxuskit-sdk  (lib/ subdirectory is inferred)
   ```

## Add Dependency

In your `Cargo.toml`:

```toml
[dependencies]
nxuskit = { git = "https://github.com/nxus-SYSTEMS/nxusKit", subdirectory = "packages/nxuskit" }
```

## Quickstart

### Capability Manifest Public Preview

The wrapper exposes the stable public Capability Manifest v2 projection types.
The public shape is intentionally small: status values and reviewed-on metadata
are public, while evidence records, model overrides, and provider-specific
internals stay in the engine registry.

```rust
use nxuskit::{
    CapabilityStatus, ManifestPublicationPosture, PublicCapabilityManifest,
    PublicProviderCapability, PUBLIC_CAPABILITY_FIELDS,
};
use std::collections::HashMap;

let manifest = PublicCapabilityManifest {
    schema_version: "capability-manifest-v2-public-preview/1".into(),
    posture: ManifestPublicationPosture::Split,
    providers: vec![PublicProviderCapability {
        name: "openai".into(),
        display_name: "OpenAI".into(),
        last_reviewed_on: "2026-05-09".into(),
        provider_status: "unknown".into(),
        capabilities: HashMap::from([
            ("json_schema_strict".into(), CapabilityStatus::Supported),
            ("rerank".into(), CapabilityStatus::Future),
        ]),
    }],
};

assert!(PUBLIC_CAPABILITY_FIELDS.contains(&"json_schema_strict"));
assert_eq!(manifest.providers[0].capabilities["rerank"], CapabilityStatus::Future);
```

### Synchronous Chat

```rust
use nxuskit::{ChatRequest, Message, NxuskitProvider, ProviderConfig, Role};

fn main() -> Result<(), nxuskit::NxuskitError> {
    let provider = NxuskitProvider::new(ProviderConfig {
        provider_type: "openai".into(),
        api_key: Some("sk-...".into()),
        model: Some("gpt-4o".into()),
        ..Default::default()
    })?;

    let response = provider.chat(ChatRequest {
        model: "gpt-4o".into(),
        messages: vec![
            Message { role: Role::User, content: "Hello!".into() },
        ],
        ..Default::default()
    })?;

    println!("Response: {}", response.content);
    println!("Tokens: {}", response.usage.estimated.completion_tokens);
    Ok(())
}
```

### Streaming Chat

```rust
use nxuskit::{ChatRequest, Message, NxuskitProvider, ProviderConfig, Role};

fn main() -> Result<(), nxuskit::NxuskitError> {
    let provider = NxuskitProvider::new(ProviderConfig {
        provider_type: "claude".into(),
        api_key: Some("sk-ant-...".into()),
        ..Default::default()
    })?;

    let stream = provider.chat_stream(ChatRequest {
        model: "claude-sonnet-4-5-20250929".into(),
        messages: vec![
            Message { role: Role::User, content: "Tell me a story".into() },
        ],
        ..Default::default()
    })?;

    for chunk in stream {
        match chunk {
            Ok(c) => print!("{}", c.delta),
            Err(e) => eprintln!("\nStream error: {e}"),
        }
    }
    println!();
    Ok(())
}
```

### CLIPS Inference

```rust
use nxuskit::{ChatRequest, Message, NxuskitProvider, ProviderConfig, Role};

fn main() -> Result<(), nxuskit::NxuskitError> {
    let provider = NxuskitProvider::new(ProviderConfig {
        provider_type: "clips".into(),
        model: Some("rules/medical-triage.clp".into()),
        ..Default::default()
    })?;

    let response = provider.chat(ChatRequest {
        model: "rules/medical-triage.clp".into(),
        messages: vec![
            Message {
                role: Role::User,
                content: r#"{"facts": [{"template": "patient", "values": {"symptoms": ["fever", "cough"], "age": 45}}]}"#.into(),
            },
        ],
        ..Default::default()
    })?;

    println!("Conclusions: {}", response.content);
    Ok(())
}
```

### Async Chat

```rust
use nxuskit::{ChatRequest, Message, NxuskitProvider, ProviderConfig, Role};

#[tokio::main]
async fn main() -> Result<(), nxuskit::NxuskitError> {
    let provider = NxuskitProvider::new(ProviderConfig {
        provider_type: "openai".into(),
        model: Some("gpt-4o".into()),
        ..Default::default()
    })?;

    let response = provider.chat_async(ChatRequest {
        model: "gpt-4o".into(),
        messages: vec![
            Message { role: Role::User, content: "Hello from async!".into() },
        ],
        ..Default::default()
    }).await?;

    println!("Response: {}", response.content);
    Ok(())
}
```

### Concurrent Async Requests

```rust
use nxuskit::{ChatRequest, Message, NxuskitProvider, ProviderConfig, Role};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = Arc::new(NxuskitProvider::new(ProviderConfig {
        provider_type: "openai".into(),
        model: Some("gpt-4o".into()),
        ..Default::default()
    })?);

    let mut handles = vec![];
    for i in 0..3 {
        let p = provider.clone();
        handles.push(tokio::spawn(async move {
            p.chat_async(ChatRequest {
                model: "gpt-4o".into(),
                messages: vec![Message {
                    role: Role::User,
                    content: format!("Task {i}: What is {i} + {i}?"),
                }],
                ..Default::default()
            }).await
        }));
    }

    for handle in handles {
        let response = handle.await??;
        println!("{}", response.content);
    }
    Ok(())
}
```

### Polymorphic Dispatch with `AsyncProvider`

```rust
use nxuskit::{AsyncProvider, ChatRequest, Message, NxuskitProvider, NxuskitError, ProviderConfig, Role};

async fn ask(provider: &dyn AsyncProvider, question: &str) -> Result<String, NxuskitError> {
    let request = ChatRequest {
        model: "gpt-4o".into(),
        messages: vec![Message { role: Role::User, content: question.into() }],
        ..Default::default()
    };
    Ok(provider.chat(request).await?.content)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider: Box<dyn AsyncProvider> = Box::new(
        NxuskitProvider::new(ProviderConfig {
            provider_type: "openai".into(),
            ..Default::default()
        })?
    );

    let answer = ask(&*provider, "What is Rust?").await?;
    println!("{answer}");
    Ok(())
}
```

### Async Model Discovery

```rust
use nxuskit::{NxuskitProvider, ProviderConfig};

#[tokio::main]
async fn main() -> Result<(), nxuskit::NxuskitError> {
    let provider = NxuskitProvider::new(ProviderConfig {
        provider_type: "ollama".into(),
        ..Default::default()
    })?;

    for model in provider.list_models_async().await? {
        println!("{}: {}", model.id, model.name);
    }
    Ok(())
}
```

### Error Handling

```rust
use nxuskit::{NxuskitError, NxuskitProvider, ProviderConfig};

fn main() {
    match NxuskitProvider::new(ProviderConfig {
        provider_type: "openai".into(),
        ..Default::default()
    }) {
        Ok(_) => println!("Provider created"),
        Err(NxuskitError::Configuration { message }) => {
            eprintln!("Config error: {message}");
        }
        Err(e) => eprintln!("Unexpected error: {e}"),
    }
}
```

### Model Discovery

```rust
use nxuskit::{NxuskitProvider, ProviderConfig};

fn main() -> Result<(), nxuskit::NxuskitError> {
    let provider = NxuskitProvider::new(ProviderConfig {
        provider_type: "ollama".into(),
        ..Default::default()
    })?;

    for model in provider.list_models()? {
        println!("{}: {}", model.id, model.name);
    }
    Ok(())
}
```

## MockProvider (Test Isolation)

`MockProvider` implements `AsyncProvider` without requiring the `libnxuskit` binary.
Use it in unit tests to isolate LLM integration logic.

### Basic Usage

```rust
use nxuskit::{AsyncProvider, ChatRequest, Message, MockProvider};

#[tokio::test]
async fn test_my_llm_logic() {
    let provider = MockProvider::new("The answer is 42");
    let request = ChatRequest::new("mock-model")
        .with_message(Message::user("What is the answer?"));
    let response = provider.chat(request).await.unwrap();
    assert_eq!(response.content, "The answer is 42");
}
```

### Sequential Responses

```rust
let provider = MockProvider::with_responses(vec!["Hello!", "Sure thing.", "Goodbye!"]);
// Each chat() call returns the next response; after exhaustion, last repeats.
```

### Request Recording

```rust
let provider = MockProvider::new("OK");
provider.chat(request).await.unwrap();

let recorded = provider.requests();
assert_eq!(recorded.len(), 1);
assert_eq!(recorded[0].model, "gpt-4o");
```

### Polymorphic Dispatch

```rust
let provider: Box<dyn AsyncProvider> = Box::new(MockProvider::new("Hello"));
// Use identically to a real NxuskitProvider
```

### Builder Configuration

```rust
use nxuskit::{MockProvider, ModelInfo};

let provider = MockProvider::builder()
    .with_response("Custom response")
    .with_model_name("gpt-4o")
    .with_models(vec![ModelInfo {
        id: "gpt-4o".into(),
        name: "GPT-4o".into(),
        size_bytes: None,
        context_window: Some(128000),
    }])
    .build();
```

## Builder Pattern

Ergonomic construction for `ChatRequest` and `Message`:

```rust
use nxuskit::{ChatRequest, Message, ThinkingMode};

let request = ChatRequest::new("gpt-4o")
    .with_message(Message::system("You are helpful."))
    .with_message(Message::user("Hello!"))
    .with_temperature(0.7)
    .with_max_tokens(1000)
    .with_thinking_mode(ThinkingMode::Enabled);
```

### Message Factories

```rust
use nxuskit::Message;

let system = Message::system("You are helpful.");
let user = Message::user("Hello!");
let assistant = Message::assistant("Hi! How can I help?");
```

These produce identical results to struct initialization:

```rust
// Before (verbose):
let msg = Message { role: Role::User, content: "Hello".into() };
// After (concise):
let msg = Message::user("Hello");
```

### Token Logprobs (unary chat since v0.9.3; streaming since v0.9.4)

First-class `with_logprobs(bool)` and `with_top_logprobs(u8)` builders;
typed `ChatResponse.logprobs: Option<LogprobsData>` with selected token
+ alternative tokens + UTF-8 bytes. Engine warn-and-drops when the
provider lacks `supports_logprobs` rather than tunneling through
`provider_options`. Streaming logprobs (`StreamChunk.logprobs:
Option<StreamLogprobsDelta>`) shipped in v0.9.4 - see "Streaming Logprobs"
below.

```rust
use nxuskit::{ChatRequest, Message};

let request = ChatRequest::new("gpt-5.4")
    .with_message(Message::user("Score the next token."))
    .with_logprobs(true)
    .with_top_logprobs(5);

let response = provider.chat(request)?;
if let Some(lp) = response.logprobs {
    let token = &lp.content[0];
    println!("{} (logprob {})", token.token, token.logprob);
    for alt in &token.top_logprobs {
        println!("  alt: {} ({})", alt.token, alt.logprob);
    }
}
```

The same wire shape (`logprobs.content[]` with typed `TokenLogprob` /
`TopLogprob` entries) is used by the Python SDK and the C ABI — see the
[migration guide](../../sdk-packaging/docs/logprobs-migration.md) for
before/after examples and Peeler adoption notes.

### Streaming Logprobs (v0.9.4+)

Per-chunk logprob deltas are now surfaced on streaming responses for
providers that support them (OpenAI). Check the capability flag before
issuing the call; non-supporting providers always emit `logprobs: None`
on every chunk (FR-007 — no phantom data).

```rust
use nxuskit::{ChatRequest, NxuskitProvider, Role, StreamChunk};
use futures::StreamExt;

let provider = NxuskitProvider::openai_from_env()?;

// Check capability before issuing the call.
let caps = provider.capabilities();
if !caps.supports_streaming_logprobs {
    eprintln!("Provider does not support streaming logprobs; logprobs field will be None.");
}

let req = ChatRequest::new("gpt-5.4")
    .with_message(Role::User, "Say hello.")
    .with_logprobs(true)
    .with_top_logprobs(3);

let mut stream = provider.chat_stream(&req).await?;
while let Some(chunk) = stream.next().await {
    let chunk: StreamChunk = chunk?;
    print!("{}", chunk.delta);
    if let Some(lp) = chunk.logprobs {
        for tok in lp.content {
            eprintln!("  token={:?} logprob={:.4}", tok.token, tok.logprob);
        }
    }
}
```

## Production Activation (v0.9.3)

Release builds default to `https://nxus.systems/licensing-api/v1` with
the embedded ES256 production key (`kid: es256-v1`). Activation is
offline-first after the first refresh — see
[`license-activation-guide.md`](../../sdk-packaging/docs/license-activation-guide.md)
for the full flow:

```bash
nxuskit-cli license login                     # one-time device-auth
nxuskit-cli license activate --key <id> --accept-eula --json
nxuskit-cli license status --json             # endpoint + key + edition
```

## License Key

Pass an optional license key for tiered feature access:

```rust
use nxuskit::ProviderConfig;

let config = ProviderConfig {
    provider_type: "openai".into(),
    license_key: Some("PRO-XXXX-YYYY".into()),
    ..Default::default()
};
```

The key is passed through to the SDK binary — no client-side validation. When
omitted, the field is absent from JSON (backward compatible with older SDK versions).

## Environment Variables

| Variable | Purpose | Example |
|----------|---------|---------|
| `NXUSKIT_LIB_DIR` | Direct path to directory containing `libnxuskit` | `/opt/nxuskit-sdk/lib` |
| `NXUSKIT_SDK_DIR` | SDK root directory (`lib/` subdirectory is inferred) | `/opt/nxuskit-sdk` |

**Use absolute paths.** Relative paths (e.g., `./sdk/lib`) are resolved against
the process working directory, which can differ between `cargo build`, `cargo
test`, and your binary at runtime. If you must use relative paths, the wrapper
will attempt to canonicalize them, but absolute paths are always more reliable.

If neither variable is set, the system library search path is used
(`LD_LIBRARY_PATH`, `DYLD_LIBRARY_PATH`, or Windows DLL search order).

Priority order: `NXUSKIT_LIB_DIR` > `NXUSKIT_SDK_DIR/lib` > system search path.

## Feature Flags

| Flag | Default | Description |
|------|---------|-------------|
| `dynamic-link` | **yes** | Load `libnxuskit` at runtime via `libloading`. No build-time SDK needed. |
| `static-link` | no | Link `libnxuskit` at build time. Requires SDK headers/libraries at compile time. |
| `licensing-client` | **yes** | Enable the built-in license token verification client. |

### `licensing-client` (default feature)

The `licensing-client` feature bundles the nxus licensing client, which handles
license token validation, tier enforcement, and machine binding. It is included
by default so that Pro features work out of the box without extra configuration.

**What it does:**
- Validates the license token from `NXUSKIT_LICENSE_TOKEN`, `~/.nxuskit/license.token`, or the `license_key` field in `ProviderConfig`
- Enforces edition tiers (Community / Pro) at the SDK layer
- Reports `NxuskitError::LicenseExpired`, `NxuskitError::EditionInsufficient`, etc.

**What `--no-default-features` means:**

Disabling default features removes `licensing-client` (and `dynamic-link`).
Without `licensing-client`, license validation is skipped at the SDK layer —
useful for:

- **Offline / air-gapped builds** where the licensing service is unreachable
- **Community Edition-only deployments** that never use Pro features
- **Test environments** where you want to avoid token management overhead

```toml
# Offline build — skip licensing client, use dynamic linking only
[dependencies]
nxuskit = { git = "...", subdirectory = "packages/nxuskit", default-features = false, features = ["dynamic-link"] }
```

> **Note:** Disabling `licensing-client` does not grant access to Pro features.
> Pro feature gates are also enforced server-side in the `libnxuskit` binary.
> Omitting the feature only bypasses the client-side pre-check.

To use static linking:

```toml
[dependencies]
nxuskit = { git = "...", subdirectory = "packages/nxuskit", default-features = false, features = ["static-link"] }
```

## Platform Support

| Platform | Architecture | Library | Status |
|----------|-------------|---------|--------|
| Linux | x86_64 | `libnxuskit.so` | Supported |
| macOS | aarch64 (Apple Silicon) | `libnxuskit.dylib` | Supported |
| Windows | x86_64 | `nxuskit.dll` | Supported |

## Performance Notes

Streaming overhead is minimal:
- Each chunk involves a single JSON parse + channel send
- Channel buffer size is 32 chunks (configurable internally)
- Expected per-chunk overhead is well under 5ms
- No additional allocations beyond JSON deserialization

Large responses are handled transparently — no special configuration is needed
for multi-MB JSON payloads.

## Edge Cases

- **Wrong-platform SDK binary**: Produces a linker or loader error at provider
  creation time. Ensure the SDK binary matches your target platform.
- **Missing SDK at runtime** (dynamic-link mode): Returns
  `NxuskitError::LibraryNotFound` with a message listing the search paths tried.
- **Version mismatch**: If the SDK major version differs from the wrapper, or
  the SDK minor version is lower, returns `NxuskitError::VersionMismatch`.

## CI Integration

### GitHub Actions

```yaml
name: Build with nxusKit SDK

on: [push]

jobs:
  build:
    strategy:
      matrix:
        include:
          - os: ubuntu-latest
            artifact: nxuskit-sdk-*-linux-x86_64.tar.gz
            lib_path_var: LD_LIBRARY_PATH
          - os: macos-latest
            artifact: nxuskit-sdk-*-macos-arm64.tar.gz
            lib_path_var: DYLD_LIBRARY_PATH
          - os: windows-latest
            artifact: nxuskit-sdk-*-windows-x86_64.zip
            lib_path_var: PATH
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4

      - name: Download nxusKit SDK
        env:
          GH_TOKEN: ${{ secrets.NXUSKIT_TOKEN }}
        run: |
          gh release download latest \
            --repo nxus-SYSTEMS/nxusKit \
            --pattern "${{ matrix.artifact }}"

      - name: Extract SDK (Unix)
        if: runner.os != 'Windows'
        run: tar xzf nxuskit-sdk-*.tar.gz

      - name: Extract SDK (Windows)
        if: runner.os == 'Windows'
        run: 7z x nxuskit-sdk-*.zip

      - name: Set SDK path
        run: echo "NXUSKIT_LIB_DIR=$PWD/nxuskit-sdk/lib" >> $GITHUB_ENV

      - name: Build
        run: cargo build --release

      - name: Test
        run: |
          export ${{ matrix.lib_path_var }}=$NXUSKIT_LIB_DIR:${!matrix.lib_path_var}
          cargo test
```

### Required Secrets

| Secret | Purpose | Setup |
|--------|---------|-------|
| `NXUSKIT_TOKEN` | Fine-grained PAT for downloading SDK releases | GitHub Settings > Developer Settings > Personal Access Tokens > Fine-grained > repo: `nxus-SYSTEMS/nxusKit` > Permission: Contents (read-only) |

## Migration from nxuskit-engine / clips-sys

### Before (source dependencies)

```toml
# Cargo.toml
[dependencies]
nxuskit-engine = { path = "../tmp-nxusKit/packages/nxuskit-engine/crates/nxuskit-engine" }
clips-sys = { path = "../tmp-nxusKit/packages/nxuskit-engine/crates/clips-sys" }
```

```rust
use nxuskit-engine::{ChatRequest, ChatResponse, Message, Role};
use clips_sys::ClipsEnvironment;  // Direct CLIPS access
```

### After (SDK wrapper)

```toml
# Cargo.toml
[dependencies]
nxuskit = { git = "https://github.com/nxus-SYSTEMS/nxusKit", subdirectory = "packages/nxuskit" }
```

```rust
use nxuskit::{ChatRequest, ChatResponse, Message, Role};
// CLIPS is accessed via provider_type: "clips" — no clips-sys import
```

### Migration Steps

1. Replace `nxuskit-engine` + `clips-sys` path dependencies with `nxuskit` git dependency
2. Change `use nxuskit-engine::*` to `use nxuskit::*`
3. Replace provider creation (builder pattern to `ProviderConfig` struct)
4. Remove any direct `clips-sys` usage (use `provider_type: "clips"` instead)
5. Set `NXUSKIT_LIB_DIR` environment variable in CI
