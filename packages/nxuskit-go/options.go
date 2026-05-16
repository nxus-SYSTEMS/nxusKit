package nxuskit

import (
	"fmt"
)

// Option is a functional option for configuring a ChatRequest.
type Option func(*ChatRequest) error

// NewChatRequest creates a new ChatRequest with the given model and options.
// The default ThinkingMode is ThinkingModeAuto.
func NewChatRequest(model string, opts ...Option) (*ChatRequest, error) {
	if model == "" {
		return nil, fmt.Errorf("model cannot be empty")
	}

	req := &ChatRequest{
		Model:        model,
		ThinkingMode: ThinkingModeAuto,
	}

	for _, opt := range opts {
		if err := opt(req); err != nil {
			return nil, err
		}
	}

	return req, nil
}

// WithMessages sets the conversation messages.
func WithMessages(messages ...Message) Option {
	return func(r *ChatRequest) error {
		r.Messages = messages
		return nil
	}
}

// WithTemperature sets the sampling temperature (0.0-2.0).
func WithTemperature(t float64) Option {
	return func(r *ChatRequest) error {
		if t < 0 || t > 2 {
			return fmt.Errorf("temperature must be between 0 and 2, got %f", t)
		}
		r.Temperature = &t
		return nil
	}
}

// WithMaxTokens sets the maximum number of tokens to generate.
func WithMaxTokens(n int) Option {
	return func(r *ChatRequest) error {
		if n <= 0 {
			return fmt.Errorf("max_tokens must be positive, got %d", n)
		}
		r.MaxTokens = &n
		return nil
	}
}

// WithTopP sets the nucleus sampling threshold (0.0-1.0).
func WithTopP(p float64) Option {
	return func(r *ChatRequest) error {
		if p < 0 || p > 1 {
			return fmt.Errorf("top_p must be between 0 and 1, got %f", p)
		}
		r.TopP = &p
		return nil
	}
}

// WithStream enables streaming responses.
func WithStream(stream bool) Option {
	return func(r *ChatRequest) error {
		r.Stream = stream
		return nil
	}
}

// WithStop sets the stop sequences.
func WithStop(sequences ...string) Option {
	return func(r *ChatRequest) error {
		r.Stop = sequences
		return nil
	}
}

// WithSeed sets the random seed for deterministic generation.
func WithSeed(seed int) Option {
	return func(r *ChatRequest) error {
		r.Seed = &seed
		return nil
	}
}

// WithPresencePenalty sets the presence penalty (-2.0 to 2.0).
func WithPresencePenalty(p float64) Option {
	return func(r *ChatRequest) error {
		if p < -2 || p > 2 {
			return fmt.Errorf("presence_penalty must be between -2 and 2, got %f", p)
		}
		r.PresencePenalty = &p
		return nil
	}
}

// WithFrequencyPenalty sets the frequency penalty (-2.0 to 2.0).
func WithFrequencyPenalty(p float64) Option {
	return func(r *ChatRequest) error {
		if p < -2 || p > 2 {
			return fmt.Errorf("frequency_penalty must be between -2 and 2, got %f", p)
		}
		r.FrequencyPenalty = &p
		return nil
	}
}

// WithThinkingMode sets the thinking/reasoning mode.
func WithThinkingMode(mode ThinkingMode) Option {
	return func(r *ChatRequest) error {
		r.ThinkingMode = mode
		return nil
	}
}

// WithResponseFormat sets the desired output format.
// Use ResponseFormatText(), ResponseFormatJSON(), or ResponseFormatJSONSchema().
func WithResponseFormat(format *ResponseFormat) Option {
	return func(r *ChatRequest) error {
		r.ResponseFormat = format
		return nil
	}
}

// WithTools sets the available tools/functions the model can call.
func WithTools(tools ...Tool) Option {
	return func(r *ChatRequest) error {
		r.Tools = tools
		return nil
	}
}

// WithToolChoice sets how the model should use available tools.
// Use ToolChoiceAuto(), ToolChoiceNone(), ToolChoiceRequired(), or ToolChoiceFunc().
func WithToolChoice(choice *ToolChoice) Option {
	return func(r *ChatRequest) error {
		r.ToolChoice = choice
		return nil
	}
}

// WithTopK sets the top-k sampling parameter.
// Limits token selection to the top K most likely tokens.
func WithTopK(k int) Option {
	return func(r *ChatRequest) error {
		if k <= 0 {
			return fmt.Errorf("top_k must be positive, got %d", k)
		}
		r.TopK = &k
		return nil
	}
}

// WithMinP sets the minimum probability threshold (0.0-1.0).
// Tokens with probability below this threshold are filtered out.
func WithMinP(p float64) Option {
	return func(r *ChatRequest) error {
		if p < 0 || p > 1 {
			return fmt.Errorf("min_p must be between 0 and 1, got %f", p)
		}
		r.MinP = &p
		return nil
	}
}
