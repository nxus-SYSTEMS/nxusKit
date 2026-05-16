"""Unit tests for OpenAI-compatible provider base class."""

from nxuskit import ImageSource, ImageSourceType, Message
from nxuskit.providers.openai_compatible import OpenAICompatibleProvider


class MockOpenAIProvider(OpenAICompatibleProvider):
    """Concrete implementation of OpenAICompatibleProvider for testing."""

    DEFAULT_API_URL = "https://api.mock.example.com"

    @property
    def provider_name(self) -> str:
        return "mock"

    def _build_headers(self) -> dict:
        return {
            "content-type": "application/json",
            "authorization": f"Bearer {self._api_key}",
        }


class TestOpenAICompatibleBuildRequest:
    """Tests for _build_request method."""

    def test_build_request_simple_text(self):
        """Should build request for simple text message."""
        provider = MockOpenAIProvider(
            model="mock-model",
            api_key="test-key",
        )

        request = provider._build_request(
            [Message.user("Hello")], effective_model="mock-model", stream=False
        )

        assert request["model"] == "mock-model"
        assert request["stream"] is False
        assert len(request["messages"]) == 1
        assert request["messages"][0]["role"] == "user"
        assert request["messages"][0]["content"] == "Hello"

    def test_build_request_with_system_message(self):
        """Should include system messages in request."""
        messages = [
            Message.system("You are helpful"),
            Message.user("Hello"),
        ]

        provider = MockOpenAIProvider(
            model="mock-model",
            api_key="test-key",
        )

        request = provider._build_request(messages, effective_model="mock-model", stream=False)

        assert len(request["messages"]) == 2
        assert request["messages"][0]["role"] == "system"
        assert request["messages"][1]["role"] == "user"

    def test_build_request_with_url_image(self):
        """Should format URL images correctly."""
        message = Message.user("What's this?").with_image_url("https://example.com/image.jpg")

        provider = MockOpenAIProvider(
            model="mock-model",
            api_key="test-key",
        )

        request = provider._build_request([message], effective_model="mock-model", stream=False)

        assert len(request["messages"]) == 1
        msg_content = request["messages"][0]["content"]
        assert isinstance(msg_content, list)
        assert msg_content[0]["type"] == "text"
        assert msg_content[1]["type"] == "image_url"
        assert "https://example.com/image.jpg" in msg_content[1]["image_url"]["url"]

    def test_build_request_with_base64_image(self):
        """Should format base64 images correctly."""
        image = ImageSource(
            source_type=ImageSourceType.BASE64,
            data="iVBORw0KGgo",  # Partial base64
        )
        message = Message.user("Analyze")
        message.images = [image]

        provider = MockOpenAIProvider(
            model="mock-model",
            api_key="test-key",
        )

        request = provider._build_request([message], effective_model="mock-model", stream=False)

        msg_content = request["messages"][0]["content"]
        assert isinstance(msg_content, list)
        assert msg_content[1]["type"] == "image_url"
        assert "data:image/jpeg;base64," in msg_content[1]["image_url"]["url"]

    def test_build_request_stream_flag(self):
        """Should set stream flag correctly."""
        provider = MockOpenAIProvider(
            model="mock-model",
            api_key="test-key",
        )

        request_nonstream = provider._build_request(
            [Message.user("Hi")], effective_model="mock-model", stream=False
        )
        request_stream = provider._build_request(
            [Message.user("Hi")], effective_model="mock-model", stream=True
        )

        assert request_nonstream["stream"] is False
        assert request_stream["stream"] is True


class TestOpenAICompatibleParseResponse:
    """Tests for _parse_response method."""

    def test_parse_response_simple(self):
        """Should parse simple response correctly."""
        provider = MockOpenAIProvider(
            model="mock-model",
            api_key="test-key",
        )

        response_data = {
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "Hello back!",
                    },
                    "finish_reason": "stop",
                }
            ],
            "usage": {
                "prompt_tokens": 5,
                "completion_tokens": 8,
                "total_tokens": 13,
            },
        }

        response = provider._parse_response(response_data, "mock-model")

        assert response.content == "Hello back!"
        assert response.model == "mock-model"
        assert response.finish_reason == "stop"
        assert response.usage.prompt_tokens == 5
        assert response.usage.completion_tokens == 8
        assert response.usage.total_tokens == 13

    def test_parse_response_empty_content(self):
        """Should handle empty response content."""
        provider = MockOpenAIProvider(
            model="mock-model",
            api_key="test-key",
        )

        response_data = {
            "choices": [],
            "usage": {
                "prompt_tokens": 0,
                "completion_tokens": 0,
                "total_tokens": 0,
            },
        }

        response = provider._parse_response(response_data, "mock-model")

        assert response.content is None
        assert response.finish_reason is None

    def test_parse_response_missing_usage(self):
        """Should handle missing usage information."""
        provider = MockOpenAIProvider(
            model="mock-model",
            api_key="test-key",
        )

        response_data = {
            "choices": [
                {
                    "message": {"role": "assistant", "content": "Response"},
                    "finish_reason": "stop",
                }
            ],
        }

        response = provider._parse_response(response_data, "mock-model")

        assert response.content == "Response"
        assert response.usage.prompt_tokens == 0
        assert response.usage.completion_tokens == 0


class TestOpenAICompatibleParseStreamEvent:
    """Tests for _parse_stream_event method."""

    def test_parse_stream_event_with_content(self):
        """Should parse stream event with content."""
        provider = MockOpenAIProvider(
            model="mock-model",
            api_key="test-key",
        )

        event_data = {
            "choices": [
                {
                    "delta": {"content": "Hello"},
                    "finish_reason": None,
                }
            ]
        }

        chunk = provider._parse_stream_event(event_data, "mock-model")

        assert chunk is not None
        assert chunk.delta == "Hello"
        assert chunk.model == "mock-model"

    def test_parse_stream_event_final_with_finish_reason(self):
        """Should return chunk with finish_reason when stream ends."""
        provider = MockOpenAIProvider(
            model="mock-model",
            api_key="test-key",
        )

        event_data = {
            "choices": [
                {
                    "delta": {},  # No content
                    "finish_reason": "stop",
                }
            ]
        }

        chunk = provider._parse_stream_event(event_data, "mock-model")

        assert chunk is not None
        assert chunk.delta == ""
        assert chunk.finish_reason == "stop"
        assert chunk.is_final()

    def test_parse_stream_event_empty_choices(self):
        """Should handle empty choices."""
        provider = MockOpenAIProvider(
            model="mock-model",
            api_key="test-key",
        )

        event_data = {"choices": []}

        chunk = provider._parse_stream_event(event_data, "mock-model")

        assert chunk is None


class TestOpenAICompatibleHeaders:
    """Tests for header building."""

    def test_build_headers(self):
        """Should build headers with API key."""
        provider = MockOpenAIProvider(
            model="mock-model",
            api_key="secret-key-123",
        )

        headers = provider._build_headers()

        assert headers["authorization"] == "Bearer secret-key-123"
        assert headers["content-type"] == "application/json"
