package nxuskit

import (
	"context"
)

const (
	mcpProviderName = "mcp"
	mcpFeatureName  = "MCP (Model Context Protocol)"
)

// McpProvider is a stub for the MCP (Model Context Protocol) feature.
//
// MCP enables LLM applications to connect to external tools, data sources, and
// services through a standardized protocol. This stub allows developers to
// discover the feature.
//
// All methods return LicenseRequired errors as this feature is not yet available.
//
// Example:
//
//	provider, err := nxuskit.NewMcpProvider()
//	if err != nil {
//	    log.Fatal(err)
//	}
//
//	resp, err := provider.Chat(ctx, req)
//	if errors.Is(err, nxuskit.ErrLicenseRequired) {
//	    // Handle not implemented feature
//	    fmt.Println("MCP is not yet implemented")
//	}
type McpProvider struct{}

// NewMcpProvider creates a new MCP provider stub.
//
// This always succeeds. Errors are returned when methods are called,
// allowing developers to discover the feature exists.
func NewMcpProvider() (*McpProvider, error) {
	return &McpProvider{}, nil
}

// ProviderName returns "mcp".
func (p *McpProvider) ProviderName() string {
	return mcpProviderName
}

// Chat returns a LicenseRequired error for MCP.
//
// The error message indicates that MCP is not yet implemented.
func (p *McpProvider) Chat(ctx context.Context, req *ChatRequest) (*ChatResponse, error) {
	return nil, NewLicenseRequiredError(mcpFeatureName)
}

// ChatStream returns error channels with LicenseRequired error.
//
// The error is sent on the error channel, not returned directly.
// The chunks channel is closed immediately.
func (p *McpProvider) ChatStream(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan error) {
	chunks := make(chan StreamChunk)
	errs := make(chan error, 1)

	go func() {
		defer close(chunks)
		defer close(errs)
		errs <- NewLicenseRequiredError(mcpFeatureName)
	}()

	return chunks, errs
}

// ListModels returns a LicenseRequired error.
func (p *McpProvider) ListModels(ctx context.Context) ([]ModelInfo, error) {
	return nil, NewLicenseRequiredError(mcpFeatureName)
}

// Ping returns a LicenseRequired error.
func (p *McpProvider) Ping(ctx context.Context) error {
	return NewLicenseRequiredError(mcpFeatureName)
}

// GetCapabilities returns default capabilities for the stub provider.
func (p *McpProvider) GetCapabilities() ProviderCapabilities {
	// Return empty capabilities for stub providers
	return DefaultCapabilities()
}

// StreamWithUsage returns error channels with LicenseRequired error.
func (p *McpProvider) StreamWithUsage(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan TokenUsage) {
	chunks := make(chan StreamChunk)
	usageChan := make(chan TokenUsage, 1)

	go func() {
		defer close(chunks)
		defer close(usageChan)
		// Send empty usage since we're returning an error
		usageChan <- TokenUsage{IsComplete: false}
	}()

	return chunks, usageChan
}

// Ensure McpProvider implements LLMProvider
var _ LLMProvider = (*McpProvider)(nil)
