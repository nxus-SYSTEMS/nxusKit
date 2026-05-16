# Changelog

All notable changes to nxuskit-go will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.9.3] - 2026-04-29

### Changed

- **SDK lockstep versioning**: Go module metadata and version constants are
  aligned with the published SDK `0.9.3` release.
- **FFI version guard coverage**: Added tests for expected host SDK version
  compatibility when using the native SDK through the Go wrapper.

### Notes

- This was a narrow SDK release. The first-class v0.9.3 logprobs API work
  shipped for Rust, Python, and the C ABI; Go remains version-aligned and
  compatible with the v0.9.3 SDK bundles.

## [0.2.0] - 2026-01-30

### Added

- **Test Utilities** (`internal/testutil`):
  - `MockChatResponse`, `MockClaudeResponse`, `MockOllamaResponse` - Mock response fixtures
  - `MockStreamingChunks`, `MockClaudeStreamingChunks`, `MockOllamaStreamingChunks` - SSE streaming helpers
  - `SetupOpenAIMock`, `SetupClaudeMock`, `SetupOllamaMock` - Quick mock registration
  - `SSEResponder`, `NDJSONResponder` - Low-level streaming responders
  - `MockErrorResponse`, `MockRateLimitResponse` - Error simulation
  - `MockTimeoutResponder`, `MockTimeoutError` - Timeout testing
  - `ActivateMock`, `AssertCallCount`, `AssertTotalCallCount` - Test helpers
  - `RequireEnvOrSkip`, `RequireEnvsOrSkip` - Integration test helpers
  - `IntPtr`, `Float64Ptr`, `StringPtr`, `BoolPtr` - Pointer helpers

- **Response Format Types**:
  - `ResponseFormat` - Specify desired output format (text, json_object, json_schema)
  - `JSONSchema` - Schema definition for structured outputs
  - `ResponseFormatText()`, `ResponseFormatJSON()`, `ResponseFormatJSONSchema()` - Constructors

- **Tool/Function Calling Types**:
  - `Tool` - Function tool definition
  - `ToolFunction` - Function name, description, and parameters
  - `ToolChoice` - Control tool selection (auto, none, required, function)
  - `NewTool()` - Tool constructor
  - `ToolChoiceAuto()`, `ToolChoiceNone()`, `ToolChoiceRequired()`, `ToolChoiceFunc()` - Choice constructors

- **Sampling Parameters**:
  - `TopK` - Top-K sampling (Claude, Ollama, Together)
  - `MinP` - Min-P sampling (Ollama)

- **Functional Options**:
  - `WithResponseFormat()` - Set response format
  - `WithTools()` - Add tools to request
  - `WithToolChoice()` - Set tool choice
  - `WithTopK()` - Set Top-K sampling
  - `WithMinP()` - Set Min-P sampling

- **Provider Capability Fields**:
  - `SupportsTools` - Whether provider supports function calling
  - `SupportsResponseFormat` - Whether provider supports response_format parameter
  - `SupportsTopK` - Whether provider supports Top-K sampling
  - `SupportsMinP` - Whether provider supports Min-P sampling

- **Integration Tests**:
  - OpenAI, Claude, Groq, Ollama integration test files
  - Tests for Chat, ChatStream, JSONMode, Tools, TopK/MinP

### Changed

- Updated provider capabilities with accurate tool, response format, and sampling support

## [0.1.0] - 2026-01-29

### Added

- **Core Types**: ChatRequest, ChatResponse, Message, TokenUsage, StreamChunk, LLMError
- **LLMProvider Interface**: Chat, ChatStream, ListModels, Ping, GetCapabilities, StreamWithUsage
- **14 Providers**:
  - Cloud: OpenAI, Claude, Groq, Fireworks, Mistral, OpenRouter, Perplexity, Together
  - Local: Ollama, LM Studio
  - Utility: Mock, Loopback
  - Pro Stubs: MCP, CLIPS (return ErrLicenseRequired)
- **Convenience APIs**: Completion(), CompletionStream(), CompletionWithMessages()
- **Provider Registry**: Auto-detection of providers from model strings (e.g., "openai/gpt-4o")
- **ProviderCapabilities**: Capability discovery for all providers
- **ModelCapabilities**: Per-model capability detection (Ollama via show API)
- **CapabilityCache**: TTL-based caching for model capabilities
- **ParameterAdapter**: Graceful parameter degradation with warnings
  - Stop sequence truncation
  - Penalty/seed removal for unsupported providers
  - Logprobs adjustment to provider limits
  - JSON mode fallback to system message
- **ParseRetryAfter**: RFC 7231 compliant Retry-After header parsing
- **Vision Support**: Multimodal messages with base64 and URL images
- **Thinking Mode**: Chain-of-thought reasoning support (Ollama, Claude)
- **Streaming**: SSE-based streaming with token usage tracking

### Provider Capabilities Summary

| Provider | Streaming | Vision | Penalties | Seed | Logprobs | JSON Mode |
|----------|-----------|--------|-----------|------|----------|-----------|
| OpenAI | ✓ | ✓ | ✓ | ✓ | ✓ (max 20) | ✓ |
| Claude | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ |
| Ollama | ✓ | ✓* | ✓ | ✓ | ✗ | ✓ |
| LM Studio | ✓ | ✓* | ✓ | ✓ | ✗ | ✗ |
| Groq | ✓ | ✗ | ✗ | ✓ | ✗ | ✓ |
| Fireworks | ✓ | ✗ | ✓ | ✓ | ✗ | ✓ |
| Mistral | ✓ | ✗ | ✗ | ✓ | ✗ | ✓ |
| OpenRouter | ✓ | ✓* | ✓ | ✓ | ✓* | ✓ |
| Perplexity | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ |
| Together | ✓ | ✗ | ✓ | ✓ | ✓ | ✓ |

*Model-dependent
