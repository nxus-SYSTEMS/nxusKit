package nxuskit

import (
	"encoding/json"
	"testing"
)

func TestModelInfoFormattedSize(t *testing.T) {
	tests := []struct {
		name      string
		sizeBytes *int64
		expected  string
	}{
		{"nil", nil, ""},
		{"zero", ptr(int64(0)), ""},
		{"small", ptr(int64(100)), "100 B"},
		{"1KB", ptr(int64(1024)), "1.0 kB"},
		{"1MB", ptr(int64(1048576)), "1.0 MB"},
		{"1GB", ptr(int64(1073741824)), "1.1 GB"},
		{"3.7GB", ptr(int64(3700000000)), "3.7 GB"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			m := ModelInfo{Name: "test", SizeBytes: tt.sizeBytes}
			result := m.FormattedSize()
			if result != tt.expected {
				t.Errorf("FormattedSize() = %q, want %q", result, tt.expected)
			}
		})
	}
}

func TestModelInfoFormattedContextWindow(t *testing.T) {
	tests := []struct {
		name          string
		contextWindow *int
		expected      string
	}{
		{"nil", nil, ""},
		{"zero", intPtr(0), ""},
		{"small", intPtr(100), "100"},
		{"4K", intPtr(4000), "4k"},
		{"4096", intPtr(4096), "4.1k"},
		{"8K", intPtr(8000), "8k"},
		{"32K", intPtr(32000), "32k"},
		{"128K", intPtr(128000), "128k"},
		{"200K", intPtr(200000), "200k"},
		{"1M", intPtr(1000000), "1M"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			m := ModelInfo{Name: "test", ContextWindow: tt.contextWindow}
			result := m.FormattedContextWindow()
			if result != tt.expected {
				t.Errorf("FormattedContextWindow() = %q, want %q", result, tt.expected)
			}
		})
	}
}

func TestModelInfoSupportsVision(t *testing.T) {
	tests := []struct {
		name     string
		metadata map[string]any
		expected bool
	}{
		{"nil metadata", nil, false},
		{"empty metadata", map[string]any{}, false},
		{"vision true", map[string]any{"vision": true}, true},
		{"vision false", map[string]any{"vision": false}, false},
		{"supports_vision true", map[string]any{"supports_vision": true}, true},
		{"vision in modalities", map[string]any{"modalities": []string{"text", "vision"}}, true},
		{"image in modalities", map[string]any{"modalities": []string{"text", "image"}}, true},
		{"text only modalities", map[string]any{"modalities": []string{"text"}}, false},
		{"unrelated metadata", map[string]any{"foo": "bar"}, false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			m := ModelInfo{Name: "test", Metadata: tt.metadata}
			result := m.SupportsVision()
			if result != tt.expected {
				t.Errorf("SupportsVision() = %v, want %v", result, tt.expected)
			}
		})
	}
}

func TestModelInfoModalities(t *testing.T) {
	tests := []struct {
		name     string
		metadata map[string]any
		expected []string
	}{
		{"nil metadata", nil, nil},
		{"empty metadata", map[string]any{}, nil},
		{"no modalities", map[string]any{"foo": "bar"}, nil},
		{"string slice", map[string]any{"modalities": []string{"text", "vision"}}, []string{"text", "vision"}},
		{"any slice", map[string]any{"modalities": []any{"text", "vision", "audio"}}, []string{"text", "vision", "audio"}},
		{"mixed any slice", map[string]any{"modalities": []any{"text", 123}}, []string{"text"}},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			m := ModelInfo{Name: "test", Metadata: tt.metadata}
			result := m.Modalities()

			if tt.expected == nil {
				if result != nil {
					t.Errorf("Modalities() = %v, want nil", result)
				}
				return
			}

			if len(result) != len(tt.expected) {
				t.Errorf("Modalities() len = %d, want %d", len(result), len(tt.expected))
				return
			}

			for i, v := range result {
				if v != tt.expected[i] {
					t.Errorf("Modalities()[%d] = %q, want %q", i, v, tt.expected[i])
				}
			}
		})
	}
}

func TestModelInfoJSON(t *testing.T) {
	t.Run("full model info", func(t *testing.T) {
		size := int64(3700000000)
		ctx := 128000
		desc := "A powerful model"
		m := ModelInfo{
			Name:          "gpt-4o",
			SizeBytes:     &size,
			ContextWindow: &ctx,
			Description:   &desc,
			Metadata:      map[string]any{"vision": true},
		}

		data, err := json.Marshal(m)
		if err != nil {
			t.Fatalf("Marshal error: %v", err)
		}

		var decoded ModelInfo
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("Unmarshal error: %v", err)
		}

		if decoded.Name != "gpt-4o" {
			t.Errorf("Name = %q, want %q", decoded.Name, "gpt-4o")
		}
		if decoded.SizeBytes == nil || *decoded.SizeBytes != size {
			t.Errorf("SizeBytes = %v, want %d", decoded.SizeBytes, size)
		}
		if decoded.ContextWindow == nil || *decoded.ContextWindow != ctx {
			t.Errorf("ContextWindow = %v, want %d", decoded.ContextWindow, ctx)
		}
	})

	t.Run("minimal model info", func(t *testing.T) {
		m := ModelInfo{Name: "test-model"}

		data, err := json.Marshal(m)
		if err != nil {
			t.Fatalf("Marshal error: %v", err)
		}

		var decoded ModelInfo
		if err := json.Unmarshal(data, &decoded); err != nil {
			t.Fatalf("Unmarshal error: %v", err)
		}

		if decoded.Name != "test-model" {
			t.Errorf("Name = %q, want %q", decoded.Name, "test-model")
		}
		if decoded.SizeBytes != nil {
			t.Error("SizeBytes should be nil")
		}
	})
}

func TestModelInfoFields(t *testing.T) {
	size := int64(1000000)
	ctx := 4096
	desc := "Test model"

	m := ModelInfo{
		Name:          "test",
		SizeBytes:     &size,
		ContextWindow: &ctx,
		Description:   &desc,
		Metadata:      map[string]any{"key": "value"},
	}

	if m.Name != "test" {
		t.Errorf("Name = %q, want %q", m.Name, "test")
	}
	if m.SizeBytes == nil || *m.SizeBytes != 1000000 {
		t.Errorf("SizeBytes = %v, want 1000000", m.SizeBytes)
	}
	if m.ContextWindow == nil || *m.ContextWindow != 4096 {
		t.Errorf("ContextWindow = %v, want 4096", m.ContextWindow)
	}
	if m.Description == nil || *m.Description != "Test model" {
		t.Errorf("Description = %v, want %q", m.Description, "Test model")
	}
	if m.Metadata["key"] != "value" {
		t.Errorf("Metadata[key] = %v, want %q", m.Metadata["key"], "value")
	}
}

// Helper functions for creating pointers
func ptr(v int64) *int64 {
	return &v
}

func intPtr(v int) *int {
	return &v
}
