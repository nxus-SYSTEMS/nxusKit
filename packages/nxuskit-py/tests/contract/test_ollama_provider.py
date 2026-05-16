"""Contract tests for Ollama provider with mocked HTTP."""

import pytest
from pytest_httpserver import HTTPServer

from nxuskit import ChatResponse, Message, Provider, ProviderError


@pytest.fixture
def mock_ollama_server(httpserver: HTTPServer):
    """Fixture to provide a mock Ollama API server."""
    return httpserver


class TestOllamaBasicChat:
    """Contract tests for Ollama basic chat functionality."""

    def test_ollama_chat_simple_message(self, mock_ollama_server: HTTPServer):
        """Ollama provider should handle simple text message."""
        mock_ollama_server.expect_request(
            "/api/chat",
            method="POST",
        ).respond_with_json(
            {
                "model": "mistral",
                "created_at": "2024-01-15T12:00:00Z",
                "message": {
                    "role": "assistant",
                    "content": "Hello! I'm here to help.",
                },
                "done": True,
                "total_duration": 1000000000,
                "load_duration": 100000000,
                "prompt_eval_count": 10,
                "prompt_eval_duration": 500000000,
                "eval_count": 15,
                "eval_duration": 400000000,
            },
            status=200,
        )

        provider = Provider.ollama(
            model="mistral",
            api_url=mock_ollama_server.url_for(""),
        )
        response = provider.chat([Message.user("Hello")])

        assert isinstance(response, ChatResponse)
        assert response.content == "Hello! I'm here to help."
        assert response.model == "mistral"
        # Ollama doesn't return token counts in the same way, but we track them
        assert response.usage.total_tokens >= 0

    def test_ollama_chat_with_system_message(self, mock_ollama_server: HTTPServer):
        """Ollama provider should handle system messages."""
        mock_ollama_server.expect_request(
            "/api/chat",
            method="POST",
        ).respond_with_json(
            {
                "model": "mistral",
                "created_at": "2024-01-15T12:00:00Z",
                "message": {
                    "role": "assistant",
                    "content": "I'm a helpful local assistant.",
                },
                "done": True,
                "total_duration": 2000000000,
                "load_duration": 200000000,
                "prompt_eval_count": 25,
                "prompt_eval_duration": 1000000000,
                "eval_count": 12,
                "eval_duration": 800000000,
            },
            status=200,
        )

        provider = Provider.ollama(
            model="mistral",
            api_url=mock_ollama_server.url_for(""),
        )
        response = provider.chat(
            [
                Message.system("You are helpful"),
                Message.user("Who are you?"),
            ]
        )

        assert response.content == "I'm a helpful local assistant."

    def test_ollama_chat_message_history(self, mock_ollama_server: HTTPServer):
        """Ollama provider should handle multi-turn conversation."""
        mock_ollama_server.expect_request(
            "/api/chat",
            method="POST",
        ).respond_with_json(
            {
                "model": "mistral",
                "created_at": "2024-01-15T12:00:00Z",
                "message": {
                    "role": "assistant",
                    "content": "Correct! 2+2=4.",
                },
                "done": True,
                "total_duration": 1500000000,
                "load_duration": 150000000,
                "prompt_eval_count": 40,
                "prompt_eval_duration": 800000000,
                "eval_count": 8,
                "eval_duration": 550000000,
            },
            status=200,
        )

        provider = Provider.ollama(
            model="mistral",
            api_url=mock_ollama_server.url_for(""),
        )
        response = provider.chat(
            [
                Message.user("What is 2+2?"),
                Message.assistant("2+2=4"),
                Message.user("Are you sure?"),
            ]
        )

        assert response.content == "Correct! 2+2=4."


class TestOllamaErrors:
    """Contract tests for Ollama error handling."""

    def test_ollama_model_not_found(self, mock_ollama_server: HTTPServer):
        """Ollama provider should raise ProviderError on 404."""
        mock_ollama_server.expect_request("/api/chat").respond_with_json(
            {"error": "model not found"},
            status=404,
        )

        provider = Provider.ollama(
            model="nonexistent",
            api_url=mock_ollama_server.url_for(""),
        )

        with pytest.raises(ProviderError):
            provider.chat([Message.user("Hello")])

    def test_ollama_server_error(self, mock_ollama_server: HTTPServer):
        """Ollama provider should raise ProviderError on 500."""
        mock_ollama_server.expect_request("/api/chat").respond_with_json(
            {"error": "internal server error"},
            status=500,
        )

        provider = Provider.ollama(
            model="mistral",
            api_url=mock_ollama_server.url_for(""),
        )

        with pytest.raises(ProviderError):
            provider.chat([Message.user("Hello")])


class TestOllamaStreaming:
    """Contract tests for Ollama streaming functionality."""

    def test_ollama_chat_stream(self, mock_ollama_server: HTTPServer):
        """Ollama provider should support streaming responses."""
        stream_response = """{"model":"mistral","created_at":"2024-01-15T12:00:00Z","message":{"role":"assistant","content":"Hello"},"done":false}
{"model":"mistral","created_at":"2024-01-15T12:00:00Z","message":{"role":"assistant","content":" world"},"done":true,"total_duration":1000000000,"load_duration":100000000,"prompt_eval_count":5,"prompt_eval_duration":200000000,"eval_count":2,"eval_duration":700000000}
"""
        mock_ollama_server.expect_request("/api/chat").respond_with_data(
            stream_response,
            status=200,
            content_type="application/x-ndjson",
        )

        provider = Provider.ollama(
            model="mistral",
            api_url=mock_ollama_server.url_for(""),
        )

        chunks = list(provider.chat_stream([Message.user("Say hello")]))

        assert len(chunks) >= 2
        assert "Hello" in "".join(c.delta for c in chunks)

    def test_ollama_stream_empty_response(self, mock_ollama_server: HTTPServer):
        """Ollama provider should handle empty streaming response."""
        stream_response = """{"model":"mistral","created_at":"2024-01-15T12:00:00Z","message":{"role":"assistant","content":""},"done":true,"total_duration":100000000,"load_duration":100000000,"prompt_eval_count":0,"prompt_eval_duration":0,"eval_count":0,"eval_duration":0}
"""
        mock_ollama_server.expect_request("/api/chat").respond_with_data(
            stream_response,
            status=200,
            content_type="application/x-ndjson",
        )

        provider = Provider.ollama(
            model="mistral",
            api_url=mock_ollama_server.url_for(""),
        )

        chunks = list(provider.chat_stream([Message.user("Empty")]))
        assert isinstance(chunks, list)
