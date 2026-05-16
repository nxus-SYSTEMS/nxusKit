package nxuskit

import (
	"testing"
)

func TestSecurityValidator_DetectSystemCall(t *testing.T) {
	validator := NewSecurityValidator(SecuritySeverityError)
	rules := `
(defrule dangerous-rule
    (trigger)
    =>
    (system "rm -rf /"))
`

	result := validator.ValidateRules(rules)
	if result.Passed {
		t.Error("Expected validation to fail for system() call")
	}
	if len(result.Issues) != 1 {
		t.Errorf("Expected 1 issue, got %d", len(result.Issues))
	}
	if result.Issues[0].Pattern != "system()" {
		t.Errorf("Expected pattern 'system()', got '%s'", result.Issues[0].Pattern)
	}
}

func TestSecurityValidator_DetectOpenFile(t *testing.T) {
	validator := NewSecurityValidator(SecuritySeverityError)
	rules := `
(defrule file-reader
    (read-file ?filename)
    =>
    (open ?filename "r" file-handle))
`

	result := validator.ValidateRules(rules)
	if result.Passed {
		t.Error("Expected validation to fail for open() call")
	}

	hasOpenPattern := false
	for _, issue := range result.Issues {
		if issue.Pattern == "open()" {
			hasOpenPattern = true
			break
		}
	}
	if !hasOpenPattern {
		t.Error("Expected to find open() pattern in issues")
	}
}

func TestSecurityValidator_SafeRulesPass(t *testing.T) {
	validator := NewSecurityValidator(SecuritySeverityError)
	rules := `
(deftemplate patient
    (slot name (type STRING))
    (slot age (type INTEGER)))

(defrule check-age
    (patient (name ?n) (age ?a))
    (test (> ?a 65))
    =>
    (assert (elderly-patient (name ?n))))
`

	result := validator.ValidateRules(rules)
	if !result.Passed {
		t.Error("Expected safe rules to pass validation")
	}
	if len(result.Issues) != 0 {
		t.Errorf("Expected 0 issues, got %d", len(result.Issues))
	}
}

func TestSecurityValidator_WarningSeverityAllowsDangerous(t *testing.T) {
	validator := NewSecurityValidator(SecuritySeverityWarning)
	rules := `(system "echo hello")`

	result := validator.ValidateRules(rules)
	if !result.Passed {
		t.Error("Expected warning severity to allow proceeding")
	}
	if len(result.Issues) == 0 {
		t.Error("Expected issues to still be reported with warning severity")
	}
}

func TestSecurityValidator_IgnoreSeveritySkipsValidation(t *testing.T) {
	validator := NewSecurityValidator(SecuritySeverityIgnore)
	rules := `(system "rm -rf /")`

	result := validator.ValidateRules(rules)
	if !result.Passed {
		t.Error("Expected ignore severity to pass")
	}
	if len(result.Issues) != 0 {
		t.Error("Expected no issues with ignore severity")
	}
}

func TestSecurityValidator_MultipleIssues(t *testing.T) {
	validator := NewSecurityValidator(SecuritySeverityError)
	rules := `(system "cat") (open "file" "r" h)`

	result := validator.ValidateRules(rules)
	if result.Passed {
		t.Error("Expected validation to fail")
	}
	if len(result.Issues) != 2 {
		t.Errorf("Expected 2 issues, got %d", len(result.Issues))
	}
}

func TestSecurityValidator_DetectBatchLoad(t *testing.T) {
	validator := NewSecurityValidator(SecuritySeverityError)
	rules := `(batch "malicious.clp")`

	result := validator.ValidateRules(rules)
	if result.Passed {
		t.Error("Expected validation to fail for batch() call")
	}

	hasBatchPattern := false
	for _, issue := range result.Issues {
		if issue.Pattern == "batch()" {
			hasBatchPattern = true
			break
		}
	}
	if !hasBatchPattern {
		t.Error("Expected to find batch() pattern in issues")
	}
}

func TestSecurityValidator_LineNumbers(t *testing.T) {
	validator := NewSecurityValidator(SecuritySeverityError)
	rules := `; safe comment
(defrule safe-rule =>)
(system "dangerous")
; another comment
`

	result := validator.ValidateRules(rules)
	if result.Passed {
		t.Error("Expected validation to fail")
	}
	if len(result.Issues) != 1 {
		t.Errorf("Expected 1 issue, got %d", len(result.Issues))
	}
	if result.Issues[0].LineNumber != 3 {
		t.Errorf("Expected line number 3, got %d", result.Issues[0].LineNumber)
	}
}

func TestDefaultSecurityValidator(t *testing.T) {
	validator := DefaultSecurityValidator()
	if validator.Severity != SecuritySeverityError {
		t.Errorf("expected default severity to be 'error', got %q", validator.Severity)
	}

	// Verify it blocks dangerous rules by default
	rules := `(system "echo test")`
	result := validator.ValidateRules(rules)
	if result.Passed {
		t.Error("expected default validator to block dangerous rules")
	}
}

func TestSecuritySeverityConstants(t *testing.T) {
	// Ensure the severity constants are defined
	if SecuritySeverityError != "error" {
		t.Errorf("expected 'error', got %q", SecuritySeverityError)
	}
	if SecuritySeverityWarning != "warning" {
		t.Errorf("expected 'warning', got %q", SecuritySeverityWarning)
	}
	if SecuritySeverityInfo != "info" {
		t.Errorf("expected 'info', got %q", SecuritySeverityInfo)
	}
	if SecuritySeverityIgnore != "ignore" {
		t.Errorf("expected 'ignore', got %q", SecuritySeverityIgnore)
	}
}
