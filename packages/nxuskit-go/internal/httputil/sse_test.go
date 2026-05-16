package httputil

import (
	"io"
	"strings"
	"testing"
)

func TestSSEReader_BasicEvent(t *testing.T) {
	input := "data: hello world\n\n"
	reader := NewSSEReader(strings.NewReader(input))

	event, err := reader.Read()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if event.Data != "hello world" {
		t.Errorf("expected 'hello world', got '%s'", event.Data)
	}
}

func TestSSEReader_MultipleEvents(t *testing.T) {
	input := "data: first\n\ndata: second\n\n"
	reader := NewSSEReader(strings.NewReader(input))

	event1, err := reader.Read()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if event1.Data != "first" {
		t.Errorf("expected 'first', got '%s'", event1.Data)
	}

	event2, err := reader.Read()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if event2.Data != "second" {
		t.Errorf("expected 'second', got '%s'", event2.Data)
	}

	_, err = reader.Read()
	if err != io.EOF {
		t.Errorf("expected EOF, got %v", err)
	}
}

func TestSSEReader_MultilineData(t *testing.T) {
	input := "data: line1\ndata: line2\n\n"
	reader := NewSSEReader(strings.NewReader(input))

	event, err := reader.Read()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if event.Data != "line1\nline2" {
		t.Errorf("expected 'line1\\nline2', got '%s'", event.Data)
	}
}

func TestSSEReader_EventType(t *testing.T) {
	input := "event: message\ndata: content\n\n"
	reader := NewSSEReader(strings.NewReader(input))

	event, err := reader.Read()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if event.Event != "message" {
		t.Errorf("expected event 'message', got '%s'", event.Event)
	}
	if event.Data != "content" {
		t.Errorf("expected data 'content', got '%s'", event.Data)
	}
}

func TestSSEReader_EventID(t *testing.T) {
	input := "id: 123\ndata: content\n\n"
	reader := NewSSEReader(strings.NewReader(input))

	event, err := reader.Read()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if event.ID != "123" {
		t.Errorf("expected ID '123', got '%s'", event.ID)
	}
}

func TestSSEReader_NoSpaceAfterColon(t *testing.T) {
	input := "data:no-space\n\n"
	reader := NewSSEReader(strings.NewReader(input))

	event, err := reader.Read()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if event.Data != "no-space" {
		t.Errorf("expected 'no-space', got '%s'", event.Data)
	}
}

func TestSSEReader_JSONData(t *testing.T) {
	input := `data: {"id":"123","content":"hello"}` + "\n\n"
	reader := NewSSEReader(strings.NewReader(input))

	event, err := reader.Read()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	expected := `{"id":"123","content":"hello"}`
	if event.Data != expected {
		t.Errorf("expected '%s', got '%s'", expected, event.Data)
	}
}

func TestSSEEvent_IsDone(t *testing.T) {
	tests := []struct {
		data     string
		expected bool
	}{
		{"[DONE]", true},
		{"done", false},
		{`{"content":"hello"}`, false},
		{"", false},
	}

	for _, tt := range tests {
		event := &SSEEvent{Data: tt.data}
		if event.IsDone() != tt.expected {
			t.Errorf("IsDone() for '%s': expected %v, got %v", tt.data, tt.expected, event.IsDone())
		}
	}
}

func TestSSEReader_EmptyStream(t *testing.T) {
	reader := NewSSEReader(strings.NewReader(""))

	_, err := reader.Read()
	if err != io.EOF {
		t.Errorf("expected EOF, got %v", err)
	}
}

func TestSSEReader_OnlyEmptyLines(t *testing.T) {
	input := "\n\n\n"
	reader := NewSSEReader(strings.NewReader(input))

	_, err := reader.Read()
	if err != io.EOF {
		t.Errorf("expected EOF for empty lines only, got %v", err)
	}
}

func TestSSEReader_DoneEvent(t *testing.T) {
	input := "data: [DONE]\n\n"
	reader := NewSSEReader(strings.NewReader(input))

	event, err := reader.Read()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if !event.IsDone() {
		t.Error("expected IsDone() to return true")
	}
}
