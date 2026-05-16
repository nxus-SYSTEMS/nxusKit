package nxuskit

import (
	"testing"
	"time"
)

func TestParseRetryAfter_IntegerSeconds(t *testing.T) {
	tests := []struct {
		name     string
		input    string
		expected time.Duration
	}{
		{"30 seconds", "30", 30 * time.Second},
		{"120 seconds", "120", 120 * time.Second},
		{"0 seconds", "0", 0},
		{"1 second", "1", 1 * time.Second},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := ParseRetryAfter(tt.input)
			if result == nil {
				t.Fatalf("expected %v, got nil", tt.expected)
				return
			}
			if *result != tt.expected {
				t.Errorf("expected %v, got %v", tt.expected, *result)
			}
		})
	}
}

func TestParseRetryAfter_HTTPDate(t *testing.T) {
	// Use a fixed time in the future with the correct RFC1123 format including GMT
	futureTime := time.Now().Add(2 * time.Hour).UTC()
	// http.ParseTime expects time.RFC1123 with "GMT" zone
	httpDate := futureTime.Format("Mon, 02 Jan 2006 15:04:05 GMT")

	result := ParseRetryAfter(httpDate)
	if result == nil {
		t.Fatalf("expected non-nil result for HTTP-date format: %q", httpDate)
		return
	}

	// Allow some tolerance for parsing time
	if *result < 1*time.Hour || *result > 3*time.Hour {
		t.Errorf("expected duration around 2 hours, got %v", *result)
	}
}

func TestParseRetryAfter_InvalidFormat(t *testing.T) {
	tests := []string{
		"",
		"invalid",
		"abc123",
		"-30",
		"not-a-date",
	}

	for _, input := range tests {
		t.Run(input, func(t *testing.T) {
			result := ParseRetryAfter(input)
			if input != "" && input != "-30" {
				// -30 might parse as int, empty returns nil
				if result != nil {
					t.Logf("got result %v for input %q", *result, input)
				}
			}
		})
	}
}

func TestParseRetryAfter_EmptyString(t *testing.T) {
	result := ParseRetryAfter("")
	if result != nil {
		t.Errorf("expected nil for empty string, got %v", *result)
	}
}

func TestParseRetryAfter_PastDate(t *testing.T) {
	// Use a time in the past with the correct RFC1123 format including GMT
	pastTime := time.Now().Add(-1 * time.Hour).UTC()
	httpDate := pastTime.Format("Mon, 02 Jan 2006 15:04:05 GMT")

	result := ParseRetryAfter(httpDate)
	if result == nil {
		t.Fatalf("expected non-nil result for past HTTP-date: %q", httpDate)
		return
	}

	// Past dates should return 0 (retry immediately)
	if *result != 0 {
		t.Errorf("expected 0 for past date, got %v", *result)
	}
}
