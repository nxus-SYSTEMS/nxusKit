package nxuskit

import (
	"context"
	"testing"
)

func TestLoopbackProvider_ProviderName(t *testing.T) {
	provider := NewLoopbackProvider()
	if provider.ProviderName() != "loopback" {
		t.Errorf("expected 'loopback', got '%s'", provider.ProviderName())
	}
}

func TestLoopbackProvider_Chat(t *testing.T) {
	t.Run("echoes last user message", func(t *testing.T) {
		provider := NewLoopbackProvider()

		resp, err := provider.Chat(context.Background(), &ChatRequest{
			Model: "any",
			Messages: []Message{
				SystemMessage("Be helpful"),
				UserMessage("Hello, how are you?"),
			},
		})
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}

		if resp.Content != "Hello, how are you?" {
			t.Errorf("expected 'Hello, how are you?', got '%s'", resp.Content)
		}
		if resp.Model != "loopback" {
			t.Errorf("expected model 'loopback', got '%s'", resp.Model)
		}
		if resp.FinishReason == nil || *resp.FinishReason != FinishReasonStop {
			t.Error("expected FinishReasonStop")
		}
		if resp.Usage.Actual == nil {
			t.Error("expected usage to be set")
		}
	})

	t.Run("echoes most recent user message", func(t *testing.T) {
		provider := NewLoopbackProvider()

		resp, err := provider.Chat(context.Background(), &ChatRequest{
			Model: "any",
			Messages: []Message{
				UserMessage("First message"),
				AssistantMessage("Response"),
				UserMessage("Second message"),
			},
		})
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}

		if resp.Content != "Second message" {
			t.Errorf("expected 'Second message', got '%s'", resp.Content)
		}
	})

	t.Run("fallback to system message if no user message", func(t *testing.T) {
		provider := NewLoopbackProvider()

		resp, err := provider.Chat(context.Background(), &ChatRequest{
			Model: "any",
			Messages: []Message{
				SystemMessage("System instructions"),
			},
		})
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}

		if resp.Content != "System instructions" {
			t.Errorf("expected 'System instructions', got '%s'", resp.Content)
		}
	})

	t.Run("empty response for no messages", func(t *testing.T) {
		provider := NewLoopbackProvider()

		resp, err := provider.Chat(context.Background(), &ChatRequest{
			Model:    "any",
			Messages: []Message{},
		})
		if err != nil {
			t.Fatalf("unexpected error: %v", err)
		}

		if resp.Content != "" {
			t.Errorf("expected empty content, got '%s'", resp.Content)
		}
	})
}

func TestLoopbackProvider_ChatStream(t *testing.T) {
	t.Run("echoes message in stream", func(t *testing.T) {
		provider := NewLoopbackProvider()

		chunks, errs := provider.ChatStream(context.Background(), &ChatRequest{
			Model: "any",
			Messages: []Message{
				UserMessage("Stream test"),
			},
		})

		var content string
		var finalChunk StreamChunk
		for chunk := range chunks {
			content += chunk.Delta
			finalChunk = chunk
		}

		if err := <-errs; err != nil {
			t.Fatalf("unexpected error: %v", err)
		}

		if content != "Stream test" {
			t.Errorf("expected 'Stream test', got '%s'", content)
		}
		if finalChunk.FinishReason == nil || *finalChunk.FinishReason != FinishReasonStop {
			t.Error("expected FinishReasonStop")
		}
		if finalChunk.Usage == nil {
			t.Error("expected usage in final chunk")
		}
	})

	t.Run("context cancellation", func(t *testing.T) {
		provider := NewLoopbackProvider()

		ctx, cancel := context.WithCancel(context.Background())
		cancel() // Cancel immediately

		chunks, errs := provider.ChatStream(ctx, &ChatRequest{
			Model:    "any",
			Messages: []Message{UserMessage("Test")},
		})

		// Drain chunks
		for range chunks {
		}

		err := <-errs
		// The error may be nil if the send completed before cancellation was detected
		if err != nil && err != context.Canceled {
			t.Errorf("expected context.Canceled or nil, got %v", err)
		}
	})
}

func TestLoopbackProvider_ListModels(t *testing.T) {
	provider := NewLoopbackProvider()

	models, err := provider.ListModels(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(models) != 1 {
		t.Fatalf("expected 1 model, got %d", len(models))
	}
	if models[0].Name != "loopback" {
		t.Errorf("expected model name 'loopback', got '%s'", models[0].Name)
	}
	if models[0].Description == nil {
		t.Error("expected description to be set")
	}
}

func TestLoopbackProvider_Ping(t *testing.T) {
	provider := NewLoopbackProvider()
	err := provider.Ping(context.Background())
	if err != nil {
		t.Errorf("unexpected error: %v", err)
	}
}
