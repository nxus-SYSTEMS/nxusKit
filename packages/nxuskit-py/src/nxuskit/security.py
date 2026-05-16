"""CLIPS rule security validation — pure Python port of the Rust SecurityValidator.

Detects 14 dangerous CLIPS patterns (file I/O, system commands, dynamic loading)
and reports line-level security issues. Matches the Rust implementation in
``nxuskit-engine/src/providers/clips/security.rs``.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from enum import Enum
from typing import List


class SecuritySeverity(Enum):
    """Severity level controlling validation behavior.

    - ``ERROR``: Reject rules containing dangerous patterns (``passed=False``).
    - ``WARNING``: Log issues but allow rules (``passed=True``, issues collected).
    - ``INFO``: Informational only (``passed=True``, issues collected).
    - ``IGNORE``: Skip scanning entirely (``passed=True``, no issues).
    """

    ERROR = "error"
    WARNING = "warning"
    INFO = "info"
    IGNORE = "ignore"


@dataclass
class SecurityIssue:
    """A single security issue found in CLIPS rule source.

    Attributes:
        pattern: Name of the dangerous pattern matched (e.g., ``"system"``).
        description: Human-readable description of the security concern.
        line_number: 1-indexed line number where the issue was found.
        matched_text: The actual text that matched the pattern.
    """

    pattern: str
    description: str
    line_number: int
    matched_text: str


@dataclass
class SecurityValidationResult:
    """Result of security validation.

    Attributes:
        passed: Whether validation passed (no blocking issues found).
        issues: List of security issues detected.
    """

    passed: bool
    issues: List[SecurityIssue] = field(default_factory=list)


# ── Dangerous pattern definitions ────────────────────────────────────

_DANGEROUS_PATTERNS: list[tuple[re.Pattern[str], str, str]] = [
    (re.compile(r"\(\s*system\b"), "system", "System command execution"),
    (re.compile(r"\(\s*batch\b"), "batch", "Batch file execution"),
    (re.compile(r"\(\s*load\b"), "load", "Dynamic file loading"),
    (re.compile(r"\(\s*bload\b"), "bload", "Binary file loading"),
    (re.compile(r"\(\s*open\b"), "open", "File open operations"),
    (re.compile(r"\(\s*close\b"), "close", "File close operations"),
    (re.compile(r"\(\s*read\b"), "read", "File read operations"),
    (re.compile(r"\(\s*readline\b"), "readline", "File readline operations"),
    (re.compile(r"\(\s*remove\b"), "remove", "File deletion"),
    (re.compile(r"\(\s*rename\b"), "rename", "File rename operations"),
    (
        re.compile(r"\(\s*printout\s+(?!t\b|stdout\b)"),
        "printout",
        "File output (printout with non-terminal handle)",
    ),
    (
        re.compile(r"\(\s*format\s+(?!t\b|stdout\b)"),
        "format",
        "Formatted file output (format with non-terminal handle)",
    ),
    (re.compile(r"\(\s*save\b"), "save", "Environment save"),
    (re.compile(r"\(\s*bsave\b"), "bsave", "Binary environment save"),
]


class SecurityValidator:
    """Validates CLIPS rule source for dangerous patterns.

    Example::

        >>> validator = SecurityValidator()
        >>> result = validator.validate('(defrule bad => (system "rm -rf /"))')
        >>> result.passed
        False
        >>> result.issues[0].pattern
        'system'
    """

    def __init__(self, severity: SecuritySeverity = SecuritySeverity.ERROR) -> None:
        self.severity = severity

    def validate(
        self,
        rule_source: str,
        severity: SecuritySeverity | None = None,
    ) -> SecurityValidationResult:
        """Validate CLIPS rule source for dangerous patterns.

        Args:
            rule_source: CLIPS rule source code to validate.
            severity: Override the validator's default severity for this call.

        Returns:
            A ``SecurityValidationResult`` with ``passed`` status and any issues found.
        """
        effective_severity = severity if severity is not None else self.severity

        if effective_severity == SecuritySeverity.IGNORE:
            return SecurityValidationResult(passed=True, issues=[])

        issues: list[SecurityIssue] = []

        for line_number, line in enumerate(rule_source.splitlines(), start=1):
            for pattern, name, description in _DANGEROUS_PATTERNS:
                for match in pattern.finditer(line):
                    issues.append(
                        SecurityIssue(
                            pattern=name,
                            description=description,
                            line_number=line_number,
                            matched_text=match.group(),
                        )
                    )

        if effective_severity == SecuritySeverity.ERROR:
            passed = len(issues) == 0
        else:
            # WARNING and INFO: collect issues but don't fail
            passed = True

        return SecurityValidationResult(passed=passed, issues=issues)
