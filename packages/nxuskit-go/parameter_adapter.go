package nxuskit

import (
	"fmt"
	"strings"
)

// WarningSeverity indicates the importance of a parameter warning.
type WarningSeverity int

const (
	// WarningSeverityInfo indicates the parameter was ignored with no impact.
	// Example: "seed parameter not supported, ignored"
	WarningSeverityInfo WarningSeverity = iota

	// WarningSeverityWarning indicates the parameter was modified.
	// Example: "stop sequences truncated from 6 to 4"
	WarningSeverityWarning
)

// String returns the string representation of WarningSeverity.
func (s WarningSeverity) String() string {
	switch s {
	case WarningSeverityWarning:
		return "warning"
	default:
		return "info"
	}
}

// ParameterWarning describes an adaptation made to a request parameter.
type ParameterWarning struct {
	// Parameter is the name of the adapted parameter (e.g., "stop", "seed").
	Parameter string

	// Message describes what adaptation was made.
	Message string

	// Severity indicates the importance of this warning.
	Severity WarningSeverity
}

// String returns a human-readable representation of the warning.
func (w ParameterWarning) String() string {
	return fmt.Sprintf("[%s] %s: %s", w.Severity, w.Parameter, w.Message)
}

// AdaptedRequest contains a request adapted to provider capabilities
// along with any warnings about changes made.
type AdaptedRequest struct {
	// Request is the adapted ChatRequest (copy of original with modifications).
	Request *ChatRequest

	// Warnings contains descriptions of any adaptations made.
	Warnings []ParameterWarning
}

// HasWarnings returns true if any warnings were generated during adaptation.
func (ar AdaptedRequest) HasWarnings() bool {
	return len(ar.Warnings) > 0
}

// WarningMessages returns just the warning message strings.
func (ar AdaptedRequest) WarningMessages() []string {
	msgs := make([]string, len(ar.Warnings))
	for i, w := range ar.Warnings {
		msgs[i] = w.String()
	}
	return msgs
}

// ParameterAdapter adapts chat requests to fit provider capabilities.
//
// Use this service to gracefully handle requests that include parameters
// not supported by the target provider. Instead of failing, the adapter
// modifies the request and returns warnings about what was changed.
//
// Example:
//
//	caps := provider.GetCapabilities()
//	adapted := ParameterAdapter{}.Adapt(request, caps)
//	for _, w := range adapted.Warnings {
//	    log.Printf("Parameter adaptation: %s", w)
//	}
//	response, err := provider.Chat(ctx, adapted.Request)
type ParameterAdapter struct{}

// Adapt modifies a ChatRequest to fit the provider's capabilities.
//
// The returned AdaptedRequest contains:
//   - Request: A potentially modified copy of the original request
//   - Warnings: Descriptions of any changes made
//
// Adaptations performed:
//   - Stop sequences truncated to MaxStopSequences limit
//   - Unsupported penalty parameters removed
//   - Unsupported seed parameter removed
//   - Logprobs adjusted to MaxLogprobs limit
//   - JSON mode converted to system message prompt when not natively supported
//
// The original request is not modified.
func (ParameterAdapter) Adapt(req *ChatRequest, caps ProviderCapabilities) AdaptedRequest {
	// Create a copy of the request
	adapted := *req

	// Copy slices to avoid modifying the original
	if len(req.Messages) > 0 {
		adapted.Messages = make([]Message, len(req.Messages))
		copy(adapted.Messages, req.Messages)
	}
	if len(req.Stop) > 0 {
		adapted.Stop = make([]string, len(req.Stop))
		copy(adapted.Stop, req.Stop)
	}

	var warnings []ParameterWarning

	// 1. Stop sequences: Truncate if exceeds provider limit
	if caps.MaxStopSequences != nil && len(adapted.Stop) > *caps.MaxStopSequences {
		oldLen := len(adapted.Stop)
		adapted.Stop = adapted.Stop[:*caps.MaxStopSequences]
		warnings = append(warnings, ParameterWarning{
			Parameter: "stop",
			Message:   fmt.Sprintf("truncated from %d to %d sequences (provider limit)", oldLen, *caps.MaxStopSequences),
			Severity:  WarningSeverityWarning,
		})
	}

	// 2. Presence penalty: Remove if not supported
	if adapted.PresencePenalty != nil && !caps.SupportsPresencePenalty {
		adapted.PresencePenalty = nil
		warnings = append(warnings, ParameterWarning{
			Parameter: "presence_penalty",
			Message:   "not supported by provider, ignored",
			Severity:  WarningSeverityInfo,
		})
	}

	// 3. Frequency penalty: Remove if not supported
	if adapted.FrequencyPenalty != nil && !caps.SupportsFrequencyPenalty {
		adapted.FrequencyPenalty = nil
		warnings = append(warnings, ParameterWarning{
			Parameter: "frequency_penalty",
			Message:   "not supported by provider, ignored",
			Severity:  WarningSeverityInfo,
		})
	}

	// 4. Penalty range warnings
	if caps.PenaltyRange != nil {
		if adapted.PresencePenalty != nil {
			if *adapted.PresencePenalty < caps.PenaltyRange.Min || *adapted.PresencePenalty > caps.PenaltyRange.Max {
				warnings = append(warnings, ParameterWarning{
					Parameter: "presence_penalty",
					Message:   fmt.Sprintf("value %.2f outside provider range [%.2f, %.2f]", *adapted.PresencePenalty, caps.PenaltyRange.Min, caps.PenaltyRange.Max),
					Severity:  WarningSeverityWarning,
				})
			}
		}
		if adapted.FrequencyPenalty != nil {
			if *adapted.FrequencyPenalty < caps.PenaltyRange.Min || *adapted.FrequencyPenalty > caps.PenaltyRange.Max {
				warnings = append(warnings, ParameterWarning{
					Parameter: "frequency_penalty",
					Message:   fmt.Sprintf("value %.2f outside provider range [%.2f, %.2f]", *adapted.FrequencyPenalty, caps.PenaltyRange.Min, caps.PenaltyRange.Max),
					Severity:  WarningSeverityWarning,
				})
			}
		}
	}

	// 5. Seed: Remove if not supported
	if adapted.Seed != nil && !caps.SupportsSeed {
		adapted.Seed = nil
		warnings = append(warnings, ParameterWarning{
			Parameter: "seed",
			Message:   "not supported by provider, ignored",
			Severity:  WarningSeverityInfo,
		})
	}

	// 6. Logprobs: Remove if not supported
	if adapted.Logprobs != nil && *adapted.Logprobs && !caps.SupportsLogprobs {
		adapted.Logprobs = nil
		adapted.TopLogprobs = nil
		warnings = append(warnings, ParameterWarning{
			Parameter: "logprobs",
			Message:   "not supported by provider, ignored",
			Severity:  WarningSeverityInfo,
		})
	}

	// 7. TopLogprobs: Adjust to limit if exceeds provider maximum
	if adapted.TopLogprobs != nil && caps.MaxLogprobs != nil {
		if *adapted.TopLogprobs > *caps.MaxLogprobs {
			oldVal := *adapted.TopLogprobs
			newVal := *caps.MaxLogprobs
			adapted.TopLogprobs = &newVal
			warnings = append(warnings, ParameterWarning{
				Parameter: "top_logprobs",
				Message:   fmt.Sprintf("reduced from %d to %d (provider limit)", oldVal, newVal),
				Severity:  WarningSeverityWarning,
			})
		}
	}

	// 8. JSON mode: Add system message fallback if not natively supported
	if adapted.JSONMode && !caps.SupportsJSONMode {
		adapted.JSONMode = false // Remove the flag since provider doesn't support it
		jsonInstruction := "You must respond with valid JSON only. Do not include any text outside the JSON object."
		// Prepend a system message instructing JSON output
		jsonSystemMsg := Message{
			Role:    RoleSystem,
			Content: MessageContent{Text: jsonInstruction},
		}
		// Insert at beginning of messages, or create if empty
		if len(adapted.Messages) == 0 {
			adapted.Messages = []Message{jsonSystemMsg}
		} else {
			// Check if first message is already a system message
			if adapted.Messages[0].Role == RoleSystem {
				// Append JSON instruction to existing system message
				adapted.Messages[0].Content.Text = adapted.Messages[0].Content.Text + "\n\nIMPORTANT: " + jsonInstruction
			} else {
				// Prepend new system message
				adapted.Messages = append([]Message{jsonSystemMsg}, adapted.Messages...)
			}
		}
		warnings = append(warnings, ParameterWarning{
			Parameter: "json_mode",
			Message:   "not natively supported, added system message for JSON output",
			Severity:  WarningSeverityWarning,
		})
	}

	// GPT-5.4 reasoning-compat: warn-and-drop temperature/top_p/logprobs when
	// reasoning effort is active. Mirrors the Rust adapt_gpt54_reasoning_compat rule.
	// ChatRequest has no first-class ReasoningEffort field yet; the guard reads
	// from Metadata["reasoning_effort"] so callers can signal it without a new field.
	if effort, _ := adapted.Metadata["reasoning_effort"].(string); effort != "" {
		adaptGPT54ReasoningCompat(&adapted, effort, &warnings)
	}

	return AdaptedRequest{
		Request:  &adapted,
		Warnings: warnings,
	}
}

// adaptGPT54ReasoningCompat drops temperature, top_p, and logprobs from
// requests targeting a GPT-5.4 family model when reasoning effort is not "none".
// Mirrors the Rust adapt_gpt54_reasoning_compat function.
func adaptGPT54ReasoningCompat(req *ChatRequest, effort string, warnings *[]ParameterWarning) {
	if !isGPT54Family(req.Model) {
		return
	}
	if effort == "" || effort == "none" {
		return
	}

	if req.Temperature != nil {
		*warnings = append(*warnings, ParameterWarning{
			Parameter: "temperature",
			Message:   fmt.Sprintf("GPT-5.4 with reasoning.effort='%s' does not accept temperature; dropped", effort),
			Severity:  WarningSeverityWarning,
		})
		req.Temperature = nil
	}
	if req.TopP != nil {
		*warnings = append(*warnings, ParameterWarning{
			Parameter: "top_p",
			Message:   fmt.Sprintf("GPT-5.4 with reasoning.effort='%s' does not accept top_p; dropped", effort),
			Severity:  WarningSeverityWarning,
		})
		req.TopP = nil
	}
	if req.Logprobs != nil || req.TopLogprobs != nil {
		*warnings = append(*warnings, ParameterWarning{
			Parameter: "logprobs",
			Message:   fmt.Sprintf("GPT-5.4 with reasoning.effort='%s' does not accept logprobs; dropped", effort),
			Severity:  WarningSeverityWarning,
		})
		req.Logprobs = nil
		req.TopLogprobs = nil
	}
}

// isGPT54Family reports whether model is in the GPT-5.4 family (case-insensitive prefix match).
func isGPT54Family(model string) bool {
	lower := strings.ToLower(model)
	return strings.HasPrefix(lower, "gpt-5.4")
}
