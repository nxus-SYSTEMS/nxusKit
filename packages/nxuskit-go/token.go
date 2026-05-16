package nxuskit

// TokenCount contains the count of tokens used in a request or response.
type TokenCount struct {
	// PromptTokens is the number of tokens in the input prompt.
	PromptTokens int `json:"prompt_tokens"`
	// CompletionTokens is the number of tokens in the generated completion.
	CompletionTokens int `json:"completion_tokens"`
}

// Total returns the sum of prompt and completion tokens.
func (tc TokenCount) Total() int {
	return tc.PromptTokens + tc.CompletionTokens
}

// TokenUsage contains token consumption information for a request.
type TokenUsage struct {
	// Actual contains the actual token counts from the provider.
	// May be nil if the provider didn't return usage data.
	Actual *TokenCount `json:"actual,omitempty"`
	// Estimated contains estimated token counts (always available).
	Estimated TokenCount `json:"estimated"`
	// IsComplete indicates whether the generation completed normally.
	IsComplete bool `json:"is_complete"`
}

// BestAvailable returns Actual if available, otherwise Estimated.
func (tu TokenUsage) BestAvailable() TokenCount {
	if tu.Actual != nil {
		return *tu.Actual
	}
	return tu.Estimated
}

// TotalTokens returns the total token count from the best available source.
func (tu TokenUsage) TotalTokens() int {
	return tu.BestAvailable().Total()
}
