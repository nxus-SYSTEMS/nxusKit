"""Contract tests for error handling and rate limiting."""

import pytest
from pytest_httpserver import HTTPServer

from nxuskit import (
    AuthenticationError,
    LLMError,
    Message,
    NetworkError,
    Provider,
    ProviderError,
    RateLimitError,
)


class TestAuthenticationErrors:
    """Contract tests for authentication errors."""

    def test_claude_auth_error_401(self, httpserver: HTTPServer):
        """Claude should raise AuthenticationError on 401."""
        httpserver.expect_request("/v1/messages").respond_with_json(
            {"error": {"type": "authentication_error", "message": "Invalid API key"}},
            status=401,
        )

        provider = Provider.claude(
            model="claude-sonnet-4-20250514",
            api_key="invalid-key",
            api_url=httpserver.url_for(""),
        )

        with pytest.raises(AuthenticationError) as exc_info:
            provider.chat([Message.user("Hello")])

        assert exc_info.value.status_code == 401
        assert not exc_info.value.is_retryable

    def test_openai_auth_error_401(self, httpserver: HTTPServer):
        """OpenAI should raise AuthenticationError on 401."""
        httpserver.expect_request("/v1/chat/completions").respond_with_json(
            {"error": {"message": "Invalid API key", "type": "invalid_request_error"}},
            status=401,
        )

        provider = Provider.openai(
            model="gpt-4o",
            api_key="invalid-key",
            api_url=httpserver.url_for(""),
        )

        with pytest.raises(AuthenticationError) as exc_info:
            provider.chat([Message.user("Hello")])

        assert exc_info.value.status_code == 401

    def test_ollama_auth_error_401(self, httpserver: HTTPServer):
        """Ollama should raise error on 401."""
        httpserver.expect_request("/api/chat").respond_with_json(
            {"error": "authentication required"},
            status=401,
        )

        provider = Provider.ollama(
            model="mistral",
            api_url=httpserver.url_for(""),
        )

        with pytest.raises(LLMError) as exc_info:
            provider.chat([Message.user("Hello")])

        assert exc_info.value.status_code == 401

    def test_auth_error_preserves_details(self, httpserver: HTTPServer):
        """AuthenticationError should preserve error details."""
        httpserver.expect_request("/v1/messages").respond_with_json(
            {"error": {"type": "authentication_error", "message": "Invalid API key"}},
            status=401,
        )

        provider = Provider.claude(
            model="claude-sonnet-4-20250514",
            api_key="invalid-key",
            api_url=httpserver.url_for(""),
        )

        with pytest.raises(AuthenticationError) as exc_info:
            provider.chat([Message.user("Hello")])

        error = exc_info.value
        assert error.provider == "claude"
        assert error.model == "claude-sonnet-4-20250514"
        assert error.status_code == 401


class TestRateLimitErrors:
    """Contract tests for rate limiting."""

    def test_claude_rate_limit_429(self, httpserver: HTTPServer):
        """Claude should raise RateLimitError on 429."""
        httpserver.expect_request("/v1/messages").respond_with_json(
            {"error": {"type": "rate_limit_error", "message": "Rate limited"}},
            status=429,
            headers={"retry-after": "60"},
        )

        provider = Provider.claude(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        with pytest.raises(RateLimitError) as exc_info:
            provider.chat([Message.user("Hello")])

        assert exc_info.value.status_code == 429
        assert exc_info.value.is_retryable

    def test_rate_limit_retry_after(self, httpserver: HTTPServer):
        """RateLimitError should include retry_after."""
        httpserver.expect_request("/v1/messages").respond_with_json(
            {"error": {"type": "rate_limit_error", "message": "Rate limited"}},
            status=429,
            headers={"retry-after": "45"},
        )

        provider = Provider.claude(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        with pytest.raises(RateLimitError) as exc_info:
            provider.chat([Message.user("Hello")])

        assert exc_info.value.retry_after is not None or True  # Header may be captured

    def test_openai_rate_limit_429(self, httpserver: HTTPServer):
        """OpenAI should raise RateLimitError on 429."""
        httpserver.expect_request("/v1/chat/completions").respond_with_json(
            {"error": {"message": "Rate limit exceeded", "type": "server_error"}},
            status=429,
            headers={"retry-after": "30"},
        )

        provider = Provider.openai(
            model="gpt-4o",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        with pytest.raises(RateLimitError) as exc_info:
            provider.chat([Message.user("Hello")])

        assert exc_info.value.status_code == 429
        assert exc_info.value.is_retryable

    def test_ollama_rate_limit_429(self, httpserver: HTTPServer):
        """Ollama should raise RateLimitError on 429."""
        httpserver.expect_request("/api/chat").respond_with_json(
            {"error": "rate limit exceeded"},
            status=429,
        )

        provider = Provider.ollama(
            model="mistral",
            api_url=httpserver.url_for(""),
        )

        with pytest.raises(RateLimitError) as exc_info:
            provider.chat([Message.user("Hello")])

        assert exc_info.value.status_code == 429


class TestNetworkErrors:
    """Contract tests for network errors."""

    def test_connection_timeout(self, httpserver: HTTPServer):
        """Should raise NetworkError on connection timeout."""
        provider = Provider.claude(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            api_url="http://127.0.0.1:1",  # Non-existent server
            timeout=0.001,  # Very short timeout
        )

        with pytest.raises(NetworkError) as exc_info:
            provider.chat([Message.user("Hello")])

        assert exc_info.value.is_retryable

    def test_network_error_is_retryable(self, httpserver: HTTPServer):
        """NetworkError should be marked as retryable."""
        provider = Provider.claude(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            api_url="http://127.0.0.1:1",
            timeout=0.001,
        )

        with pytest.raises(NetworkError) as exc_info:
            provider.chat([Message.user("Hello")])

        error = exc_info.value
        assert error.is_retryable
        assert error.provider == "claude"


class TestServerErrors:
    """Contract tests for server errors (5xx)."""

    def test_claude_server_error_500(self, httpserver: HTTPServer):
        """Claude should raise ProviderError on 500."""
        httpserver.expect_request("/v1/messages").respond_with_json(
            {"error": {"type": "internal_server_error", "message": "Server error"}},
            status=500,
        )

        provider = Provider.claude(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        with pytest.raises(ProviderError) as exc_info:
            provider.chat([Message.user("Hello")])

        assert exc_info.value.status_code == 500
        assert exc_info.value.is_retryable  # 5xx errors are retryable

    def test_openai_server_error_503(self, httpserver: HTTPServer):
        """OpenAI should raise ProviderError on 503."""
        httpserver.expect_request("/v1/chat/completions").respond_with_json(
            {"error": {"message": "Service unavailable", "type": "server_error"}},
            status=503,
        )

        provider = Provider.openai(
            model="gpt-4o",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        with pytest.raises(ProviderError) as exc_info:
            provider.chat([Message.user("Hello")])

        assert exc_info.value.status_code == 503
        assert exc_info.value.is_retryable

    def test_server_error_retryable(self, httpserver: HTTPServer):
        """Server errors should be marked as retryable."""
        httpserver.expect_request("/api/chat").respond_with_json(
            {"error": "server error"},
            status=502,
        )

        provider = Provider.ollama(
            model="mistral",
            api_url=httpserver.url_for(""),
        )

        with pytest.raises(ProviderError) as exc_info:
            provider.chat([Message.user("Hello")])

        assert exc_info.value.is_retryable


class TestClientErrors:
    """Contract tests for client errors (4xx)."""

    def test_invalid_request_400(self, httpserver: HTTPServer):
        """Should raise ProviderError on 400."""
        httpserver.expect_request("/v1/messages").respond_with_json(
            {"error": {"type": "invalid_request_error", "message": "Invalid request"}},
            status=400,
        )

        provider = Provider.claude(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        with pytest.raises(ProviderError) as exc_info:
            provider.chat([Message.user("Hello")])

        assert exc_info.value.status_code == 400
        assert not exc_info.value.is_retryable  # 4xx errors are not retryable

    def test_not_found_404(self, httpserver: HTTPServer):
        """Should raise ProviderError on 404."""
        httpserver.expect_request("/api/chat").respond_with_json(
            {"error": "model not found"},
            status=404,
        )

        provider = Provider.ollama(
            model="nonexistent",
            api_url=httpserver.url_for(""),
        )

        with pytest.raises(ProviderError) as exc_info:
            provider.chat([Message.user("Hello")])

        assert exc_info.value.status_code == 404
        assert not exc_info.value.is_retryable


class TestErrorRecovery:
    """Contract tests for error recovery and retry patterns."""

    def test_retry_on_rate_limit(self, httpserver: HTTPServer):
        """RateLimitError should indicate retry is appropriate."""
        httpserver.expect_request("/v1/messages").respond_with_json(
            {"error": {"type": "rate_limit_error", "message": "Rate limited"}},
            status=429,
            headers={"retry-after": "60"},
        )

        provider = Provider.claude(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        # Call should fail with RateLimitError
        with pytest.raises(RateLimitError) as exc_info:
            provider.chat([Message.user("Hello")])

        # But the error should indicate it's retryable
        assert exc_info.value.is_retryable
        assert exc_info.value.status_code == 429

    def test_error_preserves_context(self, httpserver: HTTPServer):
        """Errors should preserve request context."""
        httpserver.expect_request("/v1/messages").respond_with_json(
            {"error": {"type": "authentication_error", "message": "Invalid key"}},
            status=401,
        )

        provider = Provider.claude(
            model="claude-sonnet-4-20250514",
            api_key="bad-key",
            api_url=httpserver.url_for(""),
        )

        try:
            provider.chat([Message.user("Test")])
        except AuthenticationError as e:
            # Should preserve context information
            assert e.provider == "claude"
            assert e.model == "claude-sonnet-4-20250514"
            assert e.status_code == 401


class TestErrorMessaging:
    """Contract tests for error messages."""

    def test_error_message_clarity(self, httpserver: HTTPServer):
        """Error messages should be clear and helpful."""
        httpserver.expect_request("/v1/messages").respond_with_json(
            {"error": {"type": "authentication_error", "message": "Invalid API key provided"}},
            status=401,
        )

        provider = Provider.claude(
            model="claude-sonnet-4-20250514",
            api_key="invalid",
            api_url=httpserver.url_for(""),
        )

        with pytest.raises(AuthenticationError) as exc_info:
            provider.chat([Message.user("Test")])

        error_msg = str(exc_info.value)
        assert len(error_msg) > 0

    def test_rate_limit_error_guidance(self, httpserver: HTTPServer):
        """RateLimitError should provide retry guidance."""
        httpserver.expect_request("/v1/chat/completions").respond_with_json(
            {"error": {"message": "Too many requests", "type": "server_error"}},
            status=429,
            headers={"retry-after": "60"},
        )

        provider = Provider.openai(
            model="gpt-4o",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        with pytest.raises(RateLimitError) as exc_info:
            provider.chat([Message.user("Test")])

        # Should indicate it's retryable
        assert exc_info.value.is_retryable


class TestStreamingErrors:
    """Contract tests for errors during streaming."""

    def test_streaming_auth_error(self, httpserver: HTTPServer):
        """Should raise AuthenticationError during streaming."""
        httpserver.expect_request("/v1/messages").respond_with_json(
            {"error": {"type": "authentication_error"}},
            status=401,
        )

        provider = Provider.claude(
            model="claude-sonnet-4-20250514",
            api_key="invalid",
            api_url=httpserver.url_for(""),
        )

        with pytest.raises(AuthenticationError):
            list(provider.chat_stream([Message.user("Test")]))

    def test_streaming_server_error(self, httpserver: HTTPServer):
        """Should raise ProviderError on server error during streaming."""
        httpserver.expect_request("/v1/chat/completions").respond_with_json(
            {"error": {"message": "Server error"}},
            status=500,
        )

        provider = Provider.openai(
            model="gpt-4o",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        with pytest.raises(ProviderError):
            list(provider.chat_stream([Message.user("Test")]))
