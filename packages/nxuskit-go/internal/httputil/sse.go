package httputil

import (
	"bufio"
	"io"
	"strings"
)

// SSEEvent represents a Server-Sent Event.
type SSEEvent struct {
	// Event is the event type (from "event:" field). Empty if not specified.
	Event string
	// Data is the event data (from "data:" field).
	Data string
	// ID is the event ID (from "id:" field). Empty if not specified.
	ID string
}

// SSEReader reads Server-Sent Events from an io.Reader.
type SSEReader struct {
	scanner *bufio.Scanner
}

// NewSSEReader creates a new SSE reader from the given reader.
func NewSSEReader(r io.Reader) *SSEReader {
	return &SSEReader{
		scanner: bufio.NewScanner(r),
	}
}

// Read reads the next SSE event.
// Returns io.EOF when the stream ends.
// Returns nil, nil for empty lines (event boundaries).
func (r *SSEReader) Read() (*SSEEvent, error) {
	event := &SSEEvent{}
	hasData := false

	for r.scanner.Scan() {
		line := r.scanner.Text()

		// Empty line signals end of event
		if line == "" {
			if hasData {
				return event, nil
			}
			continue
		}

		// Parse the field
		if strings.HasPrefix(line, "data:") {
			data := strings.TrimPrefix(line, "data:")
			data = strings.TrimPrefix(data, " ") // Optional space after colon
			if event.Data != "" {
				event.Data += "\n" + data
			} else {
				event.Data = data
			}
			hasData = true
		} else if strings.HasPrefix(line, "event:") {
			event.Event = strings.TrimPrefix(strings.TrimPrefix(line, "event:"), " ")
		} else if strings.HasPrefix(line, "id:") {
			event.ID = strings.TrimPrefix(strings.TrimPrefix(line, "id:"), " ")
		}
		// Ignore comments (lines starting with :) and unknown fields
	}

	if err := r.scanner.Err(); err != nil {
		return nil, err
	}

	// Return final event if we have data
	if hasData {
		return event, nil
	}

	return nil, io.EOF
}

// IsDone returns true if the event data indicates the stream is done.
// This is typically "[DONE]" for OpenAI-compatible APIs.
func (e *SSEEvent) IsDone() bool {
	return e.Data == "[DONE]"
}
