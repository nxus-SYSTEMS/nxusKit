"""Low-level cffi bindings for the CLIPS Session C ABI.

Uses the shared FFI instance and library handle from _ffi.py.
Higher-level Python wrappers are in clips.py.
"""

from __future__ import annotations


class ClipsLibraryNotFoundError(RuntimeError):
    """Raised when the nxuskit native library cannot be found."""


_clips_lib = None


def _get_lib():
    """Get the shared library handle for CLIPS functions."""
    global _clips_lib
    if _clips_lib is not None:
        return _clips_lib
    try:
        from nxuskit._ffi import lib

        _clips_lib = lib
        return _clips_lib
    except Exception as e:
        raise ClipsLibraryNotFoundError(
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
        raise RuntimeError(f"nxuskit clips: NULL string returned: {err}")
    s = ffi.string(ptr).decode("utf-8")
    lib.nxuskit_free_string(ptr)
    return s


try:
    from nxuskit._ffi import ffi as clips_ffi
except Exception:
    clips_ffi = None  # type: ignore[assignment]
