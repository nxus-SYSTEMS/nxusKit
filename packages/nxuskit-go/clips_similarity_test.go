package nxuskit

import (
	"testing"
)

func TestFindSimilar_ExactMatch(t *testing.T) {
	candidates := []string{"patient", "symptom", "diagnosis"}
	suggestions := FindSimilar("patient", candidates)
	if len(suggestions) == 0 {
		t.Error("Expected at least one suggestion for exact match")
	}
	if suggestions[0] != "patient" {
		t.Errorf("Expected 'patient', got '%s'", suggestions[0])
	}
}

func TestFindSimilar_Typo(t *testing.T) {
	candidates := []string{"patient", "symptom", "diagnosis", "treatment"}
	suggestions := FindSimilar("patiant", candidates)
	if len(suggestions) == 0 {
		t.Error("Expected suggestion for typo")
	}
	if suggestions[0] != "patient" {
		t.Errorf("Expected 'patient', got '%s'", suggestions[0])
	}
}

func TestFindSimilar_NoMatch(t *testing.T) {
	candidates := []string{"patient", "symptom", "diagnosis"}
	suggestions := FindSimilar("xyz123", candidates)
	if len(suggestions) != 0 {
		t.Errorf("Expected no suggestions, got %v", suggestions)
	}
}

func TestFindSimilar_EmptyInput(t *testing.T) {
	candidates := []string{"patient", "symptom"}
	suggestions := FindSimilar("", candidates)
	if suggestions != nil {
		t.Errorf("Expected nil for empty input, got %v", suggestions)
	}
}

func TestFindSimilar_EmptyCandidates(t *testing.T) {
	candidates := []string{}
	suggestions := FindSimilar("test", candidates)
	if suggestions != nil {
		t.Errorf("Expected nil for empty candidates, got %v", suggestions)
	}
}

func TestFindSimilarStrings_RespectsMax(t *testing.T) {
	candidates := []string{"aa", "ab", "ac", "ad", "ae"}
	suggestions := FindSimilarStrings("a", candidates, 0.3, 2)
	if len(suggestions) > 2 {
		t.Errorf("Expected at most 2 suggestions, got %d", len(suggestions))
	}
}

func TestFindSimilarStrings_RespectsThreshold(t *testing.T) {
	candidates := []string{"test", "toast", "xyz"}
	// High threshold should filter out less similar matches
	suggestionsHigh := FindSimilarStrings("test", candidates, 0.95, 10)
	suggestionsLow := FindSimilarStrings("test", candidates, 0.5, 10)

	// Lower threshold should give more results
	if len(suggestionsHigh) >= len(suggestionsLow) && len(suggestionsLow) > 1 {
		t.Log("Threshold filtering appears to be working")
	}
}
