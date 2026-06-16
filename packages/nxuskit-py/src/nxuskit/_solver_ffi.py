"""Low-level cffi bindings for the constraint solver C ABI.

Uses the shared FFI instance and library handle from _ffi.py.
Higher-level Python wrappers are in solver.py.
"""

from __future__ import annotations


class SolverLibraryNotFoundError(RuntimeError):
    """Raised when the nxuskit native library cannot be found."""


_solver_lib = None


def _get_lib():
    """Get the shared library handle for Solver functions."""
    global _solver_lib
    if _solver_lib is not None:
        return _solver_lib
    try:
        from nxuskit._ffi import lib

        _solver_lib = lib
        return _solver_lib
    except Exception as e:
        raise SolverLibraryNotFoundError(
            f"Failed to load nxuskit native library: {e}. "
            "Set NXUSKIT_LIB_DIR, NXUSKIT_SDK_DIR, or install at ~/.nxuskit/sdk/current/."
        ) from e


def last_error() -> str:
    """Read the thread-local error message from the C ABI."""
    from nxuskit._ffi import ffi

    lib = _get_lib()
    ptr = lib.nxuskit_last_error()
    if ptr == ffi.NULL:
        return ""
    return ffi.string(ptr).decode("utf-8", errors="replace")


def read_and_free_string(ptr) -> str:
    """Convert a C string to Python, free the C memory."""
    from nxuskit._ffi import ffi

    lib = _get_lib()
    if ptr == ffi.NULL:
        err = last_error()
        raise RuntimeError(f"nxuskit solver: NULL string returned: {err}")
    s = ffi.string(ptr).decode("utf-8")
    lib.nxuskit_free_string(ptr)
    return s


try:
    from nxuskit._ffi import ffi as solver_ffi
except Exception:
    solver_ffi = None  # type: ignore[assignment]
