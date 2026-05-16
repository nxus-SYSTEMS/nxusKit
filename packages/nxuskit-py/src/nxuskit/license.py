"""License management for the nxusKit SDK.

Safe Python wrappers over the C ABI license functions: token resolution,
validation, and machine fingerprinting.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from typing import Dict, List, Optional

from nxuskit._ffi import ffi, last_error, lib


@dataclass
class LicenseResolution:
    """Token resolution result from the license precedence chain."""

    source: str
    """Where the token was found: 'env_var', 'file', 'api_param', or 'none'"""
    token_type: str
    """Token type: 'trial', 'developer', 'deployment', or 'none'"""
    valid: bool
    """Whether the token passed validation"""
    error: Optional[str]
    """Error message if validation failed"""
    product_id: Optional[str] = field(default=None)
    """Product identifier (e.g., 'nxuskit')"""
    effective_limits: Optional[Dict] = field(default=None)
    """Resolved numerical limits (catalog defaults + token overrides)"""
    features: Optional[List[str]] = field(default=None)
    """Effective feature list for the resolved edition"""


@dataclass
class TokenInfo:
    """Token validation result."""

    valid: bool
    """Whether the token is valid"""
    token_type: str
    """Token type: 'trial', 'developer', 'deployment'"""
    edition: Optional[str]
    """Edition granted by the token"""
    days_remaining: Optional[int]
    """Days until token expiry (None for deployment tokens)"""
    error: Optional[str]
    """Error message if validation failed"""
    result: str
    """Entitlement result code"""


def license_resolve(explicit_key: Optional[str] = None) -> LicenseResolution:
    """Resolve the active license token from all available sources.

    Resolution order:
        1. ``NXUSKIT_LICENSE_TOKEN`` environment variable
        2. ``~/.nxuskit/license.token`` file
        3. ``explicit_key`` parameter (if provided)

    Args:
        explicit_key: Optional explicit license key (lowest priority).

    Returns:
        Resolution result including source, token type, and validity.

    Raises:
        RuntimeError: If the C ABI call fails.
    """
    if explicit_key is not None:
        c_key = ffi.new("char[]", explicit_key.encode("utf-8"))
        ptr = lib.nxuskit_license_resolve(c_key)
    else:
        ptr = lib.nxuskit_license_resolve(ffi.NULL)

    if ptr == ffi.NULL:
        err = last_error() or "unknown error"
        raise RuntimeError(f"nxuskit_license_resolve failed: {err}")

    try:
        raw = json.loads(ffi.string(ptr).decode("utf-8"))
    finally:
        lib.nxuskit_free_string(ptr)

    return LicenseResolution(
        source=raw["source"],
        token_type=raw["token_type"],
        valid=raw["valid"],
        error=raw.get("error"),
    )


def license_validate(token: str) -> TokenInfo:
    """Validate a license token JWT string.

    Performs RS384 signature verification, claim parsing, and type-specific
    validation (expiry, machine binding, version ceiling).

    Args:
        token: The JWT token string to validate.

    Returns:
        Validation result including token type, edition, and expiry info.

    Raises:
        RuntimeError: If the C ABI call fails.
    """
    c_token = ffi.new("char[]", token.encode("utf-8"))
    ptr = lib.nxuskit_license_validate(c_token)

    if ptr == ffi.NULL:
        err = last_error() or "unknown error"
        raise RuntimeError(f"nxuskit_license_validate failed: {err}")

    try:
        raw = json.loads(ffi.string(ptr).decode("utf-8"))
    finally:
        lib.nxuskit_free_string(ptr)

    return TokenInfo(
        valid=raw["valid"],
        token_type=raw["token_type"],
        edition=raw.get("edition"),
        days_remaining=raw.get("days_remaining"),
        error=raw.get("error"),
        result=raw["result"],
    )


def license_machine_id() -> str:
    """Get the machine fingerprint for this device.

    Returns a ``sha256:<64-hex-chars>`` string derived from the OS machine ID.

    Returns:
        The machine fingerprint string.

    Raises:
        RuntimeError: If the machine ID cannot be determined
            (e.g., in Docker containers or minimal environments).
    """
    ptr = lib.nxuskit_license_machine_id()
    if ptr == ffi.NULL:
        err = last_error() or "machine ID unavailable"
        raise RuntimeError(f"license_machine_id failed: {err}")

    try:
        return ffi.string(ptr).decode("utf-8")
    finally:
        lib.nxuskit_free_string(ptr)


@dataclass
class ActivationResult:
    """Result of activating or deactivating a Pro license."""

    success: bool
    """Whether the operation succeeded"""
    seats_used: int
    """Number of machines currently using this license"""
    seats_total: int
    """Maximum number of machines allowed"""
    message: str
    """Human-readable status message"""
    error: Optional[str]
    """Error message if the operation failed"""


@dataclass
class TrialResult:
    """Result of a trial issuance or activation."""

    success: bool
    """Whether the operation succeeded"""
    days_remaining: int
    """Days until trial expiry"""
    message: str
    """Human-readable status message"""
    error: Optional[str]
    """Error message if the operation failed"""


def license_activate(purchase_id: str) -> ActivationResult:
    """Activate a Pro license on this machine.

    Calls the licensing microservice to validate the purchase ID, generate
    a machine-bound developer token, and store it locally.

    Args:
        purchase_id: The purchase ID received after buying Pro.

    Returns:
        Activation result including seat count and status.

    Raises:
        RuntimeError: If the C ABI call fails.
    """
    c_pid = ffi.new("char[]", purchase_id.encode("utf-8"))
    ptr = lib.nxuskit_license_activate(c_pid)

    if ptr == ffi.NULL:
        err = last_error() or "unknown error"
        raise RuntimeError(f"nxuskit_license_activate failed: {err}")

    try:
        raw = json.loads(ffi.string(ptr).decode("utf-8"))
    finally:
        lib.nxuskit_free_string(ptr)

    return ActivationResult(
        success=raw.get("success", True),
        seats_used=raw["seats_used"],
        seats_total=raw["seats_total"],
        message=raw["message"],
        error=raw.get("error"),
    )


def license_deactivate() -> ActivationResult:
    """Deactivate the Pro license on this machine.

    Releases this machine's seat and removes the stored token.

    Returns:
        Deactivation result including updated seat count.

    Raises:
        RuntimeError: If the C ABI call fails.
    """
    ptr = lib.nxuskit_license_deactivate()

    if ptr == ffi.NULL:
        err = last_error() or "unknown error"
        raise RuntimeError(f"nxuskit_license_deactivate failed: {err}")

    try:
        raw = json.loads(ffi.string(ptr).decode("utf-8"))
    finally:
        lib.nxuskit_free_string(ptr)

    return ActivationResult(
        success=raw.get("success", True),
        seats_used=raw["seats_used"],
        seats_total=raw["seats_total"],
        message=raw["message"],
        error=raw.get("error"),
    )


def license_trial_issue() -> TrialResult:
    """Issue a 30-day trial token for this machine.

    Returns:
        Trial issuance result including days remaining.

    Raises:
        RuntimeError: If the C ABI call fails.
    """
    ptr = lib.nxuskit_license_trial_issue()

    if ptr == ffi.NULL:
        err = last_error() or "unknown error"
        raise RuntimeError(f"nxuskit_license_trial_issue failed: {err}")

    try:
        raw = json.loads(ffi.string(ptr).decode("utf-8"))
    finally:
        lib.nxuskit_free_string(ptr)

    return TrialResult(
        success=raw.get("success", True),
        days_remaining=raw["days_remaining"],
        message=raw["message"],
        error=raw.get("error"),
    )


def license_trial_activate(code: str) -> TrialResult:
    """Activate a trial token (complete email verification).

    Args:
        code: The activation code received via email.

    Returns:
        Trial activation result.

    Raises:
        RuntimeError: If the C ABI call fails.
    """
    c_code = ffi.new("char[]", code.encode("utf-8"))
    ptr = lib.nxuskit_license_trial_activate(c_code)

    if ptr == ffi.NULL:
        err = last_error() or "unknown error"
        raise RuntimeError(f"nxuskit_license_trial_activate failed: {err}")

    try:
        raw = json.loads(ffi.string(ptr).decode("utf-8"))
    finally:
        lib.nxuskit_free_string(ptr)

    return TrialResult(
        success=raw.get("success", True),
        days_remaining=raw["days_remaining"],
        message=raw["message"],
        error=raw.get("error"),
    )
