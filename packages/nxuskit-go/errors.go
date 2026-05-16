package nxuskit

import (
	"encoding/json"
	"errors"
	"fmt"
	"time"
)

// ============================================================================
// CLIPS Error Metadata (for actionable error messages)
// ============================================================================

// ClipsErrorMetadata provides additional context for CLIPS errors,
// including available templates, did-you-mean suggestions, and resolution hints.
type ClipsErrorMetadata struct {
	// AvailableTemplates lists names of all templates in the CLIPS environment.
	AvailableTemplates []string `json:"available_templates,omitempty"`

	// TemplateSchemas contains schema details for relevant templates.
	TemplateSchemas []ClipsTemplateSchemaInfo `json:"template_schemas,omitempty"`

	// Suggestions contains did-you-mean suggestions based on string similarity.
	Suggestions []string `json:"suggestions,omitempty"`

	// Hint is a human-readable resolution hint.
	Hint string `json:"hint,omitempty"`

	// JSONSchema contains the JSON Schema for the template (when applicable).
	JSONSchema json.RawMessage `json:"json_schema,omitempty"`
}

// ClipsTemplateSchemaInfo contains brief schema info for a template.
type ClipsTemplateSchemaInfo struct {
	// Name is the template name.
	Name string `json:"name"`

	// Slots contains slot names and types.
	Slots []ClipsSlotInfo `json:"slots"`

	// Documentation is optional documentation for the template.
	Documentation string `json:"documentation,omitempty"`
}

// ClipsSlotInfo describes a template slot with its metadata.
//
// Used by both ClipsTemplateInfo (error context) and TemplateSlotInfo (FFI query).
type ClipsSlotInfo struct {
	// Name is the slot name.
	Name string `json:"name"`

	// Type is the slot type as string (e.g., "STRING", "INTEGER").
	Type string `json:"type,omitempty"`

	// Multislot indicates whether this is a multislot.
	Multislot bool `json:"multislot,omitempty"`

	// DefaultValue is the slot's default value (if defined).
	DefaultValue string `json:"default_value,omitempty"`

	// Cardinality describes the slot's cardinality constraint.
	Cardinality string `json:"cardinality,omitempty"`

	// Default is the default value if any.
	Default any `json:"default,omitempty"`

	// AllowedValues contains the allowed values constraint.
	AllowedValues []string `json:"allowed_values,omitempty"`
}

// ClipsError is an error type for CLIPS operations with optional metadata.
type ClipsError struct {
	// Code is the error code.
	Code string

	// Message is the error message.
	Message string

	// Metadata contains additional context for actionable error messages.
	Metadata *ClipsErrorMetadata

	// Err is the underlying error.
	Err error
}

// Error implements the error interface.
func (e *ClipsError) Error() string {
	if e.Err != nil {
		return fmt.Sprintf("CLIPS error [%s]: %s: %v", e.Code, e.Message, e.Err)
	}
	return fmt.Sprintf("CLIPS error [%s]: %s", e.Code, e.Message)
}

// Unwrap returns the underlying error.
func (e *ClipsError) Unwrap() error {
	return e.Err
}

// NewClipsError creates a new CLIPS error.
func NewClipsError(code, message string, err error) *ClipsError {
	return &ClipsError{
		Code:    code,
		Message: message,
		Err:     err,
	}
}

// NewClipsErrorWithMetadata creates a new CLIPS error with metadata.
func NewClipsErrorWithMetadata(code, message string, metadata *ClipsErrorMetadata, err error) *ClipsError {
	return &ClipsError{
		Code:     code,
		Message:  message,
		Metadata: metadata,
		Err:      err,
	}
}

// Sentinel errors for convenient errors.Is() checks.
var (
	// ErrAuthentication indicates an authentication failure (e.g., invalid API key).
	ErrAuthentication = errors.New("authentication failed")
	// ErrRateLimit indicates the request was rate limited.
	ErrRateLimit = errors.New("rate limit exceeded")
	// ErrNetwork indicates a network or connection error.
	ErrNetwork = errors.New("network error")
	// ErrProvider indicates a provider-specific API error.
	ErrProvider = errors.New("provider error")
	// ErrInvalidRequest indicates the request was malformed or invalid.
	ErrInvalidRequest = errors.New("invalid request")
	// ErrStream indicates an error during streaming.
	ErrStream = errors.New("stream error")
	// ErrConfiguration indicates a configuration error.
	ErrConfiguration = errors.New("configuration error")
	// ErrImageTooLarge indicates an image exceeds the provider's size limit.
	ErrImageTooLarge = errors.New("image too large")
	// ErrLicenseRequired indicates the feature requires a nxuskit Pro license.
	ErrLicenseRequired = errors.New("license required")
	// ErrNotImplemented indicates the feature is not yet implemented.
	ErrNotImplemented = errors.New("not implemented")
	// ErrLicenseExpired indicates the license has expired.
	ErrLicenseExpired = errors.New("license expired")
	// ErrEditionInsufficient indicates the current edition is not sufficient.
	ErrEditionInsufficient = errors.New("edition insufficient")
	// ErrFeatureUnavailable indicates the feature is unavailable in the current edition.
	ErrFeatureUnavailable = errors.New("feature unavailable")
)

// ErrorKind represents the category of an LLM error.
type ErrorKind int

const (
	// ErrorKindUnknown is the default/unknown error kind.
	ErrorKindUnknown ErrorKind = iota
	// ErrorKindAuthentication indicates an authentication failure.
	ErrorKindAuthentication
	// ErrorKindRateLimit indicates rate limiting.
	ErrorKindRateLimit
	// ErrorKindNetwork indicates a network error.
	ErrorKindNetwork
	// ErrorKindProvider indicates a provider-specific error.
	ErrorKindProvider
	// ErrorKindInvalidRequest indicates an invalid request.
	ErrorKindInvalidRequest
	// ErrorKindStream indicates a streaming error.
	ErrorKindStream
	// ErrorKindConfiguration indicates a configuration error.
	ErrorKindConfiguration
	// ErrorKindImageTooLarge indicates an image size error.
	ErrorKindImageTooLarge
	// ErrorKindLicenseRequired indicates a license is required.
	ErrorKindLicenseRequired
	// ErrorKindNotImplemented indicates the feature is not yet implemented.
	ErrorKindNotImplemented
	// ErrorKindLicenseExpired indicates the license has expired.
	ErrorKindLicenseExpired
	// ErrorKindEditionInsufficient indicates the current edition is not sufficient.
	ErrorKindEditionInsufficient
	// ErrorKindFeatureUnavailable indicates the feature is unavailable in the current edition.
	ErrorKindFeatureUnavailable
)

// String returns the string representation of the ErrorKind.
func (k ErrorKind) String() string {
	switch k {
	case ErrorKindAuthentication:
		return "authentication"
	case ErrorKindRateLimit:
		return "rate_limit"
	case ErrorKindNetwork:
		return "network"
	case ErrorKindProvider:
		return "provider"
	case ErrorKindInvalidRequest:
		return "invalid_request"
	case ErrorKindStream:
		return "stream"
	case ErrorKindConfiguration:
		return "configuration"
	case ErrorKindImageTooLarge:
		return "image_too_large"
	case ErrorKindLicenseRequired:
		return "license_required"
	case ErrorKindNotImplemented:
		return "not_implemented"
	case ErrorKindLicenseExpired:
		return "license_expired"
	case ErrorKindEditionInsufficient:
		return "edition_insufficient"
	case ErrorKindFeatureUnavailable:
		return "feature_unavailable"
	default:
		return "unknown"
	}
}

// LLMError is the primary error type for LLM operations.
// It supports errors.Is(), errors.As(), and error wrapping.
type LLMError struct {
	// Kind is the error category.
	Kind ErrorKind
	// Message is a human-readable error message.
	Message string
	// HTTPStatusCode is the HTTP status code (0 if not applicable).
	HTTPStatusCode int
	// RetryAfter is the suggested delay before retrying (0 if not applicable).
	RetryAfter time.Duration
	// Provider is the name of the provider that returned the error.
	Provider string
	// Err is the underlying error (for wrapping).
	Err error
}

// Error implements the error interface.
func (e *LLMError) Error() string {
	if e.HTTPStatusCode > 0 && e.Provider != "" {
		return fmt.Sprintf("%s error [%s]: %s (HTTP %d)", e.Kind, e.Provider, e.Message, e.HTTPStatusCode)
	}
	if e.Provider != "" {
		return fmt.Sprintf("%s error [%s]: %s", e.Kind, e.Provider, e.Message)
	}
	if e.HTTPStatusCode > 0 {
		return fmt.Sprintf("%s error: %s (HTTP %d)", e.Kind, e.Message, e.HTTPStatusCode)
	}
	return fmt.Sprintf("%s error: %s", e.Kind, e.Message)
}

// Unwrap returns the underlying error for error chain traversal.
func (e *LLMError) Unwrap() error {
	return e.Err
}

// Is implements custom error matching for errors.Is().
// This allows matching against sentinel errors based on error kind.
func (e *LLMError) Is(target error) bool {
	// First check if target is an LLMError and compare by kind
	if t, ok := target.(*LLMError); ok {
		return e.Kind == t.Kind
	}

	// Then check sentinel errors based on kind
	switch e.Kind {
	case ErrorKindUnknown:
		return false
	case ErrorKindAuthentication:
		return target == ErrAuthentication
	case ErrorKindRateLimit:
		return target == ErrRateLimit
	case ErrorKindNetwork:
		return target == ErrNetwork
	case ErrorKindProvider:
		return target == ErrProvider
	case ErrorKindInvalidRequest:
		return target == ErrInvalidRequest
	case ErrorKindStream:
		return target == ErrStream
	case ErrorKindConfiguration:
		return target == ErrConfiguration
	case ErrorKindImageTooLarge:
		return target == ErrImageTooLarge
	case ErrorKindLicenseRequired:
		return target == ErrLicenseRequired
	case ErrorKindNotImplemented:
		return target == ErrNotImplemented
	case ErrorKindLicenseExpired:
		return target == ErrLicenseExpired
	case ErrorKindEditionInsufficient:
		return target == ErrEditionInsufficient
	case ErrorKindFeatureUnavailable:
		return target == ErrFeatureUnavailable
	}

	return false
}

// IsRetryable returns true if the error suggests the request can be retried.
func (e *LLMError) IsRetryable() bool {
	switch e.Kind {
	case ErrorKindRateLimit, ErrorKindNetwork:
		return true
	case ErrorKindProvider:
		// 5xx errors are typically retryable
		return e.HTTPStatusCode >= 500 && e.HTTPStatusCode < 600
	default:
		return false
	}
}

// NewAuthenticationError creates an authentication failure error.
func NewAuthenticationError(provider, message string, err error) *LLMError {
	return &LLMError{
		Kind:           ErrorKindAuthentication,
		Message:        message,
		HTTPStatusCode: 401,
		Provider:       provider,
		Err:            err,
	}
}

// NewRateLimitError creates a rate limit error with optional retry-after.
func NewRateLimitError(provider string, retryAfter time.Duration, err error) *LLMError {
	return &LLMError{
		Kind:           ErrorKindRateLimit,
		Message:        "rate limit exceeded",
		HTTPStatusCode: 429,
		RetryAfter:     retryAfter,
		Provider:       provider,
		Err:            err,
	}
}

// NewNetworkError creates a network/connection error.
func NewNetworkError(provider, message string, err error) *LLMError {
	return &LLMError{
		Kind:     ErrorKindNetwork,
		Message:  message,
		Provider: provider,
		Err:      err,
	}
}

// NewProviderError creates a provider-specific API error.
func NewProviderError(provider, message string, statusCode int, err error) *LLMError {
	return &LLMError{
		Kind:           ErrorKindProvider,
		Message:        message,
		HTTPStatusCode: statusCode,
		Provider:       provider,
		Err:            err,
	}
}

// NewInvalidRequestError creates an invalid request error.
func NewInvalidRequestError(provider, message string, err error) *LLMError {
	return &LLMError{
		Kind:           ErrorKindInvalidRequest,
		Message:        message,
		HTTPStatusCode: 400,
		Provider:       provider,
		Err:            err,
	}
}

// NewStreamError creates a streaming error.
func NewStreamError(provider, message string, err error) *LLMError {
	return &LLMError{
		Kind:     ErrorKindStream,
		Message:  message,
		Provider: provider,
		Err:      err,
	}
}

// NewConfigurationError creates a configuration error.
func NewConfigurationError(message string, err error) *LLMError {
	return &LLMError{
		Kind:    ErrorKindConfiguration,
		Message: message,
		Err:     err,
	}
}

// NewImageTooLargeError creates an image size error.
func NewImageTooLargeError(provider string, size, limit int64, err error) *LLMError {
	return &LLMError{
		Kind:     ErrorKindImageTooLarge,
		Message:  fmt.Sprintf("image size %d bytes exceeds limit of %d bytes", size, limit),
		Provider: provider,
		Err:      err,
	}
}

// NewLicenseRequiredError creates a license required error.
//
// The error message includes the feature name and upgrade URL.
func NewLicenseRequiredError(feature string) *LLMError {
	return &LLMError{
		Kind:    ErrorKindLicenseRequired,
		Message: fmt.Sprintf("%s requires a nxuskit Pro license. Visit https://nxuskit.dev/pro for licensing information.", feature),
	}
}

// NewNotImplementedError creates a not implemented error.
//
// The error message includes the feature name.
func NewNotImplementedError(feature string) *LLMError {
	return &LLMError{
		Kind:    ErrorKindNotImplemented,
		Message: fmt.Sprintf("%s is not yet implemented", feature),
	}
}

// NewLicenseExpiredError creates a license expired error.
//
// The error message includes the feature name.
func NewLicenseExpiredError(feature string) *LLMError {
	return &LLMError{
		Kind:    ErrorKindLicenseExpired,
		Message: fmt.Sprintf("%s: license has expired", feature),
	}
}

// NewEditionInsufficientError creates an edition insufficient error.
//
// The error message includes the feature name and required edition.
func NewEditionInsufficientError(feature, requiredEdition string) *LLMError {
	return &LLMError{
		Kind:    ErrorKindEditionInsufficient,
		Message: fmt.Sprintf("%s: requires %s edition", feature, requiredEdition),
	}
}

// NewFeatureUnavailableError creates a feature unavailable error.
//
// The error message includes the feature name.
func NewFeatureUnavailableError(feature string) *LLMError {
	return &LLMError{
		Kind:    ErrorKindFeatureUnavailable,
		Message: fmt.Sprintf("%s: feature unavailable in current edition", feature),
	}
}
