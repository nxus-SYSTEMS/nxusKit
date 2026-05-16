package nxuskit

import (
	"context"
	"testing"
)

// MCP no-license contract tests (asserting ErrLicenseRequired) are in
// mcp_stub_no_license_test.go, gated behind //go:build no_license.
// Run on demand: go test -tags=no_license -run TestMcpContract ./...

func TestMcpProvider_NewMcpProvider(t *testing.T) {
	provider, err := NewMcpProvider()
	if err != nil {
		t.Fatalf("NewMcpProvider() error: %v", err)
	}
	if provider == nil {
		t.Fatal("NewMcpProvider() returned nil provider")
	}
}

func TestMcpProvider_ProviderName(t *testing.T) {
	provider, _ := NewMcpProvider()
	if provider.ProviderName() != "mcp" {
		t.Errorf("ProviderName() = %q, want %q", provider.ProviderName(), "mcp")
	}
}

func TestMcpProvider_ChatStream_DoesNotPanic(t *testing.T) {
	defer func() {
		if r := recover(); r != nil {
			t.Errorf("ChatStream() panicked: %v", r)
		}
	}()

	provider, _ := NewMcpProvider()
	ctx := context.Background()
	req := &ChatRequest{Model: "any"}

	chunks, errs := provider.ChatStream(ctx, req)
	for range chunks {
	}
	<-errs
}

func TestMcpProvider_ImplementsLLMProvider(t *testing.T) {
	provider, _ := NewMcpProvider()
	var _ LLMProvider = provider
}

func TestMcpProvider_GetCapabilities(t *testing.T) {
	provider, _ := NewMcpProvider()
	caps := provider.GetCapabilities()
	if caps.SupportsSystemMessages != true {
		t.Error("GetCapabilities should return default capabilities with SupportsSystemMessages=true")
	}
}

func TestMcpProvider_StreamWithUsage(t *testing.T) {
	provider, _ := NewMcpProvider()
	ctx := context.Background()
	req := &ChatRequest{Model: "any"}

	chunks, usageCh := provider.StreamWithUsage(ctx, req)
	for range chunks {
	}
	usage := <-usageCh
	if usage.IsComplete {
		t.Error("StreamWithUsage should return incomplete usage for stub")
	}
}
