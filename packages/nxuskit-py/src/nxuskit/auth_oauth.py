"""OAuth authentication for the nxusKit SDK.

Provides browser-based OAuth flow with PKCE and state/CSRF validation
for providers that support it.
"""

from __future__ import annotations

import json
from dataclasses import dataclass
from typing import List, Optional

from nxuskit._ffi import ffi, last_error, lib


@dataclass
class OAuthResult:
    """Result of an OAuth authentication flow."""

    success: bool
    """Whether the OAuth flow completed successfully"""
    provider_id: str
    """Provider that was authenticated"""
    message: str
    """Human-readable status message"""
    error: Optional[str]
    """Error message if the flow failed"""


@dataclass
class OAuthStatus:
    """OAuth authentication status for a provider."""

    authenticated: bool
    """Whether an OAuth credential is stored"""
    provider_id: str
    """Provider identifier"""
    expires_at: Optional[int]
    """Unix timestamp when the token expires (None if unknown)"""
    scopes: Optional[List[str]]
    """Scopes granted by the OAuth token (None if unknown)"""


def oauth_start(provider_id: str, timeout_secs: int = 0) -> OAuthResult:
    """Start an OAuth authentication flow for a provider.

    This is a **blocking** call — it launches a browser, starts a localhost
    callback server, and waits for the authorization code.

    Args:
        provider_id: Provider to authenticate (e.g., "azure-openai").
        timeout_secs: Max seconds to wait for callback (0 = default 120s).

    Returns:
        OAuth flow result.

    Raises:
        RuntimeError: If the C ABI call fails.
    """
    c_pid = ffi.new("char[]", provider_id.encode("utf-8"))
    ptr = lib.nxuskit_oauth_start(c_pid, timeout_secs)

    if ptr == ffi.NULL:
        err = last_error() or "unknown error"
        raise RuntimeError(f"nxuskit_oauth_start failed: {err}")

    try:
        raw = json.loads(ffi.string(ptr).decode("utf-8"))
    finally:
        lib.nxuskit_free_string(ptr)

    return OAuthResult(
        success=raw.get("success", False),
        provider_id=raw["provider_id"],
        message=raw["message"],
        error=raw.get("error"),
    )


def oauth_status(provider_id: str) -> OAuthStatus:
    """Check OAuth authentication status for a provider.

    Args:
        provider_id: Provider to check.

    Returns:
        OAuth status including whether authenticated.

    Raises:
        RuntimeError: If the C ABI call fails.
    """
    c_pid = ffi.new("char[]", provider_id.encode("utf-8"))
    ptr = lib.nxuskit_oauth_status(c_pid)

    if ptr == ffi.NULL:
        err = last_error() or "unknown error"
        raise RuntimeError(f"nxuskit_oauth_status failed: {err}")

    try:
        raw = json.loads(ffi.string(ptr).decode("utf-8"))
    finally:
        lib.nxuskit_free_string(ptr)

    return OAuthStatus(
        authenticated=raw["authenticated"],
        provider_id=raw["provider_id"],
        expires_at=raw.get("expires_at"),
        scopes=raw.get("scopes"),
    )


def oauth_revoke(provider_id: str) -> None:
    """Remove the stored OAuth token for a provider.

    Args:
        provider_id: Provider whose OAuth token to remove.

    Raises:
        RuntimeError: If the revocation fails.
    """
    c_pid = ffi.new("char[]", provider_id.encode("utf-8"))
    result = lib.nxuskit_oauth_revoke(c_pid)
    if result < 0:
        err = last_error() or "unknown error"
        raise RuntimeError(f"OAuth revoke failed: {err}")
