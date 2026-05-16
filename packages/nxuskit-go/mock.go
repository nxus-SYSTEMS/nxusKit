package nxuskit

import (
	"context"
	"sync"
)

const mockProviderName = "mock"

// MockProvider is a testing provider that returns pre-configured responses.
// It records all requests for later inspection and is safe for concurrent use.
type MockProvider struct {
	mu               sync.Mutex
	responses        []*ChatResponse
	streamResponses  [][]StreamChunk
	errors           []error
	recordedRequests []*ChatRequest
	models           []ModelInfo
	responseIndex    int
	streamIndex      int
	errorIndex       int
	// streamingLogprobs is a parallel sequence of optional logprob deltas injected
	// per-chunk during streaming. Indexed by stream-response index then chunk index.
	streamingLogprobs [][]*StreamLogprobsDelta
	// supportsStreamingLogprobs controls the capability flag for this mock.
	supportsStreamingLogprobs bool

	// originalOpts stores the options used to create this provider,
	// enabling FreshSession() to create a new instance with the same config.
	originalOpts []MockOption
}

// mockConfig holds configuration for MockProvider.
type mockConfig struct {
	responses                 []*ChatResponse
	streamResponses           [][]StreamChunk
	errors                    []error
	models                    []ModelInfo
	streamingLogprobs         [][]*StreamLogprobsDelta
	supportsStreamingLogprobs bool
}

// MockOption configures a MockProvider.
type MockOption func(*mockConfig)

// WithMockResponse adds a response to the queue.
func WithMockResponse(resp *ChatResponse) MockOption {
	return func(c *mockConfig) {
		c.responses = append(c.responses, resp)
	}
}

// WithMockResponses adds multiple responses to the queue.
func WithMockResponses(resps ...*ChatResponse) MockOption {
	return func(c *mockConfig) {
		c.responses = append(c.responses, resps...)
	}
}

// WithMockError adds an error to the queue.
func WithMockError(err error) MockOption {
	return func(c *mockConfig) {
		c.errors = append(c.errors, err)
	}
}

// WithMockStreamResponse adds stream chunks to the queue.
func WithMockStreamResponse(chunks []StreamChunk) MockOption {
	return func(c *mockConfig) {
		c.streamResponses = append(c.streamResponses, chunks)
	}
}

// WithMockModels sets the models to return from ListModels.
func WithMockModels(models []ModelInfo) MockOption {
	return func(c *mockConfig) {
		c.models = models
	}
}

// WithStreamingLogprobs attaches a per-stream-response sequence of per-chunk logprob
// deltas to the mock. Each inner slice aligns with the chunks added by the
// corresponding WithMockStreamResponse call. Use nil entries for chunks that
// should carry no logprob data.
func WithStreamingLogprobs(deltas [][]*StreamLogprobsDelta) MockOption {
	return func(c *mockConfig) {
		c.streamingLogprobs = deltas
		c.supportsStreamingLogprobs = true
	}
}

// NewMockProvider creates a new mock provider for testing.
func NewMockProvider(opts ...MockOption) *MockProvider {
	cfg := &mockConfig{
		models: []ModelInfo{{Name: "mock-model"}},
	}

	for _, opt := range opts {
		opt(cfg)
	}

	return &MockProvider{
		responses:                 cfg.responses,
		streamResponses:           cfg.streamResponses,
		errors:                    cfg.errors,
		models:                    cfg.models,
		streamingLogprobs:         cfg.streamingLogprobs,
		supportsStreamingLogprobs: cfg.supportsStreamingLogprobs,
		originalOpts:              opts, // Store for FreshSession()
	}
}

// ProviderName returns "mock".
func (p *MockProvider) ProviderName() string {
	return mockProviderName
}

// Chat returns the next queued response or error.
func (p *MockProvider) Chat(ctx context.Context, req *ChatRequest) (*ChatResponse, error) {
	p.mu.Lock()
	defer p.mu.Unlock()

	// Record the request
	reqCopy := *req
	p.recordedRequests = append(p.recordedRequests, &reqCopy)

	// Return error if queued
	if p.errorIndex < len(p.errors) {
		err := p.errors[p.errorIndex]
		p.errorIndex++
		return nil, err
	}

	// Return response if queued
	if p.responseIndex < len(p.responses) {
		resp := p.responses[p.responseIndex]
		p.responseIndex++
		// Ensure InferenceMetadata is populated if not already
		if !resp.InferenceMetadata.IsComplete && resp.InferenceMetadata.FinishReason == nil {
			resp.InferenceMetadata = buildInferenceMetadataFromResponse(resp)
		}
		return resp, nil
	}

	// Default response
	fr := FinishReasonStop
	resp := &ChatResponse{
		Content:      "Mock response",
		Model:        "mock-model",
		FinishReason: &fr,
		Usage: TokenUsage{
			Actual: &TokenCount{
				PromptTokens:     10,
				CompletionTokens: 5,
			},
			IsComplete: true,
		},
	}
	resp.InferenceMetadata = buildInferenceMetadataFromResponse(resp)
	return resp, nil
}

// ChatStream returns the next queued stream chunks.
func (p *MockProvider) ChatStream(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan error) {
	chunks := make(chan StreamChunk)
	errs := make(chan error, 1)

	go func() {
		defer close(chunks)
		defer close(errs)

		p.mu.Lock()
		reqCopy := *req
		p.recordedRequests = append(p.recordedRequests, &reqCopy)

		// Check for queued error first
		if p.errorIndex < len(p.errors) {
			err := p.errors[p.errorIndex]
			p.errorIndex++
			p.mu.Unlock()
			errs <- err
			return
		}

		// Get stream chunks
		var streamChunks []StreamChunk
		var chunkLogprobs []*StreamLogprobsDelta
		if p.streamIndex < len(p.streamResponses) {
			streamChunks = p.streamResponses[p.streamIndex]
			if p.streamIndex < len(p.streamingLogprobs) {
				chunkLogprobs = p.streamingLogprobs[p.streamIndex]
			}
			p.streamIndex++
		} else {
			// Default stream chunks
			fr := FinishReasonStop
			streamChunks = []StreamChunk{
				{Delta: "Mock "},
				{Delta: "stream "},
				{Delta: "response", FinishReason: &fr},
			}
		}
		p.mu.Unlock()

		for i, chunk := range streamChunks {
			if i < len(chunkLogprobs) && chunkLogprobs[i] != nil {
				chunk.Logprobs = chunkLogprobs[i]
			}
			select {
			case <-ctx.Done():
				errs <- ctx.Err()
				return
			case chunks <- chunk:
			}
		}
	}()

	return chunks, errs
}

// ListModels returns the configured models.
func (p *MockProvider) ListModels(ctx context.Context) ([]ModelInfo, error) {
	p.mu.Lock()
	defer p.mu.Unlock()
	return p.models, nil
}

// Ping always succeeds for the mock provider.
func (p *MockProvider) Ping(ctx context.Context) error {
	return nil
}

// GetRecordedRequests returns all requests that were sent to the mock provider.
func (p *MockProvider) GetRecordedRequests() []*ChatRequest {
	p.mu.Lock()
	defer p.mu.Unlock()
	result := make([]*ChatRequest, len(p.recordedRequests))
	copy(result, p.recordedRequests)
	return result
}

// Reset clears all recorded requests and resets response indices.
func (p *MockProvider) Reset() {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.recordedRequests = nil
	p.responseIndex = 0
	p.streamIndex = 0
	p.errorIndex = 0
}

// AddResponse adds a response to the queue (thread-safe).
func (p *MockProvider) AddResponse(resp *ChatResponse) {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.responses = append(p.responses, resp)
}

// AddError adds an error to the queue (thread-safe).
func (p *MockProvider) AddError(err error) {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.errors = append(p.errors, err)
}

// AddStreamResponse adds stream chunks to the queue (thread-safe).
func (p *MockProvider) AddStreamResponse(chunks []StreamChunk) {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.streamResponses = append(p.streamResponses, chunks)
}

// GetCapabilities returns the features and parameter limits of the mock provider.
// SupportsStreamingLogprobs reflects whether WithStreamingLogprobs was configured.
func (p *MockProvider) GetCapabilities() ProviderCapabilities {
	p.mu.Lock()
	supportsLP := p.supportsStreamingLogprobs
	p.mu.Unlock()
	return ProviderCapabilities{
		SupportsSystemMessages:    true,
		SupportsStreaming:         true,
		SupportsVision:            true,
		MaxStopSequences:          nil, // unlimited
		SupportsPresencePenalty:   true,
		SupportsFrequencyPenalty:  true,
		SupportsSeed:              true,
		SupportsLogprobs:          true,
		MaxLogprobs:               nil,
		SupportsStreamingLogprobs: supportsLP,
		SupportsJSONMode:          true,
		SupportsJSONSchema:        true,
		SupportsTools:             true,
		SupportsResponseFormat:    true,
		SupportsTopK:              true,
		SupportsMinP:              true,
		PenaltyRange:              nil, // accepts any range
	}
}

// StreamWithUsage sends a streaming chat request and provides final token usage.
func (p *MockProvider) StreamWithUsage(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan TokenUsage) {
	chunks, errs := p.ChatStream(ctx, req)
	return wrapStreamWithUsage(chunks, errs)
}

// FreshSession creates a new mock provider with the same configuration but reset state.
// This method exists for API consistency with nxuskit-engine.
//
// Unlike stateless providers that return themselves, MockProvider creates a new instance
// with fresh response queue indices. This enables deterministic testing where each test
// gets the same sequence of responses.
func (p *MockProvider) FreshSession() (LLMProvider, error) {
	return NewMockProvider(p.originalOpts...), nil
}

// ListAvailableModels returns models available from this provider.
// This method exists for API consistency with nxuskit-engine (ModelLister interface).
func (p *MockProvider) ListAvailableModels(ctx context.Context) ([]ModelInfo, error) {
	return p.ListModels(ctx)
}
