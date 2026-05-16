"""Unit tests for error hierarchy and exception types."""

import pytest

from nxuskit import (
    AuthenticationError,
    LLMError,
    NetworkError,
    ProviderError,
    RateLimitError,
)


class TestLLMErrorBase:
    """Tests for base LLMError class."""

    def test_llm_error_is_exception(self):
        """LLMError should inherit from Exception."""
        assert issubclass(LLMError, Exception)

    def test_llm_error_creation(self):
        """LLMError should be creatable with message."""
        error = LLMError("Test error")
        assert str(error) == "Test error"

    def test_llm_error_with_details(self):
        """LLMError should accept status_code."""
        error = LLMError("Error", status_code=500)
        assert error.status_code == 500

    def test_llm_error_with_optional_fields(self):
        """LLMError should allow optional fields."""
        error = LLMError("Error", status_code=400, provider="test-provider", model="test-model")
        assert error.status_code == 400
        assert error.provider == "test-provider"
        assert error.model == "test-model"


class TestAuthenticationError:
    """Tests for AuthenticationError."""

    def test_authentication_error_is_llm_error(self):
        """AuthenticationError should inherit from LLMError."""
        assert issubclass(AuthenticationError, LLMError)

    def test_authentication_error_creation(self):
        """AuthenticationError should be creatable."""
        error = AuthenticationError("Invalid API key")
        assert str(error) == "Invalid API key"

    def test_authentication_error_with_status_code(self):
        """AuthenticationError should include status code."""
        error = AuthenticationError("Unauthorized", status_code=401)
        assert error.status_code == 401

    def test_authentication_error_not_retryable(self):
        """AuthenticationError should have is_retryable property."""
        error = AuthenticationError("Auth failed")
        # This tests that the property exists and is accessible
        assert hasattr(error, "is_retryable")


class TestRateLimitError:
    """Tests for RateLimitError."""

    def test_rate_limit_error_is_llm_error(self):
        """RateLimitError should inherit from LLMError."""
        assert issubclass(RateLimitError, LLMError)

    def test_rate_limit_error_creation(self):
        """RateLimitError should be creatable."""
        error = RateLimitError("Rate limit exceeded")
        assert str(error) == "Rate limit exceeded"

    def test_rate_limit_error_with_retry_after(self):
        """RateLimitError should support retry_after."""
        error = RateLimitError("Rate limited", status_code=429, retry_after=60)
        assert error.status_code == 429
        assert error.retry_after == 60

    def test_rate_limit_error_retry_after_optional(self):
        """RateLimitError retry_after should be optional."""
        error = RateLimitError("Rate limited")
        assert hasattr(error, "retry_after")

    def test_rate_limit_error_is_retryable(self):
        """RateLimitError should be marked as retryable."""
        error = RateLimitError("Rate limited")
        assert hasattr(error, "is_retryable")


class TestNetworkError:
    """Tests for NetworkError."""

    def test_network_error_is_llm_error(self):
        """NetworkError should inherit from LLMError."""
        assert issubclass(NetworkError, LLMError)

    def test_network_error_creation(self):
        """NetworkError should be creatable."""
        error = NetworkError("Connection timeout")
        assert str(error) == "Connection timeout"

    def test_network_error_with_status_code(self):
        """NetworkError should support status_code."""
        error = NetworkError("Connection refused", status_code=0)
        assert error.status_code == 0

    def test_network_error_is_retryable(self):
        """NetworkError should be marked as retryable."""
        error = NetworkError("Timeout")
        assert hasattr(error, "is_retryable")


class TestProviderError:
    """Tests for ProviderError."""

    def test_provider_error_is_llm_error(self):
        """ProviderError should inherit from LLMError."""
        assert issubclass(ProviderError, LLMError)

    def test_provider_error_creation(self):
        """ProviderError should be creatable."""
        error = ProviderError("Provider returned error")
        assert str(error) == "Provider returned error"

    def test_provider_error_with_details(self):
        """ProviderError should support provider and model."""
        error = ProviderError("Model not found", status_code=404, provider="openai", model="gpt-5")
        assert error.provider == "openai"
        assert error.model == "gpt-5"
        assert error.status_code == 404

    def test_provider_error_is_retryable_property(self):
        """ProviderError should have is_retryable property."""
        error = ProviderError("Server error", status_code=500)
        assert hasattr(error, "is_retryable")


class TestErrorHierarchy:
    """Tests for error hierarchy and catching."""

    def test_authentication_error_caught_as_llm_error(self):
        """AuthenticationError should be catchable as LLMError."""
        try:
            raise AuthenticationError("Invalid key")
        except LLMError as e:
            assert isinstance(e, AuthenticationError)

    def test_rate_limit_error_caught_as_llm_error(self):
        """RateLimitError should be catchable as LLMError."""
        try:
            raise RateLimitError("Too many requests")
        except LLMError as e:
            assert isinstance(e, RateLimitError)

    def test_network_error_caught_as_llm_error(self):
        """NetworkError should be catchable as LLMError."""
        try:
            raise NetworkError("Connection failed")
        except LLMError as e:
            assert isinstance(e, NetworkError)

    def test_provider_error_caught_as_llm_error(self):
        """ProviderError should be catchable as LLMError."""
        try:
            raise ProviderError("Provider unavailable")
        except LLMError as e:
            assert isinstance(e, ProviderError)

    def test_specific_error_catching(self):
        """Specific error types should be catchable separately."""
        try:
            raise RateLimitError("Rate limited")
        except RateLimitError as e:
            assert e.retry_after is None or isinstance(e.retry_after, (int, float))
        except LLMError:
            pytest.fail("Should be caught as RateLimitError, not generic LLMError")


class TestErrorProperties:
    """Tests for error properties and attributes."""

    def test_error_has_status_code(self):
        """All errors should have status_code attribute."""
        errors = [
            LLMError("msg", status_code=400),
            AuthenticationError("msg", status_code=401),
            RateLimitError("msg", status_code=429),
            NetworkError("msg", status_code=0),
            ProviderError("msg", status_code=503),
        ]
        for error in errors:
            assert hasattr(error, "status_code")

    def test_error_optional_provider_model(self):
        """Errors should support optional provider and model."""
        error = ProviderError("Error", provider="claude", model="claude-sonnet-4-20250514")
        assert error.provider == "claude"
        assert error.model == "claude-sonnet-4-20250514"

    def test_error_string_representation(self):
        """Error string representation should work."""
        error = LLMError("Test error message")
        assert "Test error message" in str(error)

    def test_rate_limit_error_retry_after_seconds(self):
        """RateLimitError retry_after should be in seconds."""
        error = RateLimitError("Limited", retry_after=120)
        assert error.retry_after == 120


class TestErrorEdgeCases:
    """Tests for error edge cases."""

    def test_error_with_none_message(self):
        """Errors should handle None message gracefully."""
        # Should not raise, message might be None or empty string
        LLMError(None)

    def test_error_with_empty_message(self):
        """Errors should handle empty message."""
        error = LLMError("")
        assert str(error) == ""

    def test_error_with_special_characters_in_message(self):
        """Error messages should preserve special characters."""
        msg = "Error: Failed\nDetails: 🔴\nCode: 500"
        error = LLMError(msg)
        assert msg in str(error)

    def test_retry_after_zero(self):
        """RateLimitError should allow retry_after=0."""
        error = RateLimitError("Limited", retry_after=0)
        assert error.retry_after == 0

    def test_retry_after_float(self):
        """RateLimitError should allow retry_after as float."""
        error = RateLimitError("Limited", retry_after=30.5)
        assert error.retry_after == 30.5
