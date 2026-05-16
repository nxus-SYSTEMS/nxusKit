package nxuskit

import (
	"strings"
	"testing"
)

func TestParameterAdapter_TruncatesStopSequences(t *testing.T) {
	maxStop := 4
	caps := ProviderCapabilities{
		MaxStopSequences: &maxStop,
	}

	req := &ChatRequest{
		Model: "test",
		Stop:  []string{"a", "b", "c", "d", "e", "f"},
	}

	result := ParameterAdapter{}.Adapt(req, caps)

	if len(result.Request.Stop) != 4 {
		t.Errorf("expected 4 stop sequences, got %d", len(result.Request.Stop))
	}

	if !result.HasWarnings() {
		t.Error("expected warnings for truncation")
	}

	// Check warning message
	found := false
	for _, w := range result.Warnings {
		if w.Parameter == "stop" && strings.Contains(w.Message, "truncated") {
			found = true
			if w.Severity != WarningSeverityWarning {
				t.Errorf("expected severity warning, got %v", w.Severity)
			}
		}
	}
	if !found {
		t.Error("expected warning about stop sequence truncation")
	}
}

func TestParameterAdapter_RemovesUnsupportedPenalties(t *testing.T) {
	caps := ProviderCapabilities{
		SupportsPresencePenalty:  false,
		SupportsFrequencyPenalty: false,
	}

	presence := 0.5
	frequency := 0.3
	req := &ChatRequest{
		Model:            "test",
		PresencePenalty:  &presence,
		FrequencyPenalty: &frequency,
	}

	result := ParameterAdapter{}.Adapt(req, caps)

	if result.Request.PresencePenalty != nil {
		t.Error("expected presence_penalty to be removed")
	}
	if result.Request.FrequencyPenalty != nil {
		t.Error("expected frequency_penalty to be removed")
	}

	if len(result.Warnings) != 2 {
		t.Errorf("expected 2 warnings, got %d", len(result.Warnings))
	}

	// Check that warnings are info severity (ignored, not modified)
	for _, w := range result.Warnings {
		if w.Severity != WarningSeverityInfo {
			t.Errorf("expected info severity for %s, got %v", w.Parameter, w.Severity)
		}
	}
}

func TestParameterAdapter_RemovesUnsupportedSeed(t *testing.T) {
	caps := ProviderCapabilities{
		SupportsSeed: false,
	}

	seed := 42
	req := &ChatRequest{
		Model: "test",
		Seed:  &seed,
	}

	result := ParameterAdapter{}.Adapt(req, caps)

	if result.Request.Seed != nil {
		t.Error("expected seed to be removed")
	}

	found := false
	for _, w := range result.Warnings {
		if w.Parameter == "seed" {
			found = true
			if w.Severity != WarningSeverityInfo {
				t.Errorf("expected info severity, got %v", w.Severity)
			}
		}
	}
	if !found {
		t.Error("expected warning about seed removal")
	}
}

func TestParameterAdapter_PenaltyRangeWarning(t *testing.T) {
	caps := ProviderCapabilities{
		SupportsPresencePenalty: true,
		PenaltyRange:            &PenaltyRange{-2.0, 2.0},
	}

	// Value outside range
	presence := 5.0
	req := &ChatRequest{
		Model:           "test",
		PresencePenalty: &presence,
	}

	result := ParameterAdapter{}.Adapt(req, caps)

	// Penalty is kept but a warning is issued
	if result.Request.PresencePenalty == nil {
		t.Error("expected presence_penalty to be kept")
	}

	found := false
	for _, w := range result.Warnings {
		if w.Parameter == "presence_penalty" && strings.Contains(w.Message, "outside provider range") {
			found = true
		}
	}
	if !found {
		t.Error("expected warning about penalty outside range")
	}
}

func TestParameterAdapter_CollectsAllWarnings(t *testing.T) {
	maxStop := 2
	caps := ProviderCapabilities{
		MaxStopSequences:         &maxStop,
		SupportsPresencePenalty:  false,
		SupportsFrequencyPenalty: false,
		SupportsSeed:             false,
	}

	presence := 0.5
	frequency := 0.3
	seed := 42
	req := &ChatRequest{
		Model:            "test",
		Stop:             []string{"a", "b", "c", "d"},
		PresencePenalty:  &presence,
		FrequencyPenalty: &frequency,
		Seed:             &seed,
	}

	result := ParameterAdapter{}.Adapt(req, caps)

	// Should have 4 warnings: stop truncation, presence removed, frequency removed, seed removed
	if len(result.Warnings) != 4 {
		t.Errorf("expected 4 warnings, got %d", len(result.Warnings))
		for _, w := range result.Warnings {
			t.Logf("  %s", w)
		}
	}
}

func TestParameterAdapter_NoChangesNoWarnings(t *testing.T) {
	caps := ProviderCapabilities{
		SupportsSystemMessages:   true,
		SupportsPresencePenalty:  true,
		SupportsFrequencyPenalty: true,
		SupportsSeed:             true,
	}

	presence := 0.5
	seed := 42
	req := &ChatRequest{
		Model:           "test",
		PresencePenalty: &presence,
		Seed:            &seed,
		Stop:            []string{"end"},
	}

	result := ParameterAdapter{}.Adapt(req, caps)

	if result.HasWarnings() {
		t.Error("expected no warnings when request fits capabilities")
	}

	if result.Request.PresencePenalty == nil {
		t.Error("expected presence_penalty to be preserved")
	}
	if result.Request.Seed == nil {
		t.Error("expected seed to be preserved")
	}
}

func TestParameterAdapter_DoesNotModifyOriginal(t *testing.T) {
	maxStop := 2
	caps := ProviderCapabilities{
		MaxStopSequences: &maxStop,
	}

	originalStop := []string{"a", "b", "c", "d"}
	req := &ChatRequest{
		Model: "test",
		Stop:  originalStop,
	}

	_ = ParameterAdapter{}.Adapt(req, caps)

	// Original should be unchanged
	if len(req.Stop) != 4 {
		t.Errorf("original request modified: expected 4 stop sequences, got %d", len(req.Stop))
	}
}

func TestWarningString(t *testing.T) {
	w := ParameterWarning{
		Parameter: "stop",
		Message:   "truncated from 6 to 4",
		Severity:  WarningSeverityWarning,
	}

	str := w.String()
	if !strings.Contains(str, "warning") {
		t.Errorf("expected 'warning' in string, got %s", str)
	}
	if !strings.Contains(str, "stop") {
		t.Errorf("expected 'stop' in string, got %s", str)
	}
}

func TestWarningSeverityString(t *testing.T) {
	if WarningSeverityInfo.String() != "info" {
		t.Errorf("expected 'info', got %s", WarningSeverityInfo.String())
	}
	if WarningSeverityWarning.String() != "warning" {
		t.Errorf("expected 'warning', got %s", WarningSeverityWarning.String())
	}
}

func TestParameterAdapter_RemovesUnsupportedLogprobs(t *testing.T) {
	caps := ProviderCapabilities{
		SupportsLogprobs: false,
	}

	logprobs := true
	topLogprobs := 5
	req := &ChatRequest{
		Model:       "test",
		Logprobs:    &logprobs,
		TopLogprobs: &topLogprobs,
	}

	result := ParameterAdapter{}.Adapt(req, caps)

	if result.Request.Logprobs != nil {
		t.Error("expected logprobs to be removed")
	}
	if result.Request.TopLogprobs != nil {
		t.Error("expected top_logprobs to be removed when logprobs removed")
	}

	found := false
	for _, w := range result.Warnings {
		if w.Parameter == "logprobs" {
			found = true
			if w.Severity != WarningSeverityInfo {
				t.Errorf("expected info severity, got %v", w.Severity)
			}
		}
	}
	if !found {
		t.Error("expected warning about logprobs removal")
	}
}

func TestParameterAdapter_AdjustsTopLogprobsToLimit(t *testing.T) {
	maxLogprobs := 10
	caps := ProviderCapabilities{
		SupportsLogprobs: true,
		MaxLogprobs:      &maxLogprobs,
	}

	logprobs := true
	topLogprobs := 20 // Exceeds limit
	req := &ChatRequest{
		Model:       "test",
		Logprobs:    &logprobs,
		TopLogprobs: &topLogprobs,
	}

	result := ParameterAdapter{}.Adapt(req, caps)

	// Logprobs should be kept
	if result.Request.Logprobs == nil || !*result.Request.Logprobs {
		t.Error("expected logprobs to be preserved")
	}

	// TopLogprobs should be reduced
	if result.Request.TopLogprobs == nil {
		t.Fatal("expected top_logprobs to be set")
	}
	if *result.Request.TopLogprobs != 10 {
		t.Errorf("expected top_logprobs to be 10, got %d", *result.Request.TopLogprobs)
	}

	found := false
	for _, w := range result.Warnings {
		if w.Parameter == "top_logprobs" && strings.Contains(w.Message, "reduced") {
			found = true
			if w.Severity != WarningSeverityWarning {
				t.Errorf("expected warning severity, got %v", w.Severity)
			}
		}
	}
	if !found {
		t.Error("expected warning about top_logprobs reduction")
	}
}

func TestParameterAdapter_JSONModeFallbackToSystemMessage(t *testing.T) {
	caps := ProviderCapabilities{
		SupportsJSONMode: false,
	}

	req := &ChatRequest{
		Model:    "test",
		JSONMode: true,
		Messages: []Message{
			{Role: RoleUser, Content: MessageContent{Text: "Give me a JSON list of colors"}},
		},
	}

	result := ParameterAdapter{}.Adapt(req, caps)

	// JSONMode should be disabled
	if result.Request.JSONMode {
		t.Error("expected json_mode to be false after adaptation")
	}

	// Should have a system message added
	if len(result.Request.Messages) != 2 {
		t.Errorf("expected 2 messages after adding system, got %d", len(result.Request.Messages))
	}

	if result.Request.Messages[0].Role != RoleSystem {
		t.Errorf("expected first message to be system, got %s", result.Request.Messages[0].Role)
	}

	if !strings.Contains(result.Request.Messages[0].Content.Text, "JSON") {
		t.Error("expected system message to mention JSON")
	}

	found := false
	for _, w := range result.Warnings {
		if w.Parameter == "json_mode" && strings.Contains(w.Message, "system message") {
			found = true
			if w.Severity != WarningSeverityWarning {
				t.Errorf("expected warning severity, got %v", w.Severity)
			}
		}
	}
	if !found {
		t.Error("expected warning about JSON mode fallback")
	}
}

func TestParameterAdapter_JSONModeAppendsToExistingSystemMessage(t *testing.T) {
	caps := ProviderCapabilities{
		SupportsJSONMode: false,
	}

	req := &ChatRequest{
		Model:    "test",
		JSONMode: true,
		Messages: []Message{
			{Role: RoleSystem, Content: MessageContent{Text: "You are a helpful assistant."}},
			{Role: RoleUser, Content: MessageContent{Text: "Give me a JSON list of colors"}},
		},
	}

	result := ParameterAdapter{}.Adapt(req, caps)

	// Should still have 2 messages (appended to existing system)
	if len(result.Request.Messages) != 2 {
		t.Errorf("expected 2 messages, got %d", len(result.Request.Messages))
	}

	// System message should be augmented
	if !strings.Contains(result.Request.Messages[0].Content.Text, "helpful assistant") {
		t.Error("expected original system content to be preserved")
	}
	if !strings.Contains(result.Request.Messages[0].Content.Text, "JSON") {
		t.Error("expected JSON instruction to be appended")
	}
}

func TestParameterAdapter_JSONModeNativeSupport(t *testing.T) {
	caps := ProviderCapabilities{
		SupportsJSONMode: true,
	}

	req := &ChatRequest{
		Model:    "test",
		JSONMode: true,
		Messages: []Message{
			{Role: RoleUser, Content: MessageContent{Text: "Give me a JSON list of colors"}},
		},
	}

	result := ParameterAdapter{}.Adapt(req, caps)

	// JSONMode should be preserved
	if !result.Request.JSONMode {
		t.Error("expected json_mode to remain true when supported")
	}

	// No system message should be added
	if len(result.Request.Messages) != 1 {
		t.Errorf("expected 1 message (unchanged), got %d", len(result.Request.Messages))
	}

	// No warnings for JSON mode
	for _, w := range result.Warnings {
		if w.Parameter == "json_mode" {
			t.Errorf("unexpected warning for json_mode: %s", w.Message)
		}
	}
}

func TestAdaptGPT54ReasoningCompat_DropsParamsAndWarns(t *testing.T) {
	temp := 0.7
	topP := 0.9
	logprobs := true
	topLP := 5
	req := &ChatRequest{
		Model:       "gpt-5.4",
		Temperature: &temp,
		TopP:        &topP,
		Logprobs:    &logprobs,
		TopLogprobs: &topLP,
		Metadata:    map[string]any{"reasoning_effort": "medium"},
		Messages:    []Message{UserMessage("hi")},
	}

	caps := ProviderCapabilities{SupportsLogprobs: true}
	result := ParameterAdapter{}.Adapt(req, caps)

	if result.Request.Temperature != nil {
		t.Error("temperature should be dropped for gpt-5.4 with reasoning effort")
	}
	if result.Request.TopP != nil {
		t.Error("top_p should be dropped for gpt-5.4 with reasoning effort")
	}
	if result.Request.Logprobs != nil {
		t.Error("logprobs should be dropped for gpt-5.4 with reasoning effort")
	}
	if result.Request.TopLogprobs != nil {
		t.Error("top_logprobs should be dropped for gpt-5.4 with reasoning effort")
	}

	warnParams := make(map[string]bool)
	for _, w := range result.Warnings {
		warnParams[w.Parameter] = true
		if w.Severity != WarningSeverityWarning {
			t.Errorf("warning %s: expected severity warning, got %v", w.Parameter, w.Severity)
		}
	}
	for _, param := range []string{"temperature", "top_p", "logprobs"} {
		if !warnParams[param] {
			t.Errorf("expected warning for %s, none found", param)
		}
	}
}

func TestAdaptGPT54ReasoningCompat_NoneEffortKeepsParams(t *testing.T) {
	temp := 0.7
	logprobs := true
	req := &ChatRequest{
		Model:       "gpt-5.4",
		Temperature: &temp,
		Logprobs:    &logprobs,
		Metadata:    map[string]any{"reasoning_effort": "none"},
		Messages:    []Message{UserMessage("hi")},
	}

	caps := ProviderCapabilities{SupportsLogprobs: true}
	result := ParameterAdapter{}.Adapt(req, caps)

	if result.Request.Temperature == nil {
		t.Error("temperature should NOT be dropped when reasoning_effort=none")
	}
	if result.Request.Logprobs == nil {
		t.Error("logprobs should NOT be dropped when reasoning_effort=none")
	}
	for _, w := range result.Warnings {
		if w.Parameter == "temperature" || w.Parameter == "logprobs" {
			t.Errorf("unexpected warning %q for reasoning_effort=none", w.Parameter)
		}
	}
}

func TestAdaptGPT54ReasoningCompat_NonGPT54Unaffected(t *testing.T) {
	temp := 0.7
	req := &ChatRequest{
		Model:       "gpt-4o",
		Temperature: &temp,
		Metadata:    map[string]any{"reasoning_effort": "high"},
		Messages:    []Message{UserMessage("hi")},
	}

	caps := ProviderCapabilities{}
	result := ParameterAdapter{}.Adapt(req, caps)

	if result.Request.Temperature == nil {
		t.Error("temperature should NOT be dropped for non-gpt-5.4 model")
	}
}
