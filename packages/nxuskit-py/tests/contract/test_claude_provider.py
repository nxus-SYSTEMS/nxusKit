"""Contract tests for Claude provider with mocked HTTP."""

import pytest
from pytest_httpserver import HTTPServer

from nxuskit import AuthenticationError, ChatResponse, Message, Provider, ProviderError


@pytest.fixture
def mock_claude_server(httpserver: HTTPServer):
    """Fixture to provide a mock Claude API server."""
    return httpserver


class TestClaudeBasicChat:
    """Contract tests for Claude basic chat functionality."""

    def test_claude_chat_simple_message(self, mock_claude_server: HTTPServer):
        """Claude provider should handle simple text message."""
        # Mock response
        mock_claude_server.expect_request(
            "/v1/messages",
            method="POST",
        ).respond_with_json(
            {
                "id": "msg_test123",
                "type": "message",
                "role": "assistant",
                "content": [{"type": "text", "text": "Hello! How can I help?"}],
                "model": "claude-sonnet-4-20250514",
                "stop_reason": "end_turn",
                "stop_sequence": None,
                "usage": {"input_tokens": 10, "output_tokens": 20},
            },
            status=200,
        )

        provider = Provider.claude(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            api_url=mock_claude_server.url_for(""),
        )
        response = provider.chat([Message.user("Hello")])

        assert isinstance(response, ChatResponse)
        assert response.content == "Hello! How can I help?"
        assert response.model == "claude-sonnet-4-20250514"
        assert response.usage.input_tokens == 10
        assert response.usage.output_tokens == 20

    def test_claude_chat_with_system_message(self, mock_claude_server: HTTPServer):
        """Claude provider should handle system messages."""
        mock_claude_server.expect_request(
            "/v1/messages",
            method="POST",
        ).respond_with_json(
            {
                "id": "msg_test456",
                "type": "message",
                "role": "assistant",
                "content": [{"type": "text", "text": "I am a helpful assistant."}],
                "model": "claude-sonnet-4-20250514",
                "stop_reason": "end_turn",
                "usage": {"input_tokens": 25, "output_tokens": 15},
            },
            status=200,
        )

        provider = Provider.claude(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            api_url=mock_claude_server.url_for(""),
        )
        response = provider.chat(
            [
                Message.system("You are helpful"),
                Message.user("Who are you?"),
            ]
        )

        assert response.content == "I am a helpful assistant."
        assert response.usage.input_tokens == 25

    def test_claude_chat_message_history(self, mock_claude_server: HTTPServer):
        """Claude provider should handle multi-turn conversation."""
        mock_claude_server.expect_request(
            "/v1/messages",
            method="POST",
        ).respond_with_json(
            {
                "id": "msg_test789",
                "type": "message",
                "role": "assistant",
                "content": [{"type": "text", "text": "The answer is 4."}],
                "model": "claude-sonnet-4-20250514",
                "stop_reason": "end_turn",
                "usage": {"input_tokens": 40, "output_tokens": 10},
            },
            status=200,
        )

        provider = Provider.claude(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            api_url=mock_claude_server.url_for(""),
        )
        response = provider.chat(
            [
                Message.user("What is 2+2?"),
                Message.assistant("2+2=4"),
                Message.user("Are you sure?"),
            ]
        )

        assert response.content == "The answer is 4."


class TestClaudeErrors:
    """Contract tests for Claude error handling."""

    def test_claude_authentication_error(self, mock_claude_server: HTTPServer):
        """Claude provider should raise AuthenticationError on 401."""
        mock_claude_server.expect_request("/v1/messages").respond_with_json(
            {"error": {"type": "authentication_error", "message": "Invalid API key"}},
            status=401,
        )

        provider = Provider.claude(
            model="claude-sonnet-4-20250514",
            api_key="invalid-key",
            api_url=mock_claude_server.url_for(""),
        )

        with pytest.raises(AuthenticationError):
            provider.chat([Message.user("Hello")])

    def test_claude_rate_limit_error(self, mock_claude_server: HTTPServer):
        """Claude provider should raise RateLimitError on 429."""
        mock_claude_server.expect_request("/v1/messages").respond_with_json(
            {"error": {"type": "rate_limit_error", "message": "Rate limited"}},
            status=429,
            headers={"retry-after": "60"},
        )

        provider = Provider.claude(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            api_url=mock_claude_server.url_for(""),
        )

        from nxuskit import RateLimitError

        with pytest.raises(RateLimitError):
            provider.chat([Message.user("Hello")])

    def test_claude_server_error(self, mock_claude_server: HTTPServer):
        """Claude provider should raise ProviderError on 500."""
        mock_claude_server.expect_request("/v1/messages").respond_with_json(
            {"error": {"type": "internal_server_error", "message": "Server error"}},
            status=500,
        )

        provider = Provider.claude(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            api_url=mock_claude_server.url_for(""),
        )

        with pytest.raises(ProviderError):
            provider.chat([Message.user("Hello")])


class TestClaudeStreaming:
    """Contract tests for Claude streaming functionality."""

    def test_claude_chat_stream(self, mock_claude_server: HTTPServer):
        """Claude provider should support streaming responses."""
        stream_response = """event: content_block_start
data: {"type":"content_block_start","content_block":{"type":"text"}}

event: content_block_delta
data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"Hello"}}

event: content_block_delta
data: {"type":"content_block_delta","delta":{"type":"text_delta","text":" world"}}

event: message_stop
data: {"type":"message_stop"}
"""
        mock_claude_server.expect_request("/v1/messages").respond_with_data(
            stream_response,
            status=200,
            content_type="text/event-stream",
        )

        provider = Provider.claude(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            api_url=mock_claude_server.url_for(""),
        )

        chunks = list(provider.chat_stream([Message.user("Say hello")]))

        assert len(chunks) >= 2
        assert "Hello" in "".join(c.delta for c in chunks)

    def test_claude_stream_empty_response(self, mock_claude_server: HTTPServer):
        """Claude provider should handle empty streaming response."""
        stream_response = """event: message_stop
data: {"type":"message_stop"}
"""
        mock_claude_server.expect_request("/v1/messages").respond_with_data(
            stream_response,
            status=200,
            content_type="text/event-stream",
        )

        provider = Provider.claude(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            api_url=mock_claude_server.url_for(""),
        )

        chunks = list(provider.chat_stream([Message.user("Empty")]))
        assert isinstance(chunks, list)
