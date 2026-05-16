package format

import (
	"testing"
)

func TestBytes(t *testing.T) {
	tests := []struct {
		name     string
		bytes    int64
		expected string
	}{
		{"zero", 0, ""},
		{"negative", -100, ""},
		{"small bytes", 100, "100 B"},
		{"kilobytes", 1024, "1.0 kB"},
		{"megabytes", 1048576, "1.0 MB"},
		{"gigabytes", 1073741824, "1.1 GB"},
		{"3.7 GB", 3700000000, "3.7 GB"},
		{"large", 1099511627776, "1.1 TB"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := Bytes(tt.bytes)
			if result != tt.expected {
				t.Errorf("Bytes(%d) = %q, want %q", tt.bytes, result, tt.expected)
			}
		})
	}
}

func TestContextWindow(t *testing.T) {
	tests := []struct {
		name     string
		tokens   int
		expected string
	}{
		{"zero", 0, ""},
		{"negative", -100, ""},
		{"small", 100, "100"},
		{"1K", 1000, "1k"},
		{"4K", 4000, "4k"},
		{"4096", 4096, "4.1k"},
		{"8K", 8000, "8k"},
		{"32K", 32000, "32k"},
		{"128K", 128000, "128k"},
		{"200K", 200000, "200k"},
		{"1M", 1000000, "1M"},
		{"2M", 2000000, "2M"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := ContextWindow(tt.tokens)
			if result != tt.expected {
				t.Errorf("ContextWindow(%d) = %q, want %q", tt.tokens, result, tt.expected)
			}
		})
	}
}
