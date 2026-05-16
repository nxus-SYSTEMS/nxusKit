//! CLIPS rule security validation.
//!
//! This module provides security validation for CLIPS rules, detecting
//! potentially dangerous constructs like system calls and file I/O.

use regex::Regex;
use std::sync::LazyLock;

/// Security validation severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SecuritySeverity {
    /// Reject rules with dangerous constructs (default).
    #[default]
    Error,
    /// Log warning but proceed with loading.
    Warning,
    /// Log info message only.
    Info,
    /// Skip validation entirely.
    Ignore,
}

/// A security issue found in CLIPS rules.
#[derive(Debug, Clone)]
pub struct SecurityIssue {
    /// The dangerous pattern that was matched.
    pub pattern: String,
    /// Description of the security concern.
    pub description: String,
    /// Line number where the issue was found (1-indexed).
    pub line_number: usize,
    /// The actual matched text.
    pub matched_text: String,
}

impl std::fmt::Display for SecurityIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Line {}: {} - found '{}' (pattern: {})",
            self.line_number, self.description, self.matched_text, self.pattern
        )
    }
}

/// Result of security validation.
#[derive(Debug, Clone)]
pub struct SecurityValidationResult {
    /// Whether the validation passed (no issues or severity allows proceeding).
    pub passed: bool,
    /// List of security issues found.
    pub issues: Vec<SecurityIssue>,
}

/// Dangerous patterns to detect in CLIPS rules.
struct DangerousPattern {
    /// Regex pattern to match.
    regex: Regex,
    /// Human-readable pattern name.
    name: &'static str,
    /// Description of the security concern.
    description: &'static str,
}

/// List of dangerous patterns to detect.
static DANGEROUS_PATTERNS: LazyLock<Vec<DangerousPattern>> = LazyLock::new(|| {
    vec![
        DangerousPattern {
            regex: Regex::new(r"\(\s*system\s+").unwrap(),
            name: "system()",
            description: "System command execution can run arbitrary shell commands",
        },
        DangerousPattern {
            regex: Regex::new(r"\(\s*open\s+").unwrap(),
            name: "open()",
            description: "File operations can read/write arbitrary files",
        },
        DangerousPattern {
            regex: Regex::new(r"\(\s*close\s+").unwrap(),
            name: "close()",
            description: "File handle operations (used with open)",
        },
        DangerousPattern {
            regex: Regex::new(r"\(\s*read\s+").unwrap(),
            name: "read()",
            description: "File read operations can access sensitive data",
        },
        DangerousPattern {
            regex: Regex::new(r"\(\s*readline\s+").unwrap(),
            name: "readline()",
            description: "File read operations can access sensitive data",
        },
        DangerousPattern {
            regex: Regex::new(r"\(\s*printout\s+[^\)]*\s+\?\w+").unwrap(),
            name: "printout to file",
            description: "Writing to file handles can modify files",
        },
        DangerousPattern {
            regex: Regex::new(r"\(\s*format\s+[^\)]*\s+\?\w+").unwrap(),
            name: "format to file",
            description: "Formatted output to file handles can modify files",
        },
        DangerousPattern {
            regex: Regex::new(r"\(\s*remove\s+").unwrap(),
            name: "remove()",
            description: "File deletion can remove important files",
        },
        DangerousPattern {
            regex: Regex::new(r"\(\s*rename\s+").unwrap(),
            name: "rename()",
            description: "File renaming can modify file system structure",
        },
        DangerousPattern {
            regex: Regex::new(r"\(\s*batch\s+").unwrap(),
            name: "batch()",
            description: "Batch loading can execute arbitrary CLIPS files",
        },
        DangerousPattern {
            regex: Regex::new(r"\(\s*load\s+").unwrap(),
            name: "load()",
            description: "Loading external files can execute arbitrary code",
        },
        DangerousPattern {
            regex: Regex::new(r"\(\s*save\s+").unwrap(),
            name: "save()",
            description: "Saving can write to arbitrary file locations",
        },
        DangerousPattern {
            regex: Regex::new(r"\(\s*bsave\s+").unwrap(),
            name: "bsave()",
            description: "Binary save can write to arbitrary file locations",
        },
        DangerousPattern {
            regex: Regex::new(r"\(\s*bload\s+").unwrap(),
            name: "bload()",
            description: "Binary loading can execute arbitrary binary data",
        },
    ]
});

/// Security validator for CLIPS rules.
#[derive(Debug, Clone)]
pub struct SecurityValidator {
    /// Severity level for validation.
    pub severity: SecuritySeverity,
}

impl Default for SecurityValidator {
    fn default() -> Self {
        Self {
            severity: SecuritySeverity::Error,
        }
    }
}

impl SecurityValidator {
    /// Create a new security validator with the specified severity.
    pub fn new(severity: SecuritySeverity) -> Self {
        Self { severity }
    }

    /// Validate CLIPS rules for security issues.
    ///
    /// Returns a validation result indicating whether the rules are safe
    /// and listing any security issues found.
    pub fn validate_rules(&self, rules: &str) -> SecurityValidationResult {
        if self.severity == SecuritySeverity::Ignore {
            return SecurityValidationResult {
                passed: true,
                issues: vec![],
            };
        }

        let mut issues = Vec::new();

        for (line_idx, line) in rules.lines().enumerate() {
            let line_number = line_idx + 1;

            for pattern in DANGEROUS_PATTERNS.iter() {
                if let Some(matched) = pattern.regex.find(line) {
                    issues.push(SecurityIssue {
                        pattern: pattern.name.to_string(),
                        description: pattern.description.to_string(),
                        line_number,
                        matched_text: matched.as_str().to_string(),
                    });
                }
            }
        }

        let passed = match self.severity {
            SecuritySeverity::Error => issues.is_empty(),
            SecuritySeverity::Warning | SecuritySeverity::Info => true,
            SecuritySeverity::Ignore => true,
        };

        SecurityValidationResult { passed, issues }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_system_call() {
        let validator = SecurityValidator::new(SecuritySeverity::Error);
        let rules = r#"
(defrule dangerous-rule
    (trigger)
    =>
    (system "rm -rf /"))
"#;

        let result = validator.validate_rules(rules);
        assert!(!result.passed);
        assert_eq!(result.issues.len(), 1);
        assert_eq!(result.issues[0].pattern, "system()");
    }

    #[test]
    fn test_detect_open_file() {
        let validator = SecurityValidator::new(SecuritySeverity::Error);
        let rules = r#"
(defrule file-reader
    (read-file ?filename)
    =>
    (open ?filename "r" file-handle))
"#;

        let result = validator.validate_rules(rules);
        assert!(!result.passed);
        assert!(result.issues.iter().any(|i| i.pattern == "open()"));
    }

    #[test]
    fn test_safe_rules_pass() {
        let validator = SecurityValidator::new(SecuritySeverity::Error);
        let rules = r#"
(deftemplate patient
    (slot name (type STRING))
    (slot age (type INTEGER)))

(defrule check-age
    (patient (name ?n) (age ?a))
    (test (> ?a 65))
    =>
    (assert (elderly-patient (name ?n))))
"#;

        let result = validator.validate_rules(rules);
        assert!(result.passed);
        assert!(result.issues.is_empty());
    }

    #[test]
    fn test_warning_severity_allows_dangerous() {
        let validator = SecurityValidator::new(SecuritySeverity::Warning);
        let rules = r#"(system "echo hello")"#;

        let result = validator.validate_rules(rules);
        assert!(result.passed); // Warning allows proceeding
        assert!(!result.issues.is_empty()); // But issues are still reported
    }

    #[test]
    fn test_ignore_severity_skips_validation() {
        let validator = SecurityValidator::new(SecuritySeverity::Ignore);
        let rules = r#"(system "rm -rf /")"#;

        let result = validator.validate_rules(rules);
        assert!(result.passed);
        assert!(result.issues.is_empty());
    }

    #[test]
    fn test_multiple_issues_on_same_line() {
        let validator = SecurityValidator::new(SecuritySeverity::Error);
        let rules = r#"(system "cat") (open "file" "r" h)"#;

        let result = validator.validate_rules(rules);
        assert!(!result.passed);
        assert_eq!(result.issues.len(), 2);
    }

    #[test]
    fn test_detect_batch_load() {
        let validator = SecurityValidator::new(SecuritySeverity::Error);
        let rules = r#"(batch "malicious.clp")"#;

        let result = validator.validate_rules(rules);
        assert!(!result.passed);
        assert!(result.issues.iter().any(|i| i.pattern == "batch()"));
    }
}
