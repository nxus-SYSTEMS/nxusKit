"""Unit tests for stop sequences parameter across all providers."""

from unittest.mock import MagicMock, patch

from nxuskit import Message, Provider
from nxuskit.providers.openai_compatible import OpenAICompatibleProvider


class MockOpenAICompatibleProvider(OpenAICompatibleProvider):
    """Mock provider for testing OpenAICompatibleProvider."""

    DEFAULT_API_URL = "https://api.example.com"

    def __init__(self, model="test-model", api_key="test-key", api_url=None, timeout=30.0):
        if api_url is None:
            api_url = self.DEFAULT_API_URL
        super().__init__(model, api_key, api_url, timeout)

    def _build_headers(self):
        return {"Authorization": f"Bearer {self._api_key}"}

    @property
    def provider_name(self):
        return "mock"


class TestStopSequencesOpenAICompatible:
    """Test stop sequences with OpenAI-compatible providers."""

    def test_stop_single_string_normalized_to_list(self):
        """Stop parameter as string should be normalized to list."""
        provider = MockOpenAICompatibleProvider()
        messages = [Message.user("Hello")]

        request_body = provider._build_request(
            messages, effective_model=provider._model, stream=False, stop="\n\n"
        )

        assert "stop" in request_body
        assert request_body["stop"] == ["\n\n"]
        assert isinstance(request_body["stop"], list)

    def test_stop_list_preserved(self):
        """Stop parameter as list should be preserved."""
        provider = MockOpenAICompatibleProvider()
        messages = [Message.user("Hello")]

        stop_list = ["\n\n", "END", "STOP"]
        request_body = provider._build_request(
            messages, effective_model=provider._model, stream=False, stop=stop_list
        )

        assert "stop" in request_body
        assert request_body["stop"] == stop_list

    def test_stop_not_included_if_none(self):
        """Stop parameter should not be included if None."""
        provider = MockOpenAICompatibleProvider()
        messages = [Message.user("Hello")]

        request_body = provider._build_request(
            messages, effective_model=provider._model, stream=False, stop=None
        )

        assert "stop" not in request_body

    def test_stop_with_temperature(self):
        """Stop sequences should work alongside other parameters."""
        provider = MockOpenAICompatibleProvider()
        messages = [Message.user("Hello")]

        request_body = provider._build_request(
            messages,
            effective_model=provider._model,
            stream=False,
            temperature=0.7,
            stop=["\n\n", "END"],
        )

        assert request_body["temperature"] == 0.7
        assert request_body["stop"] == ["\n\n", "END"]

    def test_stop_with_max_tokens(self):
        """Stop sequences should work with max_tokens parameter."""
        provider = MockOpenAICompatibleProvider()
        messages = [Message.user("Hello")]

        request_body = provider._build_request(
            messages, effective_model=provider._model, stream=False, max_tokens=100, stop="\n"
        )

        assert request_body["max_tokens"] == 100
        assert request_body["stop"] == ["\n"]

    def test_stop_with_top_p(self):
        """Stop sequences should work with top_p parameter."""
        provider = MockOpenAICompatibleProvider()
        messages = [Message.user("Hello")]

        request_body = provider._build_request(
            messages, effective_model=provider._model, stream=False, top_p=0.9, stop="END"
        )

        assert request_body["top_p"] == 0.9
        assert request_body["stop"] == ["END"]

    def test_stop_with_all_parameters(self):
        """Stop sequences should work with all other parameters."""
        provider = MockOpenAICompatibleProvider()
        messages = [Message.user("Hello")]

        request_body = provider._build_request(
            messages,
            effective_model=provider._model,
            stream=False,
            temperature=0.8,
            max_tokens=500,
            top_p=0.95,
            stop=["\n\n", "STOP"],
        )

        assert request_body["temperature"] == 0.8
        assert request_body["max_tokens"] == 500
        assert request_body["top_p"] == 0.95
        assert request_body["stop"] == ["\n\n", "STOP"]

    @patch("requests.request")
    def test_stop_passed_in_chat_request(self, mock_request):
        """Stop parameter should be passed through chat() method."""
        mock_response = MagicMock()
        mock_response.json.return_value = {
            "id": "test-1",
            "choices": [
                {
                    "index": 0,
                    "message": {"role": "assistant", "content": "Response"},
                    "finish_reason": "stop",
                }
            ],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15,
            },
        }
        mock_response.status_code = 200
        mock_request.return_value = mock_response

        provider = MockOpenAICompatibleProvider()
        messages = [Message.user("Hello")]

        provider.chat(messages, stop=["\n\n", "END"])

        # Verify request was made
        assert mock_request.called
        call_args = mock_request.call_args
        json_data = call_args.kwargs["json"]

        assert "stop" in json_data
        assert json_data["stop"] == ["\n\n", "END"]

    @patch("requests.request")
    def test_stop_passed_in_stream_chat_request(self, mock_request):
        """Stop parameter should be passed through chat_stream() method."""
        mock_response = MagicMock()
        mock_response.iter_lines.return_value = [
            b'data: {"choices":[{"delta":{"content":"Hello"},"finish_reason":null}]}',
            b"data: [DONE]",
        ]
        mock_response.status_code = 200
        mock_request.return_value = mock_response

        provider = MockOpenAICompatibleProvider()
        messages = [Message.user("Hello")]

        list(provider.chat_stream(messages, stop="END"))

        # Verify request was made with stop parameter
        assert mock_request.called
        call_args = mock_request.call_args
        json_data = call_args.kwargs["json"]

        assert "stop" in json_data
        assert json_data["stop"] == ["END"]


class TestStopSequencesClaude:
    """Test stop sequences with Claude provider."""

    def test_claude_stop_uses_stop_sequences_field(self):
        """Claude should use 'stop_sequences' field instead of 'stop'."""
        from nxuskit.providers.claude import ClaudeProvider

        provider = ClaudeProvider(model="claude-sonnet-4-20250514", api_key="test-key")
        messages = [Message.user("Hello")]

        request_body = provider._build_request(
            messages, effective_model=provider._model, stream=False, stop="\n\n"
        )

        # Claude-specific: uses stop_sequences, not stop
        assert "stop_sequences" in request_body
        assert request_body["stop_sequences"] == ["\n\n"]
        assert "stop" not in request_body

    def test_claude_stop_list_in_stop_sequences(self):
        """Claude should preserve list format in stop_sequences."""
        from nxuskit.providers.claude import ClaudeProvider

        provider = ClaudeProvider(model="claude-sonnet-4-20250514", api_key="test-key")
        messages = [Message.user("Hello")]

        stop_list = ["\n\n", "END", "STOP"]
        request_body = provider._build_request(
            messages, effective_model=provider._model, stream=False, stop=stop_list
        )

        assert "stop_sequences" in request_body
        assert request_body["stop_sequences"] == stop_list

    def test_claude_stop_not_included_if_none(self):
        """Claude should not include stop_sequences if None."""
        from nxuskit.providers.claude import ClaudeProvider

        provider = ClaudeProvider(model="claude-sonnet-4-20250514", api_key="test-key")
        messages = [Message.user("Hello")]

        request_body = provider._build_request(
            messages, effective_model=provider._model, stream=False, stop=None
        )

        assert "stop_sequences" not in request_body

    def test_claude_stop_with_other_parameters(self):
        """Claude stop_sequences should work with other parameters."""
        from nxuskit.providers.claude import ClaudeProvider

        provider = ClaudeProvider(model="claude-sonnet-4-20250514", api_key="test-key")
        messages = [Message.user("Hello")]

        request_body = provider._build_request(
            messages,
            effective_model=provider._model,
            stream=False,
            temperature=0.7,
            max_tokens=200,
            top_p=0.9,
            stop=["\n\n", "END"],
        )

        assert request_body["temperature"] == 0.7
        assert request_body["max_tokens"] == 200
        assert request_body["top_p"] == 0.9
        assert request_body["stop_sequences"] == ["\n\n", "END"]


class TestStopSequencesOllama:
    """Test stop sequences with Ollama provider."""

    def test_ollama_stop_single_string_normalized(self):
        """Ollama should normalize stop string to list."""
        from nxuskit.providers.ollama import OllamaProvider

        provider = OllamaProvider(model="llama3.2")
        messages = [Message.user("Hello")]

        request_body = provider._build_request(
            messages, effective_model=provider._model, stream=False, stop="\n\n"
        )

        assert "stop" in request_body
        assert request_body["stop"] == ["\n\n"]

    def test_ollama_stop_list_preserved(self):
        """Ollama should preserve list format for stop."""
        from nxuskit.providers.ollama import OllamaProvider

        provider = OllamaProvider(model="llama3.2")
        messages = [Message.user("Hello")]

        stop_list = ["\n\n", "END"]
        request_body = provider._build_request(
            messages, effective_model=provider._model, stream=False, stop=stop_list
        )

        assert request_body["stop"] == stop_list

    def test_ollama_stop_not_included_if_none(self):
        """Ollama should not include stop if None."""
        from nxuskit.providers.ollama import OllamaProvider

        provider = OllamaProvider(model="llama3.2")
        messages = [Message.user("Hello")]

        request_body = provider._build_request(
            messages, effective_model=provider._model, stream=False, stop=None
        )

        assert "stop" not in request_body

    def test_ollama_stop_with_max_tokens(self):
        """Ollama stop should work with max_tokens (num_predict)."""
        from nxuskit.providers.ollama import OllamaProvider

        provider = OllamaProvider(model="llama3.2")
        messages = [Message.user("Hello")]

        request_body = provider._build_request(
            messages, effective_model=provider._model, stream=False, max_tokens=100, stop="\n"
        )

        # Ollama uses num_predict for max_tokens
        assert request_body["num_predict"] == 100
        assert request_body["stop"] == ["\n"]


class TestStopSequencesGroq:
    """Test stop sequences with Groq provider."""

    def test_groq_stop_normalized_to_list(self):
        """Groq should normalize stop to list."""
        from nxuskit.providers.groq import GroqProvider

        provider = GroqProvider(model="llama-3.3-70b-versatile", api_key="test-key")
        messages = [Message.user("Hello")]

        request_body = provider._build_request(
            messages, effective_model=provider._model, stream=False, stop="\n\n"
        )

        assert "stop" in request_body
        assert request_body["stop"] == ["\n\n"]

    def test_groq_stop_list_preserved(self):
        """Groq should preserve list format."""
        from nxuskit.providers.groq import GroqProvider

        provider = GroqProvider(model="llama-3.3-70b-versatile", api_key="test-key")
        messages = [Message.user("Hello")]

        stop_list = ["\n\n", "END"]
        request_body = provider._build_request(
            messages, effective_model=provider._model, stream=False, stop=stop_list
        )

        assert request_body["stop"] == stop_list


class TestStopSequencesMistral:
    """Test stop sequences with Mistral provider."""

    def test_mistral_stop_normalized_to_list(self):
        """Mistral should normalize stop to list."""
        from nxuskit.providers.mistral import MistralProvider

        provider = MistralProvider(model="mistral-large-latest", api_key="test-key")
        messages = [Message.user("Hello")]

        request_body = provider._build_request(
            messages, effective_model=provider._model, stream=False, stop="\n\n"
        )

        assert "stop" in request_body
        assert request_body["stop"] == ["\n\n"]


class TestStopSequencesAcrossAllProviders:
    """Integration-style tests for stop sequences across all providers."""

    def test_all_openai_compatible_providers_support_stop(self):
        """All OpenAI-compatible providers should support stop parameter."""
        provider_classes = [
            ("groq", "llama-3.3-70b-versatile"),
            ("mistral", "mistral-large-latest"),
            ("fireworks", "accounts/fireworks/models/llama-v3-70b"),
            ("together", "meta-llama/Llama-3-70b"),
            ("openrouter", "anthropic/claude-sonnet-4-20250514"),
            ("perplexity", "llama-3.1-sonar-small-128k-online"),
        ]

        for provider_name, model in provider_classes:
            if provider_name == "openai":
                from nxuskit.providers.openai import OpenAIProvider

                provider = OpenAIProvider(model=model, api_key="test-key")
            elif provider_name == "groq":
                from nxuskit.providers.groq import GroqProvider

                provider = GroqProvider(model=model, api_key="test-key")
            elif provider_name == "mistral":
                from nxuskit.providers.mistral import MistralProvider

                provider = MistralProvider(model=model, api_key="test-key")
            elif provider_name == "fireworks":
                from nxuskit.providers.fireworks import FireworksProvider

                provider = FireworksProvider(model=model, api_key="test-key")
            elif provider_name == "together":
                from nxuskit.providers.together import TogetherProvider

                provider = TogetherProvider(model=model, api_key="test-key")
            elif provider_name == "openrouter":
                from nxuskit.providers.openrouter import OpenRouterProvider

                provider = OpenRouterProvider(model=model, api_key="test-key")
            elif provider_name == "perplexity":
                from nxuskit.providers.perplexity import PerplexityProvider

                provider = PerplexityProvider(model=model, api_key="test-key")
            else:
                continue

            messages = [Message.user("Hello")]
            request_body = provider._build_request(
                messages, effective_model=provider._model, stream=False, stop=["\n\n", "END"]
            )

            assert "stop" in request_body, f"{provider_name} provider doesn't support stop"
            assert request_body["stop"] == ["\n\n", "END"], (
                f"{provider_name} didn't preserve stop list"
            )

    def test_all_providers_accept_stop_in_chat_method(self):
        """All providers should accept stop parameter in chat() method."""

        # Verify method signatures accept stop parameter
        claude_provider = Provider.claude(model="claude-sonnet-4-20250514", api_key="test-key")
        openai_provider = Provider.openai(model="gpt-4o", api_key="test-key")
        ollama_provider = Provider.ollama(model="llama3.2")
        groq_provider = Provider.groq(model="llama-3.3-70b-versatile", api_key="test-key")

        # Verify methods have stop parameter in signature
        import inspect

        for provider in [claude_provider, openai_provider, ollama_provider, groq_provider]:
            sig = inspect.signature(provider.chat)
            assert "stop" in sig.parameters, (
                f"{provider.provider_name} chat() missing stop parameter"
            )

            sig = inspect.signature(provider.chat_stream)
            assert "stop" in sig.parameters, (
                f"{provider.provider_name} chat_stream() missing stop parameter"
            )

    def test_empty_stop_list_handled(self):
        """Empty stop list should be handled gracefully."""
        provider = MockOpenAICompatibleProvider()
        messages = [Message.user("Hello")]

        request_body = provider._build_request(
            messages, effective_model=provider._model, stream=False, stop=[]
        )

        # Empty list should still be included
        assert "stop" in request_body
        assert request_body["stop"] == []

    def test_special_characters_in_stop_preserved(self):
        """Special characters in stop sequences should be preserved."""
        provider = MockOpenAICompatibleProvider()
        messages = [Message.user("Hello")]

        special_stops = ["\n\n", "***", "###", "---", "---END---"]
        request_body = provider._build_request(
            messages, effective_model=provider._model, stream=False, stop=special_stops
        )

        assert request_body["stop"] == special_stops
