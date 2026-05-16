"""Contract tests for Groq provider with mocked HTTP."""

import pytest
from pytest_httpserver import HTTPServer

from nxuskit import (
    AuthenticationError,
    ChatResponse,
    Message,
    Provider,
    RateLimitError,
)


class TestGroqBasicChat:
    """Contract tests for Groq basic chat functionality."""

    def test_groq_chat_simple_message(self, httpserver: HTTPServer):
        """Groq provider should handle simple text message."""
        httpserver.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_json(
            {
                "id": "chat-test123",
                "object": "chat.completion",
                "created": 1234567890,
                "model": "llama-3.3-70b-versatile",
                "choices": [
                    {
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": "Hello! How can I help?",
                        },
                        "finish_reason": "stop",
                    }
                ],
                "usage": {
                    "prompt_tokens": 10,
                    "completion_tokens": 15,
                    "total_tokens": 25,
                },
            },
            status=200,
        )

        provider = Provider.groq(
            model="llama-3.3-70b-versatile",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )
        response = provider.chat([Message.user("Hello")])

        assert isinstance(response, ChatResponse)
        assert response.content == "Hello! How can I help?"
        assert response.model == "llama-3.3-70b-versatile"
        assert response.usage.prompt_tokens == 10

    def test_groq_chat_auth_error(self, httpserver: HTTPServer):
        """Groq provider should raise AuthenticationError on 401."""
        httpserver.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_json(
            {"error": {"message": "Invalid API key"}},
            status=401,
        )

        provider = Provider.groq(
            model="llama-3.3-70b-versatile",
            api_key="invalid-key",
            api_url=httpserver.url_for(""),
        )

        with pytest.raises(AuthenticationError) as exc_info:
            provider.chat([Message.user("Hello")])

        assert exc_info.value.status_code == 401

    def test_groq_chat_rate_limit(self, httpserver: HTTPServer):
        """Groq provider should raise RateLimitError on 429."""
        httpserver.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_json(
            {"error": {"message": "Rate limit exceeded"}},
            status=429,
        )

        provider = Provider.groq(
            model="llama-3.3-70b-versatile",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        with pytest.raises(RateLimitError) as exc_info:
            provider.chat([Message.user("Hello")])

        assert exc_info.value.status_code == 429


class TestGroqStreaming:
    """Contract tests for Groq streaming functionality."""

    def test_groq_stream(self, httpserver: HTTPServer):
        """Groq provider should stream responses."""
        httpserver.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_data(
            'data: {"choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}\n'
            'data: {"choices":[{"index":0,"delta":{"content":" world"},"finish_reason":null}]}\n'
            'data: {"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}\n'
            "data: [DONE]\n",
            headers={"Content-Type": "text/event-stream"},
        )

        provider = Provider.groq(
            model="llama-3.3-70b-versatile",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        chunks = list(provider.chat_stream([Message.user("Hello")]))
        assert len(chunks) == 3
        assert chunks[0].delta == "Hello"
        assert chunks[1].delta == " world"
        assert chunks[2].is_final()
        assert chunks[2].finish_reason == "stop"


class TestGroqVision:
    """Contract tests for Groq vision support."""

    def test_groq_vision_url_image(self, httpserver: HTTPServer):
        """Groq provider should handle image URLs."""
        httpserver.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_json(
            {
                "id": "chat-test456",
                "object": "chat.completion",
                "created": 1234567890,
                "model": "llama-3.3-70b-versatile",
                "choices": [
                    {
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": "I can see an image",
                        },
                        "finish_reason": "stop",
                    }
                ],
                "usage": {
                    "prompt_tokens": 20,
                    "completion_tokens": 10,
                    "total_tokens": 30,
                },
            },
            status=200,
        )

        provider = Provider.groq(
            model="llama-3.3-70b-versatile",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        response = provider.chat(
            [Message.user("What's in this image?").with_image_url("https://example.com/image.jpg")]
        )

        assert response.content == "I can see an image"
