"""Contract tests for Perplexity provider with mocked HTTP."""

import pytest
from pytest_httpserver import HTTPServer

from nxuskit import AuthenticationError, ChatResponse, Message, Provider, RateLimitError


class TestPerplexityBasicChat:
    """Contract tests for Perplexity basic chat functionality."""

    def test_perplexity_chat_simple_message(self, httpserver: HTTPServer):
        """Perplexity provider should handle simple text message."""
        httpserver.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_json(
            {
                "id": "chat-test123",
                "model": "llama-3.1-sonar-small-128k-online",
                "choices": [
                    {
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": "Hello from Perplexity!",
                        },
                        "finish_reason": "stop",
                    }
                ],
                "usage": {
                    "prompt_tokens": 12,
                    "completion_tokens": 15,
                    "total_tokens": 27,
                },
            },
            status=200,
        )

        provider = Provider.perplexity(
            model="llama-3.1-sonar-small-128k-online",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )
        response = provider.chat([Message.user("What is AI?")])

        assert isinstance(response, ChatResponse)
        assert response.content == "Hello from Perplexity!"
        assert response.model == "llama-3.1-sonar-small-128k-online"

    def test_perplexity_chat_auth_error(self, httpserver: HTTPServer):
        """Perplexity provider should raise AuthenticationError on 401."""
        httpserver.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_json(
            {"error": {"message": "Invalid API key"}},
            status=401,
        )

        provider = Provider.perplexity(
            model="llama-3.1-sonar-small-128k-online",
            api_key="invalid-key",
            api_url=httpserver.url_for(""),
        )

        with pytest.raises(AuthenticationError):
            provider.chat([Message.user("Hello")])

    def test_perplexity_chat_rate_limit(self, httpserver: HTTPServer):
        """Perplexity provider should raise RateLimitError on 429."""
        httpserver.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_json(
            {"error": {"message": "Rate limited"}},
            status=429,
        )

        provider = Provider.perplexity(
            model="llama-3.1-sonar-small-128k-online",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        with pytest.raises(RateLimitError):
            provider.chat([Message.user("Hello")])


class TestPerplexityStreaming:
    """Contract tests for Perplexity streaming functionality."""

    def test_perplexity_stream(self, httpserver: HTTPServer):
        """Perplexity provider should stream responses."""
        httpserver.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_data(
            'data: {"choices":[{"index":0,"delta":{"content":"Perplexity"},"finish_reason":null}]}\n'
            'data: {"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}\n'
            "data: [DONE]\n",
            headers={"Content-Type": "text/event-stream"},
        )

        provider = Provider.perplexity(
            model="llama-3.1-sonar-small-128k-online",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        chunks = list(provider.chat_stream([Message.user("Hello")]))
        assert len(chunks) >= 1
        assert chunks[0].delta == "Perplexity"
