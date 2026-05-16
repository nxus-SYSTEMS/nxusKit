"""Contract tests for LM Studio provider with mocked HTTP."""

from pytest_httpserver import HTTPServer

from nxuskit import ChatResponse, Message, Provider


class TestLMStudioBasicChat:
    """Contract tests for LM Studio basic chat functionality."""

    def test_lmstudio_chat_simple_message(self, httpserver: HTTPServer):
        """LM Studio provider should handle simple text message."""
        httpserver.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_json(
            {
                "id": "chat-test123",
                "model": "local-model",
                "choices": [
                    {
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": "Hello from LM Studio!",
                        },
                        "finish_reason": "stop",
                    }
                ],
                "usage": {
                    "prompt_tokens": 9,
                    "completion_tokens": 11,
                    "total_tokens": 20,
                },
            },
            status=200,
        )

        provider = Provider.lmstudio(
            model="local-model",
            api_url=httpserver.url_for(""),
        )
        response = provider.chat([Message.user("Hello")])

        assert isinstance(response, ChatResponse)
        assert response.content == "Hello from LM Studio!"
        assert response.model == "local-model"
        assert response.usage.total_tokens == 20

    def test_lmstudio_chat_multiple_messages(self, httpserver: HTTPServer):
        """LM Studio provider should handle multi-turn conversations."""
        httpserver.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_json(
            {
                "id": "chat-test456",
                "model": "local-model",
                "choices": [
                    {
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": "I remember you asked about that.",
                        },
                        "finish_reason": "stop",
                    }
                ],
                "usage": {
                    "prompt_tokens": 20,
                    "completion_tokens": 12,
                    "total_tokens": 32,
                },
            },
            status=200,
        )

        provider = Provider.lmstudio(
            model="local-model",
            api_url=httpserver.url_for(""),
        )

        messages = [
            Message.system("You are a helpful assistant"),
            Message.user("What is 2+2?"),
            Message.assistant("4"),
            Message.user("Follow up question"),
        ]

        response = provider.chat(messages)
        assert response.content == "I remember you asked about that."


class TestLMStudioStreaming:
    """Contract tests for LM Studio streaming functionality."""

    def test_lmstudio_stream(self, httpserver: HTTPServer):
        """LM Studio provider should stream responses."""
        httpserver.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_data(
            'data: {"choices":[{"index":0,"delta":{"content":"Local"},"finish_reason":null}]}\n'
            'data: {"choices":[{"index":0,"delta":{"content":" Model"},"finish_reason":"stop"}]}\n'
            "data: [DONE]\n",
            headers={"Content-Type": "text/event-stream"},
        )

        provider = Provider.lmstudio(
            model="local-model",
            api_url=httpserver.url_for(""),
        )

        chunks = list(provider.chat_stream([Message.user("Hello")]))
        assert len(chunks) >= 1
        assert chunks[0].delta == "Local"
        assert chunks[1].delta == " Model"
