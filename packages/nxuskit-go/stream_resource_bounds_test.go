package nxuskit

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/nxus-SYSTEMS/nxusKit/packages/nxuskit-go/internal/httputil"
)

func TestResourceStreamOllamaExactRecordLimit(t *testing.T) {
	prefix := []byte(`{"model":"m","message":{"content":"`)
	suffix := []byte(`"},"done":true}`)
	n := httputil.MaxStreamRecordEventBytes - len(prefix) - len(suffix)
	if n <= 0 {
		t.Fatal("invalid test fixture size")
	}
	body := make([]byte, 0, httputil.MaxStreamRecordEventBytes+1)
	body = append(body, prefix...)
	body = append(body, bytes.Repeat([]byte{'x'}, n)...)
	body = append(body, suffix...)
	if len(body) != httputil.MaxStreamRecordEventBytes {
		t.Fatalf("record length = %d", len(body))
	}

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_, _ = w.Write(body)
		_, _ = fmt.Fprintln(w)
	}))
	defer server.Close()

	provider, _ := NewOllamaProvider(WithOllamaBaseURL(server.URL))
	req := &ChatRequest{Model: "m"}
	chunks, errs := provider.ChatStream(context.Background(), req)
	finals := 0
	for chunk := range chunks {
		if chunk.IsFinal() {
			finals++
		}
	}
	if err := <-errs; err != nil {
		t.Fatalf("exact-limit record failed: %v", err)
	}
	if finals != 1 {
		t.Fatalf("finals = %d", finals)
	}
}

func TestResourceStreamOllamaOneByteOver(t *testing.T) {
	body := bytes.Repeat([]byte{'x'}, httputil.MaxStreamRecordEventBytes+1)
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_, _ = w.Write(body)
		_, _ = fmt.Fprintln(w)
	}))
	defer server.Close()

	provider, _ := NewOllamaProvider(WithOllamaBaseURL(server.URL))
	req := &ChatRequest{Model: "m"}
	chunks, errs := provider.ChatStream(context.Background(), req)
	for range chunks {
	}
	err := <-errs
	if !errors.Is(err, httputil.ErrStreamRecordTooLarge) {
		t.Fatalf("expected bounded stream sentinel, got %v", err)
	}
}

func TestResourceStreamLmStudioOversizeIsError(t *testing.T) {
	first := httputil.MaxStreamRecordEventBytes / 2
	second := httputil.MaxStreamRecordEventBytes - first
	part1 := bytes.Repeat([]byte{'x'}, first)
	part2 := bytes.Repeat([]byte{'y'}, second)
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/event-stream")
		_, _ = fmt.Fprint(w, "data: ")
		_, _ = w.Write(part1)
		_, _ = fmt.Fprint(w, "\n")
		_, _ = fmt.Fprint(w, "data: ")
		_, _ = w.Write(part2)
		_, _ = fmt.Fprint(w, "\n\n")
	}))
	defer server.Close()

	provider, _ := NewLmStudioProvider(WithLmStudioBaseURL(server.URL))
	req := &ChatRequest{Model: "m"}
	chunks, errs := provider.ChatStream(context.Background(), req)
	for range chunks {
	}
	err := <-errs
	var typed *LLMError
	if !errors.As(err, &typed) || typed.Kind != ErrorKindStream {
		t.Fatalf("expected stream error, got %T: %v", err, err)
	}
	if !errors.Is(err, httputil.ErrStreamRecordTooLarge) {
		t.Fatalf("expected bounded stream sentinel in chain, got %v", err)
	}
}
