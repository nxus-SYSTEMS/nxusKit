"""Contract tests for xAI Grok provider with mocked HTTP."""

from pytest_httpserver import HTTPServer

from nxuskit import ChatResponse, Message, Provider


class TestXaiBasicChat:
    """Contract tests for xAI Grok basic chat functionality."""

    def test_xai_chat_simple_message(self, httpserver: HTTPServer):
        """xAI provider should handle a simple OpenAI-compatible message."""
        httpserver.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_json(
            {
                "id": "chat-test123",
                "object": "chat.completion",
                "created": 1234567890,
                "model": "grok-4",
                "choices": [
                    {
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": "Hello from Grok",
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

        provider = Provider.xai(
            model="grok-4",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )
        response = provider.chat([Message.user("Hello")])

        assert isinstance(response, ChatResponse)
        assert response.content == "Hello from Grok"
        assert response.model == "grok-4"
        assert response.usage.prompt_tokens == 10

    def test_xai_custom_v1_base_url_does_not_duplicate_version(self, httpserver: HTTPServer):
        """xAI accepts a base URL that already includes /v1."""
        httpserver.expect_request(
            "/v1/models",
            method="GET",
        ).respond_with_json(
            {"object": "list", "data": [{"id": "grok-4", "object": "model"}]},
            status=200,
        )

        provider = Provider.xai(
            model="grok-4",
            api_key="test-key",
            api_url=httpserver.url_for("/v1"),
        )

        models = provider.list_models()
        assert models[0].id == "grok-4"
