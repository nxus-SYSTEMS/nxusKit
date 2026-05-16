package nxuskit

import (
	"sort"

	"github.com/hbollon/go-edlib"
)

// DefaultSimilarityThreshold is the minimum similarity score for suggestions (0.0-1.0).
const DefaultSimilarityThreshold = 0.7

// DefaultMaxSuggestions is the maximum number of suggestions to return.
const DefaultMaxSuggestions = 3

// FindSimilarStrings finds strings similar to the input from a list of candidates.
// Uses Jaro-Winkler similarity with configurable threshold.
// Returns up to maxSuggestions results sorted by similarity (highest first).
func FindSimilarStrings(input string, candidates []string, threshold float64, maxSuggestions int) []string {
	if len(candidates) == 0 || input == "" {
		return nil
	}

	type scored struct {
		score float64
		value string
	}

	var results []scored

	for _, candidate := range candidates {
		// JaroWinklerSimilarity returns similarity as float32 between 0 and 1
		score := float64(edlib.JaroWinklerSimilarity(input, candidate))
		if score >= threshold {
			results = append(results, scored{score: score, value: candidate})
		}
	}

	// Sort by score descending
	sort.Slice(results, func(i, j int) bool {
		return results[i].score > results[j].score
	})

	// Take top results
	limit := maxSuggestions
	if len(results) < limit {
		limit = len(results)
	}

	suggestions := make([]string, limit)
	for i := 0; i < limit; i++ {
		suggestions[i] = results[i].value
	}

	return suggestions
}

// FindSimilar finds similar strings with default threshold and max suggestions.
// Uses a threshold of 0.7 and returns up to 3 suggestions.
func FindSimilar(input string, candidates []string) []string {
	return FindSimilarStrings(input, candidates, DefaultSimilarityThreshold, DefaultMaxSuggestions)
}
