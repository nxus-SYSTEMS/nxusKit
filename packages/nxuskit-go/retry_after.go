package nxuskit

import (
	"net/http"
	"strconv"
	"time"
)

// ParseRetryAfter parses the Retry-After header value.
//
// The Retry-After header can be in two formats per RFC 7231:
//   - Integer seconds (e.g., "120")
//   - HTTP-date (e.g., "Wed, 29 Jan 2026 12:00:00 GMT")
//
// Returns the duration to wait, or nil if the header is empty or invalid.
func ParseRetryAfter(header string) *time.Duration {
	if header == "" {
		return nil
	}

	// Try parsing as integer seconds first (most common)
	if seconds, err := strconv.Atoi(header); err == nil {
		d := time.Duration(seconds) * time.Second
		return &d
	}

	// Try parsing as HTTP-date format
	// http.ParseTime handles RFC 1123, RFC 850, and ANSI C asctime formats
	if t, err := http.ParseTime(header); err == nil {
		d := time.Until(t)
		if d < 0 {
			d = 0 // Date is in the past, retry immediately
		}
		return &d
	}

	return nil // Invalid format
}
