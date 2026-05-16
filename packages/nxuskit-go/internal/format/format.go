// Package format provides internal formatting helpers for human-readable output.
package format

import (
	"fmt"
	"strings"

	"github.com/dustin/go-humanize"
)

// Bytes formats a byte count as a human-readable string (e.g., "3.7 GB").
func Bytes(bytes int64) string {
	if bytes <= 0 {
		return ""
	}
	return humanize.Bytes(uint64(bytes))
}

// ContextWindow formats a context window size as a human-readable string.
// Examples: 128000 -> "128K", 1000000 -> "1M", 32768 -> "32.8K"
func ContextWindow(tokens int) string {
	if tokens <= 0 {
		return ""
	}

	val, prefix := humanize.ComputeSI(float64(tokens))

	// Clean up the prefix (remove leading space)
	prefix = strings.TrimSpace(prefix)

	// Format without decimal if it's a whole number
	if val == float64(int(val)) {
		return fmt.Sprintf("%d%s", int(val), prefix)
	}

	return fmt.Sprintf("%.1f%s", val, prefix)
}
