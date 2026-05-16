# nxuskit-go

A unified Go library for interacting with multiple LLM providers through a consistent API.

The public package is part of nxusKit SDK Community Edition, which is free and
open source. Pro adds proprietary commercial capabilities such as solver-backed
workflows, ZEN decision tables, plugin loading, and trust-policy features.

## Features

- **13 Provider Support**: OpenAI, Claude (Anthropic), Ollama, LM Studio, xAI Grok, Groq, Together, Mistral, Fireworks, OpenRouter, Perplexity, plus Mock and Loopback for testing
- **Streaming**: First-class support for streaming responses
- **Vision**: Multimodal support for image inputs
- **Tool/Function Calling**: Define tools the model can invoke
- **Structured Output**: JSON mode and JSON Schema validation
- **Provider Capabilities**: Query what each provider supports before making requests
- **Test Utilities**: HTTP mocking helpers for unit testing

## Capability Manifest Public Preview

The package exposes the stable public Capability Manifest v2 projection types.
The public shape carries status values and reviewed-on metadata only; internal
evidence records, model overrides, and provider-specific details stay private to
the engine registry.

```go
manifest := nxuskit.PublicCapabilityManifest{
    SchemaVersion: "capability-manifest-v2-public-preview/1",
    Posture:       nxuskit.ManifestPublicationPostureSplit,
    Providers: []nxuskit.PublicProviderCapability{
        {
            Name:           "openai",
            DisplayName:    "OpenAI",
            LastReviewedOn: "2026-05-09",
            ProviderStatus: "unknown",
            Capabilities: map[string]nxuskit.CapabilityStatus{
                "json_schema_strict": nxuskit.CapabilityStatusSupported,
                "rerank":             nxuskit.CapabilityStatusFuture,
            },
        },
    },
}

fields := nxuskit.PublicCapabilityFields()
fmt.Println(fields, manifest.Providers[0].Capabilities["json_schema_strict"])
```

## Installation

```bash
go get github.com/nxus-SYSTEMS/nxusKit/packages/nxuskit-go
```

## Quick Start

### Basic Chat

```go
package main

import (
    "context"
    "fmt"
    "log"

    "github.com/nxus-SYSTEMS/nxusKit/packages/nxuskit-go"
)

func main() {
    // Create provider (uses OPENAI_API_KEY env var)
    provider, err := nxuskit.NewOpenAIProvider()
    if err != nil {
        log.Fatal(err)
    }

    // Create request
    req, err := nxuskit.NewChatRequest("gpt-4o",
        nxuskit.WithMessages(
            nxuskit.SystemMessage("You are a helpful assistant."),
            nxuskit.UserMessage("Hello!"),
        ),
        nxuskit.WithTemperature(0.7),
    )
    if err != nil {
        log.Fatal(err)
    }

    // Send request
    resp, err := provider.Chat(context.Background(), req)
    if err != nil {
        log.Fatal(err)
    }

    fmt.Println(resp.Content)
}
```

### Streaming

```go
req, _ := nxuskit.NewChatRequest("gpt-4o",
    nxuskit.WithMessages(nxuskit.UserMessage("Tell me a story")),
    nxuskit.WithStream(true),
)

chunks, errs := provider.ChatStream(context.Background(), req)

for chunk := range chunks {
    fmt.Print(chunk.Delta)
}

if err := <-errs; err != nil {
    log.Fatal(err)
}
```

## Provider Examples

### OpenAI

```go
// Uses OPENAI_API_KEY environment variable
provider, err := nxuskit.NewOpenAIProvider()

// Or with explicit key
provider, err := nxuskit.NewOpenAIProvider(
    nxuskit.WithOpenAIAPIKey("sk-..."),
)
```

### Claude (Anthropic)

```go
// Uses ANTHROPIC_API_KEY environment variable
provider, err := nxuskit.NewClaudeProvider()

// Or with explicit key
provider, err := nxuskit.NewClaudeProvider(
    nxuskit.WithClaudeAPIKey("sk-ant-..."),
)
```

### Ollama (Local)

```go
// Connects to localhost:11434 by default
provider, err := nxuskit.NewOllamaProvider()

// Or with custom host
provider, err := nxuskit.NewOllamaProvider(
    nxuskit.WithOllamaHost("http://192.168.1.100:11434"),
)
```

### LM Studio (Local)

```go
// Connects to localhost:1234 by default
provider, err := nxuskit.NewLmStudioProvider()
```

## Advanced Features

### Response Format (JSON Mode)

Request JSON-formatted output:

```go
req, _ := nxuskit.NewChatRequest("gpt-4o",
    nxuskit.WithMessages(
        nxuskit.SystemMessage("Always respond in JSON format."),
        nxuskit.UserMessage("List 3 colors with hex codes"),
    ),
    nxuskit.WithResponseFormat(nxuskit.ResponseFormatJSON()),
)
```

### JSON Schema Validation

Request output conforming to a specific schema:

```go
schema := map[string]any{
    "type": "object",
    "properties": map[string]any{
        "colors": map[string]any{
            "type": "array",
            "items": map[string]any{
                "type": "object",
                "properties": map[string]any{
                    "name": map[string]any{"type": "string"},
                    "hex":  map[string]any{"type": "string"},
                },
                "required": []string{"name", "hex"},
            },
        },
    },
    "required": []string{"colors"},
}

req, _ := nxuskit.NewChatRequest("gpt-4o",
    nxuskit.WithMessages(nxuskit.UserMessage("List 3 colors")),
    nxuskit.WithResponseFormat(nxuskit.ResponseFormatJSONSchema("colors", schema)),
)
```

### Tool/Function Calling

Define tools the model can call:

```go
weatherTool := nxuskit.NewTool(
    "get_weather",
    "Get the current weather for a location",
    map[string]any{
        "type": "object",
        "properties": map[string]any{
            "location": map[string]any{
                "type":        "string",
                "description": "City and state, e.g., San Francisco, CA",
            },
        },
        "required": []string{"location"},
    },
)

req, _ := nxuskit.NewChatRequest("gpt-4o",
    nxuskit.WithMessages(nxuskit.UserMessage("What's the weather in Tokyo?")),
    nxuskit.WithTools(weatherTool),
    nxuskit.WithToolChoice(nxuskit.ToolChoiceAuto()),
)
```

### Sampling Parameters

```go
req, _ := nxuskit.NewChatRequest("claude-sonnet-4-20250514",
    nxuskit.WithMessages(nxuskit.UserMessage("Write a haiku")),
    nxuskit.WithTemperature(0.9),
    nxuskit.WithTopP(0.95),
    nxuskit.WithTopK(40),      // Claude, Ollama, Together
    nxuskit.WithMaxTokens(100),
)

// MinP sampling (Ollama only)
req, _ := nxuskit.NewChatRequest("llama3:latest",
    nxuskit.WithMessages(nxuskit.UserMessage("Write a story")),
    nxuskit.WithMinP(0.05),
)
```

### Vision (Multimodal)

```go
msg := nxuskit.UserMessage("What's in this image?").
    WithImageURL("https://example.com/image.jpg")

// Or with base64
msg := nxuskit.UserMessage("Describe this").
    WithImageBase64(base64Data, "image/png")

req, _ := nxuskit.NewChatRequest("gpt-4o",
    nxuskit.WithMessages(msg),
)
```

## Inference Metadata

Access structured completion information from responses:

```go
resp, _ := provider.Chat(ctx, req)

// Check completion status
if resp.InferenceMetadata.IsComplete {
    fmt.Printf("Finished: %s\n", *resp.InferenceMetadata.FinishReason)
}

// Access token usage
if resp.InferenceMetadata.TokenUsage != nil {
    usage := resp.InferenceMetadata.TokenUsage.Actual
    fmt.Printf("Tokens: prompt=%d, completion=%d\n",
        usage.PromptTokens, usage.CompletionTokens)
}

// Access thinking traces (Claude extended thinking)
if resp.InferenceMetadata.ThinkingTrace != nil {
    fmt.Printf("Thinking: %s\n", *resp.InferenceMetadata.ThinkingTrace)
}

// Access inference steps (tool calls, thinking)
for _, step := range resp.InferenceMetadata.InferenceSteps {
    fmt.Printf("Step: %s (%s)\n", step.StepType, step.Identifier)
}
```

## Session Reset (Testing)

All providers implement `SessionResetter` for deterministic testing:

```go
// Create fresh session for each test
fresh, _ := provider.FreshSession()

// MockProvider resets response queue index
// Other providers return self (stateless)
```

## Model Discovery

Local providers support dynamic model listing via `ModelLister`:

```go
// Check if provider supports model listing
if lister, ok := provider.(nxuskit.ModelLister); ok {
    models, _ := lister.ListAvailableModels(ctx)
    for _, m := range models {
        fmt.Printf("- %s\n", m.Name)
    }
}

// Supported by: Ollama, LM Studio, Mock, Loopback
// Not supported by: Cloud providers (OpenAI, Claude, etc.)
```

## Provider Capabilities

Query what features a provider supports:

```go
provider, _ := nxuskit.NewOpenAIProvider()
caps := provider.GetCapabilities()

if caps.SupportsTools {
    // Add tools to request
}

if caps.SupportsResponseFormat {
    // Use response_format parameter
}

if caps.MaxStopSequences != nil && len(req.Stop) > *caps.MaxStopSequences {
    req.Stop = req.Stop[:*caps.MaxStopSequences]
}
```

### Capabilities Matrix

| Provider    | Streaming | Vision | Tools | JSON Mode | JSON Schema | TopK | MinP |
|-------------|-----------|--------|-------|-----------|-------------|------|------|
| OpenAI      | Yes       | Yes    | Yes   | Yes       | Yes         | No   | No   |
| Claude      | Yes       | Yes    | Yes   | No        | No          | Yes  | No   |
| Ollama      | Yes       | Yes*   | Yes*  | Yes       | Yes         | Yes  | Yes  |
| LM Studio   | Yes       | No     | No*   | No*       | No          | No   | No   |
| xAI Grok    | Yes       | Yes    | Yes   | Yes       | Yes         | No   | No   |
| Groq        | Yes       | No     | Yes*  | Yes       | No          | No   | No   |
| Together    | Yes       | Yes*   | Yes*  | Yes       | No          | Yes  | No   |
| Mistral     | Yes       | No     | Yes   | Yes       | No          | No   | No   |
| Fireworks   | Yes       | No     | No    | Yes       | No          | No   | No   |
| OpenRouter  | Yes       | Yes*   | Yes*  | Yes       | No*         | No   | No   |
| Perplexity  | Yes       | No     | No    | No        | No          | No   | No   |

*Model-dependent or backend-dependent

## Testing

### Unit Tests with HTTP Mocking

Use the built-in test utilities with [httpmock](https://github.com/jarcoal/httpmock):

```go
import (
    "testing"
    "github.com/jarcoal/httpmock"
    "github.com/nxus-SYSTEMS/nxusKit/packages/nxuskit-go"
    "github.com/nxus-SYSTEMS/nxusKit/packages/nxuskit-go/internal/testutil"
)

func TestMyCode(t *testing.T) {
    httpmock.Activate()
    defer httpmock.DeactivateAndReset()

    // Setup mock response
    testutil.SetupOpenAIMock("Hello from mock!")

    // Your test code
    provider, _ := nxuskit.NewOpenAIProvider(
        nxuskit.WithOpenAIAPIKey("test-key"),
    )

    req, _ := nxuskit.NewChatRequest("gpt-4o",
        nxuskit.WithMessages(nxuskit.UserMessage("Hello")))

    resp, err := provider.Chat(context.Background(), req)

    // Assertions
    if err != nil {
        t.Fatalf("unexpected error: %v", err)
    }
    if resp.Content != "Hello from mock!" {
        t.Errorf("unexpected content: %s", resp.Content)
    }

    // Verify mock was called
    testutil.AssertTotalCallCount(t, 1)
}
```

### Mock Provider

For testing without HTTP mocking, use the built-in mock provider with configurable
responses, streaming chunks, and model info:

```go
// Single response
provider := nxuskit.NewMockProvider(
    nxuskit.WithMockResponse("Test response"),
)

// Sequential responses (returns next on each Chat() call)
provider := nxuskit.NewMockProvider(
    nxuskit.WithMockResponse("First"),
    nxuskit.WithMockResponse("Second"),
)

// With streaming chunks
provider := nxuskit.NewMockProvider(
    nxuskit.WithMockResponse("Test response"),
    nxuskit.WithMockStreamChunks([]nxuskit.StreamChunk{
        {Delta: "Hello"},
        {Delta: " World"},
    }),
)
```

The mock provider implements the `LLMProvider` interface, enabling polymorphic
dispatch in tests — swap the real provider for the mock without changing calling code.

### Loopback Provider

Echoes back the last user message:

```go
provider := nxuskit.NewLoopbackProvider()
```

### Streaming Logprobs (v0.9.4+)

Per-chunk logprob deltas are now surfaced on streaming responses for
providers that support them (OpenAI). Check the capability flag before
issuing the call; non-supporting providers always emit `chunk.Logprobs == nil`
on every chunk (FR-007 — no phantom data).

```go
import (
    "context"
    "fmt"
    nxuskit "github.com/nxus-SYSTEMS/nxuskit-go"
)

provider, _ := nxuskit.NewOpenAIFromEnv()

if !provider.Capabilities().SupportsStreamingLogprobs {
    fmt.Println("Provider does not support streaming logprobs.")
}

req := &nxuskit.ChatRequest{
    Model:    "gpt-5.4",
    Messages: []nxuskit.Message{{Role: "user", Content: "Say hello."}},
    Logprobs: ptrBool(true),
    TopLogprobs: ptrInt(3),
}

chunks, errs := provider.ChatStream(context.Background(), req)
for chunk := range chunks {
    fmt.Print(chunk.Delta)
    if chunk.Logprobs != nil {
        for _, t := range chunk.Logprobs.Content {
            fmt.Printf("  token=%q logprob=%.4f\n", t.Token, t.Logprob)
        }
    }
}
if err := <-errs; err != nil { /* handle */ }
```

## Error Handling

nxuskit-go provides typed errors for common scenarios:

```go
resp, err := provider.Chat(ctx, req)
if err != nil {
    switch e := err.(type) {
    case *nxuskit.AuthenticationError:
        // Invalid API key
    case *nxuskit.RateLimitError:
        // Rate limited, e.RetryAfter contains suggested wait time
    case *nxuskit.InvalidRequestError:
        // Invalid request parameters
    case *nxuskit.NetworkError:
        // Connection failed
    default:
        // Other error
    }
}
```

## Pipeline Definitions

nxuskit-go supports defining multi-stage LLM workflows as portable JSON/YAML configuration files, compatible with Peeler.

### Loading Pipelines

```go
import "github.com/nxus-SYSTEMS/nxusKit/packages/nxuskit-go/pipeline"

// Load from JSON or YAML (auto-detected by extension)
p, err := pipeline.LoadPipeline("my-pipeline.yaml")
if err != nil {
    log.Fatal(err)
}

// Validate the DAG (checks for cycles, missing dependencies)
if err := p.Validate(); err != nil {
    log.Fatal(err)
}

fmt.Printf("Pipeline '%s' has %d stages\n", p.Name, len(p.Stages))
```

### Pipeline Format

```yaml
id: "example-pipeline"
name: "Example LLM Pipeline"
version: "1.0"
stages:
  - id: "extract"
    name: "Extract Entities"
    type: "llm"
    llm_config:
      provider: "openai"
      model: "gpt-4o"
      user_prompt: "Extract entities from: {{input}}"
      temperature: 0.3
  - id: "validate"
    name: "Validate with Rules"
    type: "clips_eval"
    upstream_stage_ids: ["extract"]
    clips_config:
      rules_source: "file"
      rules_content: "./rules/validation.clp"
      validation_severity: "error"
```

### Stage Types

- `llm` - LLM-only stage (calls an LLM provider)
- `clips_eval` - CLIPS evaluation stage (runs CLIPS rules)
- `clips_gen` - CLIPS generation stage (generates facts from CLIPS)
- `hybrid` - Hybrid stage (combines LLM and CLIPS)

### Format Conversion

```go
// Load and convert between formats
p, _ := pipeline.LoadPipeline("pipeline.json")

// Convert to YAML
yaml, _ := pipeline.PipelineToYAML(p)

// Convert to JSON
json, _ := pipeline.PipelineToJSON(p)
```

## CLIPS Provider Chat

Use CLIPS as a provider via the standard Chat API. Input facts are sent as JSON
in the user message; conclusions come back as JSON in the response content.

```go
provider, _ := nxuskit.NewClipsFFIProvider("/path/to/rules")

input := map[string]interface{}{
    "facts": []map[string]interface{}{
        {"template": "sensor", "values": map[string]interface{}{"name": "temp-1", "value": 150}},
    },
    "config": map[string]interface{}{"derived_only_new": true},
}
inputJSON, _ := json.Marshal(input)

req := nxuskit.NewChatRequest("clips").
    AddMessage(nxuskit.UserMessage(string(inputJSON)))
resp, _ := provider.Chat(context.Background(), req)
fmt.Println("Conclusions:", resp.Content)
```

The user message JSON must conform to the `ClipsInput` schema (see
[Rule Authoring Guide](../../docs/user/rule-authoring.md)). Unknown fields are
rejected. For the session-oriented API, see `ClipsSession` in the FFI bindings.

## CLIPS Security Validation

> **Writing CLIPS rules?** See the [Rule Authoring Guide](../../docs/user/rule-authoring.md)
> for templates, modules, testing patterns, and debugging techniques.

The library includes security validation for CLIPS rules:

```go
// Create validator with error severity (default - rejects dangerous rules)
validator := nxuskit.NewSecurityValidator(nxuskit.SecuritySeverityError)

rules := `
(defrule dangerous
    (trigger)
    =>
    (system "rm -rf /"))
`

result := validator.ValidateRules(rules)
if !result.Passed {
    for _, issue := range result.Issues {
        fmt.Printf("Security issue on line %d: %s\n",
            issue.LineNumber, issue.Description)
    }
}
```

### Severity Levels

- `SecuritySeverityError` - Reject rules with dangerous constructs (default)
- `SecuritySeverityWarning` - Log warning but proceed
- `SecuritySeverityInfo` - Log info message only
- `SecuritySeverityIgnore` - Skip validation entirely

### Detected Patterns

The validator detects dangerous CLIPS constructs:
- `system()` - Shell command execution
- `open()`, `close()`, `read()`, `readline()` - File I/O
- `batch()`, `load()`, `bload()` - External code loading
- `save()`, `bsave()` - File writing
- `remove()`, `rename()` - Filesystem modification

## String Similarity (Did-You-Mean)

Find similar strings for helpful error messages:

```go
candidates := []string{"patient", "symptom", "diagnosis", "treatment"}

// Find similar to a misspelled word
suggestions := nxuskit.FindSimilar("patiant", candidates)
// Returns: ["patient"]

// With custom threshold and max suggestions
suggestions := nxuskit.FindSimilarStrings("test", candidates, 0.7, 3)
```

## CLI

For command-line interactions (chat, model listing, capability detection, schema conversion, pipeline management), use the Rust CLI (`nxuskit-cli`). See [GETTING_STARTED.md](../../GETTING_STARTED.md) for CLI documentation.

The Go package is a library — import it in your Go applications rather than invoking a CLI binary.

## Environment Variables

| Variable | Provider | Description |
|----------|----------|-------------|
| `OPENAI_API_KEY` | OpenAI | API key |
| `ANTHROPIC_API_KEY` | Claude | API key |
| `XAI_API_KEY` | xAI Grok | API key |
| `GROQ_API_KEY` | Groq | API key |
| `TOGETHER_API_KEY` | Together | API key |
| `MISTRAL_API_KEY` | Mistral | API key |
| `FIREWORKS_API_KEY` | Fireworks | API key |
| `OPENROUTER_API_KEY` | OpenRouter | API key |
| `PERPLEXITY_API_KEY` | Perplexity | API key |

## License

Dual-licensed under MIT and Apache 2.0. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
