"""Tests for response_format parameter."""

from unittest.mock import patch

from nxuskit import Message, ResponseFormat
from nxuskit.providers.claude import ClaudeProvider
from nxuskit.providers.openai_compatible import OpenAICompatibleProvider


class TestResponseFormat:
    """Tests for ResponseFormat enum and functionality."""

    def test_response_format_enum_values(self):
        """Test ResponseFormat enum has expected values."""
        assert ResponseFormat.JSON.value == "json"
        assert ResponseFormat.TEXT.value == "text"

    def test_response_format_is_string_enum(self):
        """Test ResponseFormat values can be used as strings."""
        assert str(ResponseFormat.JSON) == "ResponseFormat.JSON"
        assert ResponseFormat.JSON == "json"


class TestOpenAICompatibleResponseFormat:
    """Tests for response_format in OpenAI-compatible providers."""

    @patch("nxuskit.providers.openai_compatible.OpenAICompatibleProvider._make_request")
    def test_build_request_without_response_format(self, mock_request):
        """Test request building without response_format."""

        # Create a minimal concrete implementation for testing
        class TestProvider(OpenAICompatibleProvider):
            @property
            def provider_name(self) -> str:
                return "test"

            def _build_headers(self) -> dict:
                return {"authorization": "Bearer test"}

        provider = TestProvider(model="test-model", api_key="test-key", api_url="http://test.com")
        messages = [Message.user("Hello")]

        request_body = provider._build_request(
            messages, effective_model=provider._model, stream=False
        )

        assert "response_format" not in request_body

    @patch("nxuskit.providers.openai_compatible.OpenAICompatibleProvider._make_request")
    def test_build_request_with_json_response_format(self, mock_request):
        """Test request building with JSON response_format."""

        class TestProvider(OpenAICompatibleProvider):
            @property
            def provider_name(self) -> str:
                return "test"

            def _build_headers(self) -> dict:
                return {"authorization": "Bearer test"}

        provider = TestProvider(model="test-model", api_key="test-key", api_url="http://test.com")
        messages = [Message.user("Return JSON")]

        request_body = provider._build_request(
            messages,
            effective_model=provider._model,
            stream=False,
            response_format=ResponseFormat.JSON,
        )

        assert "response_format" in request_body
        assert request_body["response_format"] == {"type": "json_object"}

    @patch("nxuskit.providers.openai_compatible.OpenAICompatibleProvider._make_request")
    def test_build_request_with_text_response_format(self, mock_request):
        """Test request building with TEXT response_format (no change)."""

        class TestProvider(OpenAICompatibleProvider):
            @property
            def provider_name(self) -> str:
                return "test"

            def _build_headers(self) -> dict:
                return {"authorization": "Bearer test"}

        provider = TestProvider(model="test-model", api_key="test-key", api_url="http://test.com")
        messages = [Message.user("Hello")]

        request_body = provider._build_request(
            messages,
            effective_model=provider._model,
            stream=False,
            response_format=ResponseFormat.TEXT,
        )

        # TEXT format should not add response_format parameter
        assert "response_format" not in request_body


class TestClaudeResponseFormat:
    """Tests for response_format in Claude provider."""

    def test_build_request_without_response_format(self):
        """Test Claude request building without response_format."""
        provider = ClaudeProvider(model="claude-sonnet-4-20250514", api_key="test-key")
        messages = [Message.user("Hello")]

        request_body = provider._build_request(
            messages, effective_model=provider._model, stream=False
        )

        # No system prompt should be added for JSON instruction
        assert "system" not in request_body or "JSON" not in request_body.get("system", "")

    def test_build_request_with_json_response_format_no_system(self):
        """Test Claude request with JSON format and no existing system prompt."""
        provider = ClaudeProvider(model="claude-sonnet-4-20250514", api_key="test-key")
        messages = [Message.user("Return JSON")]

        request_body = provider._build_request(
            messages,
            effective_model=provider._model,
            stream=False,
            response_format=ResponseFormat.JSON,
        )

        assert "system" in request_body
        assert "valid JSON" in request_body["system"]

    def test_build_request_with_json_response_format_with_system(self):
        """Test Claude request with JSON format appends to existing system prompt."""
        provider = ClaudeProvider(model="claude-sonnet-4-20250514", api_key="test-key")
        messages = [
            Message.system("You are a helpful assistant."),
            Message.user("Return JSON"),
        ]

        request_body = provider._build_request(
            messages,
            effective_model=provider._model,
            stream=False,
            response_format=ResponseFormat.JSON,
        )

        assert "system" in request_body
        assert "helpful assistant" in request_body["system"]
        assert "valid JSON" in request_body["system"]

    def test_build_request_with_text_response_format(self):
        """Test Claude request with TEXT format (no change)."""
        provider = ClaudeProvider(model="claude-sonnet-4-20250514", api_key="test-key")
        messages = [Message.user("Hello")]

        request_body = provider._build_request(
            messages,
            effective_model=provider._model,
            stream=False,
            response_format=ResponseFormat.TEXT,
        )

        # TEXT format should not add JSON instruction
        assert "system" not in request_body or "JSON" not in request_body.get("system", "")
