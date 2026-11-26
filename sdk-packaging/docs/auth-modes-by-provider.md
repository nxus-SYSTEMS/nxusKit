# Auth Modes by Provider

## Overview

nxusKit supports multiple authentication methods depending on the provider.
This matrix shows which methods are available for each provider and how to
configure them.

## Provider Matrix

| Provider | Auth Required | API Key | OAuth | Env Variable | Dashboard |
|----------|:------------:|:-------:|:-----:|--------------|-----------|
| OpenAI / GPT | Yes | Yes | — | `OPENAI_API_KEY` | [platform.openai.com/api-keys](https://platform.openai.com/api-keys) |
| Anthropic / Claude | Yes | Yes | — | `ANTHROPIC_API_KEY` | [console.anthropic.com/settings/keys](https://console.anthropic.com/settings/keys) |
| xAI Grok | Yes | Yes | — | `XAI_API_KEY` | [console.x.ai](https://console.x.ai/) |
| Groq | Yes | Yes | — | `GROQ_API_KEY` | [console.groq.com/keys](https://console.groq.com/keys) |
| Mistral AI | Yes | Yes | — | `MISTRAL_API_KEY` | [console.mistral.ai/api-keys](https://console.mistral.ai/api-keys) |
| Fireworks AI | Yes | Yes | — | `FIREWORKS_API_KEY` | [fireworks.ai/account/api-keys](https://fireworks.ai/account/api-keys) |
| Together AI | Yes | Yes | — | `TOGETHER_API_KEY` | [api.together.ai/settings/api-keys](https://api.together.ai/settings/api-keys) |
| OpenRouter | Yes | Yes | — | `OPENROUTER_API_KEY` | [openrouter.ai/settings/keys](https://openrouter.ai/settings/keys) |
| Perplexity | Yes | Yes | — | `PERPLEXITY_API_KEY` | [perplexity.ai/settings/api](https://www.perplexity.ai/settings/api) |
| Ollama | No | — | — | `OLLAMA_HOST` | — |
| LM Studio | No | — | — | `LM_STUDIO_HOST` | — |

**Legend**: Yes = supported, — = not applicable/not yet available

## Authentication Methods

### API Key

The standard authentication method for cloud providers. Obtain a key from
the provider's dashboard, then configure via any of these methods:

**Environment variable** (recommended for development and CI/CD):
```bash
export OPENAI_API_KEY="sk-..."
```

**Credential store** (for persistent local development): Use the supported
language APIs below to set or remove stored credentials. The v2.0.0 CLI
does not provide commands for those operations.

**Explicit parameter** (for programmatic use):

```python
from nxuskit import Provider

provider = Provider.openai(api_key="sk-...")
```

### OAuth

The provider help lists an OAuth login command. Use an API key for the cloud
providers in the table. Signing in to nxus.SYSTEMS for SDK license activation
is a separate operation.

### No Authentication

Local providers (Ollama, LM Studio) run on the local machine and do not
require authentication. The host env variable is optional and defaults to
`localhost` on the provider's default port.

## Credential Precedence

When multiple credential sources exist, the SDK uses this precedence order:

| Priority | Source | Example |
|----------|--------|---------|
| 1 (highest) | Explicit API parameter | `Provider.openai(api_key="sk-...")` |
| 2 | Environment variable | `OPENAI_API_KEY=sk-...` |
| 3 (lowest) | OS credential store | Via the credential APIs below |

## Checking Auth Status

View authentication status for all providers:

```bash
nxuskit-cli provider status
```

For a specific provider:

```bash
nxuskit-cli provider status openai
```

JSON output (for scripts):

```bash
nxuskit-cli provider status --json
```

Example output:

```
Provider          Status              Source    Auth Methods
─────────────────────────────────────────────────────────────
openai            Authenticated       env      api_key
claude            Authenticated       store    api_key
xai               Not authenticated   —        api_key
groq              Not authenticated   —        api_key
ollama            Not required        —        —
lmstudio          Not required        —        —
```

## Per-Language Configuration

### Rust

```rust
use nxuskit::{auth_status, auth_set_credential, auth_resolve};

// Check status
let status = auth_status("openai")?;

// Store credential
auth_set_credential("openai", "sk-...")?;

// Resolve credential (follows precedence chain)
let resolution = auth_resolve("openai", None)?;
```

### Go

```go
import nxuskit "github.com/nxus-SYSTEMS/nxusKit/packages/nxuskit-go"

// Check status
status, err := nxuskit.GetAuthStatus("openai")

// Store credential
err = nxuskit.AuthSetCredential("openai", "sk-...")

// Resolve credential
resolution, err := nxuskit.AuthResolve("openai", nil)
```

### Python

```python
from nxuskit import auth_status, auth_set_credential, auth_resolve

# Check status
status = auth_status("openai")

# Store credential
auth_set_credential("openai", "sk-...")

# Resolve credential
resolution = auth_resolve("openai")
```
