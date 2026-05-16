"""Public CE solver wrapper stub."""

from __future__ import annotations


class SolverError(RuntimeError):
    """Raised when a Pro solver API is requested from public CE."""


class SolverSession:
    """Unavailable public CE solver session placeholder."""

    def __init__(self, *args, **kwargs):
        raise SolverError("Solver sessions are a Pro capability and are not shipped in public CE")
