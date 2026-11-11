# nxuskit-py: Python SDK for nxusKit

[![PyPI](https://img.shields.io/pypi/v/nxuskit-py.svg)](https://pypi.org/project/nxuskit-py/)
[![Python 3.11+](https://img.shields.io/badge/python-3.11+-blue.svg)](https://www.python.org/downloads/)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)

Pure Python library for the [nxusKit](https://github.com/nxus-SYSTEMS/nxusKit) polyglot SDK. Provides a synchronous interface to 11 LLM providers, CLIPS rule engines, Bayesian networks, and FFI-backed native engines. Z3 solver and ZEN decision table workflows require nxusKit SDK Pro.

## Features

- **11 LLM Providers** — Claude, OpenAI, Ollama, xAI Grok, Groq, Mistral, Fireworks, Together, OpenRouter, Perplexity, LM Studio
- **Per-Request Model Override** — Switch models on any `chat()` call: `provider.chat(messages, model="gpt-4o-mini")`
- **Tool Calling / Function Calling** — Pass tool definitions, receive structured tool call responses
- **Streaming** — Iterator-based streaming with `is_final()` completion detection
- **Vision / Multimodal** — Image input via URL, base64, or file path with auto-detected MIME types
- **Model Discovery** — `list_models()` with `supports_vision()`, `modalities()`, `max_images()` helpers
- **Typed Error Handling** — `TimeoutError`, `NetworkError`, `RateLimitError`, `AuthenticationError`, `ProviderError`
- **Retry Utilities** — `RetryConfig`, `retry_with_backoff`, `AdaptiveRateLimiter`
- **CLIPS / BN / Solver / ZEN** — FFI access to nxusKit reasoning engines (native library required; Solver and ZEN require Pro)

**Dependencies**: `requests`, `cffi` (FFI), `keyring` (credential storage), `PyJWT[crypto]` (license tokens)

## Installation

```bash
pip install nxuskit-py
```

For FFI features (CLIPS, BN, Solver, ZEN), install the [nxusKit SDK](https://github.com/nxus-SYSTEMS/nxusKit/releases) and set `NXUSKIT_SDK_DIR` or install at `~/.nxuskit/sdk/current/`. CLIPS and Bayesian inference are Community Edition features; Solver and ZEN require Pro.

## Quick Start

```python
import nxuskit

# Create a provider (auto-discovers API key from environment)
provider = nxuskit.Provider.claude()

# Simple chat
response = provider.chat([nxuskit.Message.user("What is 2 + 2?")])
print(response.content)
print(f"Tokens: {response.usage.total_tokens}")
```

## Capability Manifest Public Preview

### Native Ollama wire controls

With `Provider.ollama(use_ffi=False)`, generation settings are sent in Ollama's
`options` object. `response_format={"type": "json_schema", "schema": {...}}`
sends the schema directly as `format`; `ResponseFormat.JSON` sends `"json"`.
`thinking="enabled"` / `"disabled"` sends `think: true` / `false`; `None` or
`"omit"` omits it. `"auto"` retains the Rust adapter's model-name heuristic
(Qwen3, DeepSeek R1/V3, or `:thinking`), not a detected capability guarantee.
Explicit `tool_choice` is rejected rather than silently dropped.

Unary responses retain `thinking`; unary and streaming results retain
`raw_finish_reason`. Unknown or absent canonical reasons remain `None`, not
fabricated `"stop"` or a new reason variant. Stream completion is represented
independently by `terminal=True`; `is_final()` also accepts a legacy finish
reason. The defaulted `terminal` constructor argument is keyword-only.
Thinking-only chunks and message-less terminal events are preserved.

The optional keyword-only `profile` accepts `"ollama_local"` or
`"ollama_cloud_via_daemon"`. It is a caller declaration, **not verified
execution locality or server support**. Default `None` preserves existing
calls and makes neither claim. Result `declared_profile` carries only that
declaration; `endpoint_origin` contains only scheme/host/non-default port,
never userinfo, path, query or fragment. Neither localhost nor model spelling
proves local execution. Explicit cloud profiles reject structured output
before HTTP; explicit profiles on FFI are rejected until bound there.
[Ollama's documentation](https://docs.ollama.com/capabilities/structured-outputs)
states Cloud structured output is unsupported (reviewed 2026-09-12).

These controls qualify the synchronous native Python Ollama adapter only;
they do not expand native async, FFI, Go, CLI, or live-provider qualification.

The Python package exposes the stable public Capability Manifest v2 projection
types. The public shape carries status values and reviewed-on metadata only;
internal evidence records, model overrides, and provider-specific details stay
private to the engine registry.

```python
import nxuskit

manifest = nxuskit.PublicCapabilityManifest(
    schema_version="capability-manifest-v2-public-preview/1",
    posture=nxuskit.ManifestPublicationPosture.SPLIT,
    providers=[
        nxuskit.PublicProviderCapability(
            name="openai",
            display_name="OpenAI",
            last_reviewed_on="2026-05-09",
            provider_status="unknown",
            capabilities={
                "json_schema_strict": nxuskit.CapabilityStatus.SUPPORTED,
                "rerank": nxuskit.CapabilityStatus.FUTURE,
            },
        )
    ],
)

print(nxuskit.PUBLIC_CAPABILITY_FIELDS)
print(manifest.to_dict()["providers"][0]["capabilities"]["json_schema_strict"])
```

## Per-Request Model Override

```python
provider = nxuskit.Provider.openai()  # default: gpt-4o

# Override model for a single call
response = provider.chat(
    [nxuskit.Message.user("Hello")],
    model="gpt-4o-mini",
    temperature=0.5,
)
```

## Streaming

```python
for chunk in provider.chat_stream([nxuskit.Message.user("Tell me a story")]):
    print(chunk.delta, end="", flush=True)
    if chunk.is_final():
        print(f"\nTokens: {chunk.usage.total_tokens}")
```

### Streaming Logprobs (v0.9.4+)

Per-chunk logprob deltas are now surfaced on streaming responses for
providers that support them (OpenAI). Check the capability flag before
issuing the call; non-supporting providers always emit `chunk.logprobs is None`
on every chunk (FR-007 — no phantom data).

```python
from nxuskit import Provider, ChatRequest, Role
import asyncio


async def main():
    provider = Provider.openai()

    if not provider.capabilities().supports_streaming_logprobs:
        print("Provider does not support streaming logprobs.")

    req = ChatRequest(
        model="gpt-5.4",
        messages=[{"role": Role.USER, "content": "Say hello."}],
        logprobs=True,
        top_logprobs=3,
    )

    async for chunk in provider.chat_stream(req):
        print(chunk.delta, end="")
        if chunk.logprobs is not None:
            for tok in chunk.logprobs.content:
                print(f"  token={tok.token!r} logprob={tok.logprob:.4f}")


asyncio.run(main())
```

## Tool Calling

```python
weather_tool = nxuskit.ToolDefinition.create(
    name="get_weather",
    description="Get weather for a location",
    parameters={
        "type": "object",
        "properties": {"location": {"type": "string"}},
        "required": ["location"],
    },
)

response = provider.chat(
    [nxuskit.Message.user("What's the weather in Tokyo?")],
    tools=[weather_tool],
    tool_choice=nxuskit.tool_choice_auto(),
)

if response.tool_calls:
    for call in response.tool_calls:
        print(f"Call: {call.function.name}({call.function.arguments})")
```

## Vision

```python
msg = nxuskit.Message.user("What's in this image?").with_image_file("photo.png")
response = provider.chat([msg], model="gpt-4o")
```

## Error Handling

```python
try:
    response = provider.chat([nxuskit.Message.user("Hello")])
except nxuskit.TimeoutError:
    print("Request timed out — try a faster model")
except nxuskit.NetworkError:
    print("Network issue — check connection")
except nxuskit.RateLimitError as e:
    print(f"Rate limited — retry after {e.retry_after}s")
except nxuskit.AuthenticationError:
    print("Check your API key")
```

## Model Discovery

```python
models = provider.list_models()
for m in models:
    vision = "vision" if m.supports_vision() else "text-only"
    print(f"  {m.name}: {vision}")
```

## Providers

| Provider | Factory | Environment Variable |
|----------|---------|---------------------|
| Claude | `Provider.claude()` | `ANTHROPIC_API_KEY` |
| OpenAI | `Provider.openai()` | `OPENAI_API_KEY` |
| Ollama | `Provider.ollama()` | None (local) |
| xAI Grok | `Provider.xai()` | `XAI_API_KEY` |
| Groq | `Provider.groq()` | `GROQ_API_KEY` |
| Mistral | `Provider.mistral()` | `MISTRAL_API_KEY` |
| Fireworks | `Provider.fireworks()` | `FIREWORKS_API_KEY` |
| Together | `Provider.together()` | `TOGETHER_API_KEY` |
| OpenRouter | `Provider.openrouter()` | `OPENROUTER_API_KEY` |
| Perplexity | `Provider.perplexity()` | `PERPLEXITY_API_KEY` |
| LM Studio | `Provider.lmstudio()` | None (local) |

## CLIPS Session API

For direct CLIPS rule engine access (requires native library):

```python
from nxuskit.clips import ClipsSession

with ClipsSession() as s:
    s.load_json(rules_json)
    s.reset()
    s.fact_assert_string('(sensor (name "temp") (value 200))')
    fired = s.run()
```

## FFI Provider Note

When using FFI-backed features, always use context managers (`with` statement) for reliable cleanup:

```python
from nxuskit._ffi_provider import create_ffi_provider

with create_ffi_provider({"provider_type": "openai", "api_key": "sk-..."}) as p:
    response = p.chat({"model": "gpt-4o", "messages": [...]})
```

## Development

A minimal offline package smoke example is kept as a doctest so the release
package gate verifies that the documented top-level import and version remain
available:

    >>> import nxuskit
    >>> nxuskit.__version__
    '2.0.0'

```bash
pip install -e ".[dev]"
pytest tests/
ruff check src/ && ruff format --check .
```

## License

Dual-licensed under MIT and Apache 2.0. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).

See also: [nxusKit-examples](https://github.com/nxus-SYSTEMS/nxusKit-examples) for 30+ runnable examples.
# Inference-independent discovery (bounded native-v1 increment)

`nxuskit.discovery.DiscoveryClientFactory` constructs a C/Rust-backed discovery
owner from `DiscoveryConfig(provider_id, endpoint, auth_selector=None,
connect_timeout_ms=5000, read_timeout_ms=5000)`. No inference provider or model is
constructed. The supported profiles are `openai`, `ollama_local`,
`ollama_cloud_via_daemon`, and `lmstudio`; their existing authentication rules
remain unchanged. LM Studio permits an optional non-secret environment-variable
name as `auth_selector`, resolved only at request time, without prompting.

For LM Studio, explicitly use `http://localhost:1234/api/v1` (or your configured
native API base). This increment targets the native v1 contract introduced with
LM Studio **0.4.0**, not the OpenAI-compatible `/v1/models` or older `/api/v0`.
There is no fallback, version probe, service start, download, load, or inference.
See the [native API version baseline](https://lmstudio.ai/docs/developer/rest)
and [catalog schema](https://lmstudio.ai/docs/developer/rest/list).

`health` validates one catalog response; `list_models` returns model information.
The additive typed `snapshot` obtains catalog and instance metadata in **one**
response. It preserves model maximum context separately from each loaded
instance's configured context length. Any validated instance proves `loaded`;
only complete known-empty instance lists (including an empty catalog) prove
`not_loaded`. Absent/null instance metadata yields `unknown`, not unloaded.
Malformed instances, duplicate identities, unknown model kinds and incompatible
catalog shapes fail with `DiscoveryError`, never a fabricated state. Connection,
timeout, authentication, and HTTP failures remain errors, not empty catalogs.
`api.minimum_server_version` is a compatibility assumption;
`api.reported_server_version` is `None` because this catalog reports no version.

Sync methods have `*_async` counterparts; use `with` or `async with` for cleanup.
Close rejects new work and drains accepted operations without redefining Rust's
non-cancelling close semantics. Cancelling an async waiter requests native
transport cancellation; ownership remains retained until native completion.
No provider-confirmed remote stop is claimed. At most four requests are admitted
per Python owner. This increment does not change inference factory routing,
Ollama generation/schema/thinking behavior, CLI routing, or standalone bundles.
