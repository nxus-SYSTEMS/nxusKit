"""Low-level cffi bindings for the Bayesian Network C ABI.

Uses the shared FFI instance and library handle from _ffi.py.
Higher-level Python wrappers are in bn.py.
"""

from __future__ import annotations


class BnLibraryNotFoundError(RuntimeError):
    """Raised when the nxuskit native library cannot be found."""


# Lazy reference — populated on first _get_lib() call.
_bn_lib = None


def _get_lib():
    """Get the shared library handle for BN functions."""
    global _bn_lib
    if _bn_lib is not None:
        return _bn_lib
    try:
        from nxuskit._ffi import lib

        _bn_lib = lib
        return _bn_lib
    except Exception as e:
        raise BnLibraryNotFoundError(
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
        raise RuntimeError(f"nxuskit bn: NULL string returned: {err}")
    s = ffi.string(ptr).decode("utf-8")
    lib.nxuskit_free_string(ptr)
    return s


# Backward compat: code that imported bn_ffi directly can still use it
# for callback definitions. Point to the shared ffi instance.
try:
    from nxuskit._ffi import ffi as bn_ffi
except Exception:
    bn_ffi = None  # type: ignore[assignment]
