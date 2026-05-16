//go:build no_license

package nxuskit

import (
	"context"
	"errors"
	"strings"
	"testing"
)

// --- API Contract Tests: MCP Stubs Return ErrLicenseRequired ---
// These tests verify behavior when NO Pro license is present.
// Run on demand: go test -tags=no_license -run TestMcpContract ./...

func TestMcpContract_Chat_ReturnsLicenseRequired(t *testing.T) {
	provider, _ := NewMcpProvider()
	ctx := context.Background()
	req := &ChatRequest{
		Model:    "any",
		Messages: []Message{UserMessage("Hello")},
	}

	resp, err := provider.Chat(ctx, req)

	if resp != nil {
		t.Error("Chat() should return nil response")
	}
	if err == nil {
		t.Fatal("Chat() should return error")
	}
	if !errors.Is(err, ErrLicenseRequired) {
		t.Errorf("Chat() error should be ErrLicenseRequired, got: %v", err)
	}
	msg := err.Error()
	if !strings.Contains(msg, "MCP") {
		t.Errorf("error should contain feature name 'MCP', got: %s", msg)
	}
	if !strings.Contains(msg, "nxuskit Pro license") {
		t.Errorf("error should contain upgrade guidance, got: %s", msg)
	}
}

func TestMcpContract_ChatStream_ReturnsLicenseRequired(t *testing.T) {
	provider, _ := NewMcpProvider()
	ctx := context.Background()
	req := &ChatRequest{Model: "any"}

	chunks, errs := provider.ChatStream(ctx, req)

	chunkCount := 0
	for range chunks {
		chunkCount++
	}
	if chunkCount != 0 {
		t.Errorf("ChatStream() should return no chunks, got %d", chunkCount)
	}

	err := <-errs
	if err == nil {
		t.Fatal("ChatStream() should return error on error channel")
	}
	if !errors.Is(err, ErrLicenseRequired) {
		t.Errorf("ChatStream() error should be ErrLicenseRequired, got: %v", err)
	}
}

func TestMcpContract_ListModels_ReturnsLicenseRequired(t *testing.T) {
	provider, _ := NewMcpProvider()
	ctx := context.Background()

	models, err := provider.ListModels(ctx)

	if models != nil {
		t.Error("ListModels() should return nil models")
	}
	if err == nil {
		t.Fatal("ListModels() should return error")
	}
	if !errors.Is(err, ErrLicenseRequired) {
		t.Errorf("ListModels() error should be ErrLicenseRequired, got: %v", err)
	}
}

func TestMcpContract_Ping_ReturnsLicenseRequired(t *testing.T) {
	provider, _ := NewMcpProvider()
	ctx := context.Background()

	err := provider.Ping(ctx)
	if err == nil {
		t.Fatal("Ping() should return error")
	}
	if !errors.Is(err, ErrLicenseRequired) {
		t.Errorf("Ping() error should be ErrLicenseRequired, got: %v", err)
	}
}
