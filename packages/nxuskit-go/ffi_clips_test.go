//go:build nxuskit

package nxuskit

import (
	"fmt"
	"testing"
)

// These tests require the nxuskit native library.
// Run with: go test -tags nxuskit -run TestClips

func TestClipsSessionCreateDestroy(t *testing.T) {
	s, err := NewClipsSession()
	if err != nil {
		t.Fatalf("NewClipsSession failed: %v", err)
	}
	defer s.Close()

	// Session should be usable
	err = s.Reset()
	if err != nil {
		t.Fatalf("Reset failed: %v", err)
	}
}

func TestClipsSessionLoadJSONAndRun(t *testing.T) {
	s, err := NewClipsSession()
	if err != nil {
		t.Fatalf("NewClipsSession failed: %v", err)
	}
	defer s.Close()

	json := `{
		"templates": [
			{
				"name": "sensor",
				"slots": [
					{"name": "name", "type": "STRING"},
					{"name": "value", "type": "INTEGER"}
				]
			},
			{
				"name": "alert",
				"slots": [
					{"name": "sensor-name", "type": "STRING"},
					{"name": "level", "type": "SYMBOL"}
				]
			}
		],
		"rules": [
			{
				"name": "check-high",
				"source": "(defrule check-high (sensor (name ?n) (value ?v&:(> ?v 100))) => (assert (alert (sensor-name ?n) (level high))))"
			}
		]
	}`

	if err := s.LoadJSON(json); err != nil {
		t.Fatalf("LoadJSON failed: %v", err)
	}
	if err := s.Reset(); err != nil {
		t.Fatalf("Reset failed: %v", err)
	}

	// Assert a fact
	idx, err := s.FactAssertString(`(sensor (name "temp-1") (value 200))`)
	if err != nil {
		t.Fatalf("FactAssertString failed: %v", err)
	}
	if idx < 0 {
		t.Fatalf("expected positive fact index, got %d", idx)
	}

	// Run inference
	fired, err := s.Run(-1)
	if err != nil {
		t.Fatalf("Run failed: %v", err)
	}
	if fired != 1 {
		t.Fatalf("expected 1 rule fired, got %d", fired)
	}

	// Query alert facts
	alerts, err := s.FactsByTemplate("alert")
	if err != nil {
		t.Fatalf("FactsByTemplate failed: %v", err)
	}
	if len(alerts) != 1 {
		t.Fatalf("expected 1 alert, got %d", len(alerts))
	}
}

func TestClipsSessionTemplateOps(t *testing.T) {
	s, err := NewClipsSession()
	if err != nil {
		t.Fatalf("NewClipsSession failed: %v", err)
	}
	defer s.Close()

	json := `{
		"templates": [
			{
				"name": "item",
				"slots": [
					{"name": "id", "type": "INTEGER"},
					{"name": "name", "type": "STRING"}
				]
			}
		]
	}`
	if err := s.LoadJSON(json); err != nil {
		t.Fatalf("LoadJSON failed: %v", err)
	}

	if !s.TemplateExists("item") {
		t.Fatal("expected item template to exist")
	}

	names, err := s.TemplateSlotNames("item")
	if err != nil {
		t.Fatalf("TemplateSlotNames failed: %v", err)
	}
	if len(names) != 2 {
		t.Fatalf("expected 2 slot names, got %d", len(names))
	}
}

func TestClipsSessionCacheWorkflow(t *testing.T) {
	rulesJSON := `{
		"templates": [
			{
				"name": "cached_item",
				"slots": [{"name": "x", "type": "INTEGER"}]
			}
		]
	}`

	// Preload
	if err := ClipsSessionPreload("go-cache-test", rulesJSON); err != nil {
		t.Fatalf("Preload failed: %v", err)
	}

	// Get cached clone
	s1, err := ClipsSessionGetCached("go-cache-test")
	if err != nil {
		t.Fatalf("GetCached failed: %v", err)
	}
	defer s1.Close()

	if err := s1.Reset(); err != nil {
		t.Fatalf("Reset s1 failed: %v", err)
	}
	if !s1.TemplateExists("cached_item") {
		t.Fatal("expected cached_item template in s1")
	}

	// Get second clone — should be independent
	s2, err := ClipsSessionGetCached("go-cache-test")
	if err != nil {
		t.Fatalf("GetCached s2 failed: %v", err)
	}
	defer s2.Close()

	if err := s2.Reset(); err != nil {
		t.Fatalf("Reset s2 failed: %v", err)
	}

	// Modify s1
	_, err = s1.FactAssertString("(cached_item (x 42))")
	if err != nil {
		t.Fatalf("FactAssertString s1 failed: %v", err)
	}

	// s2 should be unaffected
	facts, err := s2.FactsByTemplate("cached_item")
	if err != nil {
		t.Fatalf("FactsByTemplate s2 failed: %v", err)
	}
	if len(facts) != 0 {
		t.Fatalf("expected 0 facts in s2, got %d", len(facts))
	}

	// Cleanup
	if err := ClipsSessionCacheRemove("go-cache-test"); err != nil {
		t.Fatalf("CacheRemove failed: %v", err)
	}
}

func TestClipsSessionModuleOps(t *testing.T) {
	s, err := NewClipsSession()
	if err != nil {
		t.Fatalf("NewClipsSession failed: %v", err)
	}
	defer s.Close()

	// Default module is MAIN
	mod, err := s.ModuleCurrentGet()
	if err != nil {
		t.Fatalf("ModuleCurrentGet failed: %v", err)
	}
	if mod != "MAIN" {
		t.Fatalf("expected MAIN, got %s", mod)
	}

	if !s.ModuleExists("MAIN") {
		t.Fatal("MAIN module should exist")
	}
}

func TestClipsSessionSettings(t *testing.T) {
	s, err := NewClipsSession()
	if err != nil {
		t.Fatalf("NewClipsSession failed: %v", err)
	}
	defer s.Close()

	// Toggle fact duplication
	if err := s.FactDuplicationSet(true); err != nil {
		t.Fatalf("FactDuplicationSet failed: %v", err)
	}
	if !s.FactDuplicationGet() {
		t.Fatal("expected fact duplication to be true")
	}

	if err := s.FactDuplicationSet(false); err != nil {
		t.Fatalf("FactDuplicationSet(false) failed: %v", err)
	}
	if s.FactDuplicationGet() {
		t.Fatal("expected fact duplication to be false")
	}
}

func TestClipsSessionEval(t *testing.T) {
	s, err := NewClipsSession()
	if err != nil {
		t.Fatalf("NewClipsSession failed: %v", err)
	}
	defer s.Close()

	result, err := s.Eval("(+ 2 3)")
	if err != nil {
		t.Fatalf("Eval failed: %v", err)
	}
	if result == "" {
		t.Fatal("expected non-empty eval result")
	}
}

func TestClipsSessionFBPPattern(t *testing.T) {
	s, err := NewClipsSession()
	if err != nil {
		t.Fatalf("NewClipsSession failed: %v", err)
	}
	defer s.Close()

	// Load rules
	json := `{
		"templates": [
			{
				"name": "data",
				"slots": [
					{"name": "key", "type": "STRING"},
					{"name": "val", "type": "INTEGER"}
				]
			},
			{
				"name": "result",
				"slots": [
					{"name": "key", "type": "STRING"},
					{"name": "val", "type": "INTEGER"}
				]
			}
		],
		"rules": [
			{
				"name": "double-val",
				"source": "(defrule double-val (data (key ?k) (val ?v)) => (assert (result (key ?k) (val (* ?v 2)))))"
			}
		]
	}`
	if err := s.LoadJSON(json); err != nil {
		t.Fatalf("LoadJSON failed: %v", err)
	}

	// FBP cycle
	for cycle := 0; cycle < 3; cycle++ {
		if err := s.Reset(); err != nil {
			t.Fatalf("cycle %d: Reset failed: %v", cycle, err)
		}

		// Assert facts
		for i := 0; i < 10; i++ {
			fact := fmt.Sprintf(`(data (key "k%d") (val %d))`, i, (cycle+1)*10+i)
			_, err := s.FactAssertString(fact)
			if err != nil {
				t.Fatalf("cycle %d: FactAssertString failed: %v", cycle, err)
			}
		}

		// Run
		fired, err := s.Run(-1)
		if err != nil {
			t.Fatalf("cycle %d: Run failed: %v", cycle, err)
		}
		if fired != 10 {
			t.Fatalf("cycle %d: expected 10 rules fired, got %d", cycle, fired)
		}

		// Query results
		results, err := s.FactsByTemplate("result")
		if err != nil {
			t.Fatalf("cycle %d: FactsByTemplate failed: %v", cycle, err)
		}
		if len(results) != 10 {
			t.Fatalf("cycle %d: expected 10 results, got %d", cycle, len(results))
		}
	}
}
