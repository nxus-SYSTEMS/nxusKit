package nxuskit

import (
	"context"
	"errors"
	"sync"
	"testing"
)

func TestMockProvider_SingleResponse(t *testing.T) {
	resp := &ChatResponse{
		Content: "Hello from mock!",
		Model:   "test-model",
	}

	provider := NewMockProvider(WithMockResponse(resp))

	result, err := provider.Chat(context.Background(), &ChatRequest{
		Model:    "any-model",
		Messages: []Message{UserMessage("Hi")},
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if result.Content != "Hello from mock!" {
		t.Errorf("expected content 'Hello from mock!', got '%s'", result.Content)
	}
}

func TestMockProvider_QueuedResponses(t *testing.T) {
	resp1 := &ChatResponse{Content: "First"}
	resp2 := &ChatResponse{Content: "Second"}
	resp3 := &ChatResponse{Content: "Third"}

	provider := NewMockProvider(WithMockResponses(resp1, resp2, resp3))

	req := &ChatRequest{Model: "test", Messages: []Message{UserMessage("Hi")}}

	// First call
	result, _ := provider.Chat(context.Background(), req)
	if result.Content != "First" {
		t.Errorf("expected 'First', got '%s'", result.Content)
	}

	// Second call
	result, _ = provider.Chat(context.Background(), req)
	if result.Content != "Second" {
		t.Errorf("expected 'Second', got '%s'", result.Content)
	}

	// Third call
	result, _ = provider.Chat(context.Background(), req)
	if result.Content != "Third" {
		t.Errorf("expected 'Third', got '%s'", result.Content)
	}

	// Fourth call - should get default response
	result, _ = provider.Chat(context.Background(), req)
	if result.Content != "Mock response" {
		t.Errorf("expected default 'Mock response', got '%s'", result.Content)
	}
}

func TestMockProvider_ErrorResponse(t *testing.T) {
	testErr := errors.New("test error")
	provider := NewMockProvider(WithMockError(testErr))

	_, err := provider.Chat(context.Background(), &ChatRequest{
		Model:    "test",
		Messages: []Message{UserMessage("Hi")},
	})
	if err != testErr {
		t.Errorf("expected test error, got %v", err)
	}
}

func TestMockProvider_ChatStream(t *testing.T) {
	fr := FinishReasonStop
	chunks := []StreamChunk{
		{Delta: "Hello"},
		{Delta: " world"},
		{Delta: "!", FinishReason: &fr},
	}

	provider := NewMockProvider(WithMockStreamResponse(chunks))

	streamChunks, errs := provider.ChatStream(context.Background(), &ChatRequest{
		Model:    "test",
		Messages: []Message{UserMessage("Hi")},
	})

	var content string
	var finalChunk StreamChunk
	for chunk := range streamChunks {
		content += chunk.Delta
		finalChunk = chunk
	}

	if err := <-errs; err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if content != "Hello world!" {
		t.Errorf("expected 'Hello world!', got '%s'", content)
	}
	if finalChunk.FinishReason == nil || *finalChunk.FinishReason != FinishReasonStop {
		t.Error("expected final chunk with FinishReasonStop")
	}
}

func TestMockProvider_RequestRecording(t *testing.T) {
	provider := NewMockProvider()

	// Make some requests
	req1 := &ChatRequest{Model: "model1", Messages: []Message{UserMessage("First")}}
	req2 := &ChatRequest{Model: "model2", Messages: []Message{UserMessage("Second")}}

	_, _ = provider.Chat(context.Background(), req1)
	_, _ = provider.Chat(context.Background(), req2)

	recorded := provider.GetRecordedRequests()
	if len(recorded) != 2 {
		t.Fatalf("expected 2 recorded requests, got %d", len(recorded))
	}
	if recorded[0].Model != "model1" {
		t.Errorf("expected model1, got '%s'", recorded[0].Model)
	}
	if recorded[1].Model != "model2" {
		t.Errorf("expected model2, got '%s'", recorded[1].Model)
	}
}

func TestMockProvider_Reset(t *testing.T) {
	resp := &ChatResponse{Content: "Test"}
	provider := NewMockProvider(WithMockResponse(resp))

	req := &ChatRequest{Model: "test", Messages: []Message{UserMessage("Hi")}}

	// First call
	result, _ := provider.Chat(context.Background(), req)
	if result.Content != "Test" {
		t.Errorf("expected 'Test', got '%s'", result.Content)
	}

	// Second call - should get default
	result, _ = provider.Chat(context.Background(), req)
	if result.Content != "Mock response" {
		t.Errorf("expected default response")
	}

	// Reset
	provider.Reset()

	// After reset - should get "Test" again
	result, _ = provider.Chat(context.Background(), req)
	if result.Content != "Test" {
		t.Errorf("expected 'Test' after reset, got '%s'", result.Content)
	}

	// Check recorded requests were cleared
	if len(provider.GetRecordedRequests()) != 1 {
		t.Errorf("expected 1 recorded request after reset, got %d", len(provider.GetRecordedRequests()))
	}
}

func TestMockProvider_ThreadSafety(t *testing.T) {
	provider := NewMockProvider()

	var wg sync.WaitGroup
	numGoroutines := 100

	for i := 0; i < numGoroutines; i++ {
		wg.Add(1)
		go func(n int) {
			defer wg.Done()
			req := &ChatRequest{Model: "test", Messages: []Message{UserMessage("Hi")}}
			_, _ = provider.Chat(context.Background(), req)
		}(i)
	}

	wg.Wait()

	recorded := provider.GetRecordedRequests()
	if len(recorded) != numGoroutines {
		t.Errorf("expected %d recorded requests, got %d", numGoroutines, len(recorded))
	}
}

func TestMockProvider_ProviderName(t *testing.T) {
	provider := NewMockProvider()
	if provider.ProviderName() != "mock" {
		t.Errorf("expected 'mock', got '%s'", provider.ProviderName())
	}
}

func TestMockProvider_ListModels(t *testing.T) {
	models := []ModelInfo{
		{Name: "custom-model-1"},
		{Name: "custom-model-2"},
	}

	provider := NewMockProvider(WithMockModels(models))

	result, err := provider.ListModels(context.Background())
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(result) != 2 {
		t.Fatalf("expected 2 models, got %d", len(result))
	}
	if result[0].Name != "custom-model-1" {
		t.Errorf("expected 'custom-model-1', got '%s'", result[0].Name)
	}
}

func TestMockProvider_Ping(t *testing.T) {
	provider := NewMockProvider()
	err := provider.Ping(context.Background())
	if err != nil {
		t.Errorf("unexpected error: %v", err)
	}
}

func TestMockProvider_AddResponse(t *testing.T) {
	provider := NewMockProvider()

	// Add response after creation
	provider.AddResponse(&ChatResponse{Content: "Added"})

	result, _ := provider.Chat(context.Background(), &ChatRequest{
		Model:    "test",
		Messages: []Message{UserMessage("Hi")},
	})
	if result.Content != "Added" {
		t.Errorf("expected 'Added', got '%s'", result.Content)
	}
}

func TestMockProvider_StreamWithError(t *testing.T) {
	testErr := errors.New("stream error")
	provider := NewMockProvider(WithMockError(testErr))

	chunks, errs := provider.ChatStream(context.Background(), &ChatRequest{
		Model:    "test",
		Messages: []Message{UserMessage("Hi")},
	})

	// Drain chunks
	for range chunks {
	}

	err := <-errs
	if err != testErr {
		t.Errorf("expected test error, got %v", err)
	}
}

// Tests for InferenceMetadata population (T022)

func TestMockProvider_InferenceMetadata_DefaultResponse(t *testing.T) {
	provider := NewMockProvider()

	result, err := provider.Chat(context.Background(), &ChatRequest{
		Model:    "test",
		Messages: []Message{UserMessage("Hi")},
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// Verify InferenceMetadata is populated
	meta := result.InferenceMetadata
	if !meta.IsComplete {
		t.Error("InferenceMetadata.IsComplete should be true for default response")
	}
	if meta.TokenUsage == nil {
		t.Error("InferenceMetadata.TokenUsage should not be nil")
	}
	if meta.TokenUsage != nil && meta.TokenUsage.Actual == nil {
		t.Error("InferenceMetadata.TokenUsage.Actual should not be nil")
	}
}

func TestMockProvider_InferenceMetadata_QueuedResponse(t *testing.T) {
	fr := FinishReasonStop
	resp := &ChatResponse{
		Content:      "Queued response",
		Model:        "test-model",
		FinishReason: &fr,
		Usage: TokenUsage{
			Actual:     &TokenCount{PromptTokens: 50, CompletionTokens: 25},
			IsComplete: true,
		},
	}

	provider := NewMockProvider(WithMockResponse(resp))

	result, err := provider.Chat(context.Background(), &ChatRequest{
		Model:    "test",
		Messages: []Message{UserMessage("Hi")},
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// Verify InferenceMetadata is populated from the queued response
	meta := result.InferenceMetadata
	if !meta.IsComplete {
		t.Error("InferenceMetadata.IsComplete should be true")
	}
	if meta.FinishReason == nil || *meta.FinishReason != FinishReasonStop {
		t.Errorf("InferenceMetadata.FinishReason should be stop, got %v", meta.FinishReason)
	}
	if meta.TokenUsage == nil {
		t.Fatal("InferenceMetadata.TokenUsage should not be nil")
	}
	if meta.TokenUsage.Actual.PromptTokens != 50 {
		t.Errorf("PromptTokens = %d, want 50", meta.TokenUsage.Actual.PromptTokens)
	}
}

// Tests for FreshSession (T039)

func TestMockProvider_FreshSession(t *testing.T) {
	// Create provider with multiple responses
	provider := NewMockProvider(
		WithMockResponse(&ChatResponse{Content: "Response 1"}),
		WithMockResponse(&ChatResponse{Content: "Response 2"}),
		WithMockResponse(&ChatResponse{Content: "Response 3"}),
	)

	req := &ChatRequest{Model: "test", Messages: []Message{UserMessage("Hi")}}

	// Consume first two responses
	result, _ := provider.Chat(context.Background(), req)
	if result.Content != "Response 1" {
		t.Errorf("Expected 'Response 1', got %q", result.Content)
	}

	result, _ = provider.Chat(context.Background(), req)
	if result.Content != "Response 2" {
		t.Errorf("Expected 'Response 2', got %q", result.Content)
	}

	// Create fresh session
	fresh, err := provider.FreshSession()
	if err != nil {
		t.Fatalf("FreshSession error: %v", err)
	}

	// Fresh session should start from "Response 1" again
	result, _ = fresh.Chat(context.Background(), req)
	if result.Content != "Response 1" {
		t.Errorf("FreshSession: expected 'Response 1', got %q", result.Content)
	}

	// Fresh session should be independent - get "Response 2"
	result, _ = fresh.Chat(context.Background(), req)
	if result.Content != "Response 2" {
		t.Errorf("FreshSession: expected 'Response 2', got %q", result.Content)
	}

	// Original provider should continue from "Response 3"
	result, _ = provider.Chat(context.Background(), req)
	if result.Content != "Response 3" {
		t.Errorf("Original: expected 'Response 3', got %q", result.Content)
	}

	// Verify fresh session didn't record requests from original
	freshMock := fresh.(*MockProvider)
	if len(freshMock.GetRecordedRequests()) != 2 {
		t.Errorf("Fresh provider should have 2 recorded requests, got %d", len(freshMock.GetRecordedRequests()))
	}
}
