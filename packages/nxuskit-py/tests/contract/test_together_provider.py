"""Contract tests for Together provider with mocked HTTP."""

import pytest
from pytest_httpserver import HTTPServer

from nxuskit import ChatResponse, Message, Provider, RateLimitError


class TestTogetherBasicChat:
    """Contract tests for Together basic chat functionality."""

    def test_together_chat_simple_message(self, httpserver: HTTPServer):
        """Together provider should handle simple text message."""
        httpserver.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_json(
            {
                "id": "chat-test123",
                "model": "meta-llama/Llama-3-70b-chat-hf",
                "choices": [
                    {
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": "Hello from Together!",
                        },
                        "finish_reason": "stop",
                    }
                ],
                "usage": {
                    "prompt_tokens": 11,
                    "completion_tokens": 14,
                    "total_tokens": 25,
                },
            },
            status=200,
        )

        provider = Provider.together(
            model="meta-llama/Llama-3-70b-chat-hf",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )
        response = provider.chat([Message.user("Hello")])

        assert isinstance(response, ChatResponse)
        assert response.content == "Hello from Together!"
        assert response.model == "meta-llama/Llama-3-70b-chat-hf"

    def test_together_chat_rate_limit(self, httpserver: HTTPServer):
        """Together provider should raise RateLimitError on 429."""
        httpserver.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_json(
            {"error": {"message": "Rate limit"}},
            status=429,
        )

        provider = Provider.together(
            model="meta-llama/Llama-3-70b-chat-hf",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        with pytest.raises(RateLimitError):
            provider.chat([Message.user("Hello")])


class TestTogetherStreaming:
    """Contract tests for Together streaming functionality."""

    def test_together_stream(self, httpserver: HTTPServer):
        """Together provider should stream responses."""
        httpserver.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_data(
            'data: {"choices":[{"index":0,"delta":{"content":"Together"},"finish_reason":null}]}\n'
            'data: {"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}\n'
            "data: [DONE]\n",
            headers={"Content-Type": "text/event-stream"},
        )

        provider = Provider.together(
            model="meta-llama/Llama-3-70b-chat-hf",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        chunks = list(provider.chat_stream([Message.user("Hello")]))
        assert len(chunks) >= 1
        assert chunks[0].delta == "Together"
