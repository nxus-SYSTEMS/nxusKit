# nxusKit Project Architecture

## Overview

**nxusKit** is a polyglot LLM toolkit providing unified interfaces across multiple programming languages for interacting with LLM providers.

```
nxusKit/
├── packages/
│   ├── nxuskit-engine/          # Rust implementation (reference)
│   ├── nxuskit-go/          # Go implementation
│   ├── nxuskit-py/         # Python implementation
│   └── gateway/           # TypeScript API gateway (coming soon)
├── conformance/           # Cross-language test vectors
├── docs/                  # Documentation
└── tools/                 # Development utilities

Related repositories:
  nxusKit-examples/          # Runnable examples (apps, patterns, integrations)
```

---

## Repository Structure

### packages/

Language-specific implementations live under `packages/`:

```
packages/
├── nxuskit-engine/                 # Rust implementation
│   ├── Cargo.toml            # Workspace manifest
│   └── crates/
│       ├── nxuskit-engine/         # Core library
│       ├── nxuskit-engine-cli/     # CLI tool
│       └── clips-sys/        # CLIPS expert system bindings
├── nxuskit-go/                 # Go implementation
│   ├── go.mod                # Go module
│   └── internal/             # Internal packages
└── gateway/                  # TypeScript gateway (placeholder)
    ├── package.json
    └── README.md
```

### conformance/

Cross-language conformance testing infrastructure:

```
conformance/
├── examples_manifest.json    # Example parity definitions
├── harness/
│   └── schema.json           # Manifest schema
└── cases/
    ├── chat_completion.jsonl # Chat completion test vectors
    ├── streaming.jsonl       # Streaming test vectors
    └── tool_calls.jsonl      # Tool call test vectors
```

---

## Component 1: Rust Library (nxuskit-engine)

### Purpose
Type-safe, async interface to 14+ LLM providers. This is the reference implementation.

### Architecture
- **Provider Abstraction Layer**: Unified trait-based interface
- **Async-First Design**: Built on tokio for high-concurrency
- **Streaming Support**: Real-time response streaming
- **Vision/Multimodal**: Image inputs, vision models
- **Pro Stubs**: Interface stubs for commercial features

### Key Technologies
| Component | Purpose | Version |
|-----------|---------|---------|
| tokio | Async runtime | 1.35+ |
| reqwest | HTTP client | 0.12+ |
| serde | Serialization | 1.0 |
| thiserror | Error handling | 1.0 |

### File Structure
```
packages/nxuskit-engine/
├── Cargo.toml                # Workspace manifest
└── crates/
    ├── nxuskit-engine/
    │   └── src/
    │       ├── lib.rs        # Public API
    │       ├── providers/    # Provider implementations
    │       ├── pro.rs        # Pro feature stubs
    │       └── error.rs      # Error types
    ├── nxuskit-engine-cli/
    │   └── src/main.rs       # CLI entry point
    └── clips-sys/            # CLIPS FFI bindings
```

---

## Component 2: Go Library (nxuskit-go)

### Purpose
Idiomatic Go interface for LLM providers with context support and cloud-native focus.

### Architecture
- **Context-Based Cancellation**: Full context.Context support
- **Interface-Driven Design**: Provider interface for easy mocking
- **Streaming Support**: Channel-based streaming
- **Pro Stubs**: Interface stubs for commercial features

### Key Technologies
| Component | Purpose | Version |
|-----------|---------|---------|
| Go | Runtime | 1.22+ |
| net/http | HTTP client | stdlib |
| encoding/json | Serialization | stdlib |

### File Structure
```
packages/nxuskit-go/
├── go.mod                    # Go module
├── *.go                      # Core library files
├── pro.go                    # Pro feature stubs
├── errors.go                 # Error types
└── internal/                 # Internal packages
```

---

## Component 3: TypeScript Gateway (gateway) - Coming Soon

### Purpose
API gateway providing HTTP/WebSocket access to nxusKit functionality.

### Planned Architecture
- OpenAPI-compatible REST endpoints
- WebSocket support for streaming
- Multi-language client generation
- Rate limiting and authentication

---

## Cross-Language Parity

nxusKit maintains API consistency across implementations:

### Core Types
| Concept | Rust | Go |
|---------|------|-----|
| Chat Request | `ChatRequest` | `ChatRequest` |
| Chat Response | `ChatResponse` | `ChatResponse` |
| Message | `Message` | `Message` |
| Provider | `LLMProvider` trait | `Provider` interface |
| Error | `LlmError` enum | `*LLMError` struct |
| License Error | `LicenseRequired` | `ErrLicenseRequired` |

### Pro Features (Stubs)
Both implementations provide identical Pro feature stubs:
- `SemanticRouter` - Intelligent request routing
- `CostTracker` - Usage cost tracking
- `RetryPolicy` - Automatic retries
- `RequestBatcher` - Request batching
- `ResponseCache` - Response caching
- `LoadBalancer` - Multi-provider load balancing

---

## Development Workflow

### Setup

```bash
# Install just task runner
cargo install just

# View available tasks
just --list

# Format all code
just fmt

# Run all tests
just test

# Run conformance tests
just conformance
```

### Building

```bash
# Build Rust
just build-rust

# Build Go
just build-go

# Build all
just build
```

### Testing

```bash
# Test Rust
just test-rust

# Test Go
just test-go
```

---

## Related Repositories

- **[nxusKit-examples](https://github.com/nxus-SYSTEMS/nxusKit-examples)**: Runnable examples in Rust and Go — apps, patterns, and integration examples

---

## Decision Matrix

| I want to... | Use |
|--------------|-----|
| Build a Rust application with LLMs | `packages/nxuskit-engine/` |
| Build a Go application with LLMs | `packages/nxuskit-go/` |
| Access LLMs via HTTP API | `packages/gateway/` (coming soon) |
| Contribute a new provider | Both Rust and Go |
| Run conformance tests | `conformance/` |
| Explore examples | [nxusKit-examples](https://github.com/nxus-SYSTEMS/nxusKit-examples) |

---

## Next Steps

- **Getting Started**: [GETTING_STARTED.md](GETTING_STARTED.md)
- **Contributing**: [CONTRIBUTING.md](CONTRIBUTING.md)
- **Rust Quick Start**: [packages/nxuskit-engine/README.md](packages/nxuskit-engine/README.md)
- **Go Quick Start**: [packages/nxuskit-go/README.md](packages/nxuskit-go/README.md)
