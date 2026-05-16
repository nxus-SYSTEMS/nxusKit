# Minimal Build Guide

nxusKit uses feature flags to allow minimal builds with only the functionality you need.

## Feature Flags

| Feature | Description | Dependencies |
|---------|-------------|--------------|
| `default` | Core providers (Claude, OpenAI, Ollama, LM Studio) | None |
| `blocking-api` | Synchronous API wrapper (`BlockingProvider`) | None |
| `clips` | CLIPS expert system provider | clips-sys, parking_lot, chrono, regex |
| `pro` | Pro tier providers (Groq, Mistral, Fireworks, Together, OpenRouter, Perplexity) | None |
| `mcp` | Model Context Protocol support | rmcp |
| `stream-token-estimation` | Token counting during streaming | tiktoken-rs |
| `full` | All features (`clips` + `blocking-api`) | All clips deps |
| `all-providers` | All provider features (`pro` + `mcp`) | rmcp |

## Build Examples

### Minimal Build (Core Only)

```bash
cargo build --no-default-features -p nxuskit-engine
```

Includes: Claude, OpenAI, Ollama, LM Studio, Mock, Loopback providers.

### With Blocking API

```bash
cargo build --features blocking-api -p nxuskit-engine
```

Adds: `BlockingProvider<P>` wrapper for synchronous contexts.

### With CLIPS Expert System

```bash
cargo build --features clips -p nxuskit-engine
```

Adds: ClipsProvider for rule-based inference.

### Full Feature Set

```bash
cargo build --features full -p nxuskit-engine
```

Includes: All core features plus CLIPS and blocking API.

### Pro Providers

```bash
cargo build --features pro -p nxuskit-engine
```

Adds: Groq, Mistral, Fireworks, Together, OpenRouter, Perplexity providers.

## Cargo.toml Examples

### Minimal Dependency

```toml
[dependencies]
nxuskit-engine = "0.6"
```

### With Blocking API

```toml
[dependencies]
nxuskit-engine = { version = "0.6", features = ["blocking-api"] }
```

### Full Features

```toml
[dependencies]
nxuskit-engine = { version = "0.6", features = ["full"] }
```

### Custom Combination

```toml
[dependencies]
nxuskit-engine = { version = "0.6", features = ["blocking-api", "pro"] }
```

## Binary Size Impact

| Configuration | Approximate Impact |
|--------------|-------------------|
| Default | Baseline |
| + blocking-api | Minimal (~tokio Runtime) |
| + clips | +2-3 MB (CLIPS engine) |
| + pro | Minimal (just additional providers) |
| + stream-token-estimation | +1-2 MB (tiktoken) |

## Compile Time Impact

The `clips` feature has the largest compile time impact due to the CLIPS C library compilation. Consider:

- Using `clips` only in development builds if not needed in production
- Pre-compiling clips-sys in CI caches
- Using release builds which compile faster for CLIPS

## WASM Compatibility

nxusKit has limited WASM support. Feature compatibility:

| Feature | WASM Compatible | Notes |
|---------|-----------------|-------|
| Core providers | ✓ | Requires `wasm-bindgen` |
| `blocking-api` | ✗ | Uses tokio Runtime, not available in WASM |
| `clips` | ✗ | Requires native C library (clips-sys) |
| `stream-token-estimation` | ✗ | tiktoken-rs not WASM-compatible |
| `mcp` | Partial | Depends on transport implementation |

### WASM Build

For WASM targets, use minimal features:

```toml
[dependencies]
nxuskit-engine = { version = "0.7", default-features = false }
```

Note: WASM builds require a JavaScript fetch implementation. Consider using `gloo-net` or similar for HTTP requests.

## CI Optimization Tips

### Caching Strategies

1. **Cache cargo registry and git dependencies**:
   ```yaml
   - uses: actions/cache@v4
     with:
       path: |
         ~/.cargo/registry
         ~/.cargo/git
       key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}
   ```

2. **Cache target directory** (use with caution for feature matrix):
   ```yaml
   - uses: actions/cache@v4
     with:
       path: target
       key: ${{ runner.os }}-target-${{ hashFiles('**/Cargo.lock') }}-${{ matrix.features }}
   ```

3. **Pre-build clips-sys** in a separate job when using CLIPS feature:
   ```yaml
   clips-sys:
     runs-on: ubuntu-latest
     steps:
       - uses: actions/cache@v4
         id: clips-cache
         with:
           path: target/release/build/clips-sys-*
           key: clips-sys-${{ runner.os }}-${{ hashFiles('Cargo.lock') }}
       - if: steps.clips-cache.outputs.cache-hit != 'true'
         run: cargo build --release --features clips -p nxuskit-engine
   ```

### Feature Matrix Testing

Test feature combinations efficiently:

```yaml
strategy:
  matrix:
    include:
      - name: minimal
        features: ""
        flags: "--no-default-features"
      - name: blocking
        features: "blocking-api"
        flags: "--features blocking-api"
      - name: clips
        features: "clips"
        flags: "--features clips"
      - name: full
        features: "full"
        flags: "--features full"
```
