package testutil_test

import (
	"context"
	"testing"
	"time"

	"github.com/jarcoal/httpmock"

	"github.com/nxus-SYSTEMS/nxusKit/packages/nxuskit-go"
	"github.com/nxus-SYSTEMS/nxusKit/packages/nxuskit-go/internal/testutil"
)

// TestMockChatBasic demonstrates basic HTTP mocking for chat completions.
func TestMockChatBasic(t *testing.T) {
	// Activate mock transport
	httpmock.Activate()
	defer httpmock.DeactivateAndReset()

	// Setup mock response
	testutil.SetupOpenAIMock("Hello from mock!")

	// Create provider and make request
	provider, err := nxuskit.NewOpenAIProvider(
		nxuskit.WithOpenAIAPIKey("test-key"),
	)
	if err != nil {
		t.Fatalf("failed to create provider: %v", err)
	}

	req, err := nxuskit.NewChatRequest("gpt-4o",
		nxuskit.WithMessages(nxuskit.UserMessage("Hello")))
	if err != nil {
		t.Fatalf("failed to create request: %v", err)
	}

	resp, err := provider.Chat(context.Background(), req)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// Verify response
	if resp.Content != "Hello from mock!" {
		t.Errorf("expected 'Hello from mock!', got '%s'", resp.Content)
	}

	// Verify mock was called
	testutil.AssertTotalCallCount(t, 1)
	testutil.AssertCallCount(t, "POST", "https://api.openai.com/v1/chat/completions", 1)
}

// TestMockTimeout demonstrates timeout behavior testing.
func TestMockTimeout(t *testing.T) {
	httpmock.Activate()
	defer httpmock.DeactivateAndReset()

	// Setup a responder that delays 2 seconds
	httpmock.RegisterResponder("POST", "https://api.openai.com/v1/chat/completions",
		testutil.MockTimeoutResponder(2*time.Second, testutil.MockChatResponse("delayed")))

	provider, _ := nxuskit.NewOpenAIProvider(
		nxuskit.WithOpenAIAPIKey("test-key"),
		nxuskit.WithOpenAITimeout(100*time.Millisecond),
	)

	req, _ := nxuskit.NewChatRequest("gpt-4o",
		nxuskit.WithMessages(nxuskit.UserMessage("Hello")))

	// This should timeout
	ctx, cancel := context.WithTimeout(context.Background(), 100*time.Millisecond)
	defer cancel()

	_, err := provider.Chat(ctx, req)
	if err == nil {
		t.Error("expected timeout error, got nil")
	}
}

// TestMockRateLimit demonstrates rate limit response testing.
func TestMockRateLimit(t *testing.T) {
	httpmock.Activate()
	defer httpmock.DeactivateAndReset()

	// Setup rate limit response
	httpmock.RegisterResponder("POST", "https://api.openai.com/v1/chat/completions",
		testutil.MockRateLimitResponseWithRetryAfter("30"))

	provider, _ := nxuskit.NewOpenAIProvider(
		nxuskit.WithOpenAIAPIKey("test-key"),
	)

	req, _ := nxuskit.NewChatRequest("gpt-4o",
		nxuskit.WithMessages(nxuskit.UserMessage("Hello")))

	_, err := provider.Chat(context.Background(), req)
	if err == nil {
		t.Error("expected rate limit error, got nil")
	}

	// Verify the mock was called
	testutil.AssertTotalCallCount(t, 1)
}

// TestMockStreaming demonstrates streaming response testing.
func TestMockStreaming(t *testing.T) {
	httpmock.Activate()
	defer httpmock.DeactivateAndReset()

	// Setup streaming mock
	testutil.SetupOpenAIStreamingMock([]string{"Hello", " ", "World", "!"})

	provider, _ := nxuskit.NewOpenAIProvider(
		nxuskit.WithOpenAIAPIKey("test-key"),
	)

	req, _ := nxuskit.NewChatRequest("gpt-4o",
		nxuskit.WithMessages(nxuskit.UserMessage("Hello")),
		nxuskit.WithStream(true))

	chunks, errs := provider.ChatStream(context.Background(), req)

	// Collect streamed content
	var content string
	for chunk := range chunks {
		content += chunk.Delta
	}

	// Check for errors after streaming completes
	select {
	case err := <-errs:
		if err != nil {
			t.Fatalf("stream error: %v", err)
		}
	default:
	}

	expected := "Hello World!"
	if content != expected {
		t.Errorf("expected '%s', got '%s'", expected, content)
	}
}

// TestMockError demonstrates error response testing.
func TestMockError(t *testing.T) {
	httpmock.Activate()
	defer httpmock.DeactivateAndReset()

	// Setup error response
	httpmock.RegisterResponder("POST", "https://api.openai.com/v1/chat/completions",
		testutil.MockErrorResponse(500, "Internal server error"))

	provider, _ := nxuskit.NewOpenAIProvider(
		nxuskit.WithOpenAIAPIKey("test-key"),
	)

	req, _ := nxuskit.NewChatRequest("gpt-4o",
		nxuskit.WithMessages(nxuskit.UserMessage("Hello")))

	_, err := provider.Chat(context.Background(), req)
	if err == nil {
		t.Error("expected error, got nil")
	}
}

// TestActivateMockHelper demonstrates using the ActivateMock helper.
func TestActivateMockHelper(t *testing.T) {
	cleanup := testutil.ActivateMock()
	defer cleanup()

	testutil.SetupOpenAIMock("Hello!")

	provider, _ := nxuskit.NewOpenAIProvider(
		nxuskit.WithOpenAIAPIKey("test-key"),
	)

	req, _ := nxuskit.NewChatRequest("gpt-4o",
		nxuskit.WithMessages(nxuskit.UserMessage("Hello")))

	resp, err := provider.Chat(context.Background(), req)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if resp.Content != "Hello!" {
		t.Errorf("expected 'Hello!', got '%s'", resp.Content)
	}
}

// TestRequireEnvOrSkip demonstrates the RequireEnvOrSkip helper.
// This test will be skipped unless GOLLYLLM_TEST_VAR is set.
func TestRequireEnvOrSkip(t *testing.T) {
	// This will skip the test if the env var is not set
	_ = testutil.RequireEnvOrSkip(t, "GOLLYLLM_TEST_VAR")

	// If we reach here, the env var was set
	t.Log("GOLLYLLM_TEST_VAR is set, test continues")
}
