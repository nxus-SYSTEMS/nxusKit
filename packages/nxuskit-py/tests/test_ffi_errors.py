"""Tests for FFI error handling.

These tests verify the error hierarchy and behavior of the FFI layer.
Some tests require libnxuskit, others test pure Python error classes.

Run with: pytest tests/test_ffi_errors.py -v
"""

import json

import pytest

from nxuskit._ffi_errors import (
    ConfigError,
    EditionInsufficientError,
    FeatureUnavailableError,
    LicenseExpiredError,
    LicenseRequiredError,
    NxuskitError,
    ProviderError,
    TimeoutError,
)

# Guard for tests that need the library
try:
    from nxuskit._ffi_provider import create_ffi_provider

    HAS_NXUSKIT = True
except (OSError, Exception):
    HAS_NXUSKIT = False

# _parse_nxuskit_error is pure Python — import separately so tests run
# even when the C library is not available.
try:
    from nxuskit._ffi_provider import _parse_nxuskit_error as _parse_err

    HAS_PARSE = True
except (OSError, Exception):
    HAS_PARSE = False


class TestErrorHierarchy:
    """Test the FFI error class hierarchy (no library needed)."""

    def test_config_error_is_nxuskit_error(self):
        err = ConfigError("test")
        assert isinstance(err, NxuskitError)
        assert isinstance(err, Exception)

    def test_provider_error_is_nxuskit_error(self):
        err = ProviderError("test")
        assert isinstance(err, NxuskitError)

    def test_timeout_error_is_nxuskit_error(self):
        err = TimeoutError("test")
        assert isinstance(err, NxuskitError)

    def test_config_error_has_fields(self):
        err = ConfigError("bad config", provider="openai")
        assert err.message == "bad config"
        assert err.error_type == "configuration"
        assert err.provider == "openai"

    def test_provider_error_has_fields(self):
        err = ProviderError("api failed", provider="claude")
        assert err.message == "api failed"
        assert err.error_type == "provider"
        assert err.provider == "claude"

    def test_timeout_error_has_fields(self):
        err = TimeoutError("request timed out", provider="groq")
        assert err.message == "request timed out"
        assert err.error_type == "timeout"
        assert err.provider == "groq"

    def test_nxuskit_error_str(self):
        err = NxuskitError("test message")
        assert str(err) == "test message"


class TestEntitlementErrorHierarchy:
    """Test the entitlement error class hierarchy (no library needed)."""

    def test_license_required_error_is_nxuskit_error(self):
        err = LicenseRequiredError("license required", feature="solver")
        assert isinstance(err, NxuskitError)
        assert err.error_type == "license_required"
        assert err.feature == "solver"

    def test_license_expired_error_is_nxuskit_error(self):
        err = LicenseExpiredError("license expired", feature="zen")
        assert isinstance(err, NxuskitError)
        assert err.error_type == "license_expired"
        assert err.feature == "zen"

    def test_edition_insufficient_error_has_required_edition(self):
        err = EditionInsufficientError("need pro", feature="solver", required_edition="pro")
        assert isinstance(err, NxuskitError)
        assert err.error_type == "edition_insufficient"
        assert err.required_edition == "pro"
        assert err.feature == "solver"

    def test_feature_unavailable_error_is_nxuskit_error(self):
        err = FeatureUnavailableError("feature unavailable", feature="clips")
        assert isinstance(err, NxuskitError)
        assert err.error_type == "feature_unavailable"
        assert err.feature == "clips"

    def test_entitlement_errors_are_catchable_as_exception(self):
        for cls in (
            LicenseRequiredError,
            LicenseExpiredError,
            EditionInsufficientError,
            FeatureUnavailableError,
        ):
            err = cls("test")
            assert isinstance(err, Exception)

    def test_nxuskit_error_feature_field_defaults_to_none(self):
        err = NxuskitError("basic error")
        assert err.feature is None


@pytest.mark.skipif(not HAS_PARSE, reason="nxuskit._ffi_provider not importable")
class TestParseNxuskitError:
    """Test the _parse_nxuskit_error dispatcher."""

    def test_parse_license_required(self):
        err = _parse_err(
            {"error_type": "license_required", "message": "License needed", "feature": "solver"}
        )
        assert isinstance(err, LicenseRequiredError)
        assert err.feature == "solver"
        assert str(err) == "License needed"

    def test_parse_license_expired(self):
        err = _parse_err(
            {"error_type": "license_expired", "message": "Key expired", "feature": "zen"}
        )
        assert isinstance(err, LicenseExpiredError)
        assert err.feature == "zen"

    def test_parse_edition_insufficient(self):
        err = _parse_err(
            {
                "error_type": "edition_insufficient",
                "message": "Need pro",
                "feature": "zen",
                "required_edition": "pro",
            }
        )
        assert isinstance(err, EditionInsufficientError)
        assert err.required_edition == "pro"
        assert err.feature == "zen"

    def test_parse_feature_unavailable(self):
        err = _parse_err(
            {"error_type": "feature_unavailable", "message": "Not compiled", "feature": "clips"}
        )
        assert isinstance(err, FeatureUnavailableError)
        assert err.feature == "clips"

    def test_parse_unknown_type_falls_back_to_provider_error(self):
        err = _parse_err({"error_type": "some_new_thing", "message": "surprise"})
        assert isinstance(err, ProviderError)
        assert "some_new_thing" in str(err)

    def test_parse_missing_fields_uses_defaults(self):
        err = _parse_err({})
        assert isinstance(err, ProviderError)
        assert "internal" in str(err)
        assert "Unknown error" in str(err)


class TestLicenseKeyPassthrough:
    """Test that license_key survives JSON serialization for FFI config."""

    def test_ffi_provider_config_license_key(self):
        config = {"provider_type": "claude", "license_key": "test-key-123"}
        serialized = json.dumps(config)
        parsed = json.loads(serialized)
        assert parsed["license_key"] == "test-key-123"


@pytest.mark.skipif(not HAS_NXUSKIT, reason="nxuskit shared library not available")
class TestFFIErrorBehavior:
    """Test error behavior through the FFI layer."""

    def test_invalid_config_raises_config_error(self):
        with pytest.raises(ConfigError):
            create_ffi_provider({"provider_type": "nonexistent_provider_xyz"})

    def test_missing_provider_type_raises_config_error(self):
        with pytest.raises(ConfigError, match="provider_type"):
            create_ffi_provider({})

    def test_config_error_is_catchable_as_nxuskit_error(self):
        with pytest.raises(NxuskitError):
            create_ffi_provider({"provider_type": "nonexistent_provider_xyz"})

    def test_config_error_is_catchable_as_exception(self):
        with pytest.raises(Exception):
            create_ffi_provider({"provider_type": "nonexistent_provider_xyz"})
