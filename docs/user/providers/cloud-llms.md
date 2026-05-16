# Cloud LLM Providers

## OpenAI

```json
{
  "provider_type": "openai",
  "api_key": "sk-...",
  "base_url": "https://api.openai.com/v1",
  "timeout_ms": 30000
}
```

**Environment variable:** `OPENAI_API_KEY`

**Supported models:** `gpt-4o`, `gpt-4o-mini`, `o1`, `o1-mini`, `o3-mini`, `gpt-4-turbo`

**Capabilities:** System messages, streaming, vision, tools/function calling, JSON mode, JSON schema, seed, logprobs, presence/frequency penalty, response format

## xAI Grok

```json
{
  "provider_type": "xai",
  "api_key": "xai-...",
  "base_url": "https://api.x.ai/v1",
  "timeout_ms": 30000
}
```

**Environment variable:** `XAI_API_KEY`

**Supported models:** `grok-4`, `grok-4-latest`, `grok-4-fast`

**Capabilities:** System messages, streaming, vision, tools/function calling, JSON mode, JSON schema

`xai` is the canonical provider id for xAI Grok. `groq` remains Groq, Inc.; there is no `grok` provider alias.

## Anthropic Claude

```json
{
  "provider_type": "claude",
  "api_key": "sk-ant-...",
  "base_url": "https://api.anthropic.com",
  "timeout_ms": 30000
}
```

**Environment variable:** `ANTHROPIC_API_KEY`

**Supported models:** `claude-sonnet-4-20250514`, `claude-opus-4-20250514`, `claude-haiku-4-5-20251001`, `claude-3-5-sonnet-20241022`

**Capabilities:** System messages, streaming, vision, tools/function calling, JSON mode, top-k sampling. Max stop sequences: 8192.

## Groq

```json
{
  "provider_type": "groq",
  "api_key": "gsk_...",
  "timeout_ms": 30000
}
```

**Environment variable:** `GROQ_API_KEY`

**Supported models:** `llama-3.3-70b-versatile`, `llama-3.1-8b-instant`, `mixtral-8x7b-32768`, `gemma2-9b-it`

**Capabilities:** System messages, streaming, tools/function calling, JSON mode

## Fireworks

```json
{
  "provider_type": "fireworks",
  "api_key": "fw_...",
  "timeout_ms": 30000
}
```

**Environment variable:** `FIREWORKS_API_KEY`

**Capabilities:** System messages, streaming

## Together

```json
{
  "provider_type": "together",
  "api_key": "...",
  "timeout_ms": 30000
}
```

**Environment variable:** `TOGETHER_API_KEY`

**Capabilities:** System messages, streaming

## OpenRouter

```json
{
  "provider_type": "openrouter",
  "api_key": "sk-or-...",
  "timeout_ms": 30000
}
```

**Environment variable:** `OPENROUTER_API_KEY`

**Capabilities:** System messages, streaming, vision, tools/function calling

## Perplexity

```json
{
  "provider_type": "perplexity",
  "api_key": "pplx-...",
  "timeout_ms": 30000
}
```

**Environment variable:** `PERPLEXITY_API_KEY`

**Capabilities:** System messages, streaming

## Mistral

```json
{
  "provider_type": "mistral",
  "api_key": "...",
  "timeout_ms": 30000
}
```

**Environment variable:** `MISTRAL_API_KEY`

**Supported models:** `mistral-large-latest`, `mistral-medium-latest`, `mistral-small-latest`, `codestral-latest`

**Capabilities:** System messages, streaming, tools/function calling, JSON mode
