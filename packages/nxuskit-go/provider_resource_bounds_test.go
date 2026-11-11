package nxuskit

import (
	"bytes"
	"errors"
	"io"
	"net/http"
	"testing"
	"time"

	"github.com/nxus-SYSTEMS/nxusKit/packages/nxuskit-go/internal/httputil"
)

func resourceBoundedJSONBody(total int) []byte {
	prefix := []byte(`{"value":"`)
	suffix := []byte(`"}`)
	payload := total - len(prefix) - len(suffix)
	if payload < 0 {
		panic("requested body too small")
	}
	body := make([]byte, 0, total)
	body = append(body, prefix...)
	body = append(body, bytes.Repeat([]byte{'x'}, payload)...)
	body = append(body, suffix...)
	return body
}

func TestResourceBoundedProviderResponseExactLimit(t *testing.T) {
	resp := &http.Response{
		StatusCode: http.StatusOK,
		Body: io.NopCloser(bytes.NewReader(
			resourceBoundedJSONBody(httputil.MaxUnaryResponseBodyBytes),
		)),
	}
	var decoded struct {
		Value string `json:"value"`
	}
	if err := decodeBoundedProviderResponse(resp, "test", "failed to decode response", &decoded); err != nil {
		t.Fatalf("exact-limit response failed: %v", err)
	}
	if len(decoded.Value) != httputil.MaxUnaryResponseBodyBytes-len(`{"value":""}`) {
		t.Fatalf("decoded value length = %d", len(decoded.Value))
	}
}

func TestResourceBoundedProviderResponseOneByteOver(t *testing.T) {
	resp := &http.Response{
		StatusCode: http.StatusOK,
		Body: io.NopCloser(bytes.NewReader(
			resourceBoundedJSONBody(httputil.MaxUnaryResponseBodyBytes + 1),
		)),
	}
	var decoded map[string]any
	err := decodeBoundedProviderResponse(resp, "test", "failed to decode response", &decoded)
	if err == nil {
		t.Fatal("one-byte-over response unexpectedly succeeded")
	}
	var typed *LLMError
	if !errors.As(err, &typed) || typed.Kind != ErrorKindProvider {
		t.Fatalf("expected provider error, got %T: %v", err, err)
	}
	if !errors.Is(err, httputil.ErrResponseBodyTooLarge) {
		t.Fatalf("expected bounded-body sentinel in chain, got %v", err)
	}
}

func TestResourceBoundedErrorBodyCategories(t *testing.T) {
	over := bytes.Repeat([]byte{'x'}, httputil.MaxUnaryResponseBodyBytes+1)
	newResp := func(status int) *http.Response {
		return &http.Response{
			StatusCode: status,
			Header:     make(http.Header),
			Body:       io.NopCloser(bytes.NewReader(over)),
		}
	}

	t.Run("claude 429 remains rate limit", func(t *testing.T) {
		err := (&ClaudeProvider{}).handleErrorResponse(newResp(http.StatusTooManyRequests))
		var typed *LLMError
		if !errors.As(err, &typed) || typed.Kind != ErrorKindRateLimit || typed.HTTPStatusCode != http.StatusTooManyRequests {
			t.Fatalf("unexpected error: %#v", err)
		}
		if !errors.Is(err, httputil.ErrResponseBodyTooLarge) {
			t.Fatalf("missing bounded-body sentinel: %v", err)
		}
	})

	t.Run("openai compatible 429 preserves retry-after", func(t *testing.T) {
		resp := newResp(http.StatusTooManyRequests)
		resp.Header.Set("Retry-After", "2")
		err := handleOpenAICompatErrorResponse(resp, "openai")
		var typed *LLMError
		if !errors.As(err, &typed) || typed.Kind != ErrorKindRateLimit || typed.HTTPStatusCode != http.StatusTooManyRequests {
			t.Fatalf("unexpected error: %#v", err)
		}
		if typed.RetryAfter != 2*time.Second {
			t.Fatalf("retry-after = %v", typed.RetryAfter)
		}
		if !errors.Is(err, httputil.ErrResponseBodyTooLarge) {
			t.Fatalf("missing bounded-body sentinel: %v", err)
		}
	})

	t.Run("lmstudio preserves provider status policy", func(t *testing.T) {
		err := (&LmStudioProvider{}).handleErrorResponse(newResp(http.StatusServiceUnavailable))
		var typed *LLMError
		if !errors.As(err, &typed) || typed.Kind != ErrorKindProvider || typed.HTTPStatusCode != http.StatusServiceUnavailable {
			t.Fatalf("unexpected error: %#v", err)
		}
		if !errors.Is(err, httputil.ErrResponseBodyTooLarge) {
			t.Fatalf("missing bounded-body sentinel: %v", err)
		}
	})

	t.Run("ollama preserves provider status policy", func(t *testing.T) {
		err := (&OllamaProvider{}).handleErrorResponse(newResp(http.StatusNotFound))
		var typed *LLMError
		if !errors.As(err, &typed) || typed.Kind != ErrorKindProvider || typed.HTTPStatusCode != http.StatusNotFound {
			t.Fatalf("unexpected error: %#v", err)
		}
		if !errors.Is(err, httputil.ErrResponseBodyTooLarge) {
			t.Fatalf("missing bounded-body sentinel: %v", err)
		}
	})
}
