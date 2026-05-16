# Getting Started with nxusKit

Welcome to nxusKit! This guide helps you set up your development environment and start using the polyglot LLM toolkit.

## Table of Contents

- [Prerequisites](#prerequisites)
- [Path 1: Rust Development](#path-1-rust-development-nxuskit)
- [Path 2: Go Development](#path-2-go-development-nxuskit-go)
- [CLI Tool](#cli-tool)
  - [Level 1 Commands (Machine-Facing)](#level-1-commands-machine-facing)
- [Path 3: Contributing to nxusKit](#path-3-contributing-to-nxuskit)
- [Environment Variables](#environment-variables-reference)
- [Troubleshooting](#troubleshooting)

---

## Prerequisites

### Required Tools

| Tool | Version | Installation |
|------|---------|--------------|
| Git | 2.0+ | [git-scm.com](https://git-scm.com/) |
| Rust | 1.75+ | [rustup.rs](https://rustup.rs/) |
| Go | 1.22+ | [go.dev](https://go.dev/dl/) |

### Verify Installation

```bash
git --version      # git version 2.x.x
rustc --version    # rustc 1.75.0 or higher
go version         # go version go1.22.x or higher
```

### Optional Tools

| Tool | Purpose | Installation |
|------|---------|--------------|
| just | Task runner | `cargo install just` |
| golangci-lint | Go linting | [golangci-lint.run](https://golangci-lint.run/usage/install/) |
| Ollama | Local models | [ollama.ai](https://ollama.ai/) |

---

## Path 1: Rust Development (nxuskit)

Use this if you're building Rust applications with LLM providers.

**Time to first success: ~5 minutes**

### 1. Create a New Project

```bash
cargo new my_llm_app
cd my_llm_app
```

### 2. Add Dependencies

```toml
# Cargo.toml
[dependencies]
nxuskit = "0.7"
tokio = { version = "1.35", features = ["macros", "rt-multi-thread"] }
```

### 3. Write Your First Program

```rust
// src/main.rs
use nxuskit::completion;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set OPENAI_API_KEY environment variable first
    let response = completion("gpt-4o", "What is Rust?").await?;
    println!("{}", response);
    Ok(())
}
```

### 4. Set Up API Key

```bash
# Choose one provider:
export OPENAI_API_KEY="sk-..."           # OpenAI
export ANTHROPIC_API_KEY="sk-ant-..."    # Anthropic (Claude)
export OLLAMA_BASE_URL="http://localhost:11434"  # Local Ollama
```

### 5. Run It

```bash
cargo run
```

### Common Tasks

**Switch providers:**
```rust
let response = completion("claude-sonnet-4-5", "Hello").await?;  // Anthropic
let response = completion("ollama/llama2", "Hello").await?;      // Ollama
let response = completion("gpt-4o", "Hello").await?;             // OpenAI
```

**Stream responses:**
```rust
use nxuskit::completion_stream;
use futures::StreamExt;

let mut stream = completion_stream("gpt-4o", "Count to 5").await?;
while let Some(chunk) = stream.next().await {
    print!("{}", chunk?);
}
```

### Next Steps

- [README.md](README.md#quick-start) - More examples and features
- [packages/nxuskit-engine/](packages/nxuskit-engine/) - Engine source and internals
- Run `cargo doc --open` for API documentation

---

## Path 2: Go Development (nxuskit-go)

Use this if you're building Go applications with LLM providers.

**Time to first success: ~5 minutes**

### 1. Create a New Project

```bash
mkdir my_llm_app
cd my_llm_app
go mod init my_llm_app
```

### 2. Add Dependencies

```bash
go get github.com/nxus-SYSTEMS/nxusKit/packages/nxuskit-go
```

### 3. Write Your First Program

```go
// main.go
package main

import (
    "context"
    "fmt"
    "os"

    "github.com/nxus-SYSTEMS/nxusKit/packages/nxuskit-go"
)

func main() {
    // Create provider
    provider, err := nxuskit-go.NewOpenAIProvider(
        nxuskit-go.WithAPIKey(os.Getenv("OPENAI_API_KEY")),
    )
    if err != nil {
        panic(err)
    }

    // Send request
    req := nxuskit-go.ChatRequest{
        Model: "gpt-4o",
        Messages: []nxuskit-go.Message{
            {Role: nxuskit-go.RoleUser, Content: "What is Go?"},
        },
    }

    resp, err := provider.Chat(context.Background(), req)
    if err != nil {
        panic(err)
    }

    fmt.Println(resp.Content)
}
```

### 4. Set Up API Key

```bash
export OPENAI_API_KEY="sk-..."
```

### 5. Run It

```bash
go run .
```

### Common Tasks

**Switch providers:**
```go
// Anthropic
provider, _ := nxuskit-go.NewClaudeProvider(
    nxuskit-go.WithAPIKey(os.Getenv("ANTHROPIC_API_KEY")),
)

// Ollama (local)
provider, _ := nxuskit-go.NewOllamaProvider(
    nxuskit-go.WithBaseURL("http://localhost:11434"),
)
```

**Stream responses:**
```go
stream, err := provider.ChatStream(ctx, req)
if err != nil {
    panic(err)
}

for chunk := range stream {
    if chunk.Error != nil {
        panic(chunk.Error)
    }
    fmt.Print(chunk.Delta)
}
```

### Next Steps

- [packages/nxuskit-go/README.md](packages/nxuskit-go/README.md) - Full documentation
- [nxusKit-examples](https://github.com/nxus-SYSTEMS/nxusKit-examples) - Runnable examples in Rust and Go
- Run `go doc` for API documentation

---

## CLI Tool

nxusKit includes a Rust CLI (`nxuskit-cli`) for interacting with providers, querying models, converting schemas, and managing pipelines from the command line.

### Building

```bash
cd packages/nxuskit-engine
cargo build --release
# Binary: target/release/nxuskit-cli
```

### Commands

#### `chat` — Send a message to a provider

```bash
# Basic chat
nxuskit-cli chat -p openai -m gpt-4o "What is Rust?"

# Stream response
nxuskit-cli chat -p ollama -m llama3.2 --stream "Tell me a story"

# Set temperature
nxuskit-cli chat -p claude -m claude-sonnet-4-20250514 --temperature 0.7 "Write a haiku"
```

**Flags**: `-p/--provider`, `-m/--model`, `--stream`, `--temperature`, `--max-tokens`, `-f/--format` (text/json)

#### `models` — List available models

```bash
# List models from a provider
nxuskit-cli models -p ollama

# JSON output
nxuskit-cli models -p ollama -f json
```

**Flags**: `-p/--provider`, `-f/--format` (text/json)

#### `capabilities` — Query model capabilities

```bash
# Check what a model supports
nxuskit-cli capabilities -p ollama llava:latest

# JSON output
nxuskit-cli capabilities -p openai gpt-4o -f json
```

Returns vision mode, streaming support, and function calling support. For providers with dynamic detection (Ollama, LM Studio), results are model-specific. For other providers, conservative defaults are returned with `source: "default"`.

**Flags**: `-p/--provider`, `-f/--format` (text/json)

#### `schema` — Convert between CLIPS and JSON Schema

```bash
# CLIPS deftemplate → JSON Schema
nxuskit-cli schema to-json rules.clp

# JSON Schema → CLIPS deftemplate
nxuskit-cli schema to-clips schema.json
```

#### `pipeline` — Manage pipeline definitions

```bash
# Validate a pipeline file
nxuskit-cli pipeline validate pipeline.yaml

# Convert between JSON and YAML
nxuskit-cli pipeline convert input.json -o output.yaml
```

### Level 1 Commands (Machine-Facing)

Level 1 commands produce structured JSON/YAML/JSONL output with trace envelopes, stable exit codes, and deterministic entitlement errors. They are designed for shell scripting, CI pipelines, and Team-of-Teams workflows.

All L1 commands share common flags: `--input` (`-i`), `--output` (`-o`), `--format` (`-f`), `--quiet` (`-q`).

#### `call` — JSON-first LLM invocation

```bash
# From stdin
echo '{"prompt":"hello","provider":"loopback","model":"echo"}' \
  | nxuskit-cli call --input - --format json

# Streaming JSONL
echo '{"prompt":"hello","provider":"loopback","model":"echo"}' \
  | nxuskit-cli call --input - --format jsonl --stream

# From file
nxuskit-cli call --input prompt.json --format json --output result.json
```

#### `pipeline run` — Multi-stage pipeline execution

```bash
# Execute a YAML pipeline (stages run sequentially, halt on first failure)
nxuskit-cli pipeline run --input pipeline.yaml --format json

# Stream stage-complete events as JSONL
nxuskit-cli pipeline run --input pipeline.yaml --format jsonl
```

#### Pro ZEN and solver commands

ZEN and solver command discovery is available in CE, but implementations and detailed input contracts are Pro capabilities. See Pro-labeled product documentation for licensed usage details.

#### `clips eval` — CLIPS rule evaluation

```bash
# Rules must be on one line or use proper JSON escaping (no literal \n)
echo '{"rules":"(deftemplate temp (slot value)) (deftemplate action (slot type)) (defrule hot (temp (value ?t&:(> ?t 30))) => (assert (action (type cool))))","facts":["(temp (value 35))"]}' \
  | nxuskit-cli clips eval --input - --format json
```

> **Note**: Literal `\n` characters in the rules string cause JSON parse errors. Use single-line rules or proper JSON escaping (`\\n`).

#### `bn infer` — Bayesian network inference

```bash
# Infer posterior probabilities given evidence
echo '{"network":{"nodes":[{"name":"Rain","states":["yes","no"]},{"name":"Wet","states":["yes","no"]}],"edges":[{"from":"Rain","to":"Wet"}],"cpds":[{"node":"Rain","parents":[],"probabilities":[0.3,0.7]},{"node":"Wet","parents":["Rain"],"probabilities":[0.9,0.1,0.2,0.8]}]},"evidence":{"Rain":"yes"},"query_nodes":["Wet"]}' \
  | nxuskit-cli bn infer --input - --format json
```

#### `packet validate` — JSON Schema validation

```bash
nxuskit-cli packet validate --input data.json --schema schema.json --format json
```

#### `artifact merge` / `artifact summarize`

```bash
# Merge two artifact files (fail on conflict)
nxuskit-cli artifact merge --input a.json --input b.json --merge-strategy error

# Summarize artifact structure
nxuskit-cli artifact summarize --input artifact.json --format json
```

#### `judge select` — LLM-based candidate selection

```bash
echo '{"candidates":[{"id":"a","content":"option A"},{"id":"b","content":"option B"}],"provider":"loopback","model":"echo"}' \
  | nxuskit-cli judge select --input - --format json
```

#### `branch fork` / `branch compare` — Multi-model comparison

```bash
# Fan out prompt to multiple models concurrently
echo '{"prompt":"explain quantum computing","models":["echo","echo"],"provider":"loopback"}' \
  | nxuskit-cli branch fork --input - --format json

# Compare fork results
nxuskit-cli branch compare --input fork_results.json --format json
```

### Level 2 Commands (product surface)

Level 2 commands are a stable product surface: provider inspection, shell
completions, CLIPS session lifecycle, ZEN decision-table utilities, and the Pro
solver `what-if` comparison. They share the same structured exit codes and
`ErrorEnvelope` shape as Level 1.

#### `provider list` / `provider info` -- provider capability inspection

```bash
# List all providers with metadata
nxuskit-cli provider list --format json

# Show one provider's capabilities (incl. the streaming_logprobs row)
nxuskit-cli provider info openai --format json
```

An unknown provider name returns exit 5 (`validation`) with an `ErrorEnvelope`.

#### `completions <shell>` -- shell completion scripts

```bash
# Generate a completion script and source it
nxuskit-cli completions bash > /usr/local/etc/bash_completion.d/nxuskit-cli
nxuskit-cli completions zsh  > ~/.zfunc/_nxuskit-cli      # ensure ~/.zfunc is on $fpath
nxuskit-cli completions fish > ~/.config/fish/completions/nxuskit-cli.fish
```

**Shell support policy (v0.9.4):**

| Shell | Status | Notes |
|---|---|---|
| `bash` | Supported | `completions bash` emits a sourceable script. |
| `zsh` | Supported | `completions zsh` emits a `_nxuskit-cli` function; place on `$fpath`. |
| `fish` | Supported | `completions fish` emits a completion file under `~/.config/fish/completions/`. |
| PowerShell | Not generated in v0.9.4 | `completions` accepts only `bash`, `zsh`, `fish` (clap value-enum). Passing any other shell name is rejected by the argument parser. |

**Schema bundle locations.** The SDK bundle ships JSON schemas alongside the
binaries: CLIPS deftemplate <-> JSON Schema conversions are produced by
`nxuskit-cli schema to-json` / `schema to-clips`; packet/pipeline JSON schemas
ship under the bundle's `conformance/` directory; the C ABI header (`nxuskit.h`)
ships under `include/`. See the bundle's `README` for the exact layout.

#### `clips session create` / `list` / `destroy` -- persistent CLIPS sessions

```bash
# Create a session (optionally pre-loading rules), list, then destroy
nxuskit-cli clips session create --input rules.json --format json   # -> returns a session id
nxuskit-cli clips session list --format json
nxuskit-cli clips session destroy --session-id sess_abc12345 --format json
```

An unknown/invalid session id returns exit 5 (`validation`).

#### `zen validate` -- structural validation of a JDM (Pro)

```bash
# Validate the structure of a decision model (no evaluation; requires Pro)
nxuskit-cli zen validate --input my_table.jdm.json --format json
# -> exit 0, {"result":{"valid":true,"node_count":3,"decision_table_count":1,"rule_count":4,...}}

# A malformed model (e.g. a Function/JavaScript node) -> exit 5 with a diagnostic:
nxuskit-cli zen validate --input broken.jdm.json
# -> {"code":"zen_validate_error","details":{"problems":[{"kind":"FunctionNodeRejected",...}]}}
```

The `--input` for `zen validate` is the **JDM model itself** -- not the
`{table, input}` envelope that `zen eval` accepts.

#### `zen test` -- fixture-based decision-table testing (Pro)

```bash
# fixtures.json: {"table": <JDM>, "cases": [{"name":"...","input":{...},"expected":{...}}, ...]}
nxuskit-cli zen test --input fixtures.json --format json
# -> exit 0 when every case matches; exit 5 with a diff report on mismatch:
#    {"code":"zen_test_mismatch","details":{"failed":[{"name":"...","diff":{"tier":{"expected":"C","actual":"B"}}}]}}
```

#### `solver what-if --compare` -- Pro constraint what-if comparison

```bash
nxuskit-cli solver what-if --problem problem.json --assume "x > 5" --compare --json
# -> exit 0 with base_result / assumed_result / diff; exit 4 (entitlement_denied) without a Pro license
```

See the [CLI Input Format Reference](https://docs.nxus.systems/nxuskit/reference/cli-reference/)
for full input schemas (including `zen validate` / `zen test`).

#### Exit Codes

The exit-code set is stable across Level 1 and Level 2 (FR-001):

| Code | Meaning | Example `code` strings |
|------|---------|------------------------|
| 0 | Success | (none) |
| 1 | Internal error (file not found, provider error, engine error) | `internal`, `provider_error` |
| 2 | Timeout | `timeout`, `idle_timeout` |
| 3 | Authentication failure | `auth_failed` |
| 4 | Entitlement denied (Pro/Enterprise feature without a license) | `entitlement_denied` |
| 5 | Validation / parse error (command-specific `code`s) | `validation`, `parse_error`, `zen_validate_error`, `zen_test_mismatch`, `zen_test_eval_error` |
| 130 | Cancelled (SIGINT during streaming) | `cancelled` |

For the full shell contract and detailed input format schemas for every
command, see the [CLI Input Format Reference](https://docs.nxus.systems/nxuskit/reference/cli-reference/).

### Environment Variables

The CLI uses the same environment variables as the library SDKs:

| Variable | Provider | Required |
|----------|----------|----------|
| `OPENAI_API_KEY` | OpenAI | Yes |
| `ANTHROPIC_API_KEY` | Claude | Yes |
| `GROQ_API_KEY` | Groq | Yes |
| `MISTRAL_API_KEY` | Mistral | Yes |
| `FIREWORKS_API_KEY` | Fireworks | Yes |
| `OPENROUTER_API_KEY` | OpenRouter | Yes |
| `TOGETHER_API_KEY` | Together | Yes |
| `PERPLEXITY_API_KEY` | Perplexity | Yes |
| `OLLAMA_BASE_URL` | Ollama | No (default: `http://localhost:11434`) |
| `LMSTUDIO_BASE_URL` | LM Studio | No (default: `http://localhost:1234/v1`) |

Local providers (Ollama, LM Studio, Loopback) do not require API keys.
`GROQ_API_KEY` is for Groq, Inc.'s API; xAI Grok is a separate provider
candidate and is not currently configured by this table.

---

## Path 3: Contributing to nxusKit

For developers who want to contribute to the toolkit itself.

### 1. Clone the Repository

```bash
git clone https://github.com/nxus-SYSTEMS/nxusKit.git
cd nxusKit
```

### 2. Install Task Runner

```bash
cargo install just
```

### 3. Verify Setup

```bash
# From the tools/ directory
cd tools
just --list    # Show available commands

# Or from root
just -f tools/justfile --list
```

### 4. Build Everything

```bash
just -f tools/justfile build
```

### 5. Run Tests

```bash
just -f tools/justfile test
```

### Repository Layout

```
nxusKit/
├── packages/
│   ├── nxuskit/                  # User-facing Rust wrapper
│   ├── nxuskit-engine/           # Internal Rust engine workspace
│   │   └── crates/
│   │       ├── nxuskit-core/     # C ABI / native SDK boundary
│   │       ├── nxuskit-engine/   # Engine library
│   │       └── nxuskit-cli/      # CLI tool
│   ├── nxuskit-go/               # Go SDK
│   └── nxuskit-py/               # Python SDK
├── conformance/            # Cross-language tests
├── docs/                   # Documentation
│   └── user/              # User guides
└── tools/
    ├── justfile            # Task runner (fmt, lint, test, build)
    └── scripts/            # Build and validation scripts
```

### Common Development Commands

```bash
# Format code
just -f tools/justfile fmt

# Lint code
just -f tools/justfile lint

# Test specific language
just -f tools/justfile test-rust
just -f tools/justfile test-go
```

### Next Steps

- [CONTRIBUTING.md](CONTRIBUTING.md) - Contribution guidelines
- [ARCHITECTURE.md](ARCHITECTURE.md) - Project architecture

---

## Environment Variables Reference

### API Keys

```bash
# OpenAI
export OPENAI_API_KEY="sk-..."

# Anthropic (Claude)
export ANTHROPIC_API_KEY="sk-ant-..."

# Mistral
export MISTRAL_API_KEY="..."

# OpenRouter
export OPENROUTER_API_KEY="..."

# Together AI
export TOGETHER_API_KEY="..."

# Groq
export GROQ_API_KEY="..."

# Fireworks
export FIREWORKS_API_KEY="..."

# Perplexity
export PERPLEXITY_API_KEY="..."
```

### Local Providers

```bash
# Ollama (default: http://localhost:11434)
export OLLAMA_BASE_URL="http://localhost:11434"

# LM Studio (default: http://localhost:1234/v1)
export LMSTUDIO_BASE_URL="http://localhost:1234/v1"
```

---

## Troubleshooting

### Rust Issues

**"error: could not compile"**
- Check Rust version: `rustc --version` (need 1.75+)
- Update Rust: `rustup update`
- Clean and rebuild: `cargo clean && cargo build`

**"cannot find crate `nxuskit`"**
- Verify `Cargo.toml` has `nxuskit = "0.7"` in dependencies
- Run `cargo update` to refresh index

**"API key not found"**
- Set environment variable: `export OPENAI_API_KEY="sk-..."`
- Or pass directly: `OpenAIProvider::new("sk-...".to_string())`

### Go Issues

**"cannot find module"**
- Run `go mod tidy` to sync dependencies
- Verify import path: `github.com/nxus-SYSTEMS/nxusKit/packages/nxuskit-go`

**"connection refused"**
- For Ollama: Is `ollama serve` running?
- Check base URL: `curl http://localhost:11434/api/tags`

### Build Issues

**"CLIPS source not found"**
- Download CLIPS source or set `CLIPS_SOURCE_DIR`
- Or ignore: CLIPS features disabled with stub library

**"golangci-lint not found"**
- Install: See [golangci-lint.run](https://golangci-lint.run/usage/install/)
- Or skip lint: `just -f tools/justfile test` (runs tests only)

### Getting Help

- [GitHub Issues](https://github.com/nxus-SYSTEMS/nxusKit/issues) - Bug reports
- [GitHub Discussions](https://github.com/nxus-SYSTEMS/nxusKit/discussions) - Questions
- [ARCHITECTURE.md](ARCHITECTURE.md) - Understanding the codebase
- [CONTRIBUTING.md](CONTRIBUTING.md) - Contribution process

---

## What's Next?

| Goal | Read |
|------|------|
| Build Rust apps with LLMs | [README.md](README.md#quick-start) |
| Build Go apps with LLMs | [packages/nxuskit-go/README.md](packages/nxuskit-go/README.md) |
| Explore examples | [nxusKit-examples](https://github.com/nxus-SYSTEMS/nxusKit-examples) |
| Contribute to nxusKit | [CONTRIBUTING.md](CONTRIBUTING.md) |
| Understand architecture | [ARCHITECTURE.md](ARCHITECTURE.md) |

**Happy coding!**
