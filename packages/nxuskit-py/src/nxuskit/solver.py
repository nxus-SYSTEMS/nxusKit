"""Pythonic constraint solver wrapper over the nxusKit C ABI.

Provides SolverSession with context manager support, async solve, and
streaming optimization via solve_stream().

Example::

    from nxuskit.solver import SolverSession
    from nxuskit.solver_types import VariableDef, VariableType, ConstraintDef, ConstraintType

    with SolverSession() as s:
        s.add_variables([VariableDef(name="x", var_type=VariableType.INTEGER)])
        s.add_constraints([ConstraintDef(
            constraint_type=ConstraintType.LE,
            variables=["x"],
            parameters={"right": 5},
        )])
        result = s.solve()
        print(result.status, result.assignments)

Streaming example::

    with SolverSession() as s:
        # ... add variables, constraints, objective ...
        for chunk in s.solve_stream():
            if chunk.is_final:
                print("Done:", chunk.status)
            else:
                print(f"Iter {chunk.iteration}: obj={chunk.objective_value}")
"""

from __future__ import annotations

import asyncio
import json
from dataclasses import dataclass
from typing import Iterator, Optional

from ._solver_ffi import SolverLibraryNotFoundError, last_error, read_and_free_string, solver_ffi
from .solver_types import (
    ConstraintDef,
    ObjectiveDef,
    SessionStatus,
    SolverCapabilities,
    SolverConfig,
    SolveResult,
    SolverExplanation,
    VariableDef,
)

__all__ = [
    "SolverSession",
    "SolverStreamChunk",
    "SolverError",
    "SolverLibraryNotFoundError",
]


# ── SolverStreamChunk ────────────────────────────────────────


@dataclass
class SolverStreamChunk:
    """A streaming progress event from solver optimization.

    Yielded by :meth:`SolverSession.solve_stream` to report incremental
    progress during optimization.  The final chunk has ``is_final=True``
    and carries the terminal ``status`` (e.g. ``"optimal"``).
    """

    objective_value: Optional[float] = None
    iteration: int = 0
    total_iterations: Optional[int] = None
    bound_gap: Optional[float] = None
    status: str = "in_progress"
    elapsed_ms: int = 0
    is_final: bool = False

    @classmethod
    def from_dict(cls, data: dict) -> SolverStreamChunk:
        """Construct a SolverStreamChunk from a parsed JSON dict."""
        return cls(
            objective_value=data.get("objective_value"),
            iteration=data.get("iteration", 0),
            total_iterations=data.get("total_iterations"),
            bound_gap=data.get("bound_gap"),
            status=data.get("status", "in_progress"),
            elapsed_ms=data.get("elapsed_ms", 0),
            is_final=data.get("is_final", False),
        )


class SolverError(RuntimeError):
    """Base error for solver operations."""


def _get_lib():
    from ._solver_ffi import _get_lib as _gl

    return _gl()


def _check_ok(ok: bool, fallback: str) -> None:
    """Raise SolverError if a C ABI call returned false."""
    if not ok:
        err = last_error()
        raise SolverError(err if err else fallback)


# ── SolverSession ────────────────────────────────────────────


class SolverSession:
    """Constraint solver session with RAII cleanup.

    Use as a context manager for automatic resource cleanup::

        with SolverSession() as s:
            s.add_variables([...])
            result = s.solve()
    """

    def __init__(self, config: SolverConfig | None = None):
        """Create a new solver session with optional configuration."""
        # Initialize _handle early to avoid AttributeError in __del__ if __init__ fails
        self._handle = None
        lib = _get_lib()
        if config is not None:
            config_json = config.to_json()
            c_json = solver_ffi.new("char[]", config_json.encode("utf-8"))
        else:
            c_json = solver_ffi.NULL
        handle = lib.nxuskit_solver_session_create(c_json)
        if handle == solver_ffi.NULL:
            raise SolverError(last_error() or "failed to create solver session")
        self._handle = handle

    def close(self) -> None:
        """Destroy the session and free its memory. Safe to call multiple times."""
        if self._handle is not None and self._handle != solver_ffi.NULL:
            _get_lib().nxuskit_solver_session_destroy(self._handle)
            self._handle = None

    def __enter__(self):
        """Enter the context manager, returning the session."""
        return self

    def __exit__(self, *_):
        """Exit the context manager, destroying the session."""
        self.close()

    def __del__(self):
        """Ensure cleanup on garbage collection."""
        self.close()

    # ── Model Building ───────────────────────────────────────

    def add_variables(self, variables: list[VariableDef]) -> None:
        """Add variables to the solver model."""
        data = json.dumps([v.to_dict() for v in variables])
        c_json = solver_ffi.new("char[]", data.encode("utf-8"))
        _check_ok(
            _get_lib().nxuskit_solver_add_variables(self._handle, c_json),
            "failed to add variables",
        )

    def add_constraints(self, constraints: list[ConstraintDef]) -> None:
        """Add constraints to the solver model."""
        data = json.dumps([c.to_dict() for c in constraints])
        c_json = solver_ffi.new("char[]", data.encode("utf-8"))
        _check_ok(
            _get_lib().nxuskit_solver_add_constraints(self._handle, c_json),
            "failed to add constraints",
        )

    def set_objective(self, objective: ObjectiveDef) -> None:
        """Set a single optimization objective. Replaces any existing."""
        data = json.dumps(objective.to_dict())
        c_json = solver_ffi.new("char[]", data.encode("utf-8"))
        _check_ok(
            _get_lib().nxuskit_solver_set_objective(self._handle, c_json),
            "failed to set objective",
        )

    def add_objective(self, objective: ObjectiveDef) -> None:
        """Add an objective to the multi-objective list."""
        data = json.dumps(objective.to_dict())
        c_json = solver_ffi.new("char[]", data.encode("utf-8"))
        _check_ok(
            _get_lib().nxuskit_solver_add_objective(self._handle, c_json),
            "failed to add objective",
        )

    def retract(self, names: list[str]) -> None:
        """Remove named constraints from the model."""
        data = json.dumps(names)
        c_json = solver_ffi.new("char[]", data.encode("utf-8"))
        _check_ok(
            _get_lib().nxuskit_solver_retract(self._handle, c_json),
            "failed to retract constraints",
        )

    def retract_objective(self, name: str) -> bool:
        """Remove a named objective. Returns True if found and removed."""
        c_name = solver_ffi.new("char[]", name.encode("utf-8"))
        return bool(_get_lib().nxuskit_solver_retract_objective(self._handle, c_name))

    def add_assumptions(self, assumptions: list[ConstraintDef]) -> None:
        """Add temporary assumptions for the next solve call.

        Assumptions are auto-retracted after solve completes.
        """
        data = json.dumps([a.to_dict() for a in assumptions])
        c_json = solver_ffi.new("char[]", data.encode("utf-8"))
        _check_ok(
            _get_lib().nxuskit_solver_add_assumptions(self._handle, c_json),
            "failed to add assumptions",
        )

    # ── Scoping ──────────────────────────────────────────────

    def push(self) -> None:
        """Save the current model state for what-if analysis."""
        _check_ok(_get_lib().nxuskit_solver_push(self._handle), "failed to push scope")

    def pop(self) -> None:
        """Restore the model to the state at the matching push."""
        _check_ok(_get_lib().nxuskit_solver_pop(self._handle), "failed to pop scope")

    def reset(self) -> None:
        """Clear all variables, constraints, and objectives."""
        _check_ok(_get_lib().nxuskit_solver_reset(self._handle), "failed to reset session")

    # ── Solving ──────────────────────────────────────────────

    def solve(self, config: SolverConfig | None = None) -> SolveResult:
        """Run the constraint solver. Pass None for default config."""
        lib = _get_lib()
        if config is not None:
            config_json = config.to_json()
            c_json = solver_ffi.new("char[]", config_json.encode("utf-8"))
        else:
            c_json = solver_ffi.NULL
        ptr = lib.nxuskit_solver_solve(self._handle, c_json)
        result_str = read_and_free_string(ptr)
        return SolveResult.from_dict(json.loads(result_str))

    async def solve_async(self, config: SolverConfig | None = None) -> SolveResult:
        """Run the solver on a thread pool to avoid blocking the event loop."""
        loop = asyncio.get_running_loop()
        return await loop.run_in_executor(None, self.solve, config)

    def solve_stream(self, config: SolverConfig | None = None) -> Iterator[SolverStreamChunk]:
        """Stream solver progress as a generator of :class:`SolverStreamChunk`.

        Calls ``nxuskit_solver_solve_stream`` on a background thread and
        bridges C callbacks to the Python generator via a :class:`queue.Queue`.

        The final yielded chunk has ``is_final=True`` and carries the terminal
        status.  If the caller needs the full :class:`SolveResult` after
        streaming, call :meth:`solve` or inspect the final chunk's JSON
        payload.

        Args:
            config: Optional per-solve configuration.

        Yields:
            SolverStreamChunk for each progress update.

        Raises:
            SolverError: if the C ABI call fails.
        """
        import queue
        import threading

        q: queue.Queue[SolverStreamChunk | SolveResult | None | Exception] = queue.Queue(maxsize=64)

        # Build the config C string once, outside the callbacks.
        if config is not None:
            config_json = config.to_json()
            c_config = solver_ffi.new("char[]", config_json.encode("utf-8"))
        else:
            c_config = solver_ffi.NULL

        # ── C-callable callbacks ──────────────────────────────

        @solver_ffi.callback("int32_t(const char *, void *)")
        def on_chunk(chunk_json_ptr, _user_data):
            """Called from C for each progress update. Return 0 to continue."""
            try:
                raw = solver_ffi.string(chunk_json_ptr).decode("utf-8")
                data = json.loads(raw)
                chunk = SolverStreamChunk.from_dict(data)
                q.put(chunk)
            except Exception as e:
                q.put(e)
                return 1  # signal cancellation on error
            return 0  # continue

        @solver_ffi.callback("void(const char *, void *)")
        def on_done(result_json_ptr, _user_data):
            """Called from C when solving completes with the final result JSON."""
            try:
                raw = solver_ffi.string(result_json_ptr).decode("utf-8")
                data = json.loads(raw)
                result = SolveResult.from_dict(data)
                q.put(result)
            except Exception as e:
                q.put(e)

        # ── Background thread ─────────────────────────────────

        def run_stream():
            try:
                ok = _get_lib().nxuskit_solver_solve_stream(
                    self._handle,
                    c_config,
                    on_chunk,
                    on_done,
                    solver_ffi.NULL,
                )
                if not ok:
                    q.put(SolverError(last_error() or "streaming solve failed"))
            except Exception as e:
                q.put(e)
            finally:
                q.put(None)  # sentinel: stream is done

        thread = threading.Thread(target=run_stream, daemon=True)
        thread.start()

        # ── Yield from queue ──────────────────────────────────

        while True:
            item = q.get()
            if item is None:
                break
            if isinstance(item, Exception):
                raise item
            if isinstance(item, SolveResult):
                # Wrap the final SolveResult as a final SolverStreamChunk
                yield SolverStreamChunk(
                    objective_value=item.objective_value,
                    status=item.status.value,
                    elapsed_ms=item.stats.solve_time_ms,
                    is_final=True,
                )
                break
            yield item

    def explanation(self) -> SolverExplanation | None:
        """Return the explanation artifacts from the last solve, or None."""
        lib = _get_lib()
        ptr = lib.nxuskit_solver_explanation(self._handle)
        if ptr == solver_ffi.NULL:
            return None
        s = solver_ffi.string(ptr).decode("utf-8")
        lib.nxuskit_free_string(ptr)
        return SolverExplanation.from_dict(json.loads(s))

    # ── Introspection ────────────────────────────────────────

    def variables(self) -> list[VariableDef]:
        """Return current variable definitions."""
        ptr = _get_lib().nxuskit_solver_variables(self._handle)
        data = json.loads(read_and_free_string(ptr))
        return [VariableDef.from_dict(v) for v in data]

    def constraints(self) -> list[ConstraintDef]:
        """Return current constraint definitions."""
        ptr = _get_lib().nxuskit_solver_constraints(self._handle)
        data = json.loads(read_and_free_string(ptr))
        return [ConstraintDef.from_dict(c) for c in data]

    def objectives(self) -> list[ObjectiveDef]:
        """Return current objectives list."""
        ptr = _get_lib().nxuskit_solver_objectives(self._handle)
        data = json.loads(read_and_free_string(ptr))
        return [ObjectiveDef.from_dict(o) for o in data]

    def status(self) -> SessionStatus:
        """Return the current session status snapshot."""
        ptr = _get_lib().nxuskit_solver_status(self._handle)
        data = json.loads(read_and_free_string(ptr))
        return SessionStatus.from_dict(data)

    def capabilities(self) -> SolverCapabilities:
        """Return the solver backend capabilities."""
        ptr = _get_lib().nxuskit_solver_capabilities(self._handle)
        data = json.loads(read_and_free_string(ptr))
        return SolverCapabilities.from_dict(data)

    @property
    def num_variables(self) -> int:
        """Return the number of variables in the model."""
        n = int(_get_lib().nxuskit_solver_num_variables(self._handle))
        if n < 0:
            raise SolverError(last_error() or "num_variables returned error")
        return n

    @property
    def num_constraints(self) -> int:
        """Return the number of constraints in the model."""
        n = int(_get_lib().nxuskit_solver_num_constraints(self._handle))
        if n < 0:
            raise SolverError(last_error() or "num_constraints returned error")
        return n
