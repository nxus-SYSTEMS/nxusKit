"""Plugin trust mode management for the nxusKit SDK.

Controls whether unsigned plugins are allowed to load. When set to
AllowUnsigned, unsigned plugins will be loaded but structured audit
events are emitted for every unsigned load attempt.
"""

from __future__ import annotations

from enum import IntEnum

from nxuskit._ffi import ffi, last_error, lib


class TrustMode(IntEnum):
    """Plugin trust policy."""

    SIGNED_ONLY = 0
    """Only cryptographically signed plugins are loaded (default)."""

    ALLOW_UNSIGNED = 1
    """Unsigned plugins are loaded but emit audit events."""


def set_plugin_trust_mode(mode: TrustMode) -> None:
    """Set the global plugin trust mode.

    Args:
        mode: The trust mode to set.

    Raises:
        ValueError: If the mode is invalid.
    """
    result = lib.nxuskit_plugin_set_trust_mode(int(mode))
    if result < 0:
        err = last_error() or "unknown error"
        raise ValueError(f"Invalid trust mode: {err}")


def get_plugin_trust_mode() -> TrustMode:
    """Get the current plugin trust mode.

    Returns:
        The current trust mode.
    """
    return TrustMode(lib.nxuskit_plugin_get_trust_mode())


def load_plugins_trusted(dir: str) -> int:
    """Load plugins from a directory using the current trust mode.

    Args:
        dir: Path to the plugin directory.

    Returns:
        Number of plugins successfully loaded.

    Raises:
        RuntimeError: If the plugin load fails.
    """
    c_dir = ffi.new("char[]", dir.encode("utf-8"))
    result = lib.nxuskit_plugin_load_dir_trusted(c_dir)
    if result < 0:
        err = last_error() or "unknown error"
        raise RuntimeError(f"Plugin load failed: {err}")
    return result


def plugin_list() -> list[str]:
    """List all loaded plugin names.

    Returns:
        List of plugin name strings.
    """
    import json

    ptr = lib.nxuskit_plugin_list()
    if ptr == ffi.NULL:
        err = last_error()
        raise RuntimeError(f"plugin_list failed: {err}")
    s = ffi.string(ptr).decode("utf-8")
    lib.nxuskit_free_string(ptr)
    return json.loads(s)


def plugin_info(name: str) -> dict:
    """Get metadata for a loaded plugin.

    Args:
        name: Plugin name.

    Returns:
        Dict with plugin metadata (name, version, capabilities, etc.).
    """
    import json

    c_name = ffi.new("char[]", name.encode("utf-8"))
    ptr = lib.nxuskit_plugin_info(c_name)
    if ptr == ffi.NULL:
        err = last_error()
        raise RuntimeError(f"plugin_info failed for '{name}': {err}")
    s = ffi.string(ptr).decode("utf-8")
    lib.nxuskit_free_string(ptr)
    return json.loads(s)


def plugin_count() -> int:
    """Get the number of loaded plugins."""
    return int(lib.nxuskit_plugin_count())


def plugin_loaded(name: str) -> bool:
    """Check if a plugin is loaded.

    Args:
        name: Plugin name.

    Returns:
        True if the plugin is loaded.
    """
    c_name = ffi.new("char[]", name.encode("utf-8"))
    return bool(lib.nxuskit_plugin_loaded(c_name))


def unload_all_plugins() -> None:
    """Unload all loaded plugins."""
    lib.nxuskit_plugin_unload_all()
