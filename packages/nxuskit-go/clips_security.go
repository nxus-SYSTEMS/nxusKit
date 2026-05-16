package nxuskit

import (
	"regexp"
	"strings"
)

// SecuritySeverity controls how dangerous CLIPS constructs are handled.
type SecuritySeverity string

const (
	// SecuritySeverityError rejects rules with dangerous constructs (default).
	SecuritySeverityError SecuritySeverity = "error"
	// SecuritySeverityWarning logs a warning but proceeds with loading.
	SecuritySeverityWarning SecuritySeverity = "warning"
	// SecuritySeverityInfo logs an info message only.
	SecuritySeverityInfo SecuritySeverity = "info"
	// SecuritySeverityIgnore skips validation entirely.
	SecuritySeverityIgnore SecuritySeverity = "ignore"
)

// SecurityIssue represents a security concern found in CLIPS rules.
type SecurityIssue struct {
	// Pattern is the dangerous pattern that was matched.
	Pattern string
	// Description explains the security concern.
	Description string
	// LineNumber is where the issue was found (1-indexed).
	LineNumber int
	// MatchedText is the actual matched text.
	MatchedText string
}

// SecurityValidationResult contains the result of security validation.
type SecurityValidationResult struct {
	// Passed indicates whether validation passed (no issues or severity allows proceeding).
	Passed bool
	// Issues lists all security issues found.
	Issues []SecurityIssue
}

// dangerousPattern defines a pattern to detect in CLIPS rules.
type dangerousPattern struct {
	regex       *regexp.Regexp
	name        string
	description string
}

// dangerousPatterns lists all patterns to detect.
var dangerousPatterns = []dangerousPattern{
	{
		regex:       regexp.MustCompile(`\(\s*system\s+`),
		name:        "system()",
		description: "System command execution can run arbitrary shell commands",
	},
	{
		regex:       regexp.MustCompile(`\(\s*open\s+`),
		name:        "open()",
		description: "File operations can read/write arbitrary files",
	},
	{
		regex:       regexp.MustCompile(`\(\s*close\s+`),
		name:        "close()",
		description: "File handle operations (used with open)",
	},
	{
		regex:       regexp.MustCompile(`\(\s*read\s+`),
		name:        "read()",
		description: "File read operations can access sensitive data",
	},
	{
		regex:       regexp.MustCompile(`\(\s*readline\s+`),
		name:        "readline()",
		description: "File read operations can access sensitive data",
	},
	{
		regex:       regexp.MustCompile(`\(\s*printout\s+[^\)]*\s+\?\w+`),
		name:        "printout to file",
		description: "Writing to file handles can modify files",
	},
	{
		regex:       regexp.MustCompile(`\(\s*format\s+[^\)]*\s+\?\w+`),
		name:        "format to file",
		description: "Formatted output to file handles can modify files",
	},
	{
		regex:       regexp.MustCompile(`\(\s*remove\s+`),
		name:        "remove()",
		description: "File deletion can remove important files",
	},
	{
		regex:       regexp.MustCompile(`\(\s*rename\s+`),
		name:        "rename()",
		description: "File renaming can modify file system structure",
	},
	{
		regex:       regexp.MustCompile(`\(\s*batch\s+`),
		name:        "batch()",
		description: "Batch loading can execute arbitrary CLIPS files",
	},
	{
		regex:       regexp.MustCompile(`\(\s*load\s+`),
		name:        "load()",
		description: "Loading external files can execute arbitrary code",
	},
	{
		regex:       regexp.MustCompile(`\(\s*save\s+`),
		name:        "save()",
		description: "Saving can write to arbitrary file locations",
	},
	{
		regex:       regexp.MustCompile(`\(\s*bsave\s+`),
		name:        "bsave()",
		description: "Binary save can write to arbitrary file locations",
	},
	{
		regex:       regexp.MustCompile(`\(\s*bload\s+`),
		name:        "bload()",
		description: "Binary loading can execute arbitrary binary data",
	},
}

// SecurityValidator validates CLIPS rules for security issues.
type SecurityValidator struct {
	// Severity controls how issues are handled.
	Severity SecuritySeverity
}

// NewSecurityValidator creates a new security validator with the specified severity.
func NewSecurityValidator(severity SecuritySeverity) *SecurityValidator {
	return &SecurityValidator{Severity: severity}
}

// DefaultSecurityValidator creates a validator with error severity (default).
func DefaultSecurityValidator() *SecurityValidator {
	return &SecurityValidator{Severity: SecuritySeverityError}
}

// ValidateRules checks CLIPS rules for security issues.
// Returns a validation result indicating whether the rules are safe
// and listing any security issues found.
func (v *SecurityValidator) ValidateRules(rules string) *SecurityValidationResult {
	if v.Severity == SecuritySeverityIgnore {
		return &SecurityValidationResult{
			Passed: true,
			Issues: nil,
		}
	}

	var issues []SecurityIssue
	lines := strings.Split(rules, "\n")

	for lineIdx, line := range lines {
		lineNumber := lineIdx + 1

		for _, pattern := range dangerousPatterns {
			match := pattern.regex.FindString(line)
			if match != "" {
				issues = append(issues, SecurityIssue{
					Pattern:     pattern.name,
					Description: pattern.description,
					LineNumber:  lineNumber,
					MatchedText: match,
				})
			}
		}
	}

	passed := v.Severity != SecuritySeverityError || len(issues) == 0

	return &SecurityValidationResult{
		Passed: passed,
		Issues: issues,
	}
}
