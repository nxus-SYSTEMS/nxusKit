# nxuskit SDK — Getting Started

This guide walks you through downloading, installing, and using the nxuskit SDK
to call LLM providers from Rust, Go, Python, or the C ABI.

## Prerequisites

This guide targets SDK **2.0.0**. Run CE from the downloadable SDK bundle or
build CE from the public source. No SDK license token is required for CE.
The downloadable bundle contains Pro capabilities; Solver requires a valid
paid entitlement. New Pro purchases are paused. Launch Party Pricing details
will follow. This release does not announce new package-index availability.

For CE source builds, install Rust/Cargo compatible with the selected source
lock, a C/C++ toolchain and CMake. The current SDK requires Rust 1.94 or newer;
older compilers are no longer supported. This applies to standalone Rust
consumers as well as source builds. Retained older native qualification bundles
do not become Rust1.94 artifacts when this declaration changes. Static Rust
linking additionally requires a producer-compatible compiler; use the exact
selected package/compiler pair rather than mixing archived staticlibs.
Python SDK users need Python 3.11+
and a virtual environment; Go users need the version declared by the selected
package's `go.mod`. Windows source builds need the native Windows build tools.

## 1. Install the SDK bundle

Use the [v2.0.0 GitHub release](https://github.com/nxus-SYSTEMS/nxusKit/releases/tag/sdk-v2.0.0).
Choose the platform asset below, not GitHub's generated source archive.
No GitHub login or SDK token is required to download or use CE.

| Platform | Archive |
| --- | --- |
| Linux x86_64 | `nxuskit-sdk-2.0.0-pro-linux-x86_64.tar.gz` |
| macOS ARM64 | `nxuskit-sdk-2.0.0-pro-macos-arm64.tar.gz` |
| Windows x86_64 | `nxuskit-sdk-2.0.0-pro-windows-x86_64.zip` |

Compute SHA-256 with `sha256sum` on Linux, `shasum -a 256` on macOS,
or `Get-FileHash -Algorithm SHA256` on Windows. Compare it to the entry for
the exact filename in the release's `SHA256SUMS` before extracting into a
directory you own. Stop if the checksum is missing or differs. Use the
CLI from that directory, not a global installation. Continue with the
[credential-free local first call](first-call.md).

The bundled guides were frozen at build time. Follow the current release
notes and this guide for download availability, purchase status and pricing.
macOS x86_64 is not supported for v2.0.0.

### Alternative: build CE from public source

Use the reviewed public CE source checkout for the selected 2.0.0 revision.
Check that its workspace version is `2.0.0`. The public source builds CE,
not the Pro-capable binary distribution.

From that checkout, the native workspace is `packages/nxuskit-engine`:

Keep the complete source tree, including the engine workspace's `third-party/`
parents and root patch tables. They contain attributed dependency repairs and
are part of the source/lock identity; do not replace them with registry copies
or copy an internal engine crate into an unrelated application. Rust applications
using a supported Pro binary bundle depend on its `rust/` FFI wrapper as described
below, not directly on the internal engine crate.

```bash
export NXUSKIT_SOURCE_DIR="$(cd /absolute/path/to/nxusKit && pwd -P)"
cd "$NXUSKIT_SOURCE_DIR/packages/nxuskit-engine"
export CARGO_TARGET_DIR="/absolute/path/to/your-external-build/nxuskit-ce-2.0.0"
cargo build --locked --workspace --no-default-features
export NXUSKIT_CLI="$CARGO_TARGET_DIR/debug/nxuskit-cli"
"$NXUSKIT_CLI" --version
```

For Python FFI, stage the native library under its canonical SDK filename in
an external directory. Cargo's crate filename alone is not the loader name:

```bash
export NXUSKIT_LIB_DIR="$CARGO_TARGET_DIR/ffi-lib"
mkdir -p "$NXUSKIT_LIB_DIR"
case "$(uname -s)" in
  Darwin) cp "$CARGO_TARGET_DIR/debug/libnxuskit_core.dylib" "$NXUSKIT_LIB_DIR/libnxuskit.dylib" ;;
  Linux) cp "$CARGO_TARGET_DIR/debug/libnxuskit_core.so" "$NXUSKIT_LIB_DIR/libnxuskit.so" ;;
  *) echo "Use the native Windows library staging command below" >&2; exit 1 ;;
esac
```

On native Windows, create the external `ffi-lib` directory and copy
`$env:CARGO_TARGET_DIR/debug/nxuskit_core.dll` to
`$env:CARGO_TARGET_DIR/ffi-lib/nxuskit.dll`; set `NXUSKIT_LIB_DIR` to that
directory. Check the selected canonical library exists before loading Python;
do not permit a missing file to fall through to another installed SDK.

Keep build output outside the source checkout. A fresh dependency cache may
need public crates.io access; `--locked` forbids dependency/lock updates and
does not itself forbid network. Supply the CLIPS source input required by the
selected source build's `clips-sys` instructions; missing native tools/source
are build failures, not permission to alter the lock or enable Pro features.
On Windows use the equivalent absolute target directory and
`debug/nxuskit-cli.exe`. Expected CLI version is `nxuskit-cli 2.0.0`.

Continue with the [credential-free local first call](first-call.md).

### Set SDK Path

After extracting, set the SDK path. **Use an absolute path** — relative paths
can fail because `cargo` and other tools may change the working directory during
builds.

```bash
# Get the absolute path to the extracted SDK directory
export NXUSKIT_SDK_DIR="$(cd /absolute/path/to/nxuskit-sdk-2.0.0-pro-macos-arm64 && pwd -P)"
echo "NXUSKIT_SDK_DIR=${NXUSKIT_SDK_DIR}"
```

To persist across sessions, add to your shell profile (`~/.bashrc`, `~/.zshrc`,
etc.):

```bash
export NXUSKIT_SDK_DIR="/absolute/path/to/nxuskit-sdk-2.0.0-pro-macos-arm64"
```

Use your selected package's exact directory and platform.

### CLI shell completions (optional)

The bundle includes `nxuskit-cli`. Generate shell completions with:

```bash
"$NXUSKIT_SDK_DIR/bin/nxuskit-cli" completions bash > /usr/local/etc/bash_completion.d/nxuskit-cli
"$NXUSKIT_SDK_DIR/bin/nxuskit-cli" completions zsh  > ~/.zfunc/_nxuskit-cli
"$NXUSKIT_SDK_DIR/bin/nxuskit-cli" completions fish > ~/.config/fish/completions/nxuskit-cli.fish
```

Supported shells for `completions`: **bash**, **zsh**, **fish**.
PowerShell completion is **not generated** (the `completions` command
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

cd "$NXUSKIT_SDK_DIR/examples/c"
make basic_chat
./bin/basic_chat
```

See [examples/c/basic_chat.c](../examples/c/basic_chat.c) for the source.

## 4. First Example — Go

```bash
export OPENAI_API_KEY="sk-..."

cd "$NXUSKIT_SDK_DIR/examples/go"
go run ./basic-chat
```

See [examples/go/basic-chat/main.go](../examples/go/basic-chat/main.go) for the source.

## 5. First Example — Rust

The SDK bundles `nxuskit`, a safe Rust wrapper. Add it as a path dependency
in your `Cargo.toml` using the **absolute path** to the SDK's `rust/` directory:

```toml
# Cargo.toml
[dependencies]
nxuskit = { path = "/absolute/path/to/nxuskit-sdk-2.0.0-pro-macos-arm64/rust" }
```

Then set your environment and run:

```bash
# NXUSKIT_SDK_DIR tells the wrapper where to find libnxuskit at runtime.
# Must be an absolute path (relative paths are unreliable across tools).
export NXUSKIT_SDK_DIR="/absolute/path/to/nxuskit-sdk-2.0.0-pro-macos-arm64"
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
3. On macOS: inspect any OS security warning and follow your organization's trusted-package policy; do not bypass it for an unverified download.

See [examples/rust/](../examples/rust/) for a runnable project, and
[rust/README.md](../rust/README.md) for the full nxuskit API documentation.

### v1.1 Contract DTOs for Downstream Consumers

SDK 2.0.0 exposes the public contract authority at
`nxuskit::contract::v1_1`. Pre-RC planning language that described the
feature-105 / v1.1 APIs as unavailable, unstable, or SDK-gated is superseded
by `contract_version = "1.1"` and the DTOs in this namespace. Downstream
consumers should use these DTOs directly rather than forking local surrogate
types.

| Consumer need | SDK authority |
|---------------|---------------|
| Control-plane action request | `ActionEnvelope` |
| Control-plane action outcome | `ActionResult` |
| Arrow-shaped batch reference | `BatchRef` |
| Artifact reference | `ArtifactRef` |
| Substrait-or-equivalent plan reference | `PlanRef` |
| Provider/capability readiness | `ReadinessSnapshot` |
| Artifact/history export envelope | `ArtifactHistoryEnvelope` |
| Generic regulated workflow packet | `RegulatedWorkflowPacket` |
| Mixed-stage pipeline declaration | `PipelineDefinition` |
| Tool-stage data posture and plan-ref semantics | `ToolSemantics` |

`nxuskit-cli doctor --json` is a thin JSON projection of
`ReadinessSnapshot`. Its `captured_at` field is the command capture time in
UTC RFC 3339 form. The pure Rust constructors still accept explicit timestamps
for deterministic tests and fixtures.

Readiness posture is intentionally split between provider rows and capability
truth rows. In the default RC posture:

- `providers` lists runtime provider readiness that the doctor projection can
  truthfully report; the default local provider is `loopback`.
- `capabilities` includes `rule_eval.generic` with provider `clips`; this
  means CLIPS rule evaluation is available through the rules-provider boundary.
- `tool.emit_artifact` is `external_only` with no providers; it is a public
  tool-stage contract capability, not evidence of an executable in-SDK tool
  runtime.
- `arrow_reference_compatibility` is `declaration_only`.
- `substrait_plan_reference_compatibility` is `declaration_only`.
- `zenoh_availability` is `deferred`.
- `clips_rules_provider_availability` reports rules-provider availability only;
  it does not imply Peeler-side CLIPS orchestration or an embedded CLIPS
  orchestration engine.

## 6. First Example — Python

```bash
python3 -m venv .venv
. .venv/bin/activate
# Existing supplied bundle: install its Python source, not an unpinned index copy.
# Work outside the immutable bundle; pip builds from this disposable copy.
cp -R "$NXUSKIT_SDK_DIR/python" ./python-source
python -m pip install ./python-source
export NXUSKIT_LIB_DIR="$NXUSKIT_SDK_DIR/lib"
export OPENAI_API_KEY="sk-..."

python "$NXUSKIT_SDK_DIR/examples/python/basic_chat.py"
```

For a CE source checkout, use this alternative in an external working directory
and the virtual environment above. A detached source copy also needs its exact
selected repository's compliance files, required by the Python build hook:

```bash
cp -R "$NXUSKIT_SOURCE_DIR/packages/nxuskit-py" ./python-source
cp "$NXUSKIT_SOURCE_DIR/LICENSE" "$NXUSKIT_SOURCE_DIR/NOTICE" ./python-source/
python -m pip install ./python-source
# Retain NXUSKIT_LIB_DIR from the canonical-name staging step above.
python "$NXUSKIT_SOURCE_DIR/sdk-packaging/examples/python/basic_chat.py"
```

The two installation alternatives are exclusive: use a fresh disposable
`python-source` directory, not a merge of package revisions. Their chat example
is a live cloud-provider call, unlike the credential-free
[local first call](first-call.md); it needs a provider key and may be billable.
The installed Python distribution must report `2.0.0`.

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

## Next Steps

- [First Call](first-call.md) — local CE loopback fixture, without API credentials
- [API Reference](api-reference.md) — full C ABI documentation
- [Provider Reference](providers/cloud-llms.md) — provider-specific configuration
- [Rule Authoring Guide](rule-authoring.md) — writing, testing, and deploying custom CLIPS rules
- [Examples](../examples/) — working code for SDK languages and C ABI workflows
