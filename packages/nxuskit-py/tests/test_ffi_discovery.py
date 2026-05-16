"""Tests for FFI library discovery and deprecation warnings (US4, T047-T049).

These tests verify _find_library() deprecation warning behavior for
NXUSKIT_LIB_PATH without loading the native library. We extract the
function from source to avoid the module-level ffi.dlopen() call that
fails in CI where libnxuskit is not present.
"""

import os
import sys
import types
import warnings
from pathlib import Path
from unittest import mock


def _get_discovery_helpers():
    """Extract library discovery helpers from _ffi.py without triggering dlopen.

    Compiles the module source up to (but not including) the module-level
    library loading section, then returns the discovery helpers.
    """
    ffi_path = Path(__file__).resolve().parent.parent / "src" / "nxuskit" / "_ffi.py"
    source = ffi_path.read_text()

    # Truncate before the module-level load section to avoid dlopen
    marker = "# ── Load library"
    idx = source.find(marker)
    if idx == -1:
        raise RuntimeError("Cannot find load-library marker in _ffi.py")
    truncated = source[:idx]

    # Compile and exec in a sandboxed module namespace
    mod = types.ModuleType("_ffi_sandbox")
    mod.__file__ = str(ffi_path)
    # Provide required dependencies
    mod.__builtins__ = __builtins__

    # Ensure nxuskit._ffi_errors is importable (it has no native dependency)
    if "nxuskit._ffi_errors" not in sys.modules:
        import nxuskit._ffi_errors  # noqa: F401

    exec(compile(truncated, str(ffi_path), "exec"), mod.__dict__)  # noqa: S102
    return mod._find_library, mod._lib_name


def _create_dummy_library(tmp_path):
    """Create a dummy library file with the current platform's expected name."""
    _, _lib_name = _get_discovery_helpers()
    dummy_lib = tmp_path / _lib_name()
    dummy_lib.touch()
    return dummy_lib


# ── T047: NXUSKIT_LIB_PATH emits DeprecationWarning ─────────────────


def test_nxuskit_lib_path_deprecation_warning(tmp_path):
    """Setting NXUSKIT_LIB_PATH should emit a DeprecationWarning."""
    dummy_lib = _create_dummy_library(tmp_path)
    _find_library, _ = _get_discovery_helpers()

    env = {
        "NXUSKIT_LIB_PATH": str(dummy_lib),
    }
    # Clear higher-priority vars
    with mock.patch.dict(os.environ, env, clear=False):
        os.environ.pop("NXUSKIT_LIB_DIR", None)
        os.environ.pop("NXUSKIT_SDK_DIR", None)

        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            result = _find_library()

        deprecation_warnings = [x for x in w if issubclass(x.category, DeprecationWarning)]
        assert len(deprecation_warnings) >= 1, (
            f"Expected DeprecationWarning, got: {[x.category.__name__ for x in w]}"
        )
        assert "NXUSKIT_LIB_DIR" in str(deprecation_warnings[0].message)
        assert result == str(dummy_lib)


# ── T048: No deprecation when NXUSKIT_LIB_DIR is set ────────────────


def test_no_deprecation_when_lib_dir_set(tmp_path):
    """When NXUSKIT_LIB_DIR is set and valid, no DeprecationWarning should be emitted."""
    dummy_lib = _create_dummy_library(tmp_path)
    _find_library, _ = _get_discovery_helpers()

    env = {
        "NXUSKIT_LIB_DIR": str(tmp_path),
        "NXUSKIT_LIB_PATH": str(dummy_lib),  # Also set, but should not trigger
    }
    with mock.patch.dict(os.environ, env, clear=False):
        os.environ.pop("NXUSKIT_SDK_DIR", None)

        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            result = _find_library()

        deprecation_warnings = [x for x in w if issubclass(x.category, DeprecationWarning)]
        assert len(deprecation_warnings) == 0, (
            f"Expected no DeprecationWarning, got: {deprecation_warnings}"
        )
        assert result == str(dummy_lib)


# ── T049: No deprecation when only NXUSKIT_LIB_DIR is set ───────────


def test_no_deprecation_when_only_lib_dir(tmp_path):
    """When only NXUSKIT_LIB_DIR is set, no DeprecationWarning should be emitted."""
    dummy_lib = _create_dummy_library(tmp_path)
    _find_library, _ = _get_discovery_helpers()

    env = {
        "NXUSKIT_LIB_DIR": str(tmp_path),
    }
    with mock.patch.dict(os.environ, env, clear=False):
        os.environ.pop("NXUSKIT_LIB_PATH", None)
        os.environ.pop("NXUSKIT_SDK_DIR", None)

        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            result = _find_library()

        deprecation_warnings = [x for x in w if issubclass(x.category, DeprecationWarning)]
        assert len(deprecation_warnings) == 0, (
            f"Expected no DeprecationWarning, got: {deprecation_warnings}"
        )
        assert result == str(dummy_lib)
