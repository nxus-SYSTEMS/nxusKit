package nxuskit

import (
	"errors"
	"fmt"
	"strings"
	"testing"
	"time"
)

func TestErrorKindString(t *testing.T) {
	tests := []struct {
		kind     ErrorKind
		expected string
	}{
		{ErrorKindUnknown, "unknown"},
		{ErrorKindAuthentication, "authentication"},
		{ErrorKindRateLimit, "rate_limit"},
		{ErrorKindNetwork, "network"},
		{ErrorKindProvider, "provider"},
		{ErrorKindInvalidRequest, "invalid_request"},
		{ErrorKindStream, "stream"},
		{ErrorKindConfiguration, "configuration"},
		{ErrorKindImageTooLarge, "image_too_large"},
		{ErrorKindLicenseRequired, "license_required"},
		{ErrorKindNotImplemented, "not_implemented"},
		{ErrorKindLicenseExpired, "license_expired"},
		{ErrorKindEditionInsufficient, "edition_insufficient"},
		{ErrorKindFeatureUnavailable, "feature_unavailable"},
		{ErrorKind(999), "unknown"},
	}

	for _, tt := range tests {
		t.Run(tt.expected, func(t *testing.T) {
			if tt.kind.String() != tt.expected {
				t.Errorf("ErrorKind.String() = %q, want %q", tt.kind.String(), tt.expected)
			}
		})
	}
}

func TestSentinelErrors(t *testing.T) {
	// Verify sentinel errors are not nil
	sentinels := []error{
		ErrAuthentication,
		ErrRateLimit,
		ErrNetwork,
		ErrProvider,
		ErrInvalidRequest,
		ErrStream,
		ErrConfiguration,
		ErrImageTooLarge,
		ErrLicenseRequired,
		ErrNotImplemented,
		ErrLicenseExpired,
		ErrEditionInsufficient,
		ErrFeatureUnavailable,
	}

	for _, err := range sentinels {
		if err == nil {
			t.Error("Sentinel error should not be nil")
		}
	}
}

func TestLLMErrorError(t *testing.T) {
	t.Run("with provider and status", func(t *testing.T) {
		err := &LLMError{
			Kind:           ErrorKindProvider,
			Message:        "internal error",
			HTTPStatusCode: 500,
			Provider:       "openai",
		}
		expected := "provider error [openai]: internal error (HTTP 500)"
		if err.Error() != expected {
			t.Errorf("Error() = %q, want %q", err.Error(), expected)
		}
	})

	t.Run("with provider only", func(t *testing.T) {
		err := &LLMError{
			Kind:     ErrorKindNetwork,
			Message:  "connection refused",
			Provider: "claude",
		}
		expected := "network error [claude]: connection refused"
		if err.Error() != expected {
			t.Errorf("Error() = %q, want %q", err.Error(), expected)
		}
	})

	t.Run("with status only", func(t *testing.T) {
		err := &LLMError{
			Kind:           ErrorKindInvalidRequest,
			Message:        "bad request",
			HTTPStatusCode: 400,
		}
		expected := "invalid_request error: bad request (HTTP 400)"
		if err.Error() != expected {
			t.Errorf("Error() = %q, want %q", err.Error(), expected)
		}
	})

	t.Run("minimal", func(t *testing.T) {
		err := &LLMError{
			Kind:    ErrorKindConfiguration,
			Message: "missing API key",
		}
		expected := "configuration error: missing API key"
		if err.Error() != expected {
			t.Errorf("Error() = %q, want %q", err.Error(), expected)
		}
	})
}

func TestLLMErrorIs(t *testing.T) {
	tests := []struct {
		name     string
		err      *LLMError
		target   error
		expected bool
	}{
		{"authentication matches", &LLMError{Kind: ErrorKindAuthentication}, ErrAuthentication, true},
		{"rate limit matches", &LLMError{Kind: ErrorKindRateLimit}, ErrRateLimit, true},
		{"network matches", &LLMError{Kind: ErrorKindNetwork}, ErrNetwork, true},
		{"provider matches", &LLMError{Kind: ErrorKindProvider}, ErrProvider, true},
		{"invalid request matches", &LLMError{Kind: ErrorKindInvalidRequest}, ErrInvalidRequest, true},
		{"stream matches", &LLMError{Kind: ErrorKindStream}, ErrStream, true},
		{"configuration matches", &LLMError{Kind: ErrorKindConfiguration}, ErrConfiguration, true},
		{"image too large matches", &LLMError{Kind: ErrorKindImageTooLarge}, ErrImageTooLarge, true},
		{"license required matches", &LLMError{Kind: ErrorKindLicenseRequired}, ErrLicenseRequired, true},
		{"not implemented matches", &LLMError{Kind: ErrorKindNotImplemented}, ErrNotImplemented, true},
		{"license expired matches", &LLMError{Kind: ErrorKindLicenseExpired}, ErrLicenseExpired, true},
		{"edition insufficient matches", &LLMError{Kind: ErrorKindEditionInsufficient}, ErrEditionInsufficient, true},
		{"feature unavailable matches", &LLMError{Kind: ErrorKindFeatureUnavailable}, ErrFeatureUnavailable, true},
		{"does not match wrong sentinel", &LLMError{Kind: ErrorKindNetwork}, ErrAuthentication, false},
		{"matches same kind LLMError", &LLMError{Kind: ErrorKindNetwork}, &LLMError{Kind: ErrorKindNetwork}, true},
		{"does not match different kind LLMError", &LLMError{Kind: ErrorKindNetwork}, &LLMError{Kind: ErrorKindProvider}, false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if errors.Is(tt.err, tt.target) != tt.expected {
				t.Errorf("errors.Is() = %v, want %v", !tt.expected, tt.expected)
			}
		})
	}
}

func TestLLMErrorUnwrap(t *testing.T) {
	underlying := errors.New("underlying error")
	err := &LLMError{
		Kind:    ErrorKindNetwork,
		Message: "connection failed",
		Err:     underlying,
	}

	if errors.Unwrap(err) != underlying {
		t.Error("Unwrap should return underlying error")
	}

	if !errors.Is(err, underlying) {
		t.Error("errors.Is should find underlying error in chain")
	}
}

func TestLLMErrorAs(t *testing.T) {
	err := &LLMError{
		Kind:           ErrorKindRateLimit,
		Message:        "too many requests",
		HTTPStatusCode: 429,
		RetryAfter:     30 * time.Second,
		Provider:       "openai",
	}

	// Wrap the error
	wrapped := fmt.Errorf("API call failed: %w", err)

	var llmErr *LLMError
	if !errors.As(wrapped, &llmErr) {
		t.Error("errors.As should find LLMError")
	}

	if llmErr.Kind != ErrorKindRateLimit {
		t.Errorf("Kind = %v, want %v", llmErr.Kind, ErrorKindRateLimit)
	}
	if llmErr.RetryAfter != 30*time.Second {
		t.Errorf("RetryAfter = %v, want 30s", llmErr.RetryAfter)
	}
	if llmErr.Provider != "openai" {
		t.Errorf("Provider = %q, want %q", llmErr.Provider, "openai")
	}
}

func TestLLMErrorIsRetryable(t *testing.T) {
	tests := []struct {
		name      string
		err       *LLMError
		retryable bool
	}{
		{"rate limit", &LLMError{Kind: ErrorKindRateLimit}, true},
		{"network", &LLMError{Kind: ErrorKindNetwork}, true},
		{"provider 500", &LLMError{Kind: ErrorKindProvider, HTTPStatusCode: 500}, true},
		{"provider 502", &LLMError{Kind: ErrorKindProvider, HTTPStatusCode: 502}, true},
		{"provider 503", &LLMError{Kind: ErrorKindProvider, HTTPStatusCode: 503}, true},
		{"provider 504", &LLMError{Kind: ErrorKindProvider, HTTPStatusCode: 504}, true},
		{"provider 400", &LLMError{Kind: ErrorKindProvider, HTTPStatusCode: 400}, false},
		{"provider 401", &LLMError{Kind: ErrorKindProvider, HTTPStatusCode: 401}, false},
		{"authentication", &LLMError{Kind: ErrorKindAuthentication}, false},
		{"invalid request", &LLMError{Kind: ErrorKindInvalidRequest}, false},
		{"configuration", &LLMError{Kind: ErrorKindConfiguration}, false},
		{"license required", &LLMError{Kind: ErrorKindLicenseRequired}, false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if tt.err.IsRetryable() != tt.retryable {
				t.Errorf("IsRetryable() = %v, want %v", !tt.retryable, tt.retryable)
			}
		})
	}
}

func TestErrorConstructors(t *testing.T) {
	t.Run("NewAuthenticationError", func(t *testing.T) {
		err := NewAuthenticationError("openai", "invalid API key", nil)
		if err.Kind != ErrorKindAuthentication {
			t.Errorf("Kind = %v, want %v", err.Kind, ErrorKindAuthentication)
		}
		if err.HTTPStatusCode != 401 {
			t.Errorf("HTTPStatusCode = %d, want 401", err.HTTPStatusCode)
		}
		if err.Provider != "openai" {
			t.Errorf("Provider = %q, want %q", err.Provider, "openai")
		}
		if !errors.Is(err, ErrAuthentication) {
			t.Error("Should match ErrAuthentication sentinel")
		}
	})

	t.Run("NewRateLimitError", func(t *testing.T) {
		err := NewRateLimitError("claude", 60*time.Second, nil)
		if err.Kind != ErrorKindRateLimit {
			t.Errorf("Kind = %v, want %v", err.Kind, ErrorKindRateLimit)
		}
		if err.HTTPStatusCode != 429 {
			t.Errorf("HTTPStatusCode = %d, want 429", err.HTTPStatusCode)
		}
		if err.RetryAfter != 60*time.Second {
			t.Errorf("RetryAfter = %v, want 60s", err.RetryAfter)
		}
		if !err.IsRetryable() {
			t.Error("Should be retryable")
		}
	})

	t.Run("NewNetworkError", func(t *testing.T) {
		underlying := errors.New("connection refused")
		err := NewNetworkError("ollama", "failed to connect", underlying)
		if err.Kind != ErrorKindNetwork {
			t.Errorf("Kind = %v, want %v", err.Kind, ErrorKindNetwork)
		}
		if !errors.Is(err, underlying) {
			t.Error("Should wrap underlying error")
		}
		if !err.IsRetryable() {
			t.Error("Should be retryable")
		}
	})

	t.Run("NewProviderError", func(t *testing.T) {
		err := NewProviderError("groq", "model not found", 404, nil)
		if err.Kind != ErrorKindProvider {
			t.Errorf("Kind = %v, want %v", err.Kind, ErrorKindProvider)
		}
		if err.HTTPStatusCode != 404 {
			t.Errorf("HTTPStatusCode = %d, want 404", err.HTTPStatusCode)
		}
		if err.IsRetryable() {
			t.Error("404 should not be retryable")
		}
	})

	t.Run("NewInvalidRequestError", func(t *testing.T) {
		err := NewInvalidRequestError("openai", "messages cannot be empty", nil)
		if err.Kind != ErrorKindInvalidRequest {
			t.Errorf("Kind = %v, want %v", err.Kind, ErrorKindInvalidRequest)
		}
		if err.HTTPStatusCode != 400 {
			t.Errorf("HTTPStatusCode = %d, want 400", err.HTTPStatusCode)
		}
	})

	t.Run("NewStreamError", func(t *testing.T) {
		err := NewStreamError("claude", "stream interrupted", nil)
		if err.Kind != ErrorKindStream {
			t.Errorf("Kind = %v, want %v", err.Kind, ErrorKindStream)
		}
	})

	t.Run("NewConfigurationError", func(t *testing.T) {
		err := NewConfigurationError("API key not set", nil)
		if err.Kind != ErrorKindConfiguration {
			t.Errorf("Kind = %v, want %v", err.Kind, ErrorKindConfiguration)
		}
		if err.Provider != "" {
			t.Errorf("Provider = %q, want empty", err.Provider)
		}
	})

	t.Run("NewImageTooLargeError", func(t *testing.T) {
		err := NewImageTooLargeError("openai", 25*1024*1024, 20*1024*1024, nil)
		if err.Kind != ErrorKindImageTooLarge {
			t.Errorf("Kind = %v, want %v", err.Kind, ErrorKindImageTooLarge)
		}
		if err.Message == "" {
			t.Error("Message should describe size issue")
		}
	})

	t.Run("NewLicenseRequiredError", func(t *testing.T) {
		err := NewLicenseRequiredError("MCP provider")
		if err.Kind != ErrorKindLicenseRequired {
			t.Errorf("Kind = %v, want %v", err.Kind, ErrorKindLicenseRequired)
		}
		if err.Message == "" {
			t.Error("Message should mention the feature")
		}
	})
}

func TestClipsError(t *testing.T) {
	t.Run("Error without underlying error", func(t *testing.T) {
		err := NewClipsError("TEMPLATE_NOT_FOUND", "template 'person' not found", nil)
		expected := "CLIPS error [TEMPLATE_NOT_FOUND]: template 'person' not found"
		if err.Error() != expected {
			t.Errorf("Error() = %q, want %q", err.Error(), expected)
		}
	})

	t.Run("Error with underlying error", func(t *testing.T) {
		underlying := errors.New("parse failed")
		err := NewClipsError("PARSE_ERROR", "failed to parse rules", underlying)
		if !errors.Is(err, underlying) {
			t.Error("Should wrap underlying error")
		}
		if err.Unwrap() != underlying {
			t.Error("Unwrap should return underlying error")
		}
	})

	t.Run("Error with metadata", func(t *testing.T) {
		metadata := &ClipsErrorMetadata{
			AvailableTemplates: []string{"user", "order", "product"},
			Suggestions:        []string{"person"},
			Hint:               "Did you mean 'user'?",
		}
		err := NewClipsErrorWithMetadata("TEMPLATE_NOT_FOUND", "template 'person' not found", metadata, nil)
		if err.Metadata == nil {
			t.Fatal("Metadata should not be nil")
		}
		if len(err.Metadata.AvailableTemplates) != 3 {
			t.Errorf("AvailableTemplates count = %d, want 3", len(err.Metadata.AvailableTemplates))
		}
		if err.Metadata.Hint != "Did you mean 'user'?" {
			t.Errorf("Hint = %q, want %q", err.Metadata.Hint, "Did you mean 'user'?")
		}
	})

	t.Run("ClipsTemplateSchemaInfo", func(t *testing.T) {
		schema := ClipsTemplateSchemaInfo{
			Name: "order",
			Slots: []ClipsSlotInfo{
				{Name: "id", Type: "INTEGER"},
				{Name: "items", Type: "STRING", Multislot: true},
				{Name: "status", Type: "SYMBOL", AllowedValues: []string{"pending", "shipped", "delivered"}},
			},
			Documentation: "An order record",
		}
		if schema.Name != "order" {
			t.Errorf("Name = %q, want %q", schema.Name, "order")
		}
		if len(schema.Slots) != 3 {
			t.Errorf("Slots count = %d, want 3", len(schema.Slots))
		}
		if !schema.Slots[1].Multislot {
			t.Error("items slot should be multislot")
		}
		if len(schema.Slots[2].AllowedValues) != 3 {
			t.Errorf("AllowedValues count = %d, want 3", len(schema.Slots[2].AllowedValues))
		}
	})
}

func TestNewLicenseExpiredError(t *testing.T) {
	err := NewLicenseExpiredError("solver")
	if err.Kind != ErrorKindLicenseExpired {
		t.Errorf("Kind = %v, want %v", err.Kind, ErrorKindLicenseExpired)
	}
	if !strings.Contains(err.Message, "solver") {
		t.Errorf("Message should contain feature name, got %q", err.Message)
	}
	if !strings.Contains(err.Message, "license has expired") {
		t.Errorf("Message should mention expiration, got %q", err.Message)
	}
	if !errors.Is(err, ErrLicenseExpired) {
		t.Error("Should match ErrLicenseExpired sentinel")
	}
}

func TestNewEditionInsufficientError(t *testing.T) {
	err := NewEditionInsufficientError("bayesian-network", "pro")
	if err.Kind != ErrorKindEditionInsufficient {
		t.Errorf("Kind = %v, want %v", err.Kind, ErrorKindEditionInsufficient)
	}
	if !strings.Contains(err.Message, "bayesian-network") {
		t.Errorf("Message should contain feature name, got %q", err.Message)
	}
	if !strings.Contains(err.Message, "pro") {
		t.Errorf("Message should contain required edition, got %q", err.Message)
	}
	if !errors.Is(err, ErrEditionInsufficient) {
		t.Error("Should match ErrEditionInsufficient sentinel")
	}
}

func TestNewFeatureUnavailableError(t *testing.T) {
	err := NewFeatureUnavailableError("z3-solver")
	if err.Kind != ErrorKindFeatureUnavailable {
		t.Errorf("Kind = %v, want %v", err.Kind, ErrorKindFeatureUnavailable)
	}
	if !strings.Contains(err.Message, "z3-solver") {
		t.Errorf("Message should contain feature name, got %q", err.Message)
	}
	if !strings.Contains(err.Message, "feature unavailable") {
		t.Errorf("Message should mention unavailability, got %q", err.Message)
	}
	if !errors.Is(err, ErrFeatureUnavailable) {
		t.Error("Should match ErrFeatureUnavailable sentinel")
	}
}

func TestErrorKindLicenseExpiredString(t *testing.T) {
	if ErrorKindLicenseExpired.String() != "license_expired" {
		t.Errorf("expected 'license_expired', got %q", ErrorKindLicenseExpired.String())
	}
}

func TestErrorKindEditionInsufficientString(t *testing.T) {
	if ErrorKindEditionInsufficient.String() != "edition_insufficient" {
		t.Errorf("expected 'edition_insufficient', got %q", ErrorKindEditionInsufficient.String())
	}
}

func TestErrorKindFeatureUnavailableString(t *testing.T) {
	if ErrorKindFeatureUnavailable.String() != "feature_unavailable" {
		t.Errorf("expected 'feature_unavailable', got %q", ErrorKindFeatureUnavailable.String())
	}
}

func TestLLMErrorUnknownKind(t *testing.T) {
	err := &LLMError{
		Kind:    ErrorKindUnknown,
		Message: "something went wrong",
	}

	// ErrorKindUnknown should not match any sentinel
	if errors.Is(err, ErrAuthentication) {
		t.Error("Unknown kind should not match ErrAuthentication")
	}
	if errors.Is(err, ErrRateLimit) {
		t.Error("Unknown kind should not match ErrRateLimit")
	}
	if errors.Is(err, ErrNetwork) {
		t.Error("Unknown kind should not match ErrNetwork")
	}

	// Should not be retryable
	if err.IsRetryable() {
		t.Error("Unknown kind should not be retryable")
	}
}
