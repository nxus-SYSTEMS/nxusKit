// Package testutil provides testing utilities for nxuskit.
//
// This package contains HTTP mocking helpers, mock response fixtures,
// and utility functions for testing LLM provider implementations
// without making actual network requests.
//
// The package is internal and should only be used within nxuskit tests.
//
// # HTTP Mocking
//
// The package provides helpers for use with github.com/jarcoal/httpmock:
//
//   - MockChatResponse: Creates standard successful chat response fixtures
//   - MockStreamingChunks: Creates SSE chunks for streaming tests
//   - MockRateLimitResponse: Creates 429 responses with Retry-After headers
//   - MockTimeoutResponder: Creates responders that simulate timeouts
//   - MockErrorResponse: Creates error response fixtures
//
// # Test Helpers
//
//   - SetupMockProvider: Configures httpmock for a specific provider
//   - RequireEnvOrSkip: Skips test if required environment variable is not set
//
// # Usage
//
//	func TestChat(t *testing.T) {
//	    httpmock.Activate()
//	    defer httpmock.DeactivateAndReset()
//
//	    testutil.SetupOpenAIMock("Hello!")
//
//	    provider, _ := nxuskit.NewOpenAIProvider().Build()
//	    resp, err := provider.Chat(ctx, req)
//	    // assertions...
//	}
package testutil
