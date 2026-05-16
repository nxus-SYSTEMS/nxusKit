"""Pythonic CLIPS Session wrapper over the nxusKit C ABI.

Provides ClipsSession with context manager support and Pythonic naming.

Example::

    from nxuskit.clips import ClipsSession

    with ClipsSession() as s:
        s.load_json('''{"templates": [...], "rules": [...]}''')
        s.reset()
        s.fact_assert_string('(sensor (name "temp") (value 200))')
        fired = s.run()
        facts = s.facts_by_template("alert")
"""

from __future__ import annotations

import json as _json
from typing import Any, Optional

from ._clips_ffi import (
    ClipsLibraryNotFoundError,
    clips_ffi,
    last_error,
    read_and_free_string,
)

__all__ = [
    "ClipsSession",
    "ClipsError",
    "ClipsLibraryNotFoundError",
]


class ClipsError(RuntimeError):
    """Base error for CLIPS session operations."""


def _get_lib():
    from ._clips_ffi import _get_lib as _gl

    return _gl()


def _check_status(rc: int, fallback: str) -> None:
    """Raise ClipsError if a C ABI call returned non-zero."""
    if rc != 0:
        err = last_error()
        raise ClipsError(err if err else fallback)


def _check_count(n: int, fallback: str) -> int:
    """Raise ClipsError if a C ABI call returned negative."""
    if n < 0:
        err = last_error()
        raise ClipsError(err if err else fallback)
    return n


def _to_bytes(s: str) -> bytes:
    """Encode string to UTF-8 bytes for cffi."""
    return s.encode("utf-8")


# ── ClipsSession ──────────────────────────────────────────────


class ClipsSession:
    """CLIPS inference session with context manager support.

    Use as a context manager for automatic resource cleanup::

        with ClipsSession() as s:
            s.load_json(rules_json)
            s.reset()
            s.fact_assert_string('(data (key "x") (val 42))')
            fired = s.run()
    """

    def __init__(self) -> None:
        lib = _get_lib()
        self._handle: int = lib.nxuskit_clips_session_create()
        if self._handle == 0:
            err = last_error()
            raise ClipsError(f"failed to create CLIPS session: {err}")

    def __enter__(self) -> ClipsSession:
        return self

    def __exit__(self, *_: Any) -> None:
        self.close()

    def __del__(self) -> None:
        try:
            self.close()
        except Exception:
            pass

    def close(self) -> None:
        """Destroy session and free resources. Safe to call multiple times."""
        if self._handle != 0:
            _get_lib().nxuskit_clips_session_destroy(self._handle)
            self._handle = 0

    # ── Session Lifecycle ─────────────────────────────────────

    def reset(self) -> None:
        """Retract all facts and restore initial state, preserving rules."""
        _check_status(_get_lib().nxuskit_clips_session_reset(self._handle), "reset failed")

    def clear(self) -> None:
        """Remove all constructs (rules, templates, facts, modules)."""
        _check_status(_get_lib().nxuskit_clips_session_clear(self._handle), "clear failed")

    def info(self) -> dict:
        """Return session metadata as a dict."""
        ptr = _get_lib().nxuskit_clips_session_info(self._handle)
        return _json.loads(read_and_free_string(ptr))

    # ── Construct Loading ─────────────────────────────────────

    def load_file(self, path: str) -> None:
        """Load CLIPS constructs from a .clp file."""
        _check_status(
            _get_lib().nxuskit_clips_session_load_file(self._handle, _to_bytes(path)),
            "load_file failed",
        )

    def load_string(self, constructs: str) -> None:
        """Load CLIPS constructs from a string."""
        _check_status(
            _get_lib().nxuskit_clips_session_load_string(self._handle, _to_bytes(constructs)),
            "load_string failed",
        )

    def load_binary(self, path: str) -> None:
        """Load a CLIPS binary image."""
        _check_status(
            _get_lib().nxuskit_clips_session_load_binary(self._handle, _to_bytes(path)),
            "load_binary failed",
        )

    def save_binary(self, path: str) -> None:
        """Save the current session as a CLIPS binary image."""
        _check_status(
            _get_lib().nxuskit_clips_session_save_binary(self._handle, _to_bytes(path)),
            "save_binary failed",
        )

    def build(self, construct: str) -> None:
        """Load a single CLIPS construct string."""
        _check_status(
            _get_lib().nxuskit_clips_session_build(self._handle, _to_bytes(construct)),
            "build failed",
        )

    def load_json(self, json_str: str) -> None:
        """Load modules, templates, rules, and/or facts from a JSON definition."""
        _check_status(
            _get_lib().nxuskit_clips_session_load_json(self._handle, _to_bytes(json_str)),
            "load_json failed",
        )

    def batch(self, path: str) -> None:
        """Execute a CLIPS batch file."""
        _check_status(
            _get_lib().nxuskit_clips_session_batch(self._handle, _to_bytes(path)),
            "batch failed",
        )

    # ── Fact Operations ───────────────────────────────────────

    def fact_assert_string(self, fact_string: str) -> int:
        """Assert a fact from a CLIPS string. Returns the fact index."""
        idx = int(_get_lib().nxuskit_clips_fact_assert_string(self._handle, _to_bytes(fact_string)))
        return _check_count(idx, "fact_assert_string failed")

    def fact_assert_structured(self, template_name: str, slots: dict) -> int:
        """Assert a structured fact. Returns the fact index."""
        idx = int(
            _get_lib().nxuskit_clips_fact_assert_structured(
                self._handle,
                _to_bytes(template_name),
                _to_bytes(_json.dumps(slots)),
            )
        )
        return _check_count(idx, "fact_assert_structured failed")

    def fact_retract(self, fact_index: int) -> None:
        """Retract a fact by its index."""
        _check_status(
            _get_lib().nxuskit_clips_fact_retract(self._handle, fact_index),
            "fact_retract failed",
        )

    def fact_retract_by_template(self, template_name: str) -> None:
        """Retract all facts of a given template."""
        _check_status(
            _get_lib().nxuskit_clips_fact_retract_by_template(
                self._handle, _to_bytes(template_name)
            ),
            "fact_retract_by_template failed",
        )

    def fact_exists(self, fact_index: int) -> bool:
        """Check if a fact with the given index exists."""
        return bool(_get_lib().nxuskit_clips_fact_exists(self._handle, fact_index))

    def fact_get_slot(self, fact_index: int, slot_name: str) -> Any:
        """Return a single slot value (parsed from JSON)."""
        ptr = _get_lib().nxuskit_clips_fact_get_slot(self._handle, fact_index, _to_bytes(slot_name))
        return _json.loads(read_and_free_string(ptr))

    def fact_slot_values(self, fact_index: int) -> dict:
        """Return all slot values for a fact as a dict."""
        ptr = _get_lib().nxuskit_clips_fact_slot_values(self._handle, fact_index)
        return _json.loads(read_and_free_string(ptr))

    def fact_pp_form(self, fact_index: int) -> str:
        """Return the pretty-print form of a fact."""
        ptr = _get_lib().nxuskit_clips_fact_pp_form(self._handle, fact_index)
        return read_and_free_string(ptr)

    def fact_index(self, fact_index: int) -> int:
        """Return the index of a fact."""
        idx = int(_get_lib().nxuskit_clips_fact_index(self._handle, fact_index))
        return _check_count(idx, "fact_index failed")

    def facts_list(self) -> list[int]:
        """Return all fact indices."""
        ptr = _get_lib().nxuskit_clips_facts_list(self._handle)
        return _json.loads(read_and_free_string(ptr))

    def facts_by_template(self, template_name: str) -> list[int]:
        """Return fact indices for a specific template."""
        ptr = _get_lib().nxuskit_clips_facts_by_template(self._handle, _to_bytes(template_name))
        return _json.loads(read_and_free_string(ptr))

    # ── Template Operations ───────────────────────────────────

    def template_exists(self, name: str) -> bool:
        """Check if a template exists."""
        return bool(_get_lib().nxuskit_clips_template_exists(self._handle, _to_bytes(name)))

    def template_list(self) -> list[str]:
        """Return all template names."""
        ptr = _get_lib().nxuskit_clips_template_list(self._handle)
        return _json.loads(read_and_free_string(ptr))

    def template_slot_names(self, template_name: str) -> list[str]:
        """Return slot names for a template."""
        ptr = _get_lib().nxuskit_clips_template_slot_names(self._handle, _to_bytes(template_name))
        return _json.loads(read_and_free_string(ptr))

    def template_slot_info(self, template_name: str) -> list[dict]:
        """Return detailed slot information for a template."""
        ptr = _get_lib().nxuskit_clips_template_slot_info(self._handle, _to_bytes(template_name))
        return _json.loads(read_and_free_string(ptr))

    def template_facts(self, template_name: str) -> list[int]:
        """Return fact indices for a template."""
        ptr = _get_lib().nxuskit_clips_template_facts(self._handle, _to_bytes(template_name))
        return _json.loads(read_and_free_string(ptr))

    def template_pp_form(self, template_name: str) -> str:
        """Return the pretty-print form of a template."""
        ptr = _get_lib().nxuskit_clips_template_pp_form(self._handle, _to_bytes(template_name))
        return read_and_free_string(ptr)

    # ── Rule Operations ───────────────────────────────────────

    def rule_exists(self, name: str) -> bool:
        """Check if a rule exists."""
        return bool(_get_lib().nxuskit_clips_rule_exists(self._handle, _to_bytes(name)))

    def rule_list(self) -> list[str]:
        """Return all rule names."""
        ptr = _get_lib().nxuskit_clips_rule_list(self._handle)
        return _json.loads(read_and_free_string(ptr))

    def rule_times_fired(self, rule_name: str) -> int:
        """Return the number of times a rule has fired."""
        n = int(_get_lib().nxuskit_clips_rule_times_fired(self._handle, _to_bytes(rule_name)))
        return _check_count(n, "rule_times_fired failed")

    def rule_breakpoint_set(self, rule_name: str) -> None:
        """Set a breakpoint on a rule."""
        _check_status(
            _get_lib().nxuskit_clips_rule_breakpoint_set(self._handle, _to_bytes(rule_name)),
            "rule_breakpoint_set failed",
        )

    def rule_breakpoint_remove(self, rule_name: str) -> None:
        """Remove a breakpoint from a rule."""
        _check_status(
            _get_lib().nxuskit_clips_rule_breakpoint_remove(self._handle, _to_bytes(rule_name)),
            "rule_breakpoint_remove failed",
        )

    def rule_has_breakpoint(self, rule_name: str) -> bool:
        """Check if a rule has a breakpoint set."""
        return bool(
            _get_lib().nxuskit_clips_rule_has_breakpoint(self._handle, _to_bytes(rule_name))
        )

    def rule_refresh(self, rule_name: str) -> None:
        """Refresh a rule's activations."""
        _check_status(
            _get_lib().nxuskit_clips_rule_refresh(self._handle, _to_bytes(rule_name)),
            "rule_refresh failed",
        )

    def rule_pp_form(self, rule_name: str) -> str:
        """Return the pretty-print form of a rule."""
        ptr = _get_lib().nxuskit_clips_rule_pp_form(self._handle, _to_bytes(rule_name))
        return read_and_free_string(ptr)

    def rule_delete(self, rule_name: str) -> None:
        """Delete a rule."""
        _check_status(
            _get_lib().nxuskit_clips_rule_delete(self._handle, _to_bytes(rule_name)),
            "rule_delete failed",
        )

    # ── Execution & Agenda ────────────────────────────────────

    def run(self, limit: int = -1) -> int:
        """Run the inference engine. Returns the number of rules fired.

        Args:
            limit: Maximum number of rule firings. -1 for unlimited.
        """
        fired = int(_get_lib().nxuskit_clips_session_run(self._handle, limit))
        return _check_count(fired, "run failed")

    def halt(self) -> None:
        """Signal the inference engine to stop."""
        _check_status(_get_lib().nxuskit_clips_session_halt(self._handle), "halt failed")

    def agenda_size(self) -> int:
        """Return the number of activations on the agenda."""
        n = int(_get_lib().nxuskit_clips_agenda_size(self._handle))
        return _check_count(n, "agenda_size failed")

    def agenda_clear(self) -> None:
        """Clear all activations from the agenda."""
        _check_status(_get_lib().nxuskit_clips_agenda_clear(self._handle), "agenda_clear failed")

    def agenda_reorder(self) -> None:
        """Reorder agenda activations."""
        _check_status(
            _get_lib().nxuskit_clips_agenda_reorder(self._handle),
            "agenda_reorder failed",
        )

    def strategy_get(self) -> str:
        """Return the current conflict resolution strategy."""
        ptr = _get_lib().nxuskit_clips_strategy_get(self._handle)
        return read_and_free_string(ptr)

    def strategy_set(self, strategy: str) -> None:
        """Set the conflict resolution strategy."""
        _check_status(
            _get_lib().nxuskit_clips_strategy_set(self._handle, _to_bytes(strategy)),
            "strategy_set failed",
        )

    def salience_mode_get(self) -> str:
        """Return the current salience evaluation mode."""
        ptr = _get_lib().nxuskit_clips_salience_mode_get(self._handle)
        return read_and_free_string(ptr)

    def salience_mode_set(self, mode: str) -> None:
        """Set the salience evaluation mode."""
        _check_status(
            _get_lib().nxuskit_clips_salience_mode_set(self._handle, _to_bytes(mode)),
            "salience_mode_set failed",
        )

    # ── Module & Focus Stack ──────────────────────────────────

    def module_exists(self, name: str) -> bool:
        """Check if a module exists."""
        return bool(_get_lib().nxuskit_clips_module_exists(self._handle, _to_bytes(name)))

    def module_list(self) -> list[str]:
        """Return all module names."""
        ptr = _get_lib().nxuskit_clips_module_list(self._handle)
        return _json.loads(read_and_free_string(ptr))

    def module_current_get(self) -> str:
        """Return the current module name."""
        ptr = _get_lib().nxuskit_clips_module_current_get(self._handle)
        return read_and_free_string(ptr)

    def module_current_set(self, name: str) -> None:
        """Set the current module."""
        _check_status(
            _get_lib().nxuskit_clips_module_current_set(self._handle, _to_bytes(name)),
            "module_current_set failed",
        )

    def focus_push(self, module_name: str) -> None:
        """Push a module onto the focus stack."""
        _check_status(
            _get_lib().nxuskit_clips_focus_push(self._handle, _to_bytes(module_name)),
            "focus_push failed",
        )

    def focus_get(self) -> Optional[str]:
        """Return the module at the top of the focus stack, or None if empty."""
        lib = _get_lib()
        ptr = lib.nxuskit_clips_focus_get(self._handle)
        if ptr == clips_ffi.NULL:
            return None
        s = clips_ffi.string(ptr).decode("utf-8")
        lib.nxuskit_free_string(ptr)
        return s

    def focus_pop(self) -> None:
        """Pop the top module from the focus stack."""
        _check_status(_get_lib().nxuskit_clips_focus_pop(self._handle), "focus_pop failed")

    def focus_clear(self) -> None:
        """Clear the focus stack."""
        _check_status(_get_lib().nxuskit_clips_focus_clear(self._handle), "focus_clear failed")

    # ── Global Variables ──────────────────────────────────────

    def global_exists(self, name: str) -> bool:
        """Check if a global variable exists."""
        return bool(_get_lib().nxuskit_clips_global_exists(self._handle, _to_bytes(name)))

    def global_list(self) -> list[str]:
        """Return all global variable names."""
        ptr = _get_lib().nxuskit_clips_global_list(self._handle)
        return _json.loads(read_and_free_string(ptr))

    def global_get_value(self, name: str) -> Any:
        """Return the value of a global variable (parsed from JSON)."""
        ptr = _get_lib().nxuskit_clips_global_get_value(self._handle, _to_bytes(name))
        return _json.loads(read_and_free_string(ptr))

    def global_set_value(self, name: str, value_json: str) -> None:
        """Set the value of a global variable from a JSON value string."""
        _check_status(
            _get_lib().nxuskit_clips_global_set_value(
                self._handle, _to_bytes(name), _to_bytes(value_json)
            ),
            "global_set_value failed",
        )

    # ── Expression Evaluation ─────────────────────────────────

    def eval(self, expression: str) -> Any:
        """Evaluate a CLIPS expression. Returns the result (parsed from JSON)."""
        ptr = _get_lib().nxuskit_clips_eval(self._handle, _to_bytes(expression))
        return _json.loads(read_and_free_string(ptr))

    def function_call(self, function_name: str, args_json: str) -> Any:
        """Call a CLIPS function. Returns the result (parsed from JSON)."""
        ptr = _get_lib().nxuskit_clips_function_call(
            self._handle, _to_bytes(function_name), _to_bytes(args_json)
        )
        return _json.loads(read_and_free_string(ptr))

    # ── Watch & Diagnostics ───────────────────────────────────

    def watch(self, item: str) -> None:
        """Enable watching for an item (e.g., "facts", "rules", "activations")."""
        _check_status(
            _get_lib().nxuskit_clips_watch(self._handle, _to_bytes(item)),
            "watch failed",
        )

    def unwatch(self, item: str) -> None:
        """Disable watching for an item."""
        _check_status(
            _get_lib().nxuskit_clips_unwatch(self._handle, _to_bytes(item)),
            "unwatch failed",
        )

    def dribble_on(self, path: str) -> None:
        """Start recording all CLIPS output to a file."""
        _check_status(
            _get_lib().nxuskit_clips_dribble_on(self._handle, _to_bytes(path)),
            "dribble_on failed",
        )

    def dribble_off(self) -> None:
        """Stop recording CLIPS output."""
        _check_status(_get_lib().nxuskit_clips_dribble_off(self._handle), "dribble_off failed")

    # ── Settings ──────────────────────────────────────────────

    def fact_duplication_get(self) -> bool:
        """Return whether fact duplication is allowed."""
        return bool(_get_lib().nxuskit_clips_fact_duplication_get(self._handle))

    def fact_duplication_set(self, allow: bool) -> None:
        """Set whether fact duplication is allowed."""
        _check_status(
            _get_lib().nxuskit_clips_fact_duplication_set(self._handle, allow),
            "fact_duplication_set failed",
        )

    def reset_globals_get(self) -> bool:
        """Return whether globals are reset on session reset."""
        return bool(_get_lib().nxuskit_clips_reset_globals_get(self._handle))

    def reset_globals_set(self, reset: bool) -> None:
        """Set whether globals are reset on session reset."""
        _check_status(
            _get_lib().nxuskit_clips_reset_globals_set(self._handle, reset),
            "reset_globals_set failed",
        )

    # ── Session Cache (class methods) ─────────────────────────

    @staticmethod
    def preload(name: str, rules_json: str) -> None:
        """Preload a named session with rules configuration JSON."""
        _check_status(
            _get_lib().nxuskit_clips_session_preload(_to_bytes(name), _to_bytes(rules_json)),
            "preload failed",
        )

    @staticmethod
    def get_cached(name: str) -> ClipsSession:
        """Retrieve an independent clone of a cached session."""
        lib = _get_lib()
        h = lib.nxuskit_clips_session_get_cached(_to_bytes(name))
        if h == 0:
            err = last_error()
            raise ClipsError(f"get_cached failed: {err}")
        session = object.__new__(ClipsSession)
        session._handle = h
        return session

    @staticmethod
    def cache_remove(name: str) -> None:
        """Remove a cached session by name."""
        _check_status(
            _get_lib().nxuskit_clips_session_cache_remove(_to_bytes(name)),
            "cache_remove failed",
        )
