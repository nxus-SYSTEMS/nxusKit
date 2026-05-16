"""Contract tests for OpenRouter provider with mocked HTTP."""

import pytest
from pytest_httpserver import HTTPServer

from nxuskit import AuthenticationError, ChatResponse, Message, Provider


class TestOpenRouterBasicChat:
    """Contract tests for OpenRouter basic chat functionality."""

    def test_openrouter_chat_simple_message(self, httpserver: HTTPServer):
        """OpenRouter provider should handle simple text message."""
        httpserver.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_json(
            {
                "id": "chat-test123",
                "model": "anthropic/claude-sonnet-4-20250514",
                "choices": [
                    {
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": "Hello via OpenRouter!",
                        },
                        "finish_reason": "stop",
                    }
                ],
                "usage": {
                    "prompt_tokens": 10,
                    "completion_tokens": 13,
                    "total_tokens": 23,
                },
            },
            status=200,
        )

        provider = Provider.openrouter(
            model="anthropic/claude-sonnet-4-20250514",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )
        response = provider.chat([Message.user("Hello")])

        assert isinstance(response, ChatResponse)
        assert response.content == "Hello via OpenRouter!"
        assert response.usage.total_tokens == 23

    def test_openrouter_chat_auth_error(self, httpserver: HTTPServer):
        """OpenRouter provider should raise AuthenticationError on 401."""
        httpserver.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_json(
            {"error": {"message": "Unauthorized"}},
            status=401,
        )

        provider = Provider.openrouter(
            model="anthropic/claude-sonnet-4-20250514",
            api_key="invalid-key",
            api_url=httpserver.url_for(""),
        )

        with pytest.raises(AuthenticationError):
            provider.chat([Message.user("Hello")])


class TestOpenRouterStreaming:
    """Contract tests for OpenRouter streaming functionality."""

    def test_openrouter_stream(self, httpserver: HTTPServer):
        """OpenRouter provider should stream responses."""
        httpserver.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_data(
            'data: {"choices":[{"index":0,"delta":{"content":"Open"},"finish_reason":null}]}\n'
            'data: {"choices":[{"index":0,"delta":{"content":"Router"},"finish_reason":"stop"}]}\n'
            "data: [DONE]\n",
            headers={"Content-Type": "text/event-stream"},
        )

        provider = Provider.openrouter(
            model="anthropic/claude-sonnet-4-20250514",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        chunks = list(provider.chat_stream([Message.user("Hello")]))
        assert len(chunks) >= 1
