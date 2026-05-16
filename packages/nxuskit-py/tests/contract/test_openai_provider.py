"""Contract tests for OpenAI provider with mocked HTTP."""

import pytest
from pytest_httpserver import HTTPServer

from nxuskit import (
    AuthenticationError,
    ChatResponse,
    Message,
    Provider,
    ProviderError,
    RateLimitError,
)


@pytest.fixture
def mock_openai_server(httpserver: HTTPServer):
    """Fixture to provide a mock OpenAI API server."""
    return httpserver


class TestOpenAIBasicChat:
    """Contract tests for OpenAI basic chat functionality."""

    def test_openai_chat_simple_message(self, mock_openai_server: HTTPServer):
        """OpenAI provider should handle simple text message."""
        mock_openai_server.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_json(
            {
                "id": "chatcmpl-test123",
                "object": "chat.completion",
                "created": 1234567890,
                "model": "gpt-4o",
                "choices": [
                    {
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": "Hello! How can I assist you?",
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

        provider = Provider.openai(
            model="gpt-4o",
            api_key="test-key",
            api_url=mock_openai_server.url_for(""),
        )
        response = provider.chat([Message.user("Hello")])

        assert isinstance(response, ChatResponse)
        assert response.content == "Hello! How can I assist you?"
        assert response.model == "gpt-4o"
        assert response.usage.prompt_tokens == 10
        assert response.usage.completion_tokens == 15

    def test_openai_chat_with_system_message(self, mock_openai_server: HTTPServer):
        """OpenAI provider should handle system messages."""
        mock_openai_server.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_json(
            {
                "id": "chatcmpl-test456",
                "object": "chat.completion",
                "created": 1234567890,
                "model": "gpt-4o",
                "choices": [
                    {
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": "I'm an AI assistant.",
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

        provider = Provider.openai(
            model="gpt-4o",
            api_key="test-key",
            api_url=mock_openai_server.url_for(""),
        )
        response = provider.chat(
            [
                Message.system("You are helpful"),
                Message.user("Who are you?"),
            ]
        )

        assert response.content == "I'm an AI assistant."
        assert response.usage.prompt_tokens == 20

    def test_openai_chat_message_history(self, mock_openai_server: HTTPServer):
        """OpenAI provider should handle multi-turn conversation."""
        mock_openai_server.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_json(
            {
                "id": "chatcmpl-test789",
                "object": "chat.completion",
                "created": 1234567890,
                "model": "gpt-4o",
                "choices": [
                    {
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": "Yes, 2+2 equals 4.",
                        },
                        "finish_reason": "stop",
                    }
                ],
                "usage": {
                    "prompt_tokens": 30,
                    "completion_tokens": 8,
                    "total_tokens": 38,
                },
            },
            status=200,
        )

        provider = Provider.openai(
            model="gpt-4o",
            api_key="test-key",
            api_url=mock_openai_server.url_for(""),
        )
        response = provider.chat(
            [
                Message.user("What is 2+2?"),
                Message.assistant("2+2=4"),
                Message.user("Are you sure?"),
            ]
        )

        assert response.content == "Yes, 2+2 equals 4."


class TestOpenAIErrors:
    """Contract tests for OpenAI error handling."""

    def test_openai_authentication_error(self, mock_openai_server: HTTPServer):
        """OpenAI provider should raise AuthenticationError on 401."""
        mock_openai_server.expect_request("/v1/chat/completions").respond_with_json(
            {"error": {"message": "Incorrect API key", "type": "invalid_request_error"}},
            status=401,
        )

        provider = Provider.openai(
            model="gpt-4o",
            api_key="invalid-key",
            api_url=mock_openai_server.url_for(""),
        )

        with pytest.raises(AuthenticationError):
            provider.chat([Message.user("Hello")])

    def test_openai_rate_limit_error(self, mock_openai_server: HTTPServer):
        """OpenAI provider should raise RateLimitError on 429."""
        mock_openai_server.expect_request("/v1/chat/completions").respond_with_json(
            {"error": {"message": "Rate limit exceeded", "type": "server_error"}},
            status=429,
            headers={"retry-after": "30"},
        )

        provider = Provider.openai(
            model="gpt-4o",
            api_key="test-key",
            api_url=mock_openai_server.url_for(""),
        )

        with pytest.raises(RateLimitError):
            provider.chat([Message.user("Hello")])

    def test_openai_server_error(self, mock_openai_server: HTTPServer):
        """OpenAI provider should raise ProviderError on 500."""
        mock_openai_server.expect_request("/v1/chat/completions").respond_with_json(
            {"error": {"message": "Server error", "type": "server_error"}},
            status=500,
        )

        provider = Provider.openai(
            model="gpt-4o",
            api_key="test-key",
            api_url=mock_openai_server.url_for(""),
        )

        with pytest.raises(ProviderError):
            provider.chat([Message.user("Hello")])


class TestOpenAIStreaming:
    """Contract tests for OpenAI streaming functionality."""

    def test_openai_chat_stream(self, mock_openai_server: HTTPServer):
        """OpenAI provider should support streaming responses."""
        stream_response = """data: {"id":"chatcmpl-test","object":"text_completion.chunk","created":1234567890,"model":"gpt-4o","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}

data: {"id":"chatcmpl-test","object":"text_completion.chunk","created":1234567890,"model":"gpt-4o","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}

data: {"id":"chatcmpl-test","object":"text_completion.chunk","created":1234567890,"model":"gpt-4o","choices":[{"index":0,"delta":{"content":" world"},"finish_reason":null}]}

data: {"id":"chatcmpl-test","object":"text_completion.chunk","created":1234567890,"model":"gpt-4o","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: [DONE]
"""
        mock_openai_server.expect_request("/v1/chat/completions").respond_with_data(
            stream_response,
            status=200,
            content_type="text/event-stream",
        )

        provider = Provider.openai(
            model="gpt-4o",
            api_key="test-key",
            api_url=mock_openai_server.url_for(""),
        )

        chunks = list(provider.chat_stream([Message.user("Say hello")]))

        assert len(chunks) >= 2
        assert "Hello" in "".join(c.delta for c in chunks)

    def test_openai_stream_empty_response(self, mock_openai_server: HTTPServer):
        """OpenAI provider should handle empty streaming response."""
        stream_response = """data: [DONE]
"""
        mock_openai_server.expect_request("/v1/chat/completions").respond_with_data(
            stream_response,
            status=200,
            content_type="text/event-stream",
        )

        provider = Provider.openai(
            model="gpt-4o",
            api_key="test-key",
            api_url=mock_openai_server.url_for(""),
        )

        chunks = list(provider.chat_stream([Message.user("Empty")]))
        assert isinstance(chunks, list)
