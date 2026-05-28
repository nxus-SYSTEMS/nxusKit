# nxuskit SDK — Getting Started

This guide walks you through downloading, installing, and using the nxuskit SDK
to call LLM providers from Rust, Go, Python, or the C ABI.

## Prerequisites

- [GitHub CLI](https://cli.github.com/) (`gh`) installed
- Optional: authenticate with `gh auth login` for CI reliability, higher API
  limits, or entitlement-gated/private assets

## 1. Download and Install the SDK

The examples below download Community Edition from the public
`nxus-SYSTEMS/nxusKit` release. Current release asset names use `oss` for the
Community Edition archive segment. Pro users can replace `oss` with `pro` in the
asset patterns and extracted directory names after activating or receiving a Pro
entitlement.

### macOS (Apple Silicon)

```bash
# Download, extract, and remove macOS quarantine in one go
gh release download --repo nxus-SYSTEMS/nxusKit \
  --pattern "nxuskit-sdk-*-oss-macos-arm64.tar.gz" \
  --pattern "nxuskit-sdk-*-oss-macos-arm64.tar.gz.sha256"

shasum -a 256 -c nxuskit-sdk-*-oss-macos-arm64.tar.gz.sha256
tar xzf nxuskit-sdk-*-oss-macos-arm64.tar.gz
xattr -dr com.apple.quarantine nxuskit-sdk-*/
```

The `xattr` step removes the Gatekeeper quarantine flag that macOS applies to
downloaded files. Without it you'll get "can't be opened because Apple cannot
check it for malicious software" when loading the dylib.

### Linux (x86_64)

```bash
gh release download --repo nxus-SYSTEMS/nxusKit \
  --pattern "nxuskit-sdk-*-oss-linux-x86_64.tar.gz" \
  --pattern "nxuskit-sdk-*-oss-linux-x86_64.tar.gz.sha256"

sha256sum -c nxuskit-sdk-*-oss-linux-x86_64.tar.gz.sha256
tar xzf nxuskit-sdk-*-oss-linux-x86_64.tar.gz
```

### Windows (x86_64)

```powershell
gh release download --repo nxus-SYSTEMS/nxusKit `
  --pattern "nxuskit-sdk-*-oss-windows-x86_64.zip" `
  --pattern "nxuskit-sdk-*-oss-windows-x86_64.zip.sha256"

# Extract
Expand-Archive nxuskit-sdk-*-oss-windows-x86_64.zip -DestinationPath .
```

### Set SDK Path

After extracting, set the SDK path. **Use an absolute path** — relative paths
can fail because `cargo` and other tools may change the working directory during
builds.

```bash
# Get the absolute path to the extracted SDK directory
export NXUSKIT_SDK_DIR="$(cd nxuskit-sdk-*/ && pwd)"
echo "NXUSKIT_SDK_DIR=${NXUSKIT_SDK_DIR}"
```

To persist across sessions, add to your shell profile (`~/.bashrc`, `~/.zshrc`,
etc.):

```bash
export NXUSKIT_SDK_DIR="/absolute/path/to/nxuskit-sdk-1.0.0-oss-macos-arm64"
```

For CI systems, see [Download via PAT](#download-via-pat) below.

### CLI shell completions (optional)

The bundle includes `nxuskit-cli`. Generate shell completions with:

```bash
nxuskit-cli completions bash > /usr/local/etc/bash_completion.d/nxuskit-cli
nxuskit-cli completions zsh  > ~/.zfunc/_nxuskit-cli      # add ~/.zfunc to $fpath
nxuskit-cli completions fish > ~/.config/fish/completions/nxuskit-cli.fish
```

Supported shells for `completions` in v1.0.0: **bash**, **zsh**, **fish**.
PowerShell completion is **not generated** in v1.0.0 (the `completions` command
accepts only those three shell names). JSON schemas referenced by the CLI ship
under the bundle's `include/` (the C header) and `conformance/` (packet/pipeline
schemas) directories; see [SDK Bundle Contents](#2-sdk-bundle-contents) above.

## 2. SDK Bundle Contents

```
nxuskit-sdk-{version}-{edition}-{platform}/
├── include/
│   └── nxuskit.h          # C header — all API declarations
├── lib/
│   ├── libnxuskit.so      # Shared library (Linux)
│   │   libnxuskit.dylib   # Shared library (macOS)
│   │   nxuskit.dll        # Shared library (Windows)
│   ├── libnxuskit.a       # Static library (Linux/macOS)
│   │   nxuskit.lib        # Static library (Windows)
│   └── nxuskit.dll.lib    # Import library (Windows only)
├── rust/                  # nxuskit Rust SDK wrapper (use as path dependency)
├── docs/                  # This documentation
└── examples/              # Working examples in C, Rust, Go, Python
```

## 3. First Example — C

Set your provider API key, then compile and run:

```bash
export OPENAI_API_KEY="sk-..."  # or ANTHROPIC_API_KEY, etc.

cd nxuskit-sdk-*/examples/c
make basic_chat
./bin/basic_chat
```

See [examples/c/basic_chat.c](../examples/c/basic_chat.c) for the source.

## 4. First Example — Go

```bash
export OPENAI_API_KEY="sk-..."

cd nxuskit-sdk-*/examples/go
go run basic_chat.go
```

See [examples/go/basic_chat.go](../examples/go/basic_chat.go) for the source.

## 5. First Example — Rust

The SDK bundles `nxuskit`, a safe Rust wrapper. Add it as a path dependency
in your `Cargo.toml` using the **absolute path** to the SDK's `rust/` directory:

```toml
# Cargo.toml
[dependencies]
nxuskit = { path = "/Users/you/nxuskit-sdk-1.0.0-oss-macos-arm64/rust" }
```

Then set your environment and run:

```bash
# NXUSKIT_SDK_DIR tells the wrapper where to find libnxuskit at runtime.
# Must be an absolute path (relative paths are unreliable across tools).
export NXUSKIT_SDK_DIR="/Users/you/nxuskit-sdk-1.0.0-oss-macos-arm64"
export OPENAI_API_KEY="sk-..."

cargo run
```

```rust
use nxuskit::{ChatRequest, Message, NxuskitProvider, ProviderConfig};

fn main() -> Result<(), nxuskit::NxuskitError> {
    let provider = NxuskitProvider::new(ProviderConfig {
        provider_type: "openai".into(),
        ..Default::default()
    })?;

    let request = ChatRequest::new("gpt-4o")
        .with_message(Message::user("Hello from Rust!"))
        .with_max_tokens(100);

    let response = provider.chat(request)?;
    println!("{}", response.content);
    Ok(())
}
```

**Path troubleshooting:** If you see `LibraryNotFound`, verify:
1. `NXUSKIT_SDK_DIR` is set and is an absolute path (check with `echo $NXUSKIT_SDK_DIR`)
2. The `lib/` subdirectory exists: `ls $NXUSKIT_SDK_DIR/lib/`
3. On macOS: quarantine was removed (see Step 1 above)

See [examples/rust/](../examples/rust/) for a runnable project, and
[rust/README.md](../rust/README.md) for the full nxuskit API documentation.

## 6. First Example — Python

```bash
pip install nxuskit-py
export OPENAI_API_KEY="sk-..."

python examples/python/basic_chat.py
```

See [examples/python/basic_chat.py](../examples/python/basic_chat.py) for the source.

## Core Concepts

### JSON-in / JSON-out

All data crosses the FFI boundary as JSON strings. You send a JSON config to
create a provider, send a JSON request for chat, and receive a JSON response.

### Provider Lifecycle

```
create_provider(config_json) → provider handle
    ↓
chat(provider, request_json) → response handle → response_json(response) → JSON
    ↓
free_response(response)
free_provider(provider)
```

### Streaming

```
chat_stream(provider, request_json, on_chunk, on_done, user_data) → stream handle
    ↓  (callbacks fire from background thread)
on_chunk(chunk_json, user_data)  ← called per chunk
on_done(final_json, user_data)   ← called once at end
    ↓
free_stream(stream)
```

### Thread Safety

- All `nxuskit_*` functions are thread-safe
- Provider handles can be shared across threads
- Error messages are thread-local (`nxuskit_last_error()`)

### Supported Providers

| Provider | Config `provider_type` | Required Env Var |
|----------|----------------------|------------------|
| OpenAI | `openai` | `OPENAI_API_KEY` |
| Anthropic Claude | `claude` | `ANTHROPIC_API_KEY` |
| Ollama | `ollama` | `OLLAMA_HOST` (optional) |
| LM Studio | `lmstudio` | — |
| xAI Grok | `xai` | `XAI_API_KEY` |
| Groq | `groq` | `GROQ_API_KEY` |
| Fireworks | `fireworks` | `FIREWORKS_API_KEY` |
| Together | `together` | `TOGETHER_API_KEY` |
| OpenRouter | `openrouter` | `OPENROUTER_API_KEY` |
| Perplexity | `perplexity` | `PERPLEXITY_API_KEY` |
| Mistral | `mistral` | `MISTRAL_API_KEY` |
| CLIPS | `clips` | — |
| MCP | `mcp` | — |
| Mock (testing) | `mock` | — |
| Loopback (testing) | `loopback` | — |

`xai` is xAI Grok and uses `XAI_API_KEY`. `groq` is Groq, Inc.'s provider and
uses `GROQ_API_KEY`. There is no `grok` provider alias.

### CLIPS Quick Start

CLIPS runs in-process (no API key needed). Create a provider with
`provider_type: "clips"` and `model` pointing to your rules directory. Send
facts as JSON in the user message:

```c
const char *input = "{\"facts\": [{\"template\": \"sensor\", \"values\": {\"name\": \"temp\", \"value\": 150}}]}";
// ... create provider, build request with input as user message, call nxuskit_chat()
```

The user message must conform to the `ClipsInput` schema — see the
[Rule Authoring Guide](rule-authoring.md#clipsinput-json-reference) for the full
field reference. CLIPS also provides a session API for direct engine access; see
the [API Reference](api-reference.md#clips-session-api).

## Linking Reference

### GCC / Clang (dynamic)
```bash
cc -I sdk/include -o myapp myapp.c -L sdk/lib -lnxuskit -Wl,-rpath,sdk/lib
```

### GCC / Clang (static)
```bash
cc -I sdk/include -o myapp myapp.c sdk/lib/libnxuskit.a -lpthread -ldl -lm
```

### MSVC (dynamic — recommended)
```
cl /I sdk\include myapp.c /link sdk\lib\nxuskit.dll.lib
```

### MSVC (static)
```
cl /I sdk\include myapp.c /link sdk\lib\nxuskit.lib ucrt.lib userenv.lib ntdll.lib ws2_32.lib bcrypt.lib advapi32.lib
```

### CGo
```go
// #cgo CFLAGS: -I${SRCDIR}/sdk/include
// #cgo linux LDFLAGS: -L${SRCDIR}/sdk/lib -lnxuskit -Wl,-rpath,${SRCDIR}/sdk/lib
// #cgo darwin LDFLAGS: -L${SRCDIR}/sdk/lib -lnxuskit -Wl,-rpath,${SRCDIR}/sdk/lib
// #cgo windows LDFLAGS: -L${SRCDIR}/sdk/lib -lnxuskit
// #include "nxuskit.h"
import "C"
```

### Python (cffi)
```python
from cffi import FFI
ffi = FFI()
ffi.cdef(open("sdk/include/nxuskit.h").read())
lib = ffi.dlopen("sdk/lib/libnxuskit.so")  # or .dylib / .dll
```

## Download via PAT

For CI systems that can't use `gh`, or that need authenticated GitHub API
access:

1. Create a fine-grained PAT at https://github.com/settings/personal-access-tokens
   - **Repository access**: Select `nxus-SYSTEMS/nxusKit`
   - **Permissions**: Contents → Read-only
2. Use the token:

```bash
export GH_TOKEN="github_pat_..."

# List available SDK releases
curl -H "Authorization: Bearer $GH_TOKEN" \
  "https://api.github.com/repos/nxus-SYSTEMS/nxusKit/releases?per_page=5" \
  | jq '.[].tag_name'

# Download a specific asset
curl -L -H "Authorization: Bearer $GH_TOKEN" \
  -H "Accept: application/octet-stream" \
  "https://api.github.com/repos/nxus-SYSTEMS/nxusKit/releases/assets/{ASSET_ID}" \
  -o nxuskit-sdk.tar.gz
```

## Next Steps

- [API Reference](api-reference.md) — full C ABI documentation
- [Provider Reference](providers/cloud-llms.md) — provider-specific configuration
- [Rule Authoring Guide](rule-authoring.md) — writing, testing, and deploying custom CLIPS rules
- [Examples](../examples/) — working code for SDK languages and C ABI workflows
