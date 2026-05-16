"""Tests for Python SecurityValidator (US5, T052-T059).

Validates that all 14 dangerous CLIPS patterns are detected, with correct
line numbers, severity handling, and multi-pattern detection.
"""

from nxuskit.security import (
    SecuritySeverity,
    SecurityValidator,
)

# ── T052: system pattern ─────────────────────────────────────────────


def test_security_validator_system_pattern():
    validator = SecurityValidator()
    result = validator.validate('(defrule test => (system "ls"))')
    assert not result.passed
    assert len(result.issues) == 1
    assert result.issues[0].pattern == "system"
    assert result.issues[0].line_number == 1


# ── T053: open pattern ───────────────────────────────────────────────


def test_security_validator_open_pattern():
    validator = SecurityValidator()
    result = validator.validate('(defrule test => (open "file.txt" f "w"))')
    assert not result.passed
    assert len(result.issues) == 1
    assert result.issues[0].pattern == "open"


# ── T054: clean rule passes ──────────────────────────────────────────


def test_security_validator_clean_rule():
    validator = SecurityValidator()
    result = validator.validate(
        '(defrule safe (person (name ?n)) => (printout t "Hello " ?n crlf))'
    )
    assert result.passed
    assert len(result.issues) == 0


# ── T055: multiple patterns on one line ──────────────────────────────


def test_security_validator_multiple_patterns_one_line():
    validator = SecurityValidator()
    result = validator.validate('(defrule test => (system "x") (open "y" f "r"))')
    assert not result.passed
    assert len(result.issues) == 2
    assert all(issue.line_number == 1 for issue in result.issues)


# ── T056: WARNING severity → passed=True but issues collected ────────


def test_security_validator_severity_warning():
    validator = SecurityValidator()
    result = validator.validate(
        '(defrule test => (system "ls"))',
        severity=SecuritySeverity.WARNING,
    )
    assert result.passed
    assert len(result.issues) > 0


# ── T057: IGNORE severity → passed=True, empty issues ───────────────


def test_security_validator_severity_ignore():
    validator = SecurityValidator()
    result = validator.validate(
        '(defrule test => (system "ls"))',
        severity=SecuritySeverity.IGNORE,
    )
    assert result.passed
    assert len(result.issues) == 0


# ── T058: all 14 patterns detected ──────────────────────────────────


def test_security_validator_all_14_patterns():
    patterns_and_rules = [
        ("system", '(defrule t => (system "cmd"))'),
        ("batch", '(defrule t => (batch "file.bat"))'),
        ("load", '(defrule t => (load "rules.clp"))'),
        ("bload", '(defrule t => (bload "rules.bin"))'),
        ("open", '(defrule t => (open "f.txt" f "r"))'),
        ("close", "(defrule t => (close f))"),
        ("read", "(defrule t => (read f))"),
        ("readline", "(defrule t => (readline f))"),
        ("remove", '(defrule t => (remove "f.txt"))'),
        ("rename", '(defrule t => (rename "a.txt" "b.txt"))'),
        ("printout", '(defrule t => (printout myfile "data"))'),
        ("format", '(defrule t => (format myfile "%s" "data"))'),
        ("save", '(defrule t => (save "env.clp"))'),
        ("bsave", '(defrule t => (bsave "env.bin"))'),
    ]
    validator = SecurityValidator()
    for expected_pattern, rule in patterns_and_rules:
        result = validator.validate(rule)
        assert len(result.issues) >= 1, (
            f"Expected issue for pattern '{expected_pattern}' in: {rule}"
        )
        found = any(issue.pattern == expected_pattern for issue in result.issues)
        assert found, (
            f"Expected pattern '{expected_pattern}' but got "
            f"{[i.pattern for i in result.issues]} for rule: {rule}"
        )


# ── T059: line number accuracy ───────────────────────────────────────


def test_security_validator_line_numbers():
    rule = """(defrule multi-line
    (data ?x)
    =>
    (system "dangerous")
)"""
    validator = SecurityValidator()
    result = validator.validate(rule)
    assert not result.passed
    assert len(result.issues) == 1
    assert result.issues[0].line_number == 4
    assert result.issues[0].pattern == "system"
