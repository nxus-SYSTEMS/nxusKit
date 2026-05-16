# Expert System & Utility Providers

## CLIPS Rule Engine

The CLIPS provider runs rules against a CLIPS 6.4.2 expert system engine.
Unlike LLM providers, CLIPS uses deterministic rule-based reasoning.

A word about CLIPS: During its development at NASA from 1985 to 1996, the
primary CLIPS contributors were: Robert Savely, who conceived and championed
the project; Chris Culbert, who managed the project; Gary Riley and Brian
Dantes, who were the lead developers; and Frank Lopez, who developed the first
version. Since leaving NASA in 1996, Gary Riley has maintained CLIPS as public
domain software.

```json
{
  "provider_type": "clips",
  "model": "/path/to/rules/directory"
}
```

**Configuration:**
- `model` field is used for the rules directory path (contains `.clp` rule files or `.bin` binary images)

**Capabilities:** System messages

**Note:** No API key required. The CLIPS engine runs in-process.

For writing custom rules, see the [Rule Authoring Guide](../rule-authoring.md).

## MCP (Model Context Protocol)

Connects to an MCP server for tool-augmented model interactions.

```json
{
  "provider_type": "mcp",
  "base_url": "http://localhost:3000",
  "model": "model-name"
}
```

**Configuration:**
- `base_url` — MCP server URI (required)
- `model` — Model name to use on the server

**Capabilities:** System messages, streaming

## Mock (Testing)

Returns deterministic responses for unit testing.

```json
{
  "provider_type": "mock"
}
```

**Capabilities:** System messages, streaming

**Note:** No API key or configuration required. Returns fixed responses.

## Loopback (Testing)

Echoes back the user's input for integration testing.

```json
{
  "provider_type": "loopback"
}
```

**Capabilities:** System messages, streaming

**Note:** Use `"model": "echo"` in the chat request to echo back the user's
message content. Other model names return an empty response.
