//go:build nxuskit

package nxuskit

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"strings"
	"sync"
	"time"

	"github.com/nxus-SYSTEMS/nxusKit/packages/nxuskit-go/internal/ffi"
	"github.com/nxus-SYSTEMS/nxusKit/packages/nxuskit-go/internal/httputil"
)

// ExpectedNxuskitVersion is the nxuskit library version this wrapper
// was built against. A mismatch at init time produces a clear error.
const ExpectedNxuskitVersion = "1.0.2"

// ffiProvider implements LLMProvider by delegating to the nxuskit C ABI
// through cgo. It is the common base for all FFI-backed providers.
type ffiProvider struct {
	handle       *ffi.ProviderHandle
	providerName string
	capabilities ProviderCapabilities
	// baseURL is the HTTP root for local providers (ollama, lmstudio); used for Ping only.
	baseURL string
}

// ffiProviderConfig is the JSON configuration sent to nxuskit_create_provider.
type ffiProviderConfig struct {
	ProviderType string `json:"provider_type"`
	APIKey       string `json:"api_key,omitempty"`
	Model        string `json:"model,omitempty"`
	BaseURL      string `json:"base_url,omitempty"`
	TimeoutMs    int64  `json:"timeout_ms,omitempty"`
	LicenseKey   string `json:"license_key,omitempty"`
}

// newFFIProvider creates a new FFI-backed provider. It marshals the config
// to JSON, creates the C-side provider handle, and verifies version compat.
func newFFIProvider(
	name string,
	config ffiProviderConfig,
	caps ProviderCapabilities,
) (*ffiProvider, error) {
	if err := checkNxuskitVersion(); err != nil {
		return nil, err
	}

	configJSON, err := json.Marshal(config)
	if err != nil {
		return nil, NewConfigurationError(fmt.Sprintf("failed to marshal config: %v", err), err)
	}

	handle, err := ffi.CreateProvider(string(configJSON))
	if err != nil {
		return nil, NewConfigurationError(err.Error(), err)
	}

	return &ffiProvider{
		handle:       handle,
		providerName: name,
		capabilities: caps,
		baseURL:      strings.TrimSuffix(config.BaseURL, "/"),
	}, nil
}

// ── LLMProvider interface ────────────────────────────────────

func (p *ffiProvider) ProviderName() string {
	return p.providerName
}

func (p *ffiProvider) Chat(ctx context.Context, req *ChatRequest) (*ChatResponse, error) {
	reqJSON, err := marshalChatRequest(req)
	if err != nil {
		return nil, NewInvalidRequestError(p.providerName, err.Error(), err)
	}

	// TODO: respect ctx cancellation via a goroutine + select pattern
	// For now, the blocking FFI call ignores context.
	_ = ctx

	result, err := p.handle.Chat(string(reqJSON))
	if err != nil {
		if mapped := mapNxuskitError(err); mapped != nil {
			return nil, mapped
		}
		return nil, NewProviderError(p.providerName, err.Error(), 0, err)
	}

	return unmarshalChatResponse(result)
}

func (p *ffiProvider) ChatStream(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan error) {
	chunks := make(chan StreamChunk)
	errs := make(chan error, 1)

	reqJSON, err := marshalChatRequest(req)
	if err != nil {
		close(chunks)
		errs <- NewInvalidRequestError(p.providerName, err.Error(), err)
		close(errs)
		return chunks, errs
	}

	session, err := p.handle.ChatStream(string(reqJSON))
	if err != nil {
		close(chunks)
		errs <- NewStreamError(p.providerName, err.Error(), err)
		close(errs)
		return chunks, errs
	}

	go func() {
		defer close(chunks)
		defer close(errs)
		defer session.Close()

		for {
			select {
			case <-ctx.Done():
				session.Cancel()
				errs <- ctx.Err()
				return

			case chunk, ok := <-session.Chunks:
				if !ok {
					// Chunks channel closed — read final result
					done := <-session.Done
					if done.Error != nil {
						errs <- NewStreamError(
							p.providerName,
							fmt.Sprintf("%s: %s", done.Error.ErrorType, done.Error.Message),
							nil,
						)
					}
					return
				}
				chunks <- StreamChunk{
					Delta: chunk.Content,
				}

			case done := <-session.Done:
				// Done received before chunks exhausted
				if done.Error != nil {
					errs <- NewStreamError(
						p.providerName,
						fmt.Sprintf("%s: %s", done.Error.ErrorType, done.Error.Message),
						nil,
					)
				}
				return
			}
		}
	}()

	return chunks, errs
}

func (p *ffiProvider) ListModels(ctx context.Context) ([]ModelInfo, error) {
	_ = ctx

	rawModels, err := p.handle.ListModels()
	if err != nil {
		return nil, NewProviderError(p.providerName, err.Error(), 0, err)
	}

	models := make([]ModelInfo, 0, len(rawModels))
	for _, m := range rawModels {
		info := ModelInfo{}
		// Prefer "id" as the model identifier; fall back to "name".
		if id, ok := m["id"].(string); ok {
			info.Name = id
		} else if name, ok := m["name"].(string); ok {
			info.Name = name
		}

		// Enrich with static capabilities (vision, context window, etc.)
		// The FFI layer returns bare model names; the static table has
		// the capability metadata that callers like SupportsVision() need.
		if caps := GetStaticCapabilities(p.providerName, info.Name); caps != nil {
			if info.Metadata == nil {
				info.Metadata = make(map[string]any)
			}
			info.Metadata["supports_vision"] = caps.SupportsVision
			info.Metadata["max_images"] = caps.MaxImages
			info.Metadata["supports_streaming"] = caps.SupportsStreaming
			info.Metadata["supports_json"] = caps.SupportsJSON
			if caps.MaxContextWindow > 0 {
				cw := int(caps.MaxContextWindow)
				info.ContextWindow = &cw
			}
		}

		models = append(models, info)
	}
	return models, nil
}

func (p *ffiProvider) Ping(ctx context.Context) error {
	// Local servers: use the same HTTP probes as the native Go providers.
	// A generic FFI chat "ping" uses model name "ping", which Ollama rejects (404).
	switch p.providerName {
	case "ollama":
		return p.pingLocalHTTP(ctx, "/", ollamaProviderName)
	case "lmstudio":
		return p.pingLocalHTTP(ctx, "/models", lmstudioProviderName)
	}

	req := &ChatRequest{
		Model:    "ping",
		Messages: []Message{UserMessage("ping")},
	}
	if req.MaxTokens == nil {
		one := 1
		req.MaxTokens = &one
	}
	_, err := p.Chat(ctx, req)
	if err != nil {
		return err
	}
	return nil
}

func (p *ffiProvider) pingLocalHTTP(ctx context.Context, path, name string) error {
	if p.baseURL == "" {
		return NewConfigurationError(fmt.Sprintf("%s: base URL required for ping", name), nil)
	}
	client := httputil.NewClient(p.baseURL, 15*time.Second)
	resp, err := client.Get(ctx, path)
	if err != nil {
		return NewNetworkError(name, err.Error(), err)
	}
	defer func() { _ = resp.Body.Close() }()
	if resp.StatusCode != http.StatusOK {
		return NewProviderError(name, fmt.Sprintf("ping: HTTP %d", resp.StatusCode), resp.StatusCode, nil)
	}
	return nil
}

func (p *ffiProvider) GetCapabilities() ProviderCapabilities {
	return p.capabilities
}

func (p *ffiProvider) StreamWithUsage(ctx context.Context, req *ChatRequest) (<-chan StreamChunk, <-chan TokenUsage) {
	chunks, errs := p.ChatStream(ctx, req)
	return wrapStreamWithUsage(chunks, errs)
}

// ── Version check ────────────────────────────────────────────

var versionCheckOnce sync.Once
var versionCheckErr error

func checkNxuskitVersion() error {
	versionCheckOnce.Do(func() {
		if err := ffi.Init(); err != nil {
			versionCheckErr = NewConfigurationError(
				fmt.Sprintf("nxuskit library initialization failed: %v", err),
				err,
			)
			return
		}
		actual := ffi.Version()
		if actual != ExpectedNxuskitVersion {
			versionCheckErr = NewConfigurationError(
				fmt.Sprintf(
					"nxuskit version mismatch: expected %s, got %s",
					ExpectedNxuskitVersion, actual,
				),
				nil,
			)
		}
	})
	return versionCheckErr
}

// ── JSON helpers ─────────────────────────────────────────────

func marshalChatRequest(req *ChatRequest) ([]byte, error) {
	// Build a minimal JSON request matching the nxuskit-engine ChatRequest schema.
	type jsonMessage struct {
		Role    string `json:"role"`
		Content string `json:"content"`
	}
	type jsonRequest struct {
		Model       string        `json:"model"`
		Messages    []jsonMessage `json:"messages"`
		Temperature *float64      `json:"temperature,omitempty"`
		MaxTokens   *int          `json:"max_tokens,omitempty"`
		TopP        *float64      `json:"top_p,omitempty"`
		Stream      bool          `json:"stream,omitempty"`
	}

	msgs := make([]jsonMessage, len(req.Messages))
	for i, m := range req.Messages {
		msgs[i] = jsonMessage{
			Role:    string(m.Role),
			Content: m.Content.GetText(),
		}
	}

	jr := jsonRequest{
		Model:       req.Model,
		Messages:    msgs,
		Temperature: req.Temperature,
		MaxTokens:   req.MaxTokens,
		TopP:        req.TopP,
		Stream:      req.Stream,
	}

	return json.Marshal(jr)
}

func unmarshalChatResponse(raw map[string]any) (*ChatResponse, error) {
	resp := &ChatResponse{}

	if content, ok := raw["content"].(string); ok {
		resp.Content = content
	}
	if model, ok := raw["model"].(string); ok {
		resp.Model = model
	}
	if provider, ok := raw["provider"].(string); ok {
		_ = provider // provider name is already known from the wrapper
	}

	// Parse usage if present.
	// The nxuskit core serializes usage as TokenUsage { estimated: TokenCount, actual?: TokenCount }
	if usage, ok := raw["usage"].(map[string]any); ok {
		var actual *TokenCount
		var estimated TokenCount

		if est, ok := usage["estimated"].(map[string]any); ok {
			if pt, ok := est["prompt_tokens"].(float64); ok {
				estimated.PromptTokens = int(pt)
			}
			if ct, ok := est["completion_tokens"].(float64); ok {
				estimated.CompletionTokens = int(ct)
			}
		}

		if act, ok := usage["actual"].(map[string]any); ok {
			tc := TokenCount{}
			if pt, ok := act["prompt_tokens"].(float64); ok {
				tc.PromptTokens = int(pt)
			}
			if ct, ok := act["completion_tokens"].(float64); ok {
				tc.CompletionTokens = int(ct)
			}
			actual = &tc
		}

		resp.Usage = TokenUsage{
			Actual:     actual,
			Estimated:  estimated,
			IsComplete: true,
		}
	}

	return resp, nil
}

// mapNxuskitError converts an [ffi.NxuskitError] into the corresponding
// public [LLMError]. Returns nil if the error is not a NxuskitError or
// does not map to a known entitlement error type.
func mapNxuskitError(err error) *LLMError {
	var nErr *ffi.NxuskitError
	if !errors.As(err, &nErr) {
		return nil
	}

	switch nErr.ErrorType {
	case "license_required":
		return NewLicenseRequiredError(nErr.Feature)
	case "license_expired":
		return NewLicenseExpiredError(nErr.Feature)
	case "edition_insufficient":
		return NewEditionInsufficientError(nErr.Feature, nErr.RequiredEdition)
	case "feature_unavailable":
		return NewFeatureUnavailableError(nErr.Feature)
	default:
		return nil
	}
}
