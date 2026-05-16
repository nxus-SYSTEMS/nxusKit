"""Contract tests for Fireworks provider with mocked HTTP."""

import pytest
from pytest_httpserver import HTTPServer

from nxuskit import AuthenticationError, ChatResponse, Message, Provider


class TestFireworksBasicChat:
    """Contract tests for Fireworks basic chat functionality."""

    def test_fireworks_chat_simple_message(self, httpserver: HTTPServer):
        """Fireworks provider should handle simple text message."""
        httpserver.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_json(
            {
                "id": "chat-test123",
                "model": "accounts/fireworks/models/llama-v3-70b-instruct",
                "choices": [
                    {
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": "Hello from Fireworks!",
                        },
                        "finish_reason": "stop",
                    }
                ],
                "usage": {
                    "prompt_tokens": 10,
                    "completion_tokens": 12,
                    "total_tokens": 22,
                },
            },
            status=200,
        )

        provider = Provider.fireworks(
            model="accounts/fireworks/models/llama-v3-70b-instruct",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )
        response = provider.chat([Message.user("Hello")])

        assert isinstance(response, ChatResponse)
        assert response.content == "Hello from Fireworks!"
        assert response.usage.total_tokens == 22

    def test_fireworks_chat_auth_error(self, httpserver: HTTPServer):
        """Fireworks provider should raise AuthenticationError on 401."""
        httpserver.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_json(
            {"error": {"message": "Unauthorized"}},
            status=401,
        )

        provider = Provider.fireworks(
            model="accounts/fireworks/models/llama-v3-70b-instruct",
            api_key="invalid-key",
            api_url=httpserver.url_for(""),
        )

        with pytest.raises(AuthenticationError):
            provider.chat([Message.user("Hello")])


class TestFireworksStreaming:
    """Contract tests for Fireworks streaming functionality."""

    def test_fireworks_stream(self, httpserver: HTTPServer):
        """Fireworks provider should stream responses."""
        httpserver.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_data(
            'data: {"choices":[{"index":0,"delta":{"content":"Fire"},"finish_reason":null}]}\n'
            'data: {"choices":[{"index":0,"delta":{"content":"works"},"finish_reason":"stop"}]}\n'
            "data: [DONE]\n",
            headers={"Content-Type": "text/event-stream"},
        )

        provider = Provider.fireworks(
            model="accounts/fireworks/models/llama-v3-70b-instruct",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        chunks = list(provider.chat_stream([Message.user("Hello")]))
        assert len(chunks) >= 1
        assert "Fire" in chunks[0].delta or "works" in chunks[1].delta
