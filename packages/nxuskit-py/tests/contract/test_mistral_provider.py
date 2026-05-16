"""Contract tests for Mistral provider with mocked HTTP."""

import pytest
from pytest_httpserver import HTTPServer

from nxuskit import AuthenticationError, ChatResponse, Message, Provider, RateLimitError


class TestMistralBasicChat:
    """Contract tests for Mistral basic chat functionality."""

    def test_mistral_chat_simple_message(self, httpserver: HTTPServer):
        """Mistral provider should handle simple text message."""
        httpserver.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_json(
            {
                "id": "chat-test123",
                "model": "mistral-large-latest",
                "choices": [
                    {
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": "Bonjour! Comment puis-je vous aider?",
                        },
                        "finish_reason": "stop",
                    }
                ],
                "usage": {
                    "prompt_tokens": 12,
                    "completion_tokens": 18,
                    "total_tokens": 30,
                },
            },
            status=200,
        )

        provider = Provider.mistral(
            model="mistral-large-latest",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )
        response = provider.chat([Message.user("Hello")])

        assert isinstance(response, ChatResponse)
        assert response.content == "Bonjour! Comment puis-je vous aider?"
        assert response.model == "mistral-large-latest"

    def test_mistral_chat_auth_error(self, httpserver: HTTPServer):
        """Mistral provider should raise AuthenticationError on 401."""
        httpserver.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_json(
            {"error": {"message": "Invalid API key"}},
            status=401,
        )

        provider = Provider.mistral(
            model="mistral-large-latest",
            api_key="invalid-key",
            api_url=httpserver.url_for(""),
        )

        with pytest.raises(AuthenticationError):
            provider.chat([Message.user("Hello")])

    def test_mistral_chat_rate_limit(self, httpserver: HTTPServer):
        """Mistral provider should raise RateLimitError on 429."""
        httpserver.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_json(
            {"error": {"message": "Rate limit exceeded"}},
            status=429,
        )

        provider = Provider.mistral(
            model="mistral-large-latest",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        with pytest.raises(RateLimitError):
            provider.chat([Message.user("Hello")])


class TestMistralStreaming:
    """Contract tests for Mistral streaming functionality."""

    def test_mistral_stream(self, httpserver: HTTPServer):
        """Mistral provider should stream responses."""
        httpserver.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_data(
            'data: {"choices":[{"index":0,"delta":{"content":"Bonjour"},"finish_reason":null}]}\n'
            'data: {"choices":[{"index":0,"delta":{"content":"!"},"finish_reason":"stop"}]}\n'
            "data: [DONE]\n",
            headers={"Content-Type": "text/event-stream"},
        )

        provider = Provider.mistral(
            model="mistral-large-latest",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        chunks = list(provider.chat_stream([Message.user("Hello")]))
        assert len(chunks) >= 1
        assert chunks[0].delta == "Bonjour"
