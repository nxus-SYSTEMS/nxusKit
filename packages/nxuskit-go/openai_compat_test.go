package nxuskit

import "testing"

func TestConvertToOpenAIResponseFormat(t *testing.T) {
	t.Run("basic text type", func(t *testing.T) {
		rf := &ResponseFormat{Type: "text"}
		result := convertToOpenAIResponseFormat(rf)
		if result.Type != "text" {
			t.Errorf("expected type 'text', got %q", result.Type)
		}
		if result.JSONSchema != nil {
			t.Error("expected JSONSchema to be nil for text type")
		}
	})

	t.Run("json_object type", func(t *testing.T) {
		rf := &ResponseFormat{Type: "json_object"}
		result := convertToOpenAIResponseFormat(rf)
		if result.Type != "json_object" {
			t.Errorf("expected type 'json_object', got %q", result.Type)
		}
	})

	t.Run("json_schema type with schema", func(t *testing.T) {
		schema := map[string]interface{}{
			"type": "object",
			"properties": map[string]interface{}{
				"name": map[string]interface{}{"type": "string"},
			},
		}
		rf := &ResponseFormat{
			Type: "json_schema",
			JSONSchema: &JSONSchema{
				Name:        "person",
				Description: "A person object",
				Schema:      schema,
				Strict:      true,
			},
		}
		result := convertToOpenAIResponseFormat(rf)
		if result.Type != "json_schema" {
			t.Errorf("expected type 'json_schema', got %q", result.Type)
		}
		if result.JSONSchema == nil {
			t.Fatal("expected JSONSchema to be set")
		}
		if result.JSONSchema.Name != "person" {
			t.Errorf("expected name 'person', got %q", result.JSONSchema.Name)
		}
		if result.JSONSchema.Description != "A person object" {
			t.Errorf("expected description 'A person object', got %q", result.JSONSchema.Description)
		}
		if !result.JSONSchema.Strict {
			t.Error("expected Strict to be true")
		}
	})
}

func TestConvertToOpenAITools(t *testing.T) {
	t.Run("empty tools", func(t *testing.T) {
		result := convertToOpenAITools([]Tool{})
		if len(result) != 0 {
			t.Errorf("expected empty result, got %d items", len(result))
		}
	})

	t.Run("single tool", func(t *testing.T) {
		tools := []Tool{
			{
				Type: "function",
				Function: ToolFunction{
					Name:        "get_weather",
					Description: "Get the weather for a location",
					Parameters: map[string]interface{}{
						"type": "object",
						"properties": map[string]interface{}{
							"location": map[string]interface{}{"type": "string"},
						},
					},
				},
			},
		}
		result := convertToOpenAITools(tools)
		if len(result) != 1 {
			t.Fatalf("expected 1 tool, got %d", len(result))
		}
		if result[0].Type != "function" {
			t.Errorf("expected type 'function', got %q", result[0].Type)
		}
		if result[0].Function.Name != "get_weather" {
			t.Errorf("expected name 'get_weather', got %q", result[0].Function.Name)
		}
	})

	t.Run("multiple tools", func(t *testing.T) {
		tools := []Tool{
			{
				Type:     "function",
				Function: ToolFunction{Name: "tool1", Description: "First tool"},
			},
			{
				Type:     "function",
				Function: ToolFunction{Name: "tool2", Description: "Second tool"},
			},
		}
		result := convertToOpenAITools(tools)
		if len(result) != 2 {
			t.Errorf("expected 2 tools, got %d", len(result))
		}
	})
}

func TestConvertToOpenAIToolChoice(t *testing.T) {
	t.Run("auto", func(t *testing.T) {
		tc := &ToolChoice{Type: "auto"}
		result := convertToOpenAIToolChoice(tc)
		if result != "auto" {
			t.Errorf("expected 'auto', got %v", result)
		}
	})

	t.Run("none", func(t *testing.T) {
		tc := &ToolChoice{Type: "none"}
		result := convertToOpenAIToolChoice(tc)
		if result != "none" {
			t.Errorf("expected 'none', got %v", result)
		}
	})

	t.Run("required", func(t *testing.T) {
		tc := &ToolChoice{Type: "required"}
		result := convertToOpenAIToolChoice(tc)
		if result != "required" {
			t.Errorf("expected 'required', got %v", result)
		}
	})

	t.Run("function with name", func(t *testing.T) {
		tc := &ToolChoice{
			Type:     "function",
			Function: &ToolChoiceFunction{Name: "my_function"},
		}
		result := convertToOpenAIToolChoice(tc)
		oaitc, ok := result.(openaiToolChoice)
		if !ok {
			t.Fatalf("expected openaiToolChoice, got %T", result)
		}
		if oaitc.Type != "function" {
			t.Errorf("expected type 'function', got %q", oaitc.Type)
		}
		if oaitc.Function == nil {
			t.Fatal("expected Function to be set")
		}
		if oaitc.Function.Name != "my_function" {
			t.Errorf("expected function name 'my_function', got %q", oaitc.Function.Name)
		}
	})

	t.Run("function without Function field", func(t *testing.T) {
		tc := &ToolChoice{Type: "function"}
		result := convertToOpenAIToolChoice(tc)
		if result != "auto" {
			t.Errorf("expected 'auto' fallback, got %v", result)
		}
	})

	t.Run("unknown type defaults to auto", func(t *testing.T) {
		tc := &ToolChoice{Type: "unknown"}
		result := convertToOpenAIToolChoice(tc)
		if result != "auto" {
			t.Errorf("expected 'auto', got %v", result)
		}
	})
}
