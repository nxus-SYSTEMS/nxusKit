"""Exception types for the nxuskit FFI layer.

These are the FFI-specific error types from the contract. They coexist with
the native nxuskit error types in errors.py.
"""

from __future__ import annotations


class NxuskitError(Exception):
    """Base exception for all nxuskit FFI errors."""

    def __init__(
        self,
        message: str,
        error_type: str = "internal",
        provider: str | None = None,
        feature: str | None = None,
    ):
        super().__init__(message)
        self.message = message
        self.error_type = error_type
        self.provider = provider
        self.feature = feature


class ConfigError(NxuskitError):
    """Raised for configuration errors (missing API key, version mismatch, etc.)."""

    def __init__(self, message: str, provider: str | None = None):
        super().__init__(message, error_type="configuration", provider=provider)


class ProviderError(NxuskitError):
    """Raised for provider-side errors (API failures, model not found, etc.)."""

    def __init__(self, message: str, provider: str | None = None):
        super().__init__(message, error_type="provider", provider=provider)


class TimeoutError(NxuskitError):
    """Raised when a request times out."""

    def __init__(self, message: str, provider: str | None = None):
        super().__init__(message, error_type="timeout", provider=provider)


class FeatureUnavailableError(NxuskitError):
    """Raised when a feature is not available in the current edition."""

    def __init__(self, message: str, feature: str | None = None):
        super().__init__(message, error_type="feature_unavailable", feature=feature)


class LicenseRequiredError(NxuskitError):
    """Raised when a valid license key is required but not provided."""

    def __init__(self, message: str, feature: str | None = None):
        super().__init__(message, error_type="license_required", feature=feature)


class LicenseExpiredError(NxuskitError):
    """Raised when the provided license key has expired."""

    def __init__(self, message: str, feature: str | None = None):
        super().__init__(message, error_type="license_expired", feature=feature)


class EditionInsufficientError(NxuskitError):
    """Raised when the current edition lacks required capabilities."""

    def __init__(
        self,
        message: str,
        feature: str | None = None,
        required_edition: str | None = None,
    ):
        super().__init__(message, error_type="edition_insufficient", feature=feature)
        self.required_edition = required_edition
