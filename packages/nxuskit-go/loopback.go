package nxuskit

import (
	"context"
)

const loopbackProviderName = "loopback"

// LoopbackProvider is a debugging provider that echoes the last user message.
// Useful for testing message formatting and pipeline behavior without an LLM.
type LoopbackProvider struct{}

// NewLoopbackProvider creates a new loopback provider.
func NewLoopbackProvider() *LoopbackProvider {
	return &LoopbackProvider{}
}

// ProviderName returns "loopback".
func (p *LoopbackProvider) ProviderName() string {
	return loopbackProviderName
}

// Chat echoes the last user message as the assistant's response.
func (p *LoopbackProvider) Chat(ctx context.Context, req *ChatRequest) (*ChatResponse, error) {
	// Find the last user message
	content := p.extractLastUserMessage(req.Messages)

	fr := FinishReasonStop
	resp := &ChatResponse{
		Content:      content,
		Model:        "loopback",
		FinishReason: &fr,
		Usage: TokenUsage{
			Actual: &TokenCount{
				PromptTokens:     len(content) / 4, // Rough estimate
				CompletionTokens: len(content) / 4,
			},
			IsComplete: true,
		},
	}
	resp.InferenceMetadata = buildInferenceMetadataFromResponse(resp)
	return resp, nil
}

// ChatStream echoes the last user message as a single chunk.
func (p *LoopbackProvider) ChatStream(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan error) {
	chunks := make(chan StreamChunk)
	errs := make(chan error, 1)

	go func() {
		defer close(chunks)
		defer close(errs)

		content := p.extractLastUserMessage(req.Messages)
		fr := FinishReasonStop

		select {
		case <-ctx.Done():
			errs <- ctx.Err()
			return
		case chunks <- StreamChunk{
			Delta:        content,
			FinishReason: &fr,
			Usage: &TokenUsage{
				Actual: &TokenCount{
					PromptTokens:     len(content) / 4,
					CompletionTokens: len(content) / 4,
				},
				IsComplete: true,
			},
		}:
		}
	}()

	return chunks, errs
}

// ListModels returns a single "loopback" model.
func (p *LoopbackProvider) ListModels(ctx context.Context) ([]ModelInfo, error) {
	return []ModelInfo{
		{
			Name:        "loopback",
			Description: strPtrHelper("Echoes input back as output for debugging"),
		},
	}, nil
}

// Ping always succeeds for the loopback provider.
func (p *LoopbackProvider) Ping(ctx context.Context) error {
	return nil
}

// extractLastUserMessage finds the last user message and returns its text content.
func (p *LoopbackProvider) extractLastUserMessage(messages []Message) string {
	for i := len(messages) - 1; i >= 0; i-- {
		if messages[i].Role == RoleUser {
			return messages[i].Content.GetText()
		}
	}
	// Fallback to system message or empty
	for i := len(messages) - 1; i >= 0; i-- {
		if messages[i].Role == RoleSystem {
			return messages[i].Content.GetText()
		}
	}
	return ""
}

func strPtrHelper(s string) *string {
	return &s
}

// GetCapabilities returns the features and parameter limits of the loopback provider.
// The loopback provider supports all capabilities for testing purposes.
func (p *LoopbackProvider) GetCapabilities() ProviderCapabilities {
	return ProviderCapabilities{
		SupportsSystemMessages:   true,
		SupportsStreaming:        true,
		SupportsVision:           true,
		MaxStopSequences:         nil, // unlimited
		SupportsPresencePenalty:  true,
		SupportsFrequencyPenalty: true,
		SupportsSeed:             true,
		SupportsLogprobs:         true,
		MaxLogprobs:              nil,
		SupportsJSONMode:         true,
		SupportsJSONSchema:       true,
		SupportsTools:            true,
		SupportsResponseFormat:   true,
		SupportsTopK:             true,
		SupportsMinP:             true,
		PenaltyRange:             nil, // accepts any range
	}
}

// StreamWithUsage sends a streaming chat request and provides final token usage.
func (p *LoopbackProvider) StreamWithUsage(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan TokenUsage) {
	chunks, errs := p.ChatStream(ctx, req)
	return wrapStreamWithUsage(chunks, errs)
}

// FreshSession returns the receiver unchanged (stateless provider).
// This method exists for API consistency with nxuskit-engine.
func (p *LoopbackProvider) FreshSession() (LLMProvider, error) {
	return p, nil
}

// ListAvailableModels returns models available from this provider.
// This method exists for API consistency with nxuskit-engine (ModelLister interface).
func (p *LoopbackProvider) ListAvailableModels(ctx context.Context) ([]ModelInfo, error) {
	return p.ListModels(ctx)
}
