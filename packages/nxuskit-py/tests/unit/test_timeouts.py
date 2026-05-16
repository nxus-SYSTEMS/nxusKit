"""Unit tests for configurable timeout support across all providers."""

from unittest.mock import MagicMock, patch

from nxuskit import Message, Provider


class TestTimeoutConfiguration:
    """Test timeout parameter configuration in providers."""

    def test_default_timeout_values(self):
        """Providers should use default timeout of 30 seconds."""
        from nxuskit.providers.claude import ClaudeProvider

        provider = ClaudeProvider(model="claude-sonnet-4-20250514", api_key="test-key")

        # Check internal timeout values
        assert provider._connect_timeout == 30.0
        assert provider._read_timeout == 30.0

    def test_custom_timeout_value(self):
        """Providers should accept custom timeout value."""
        from nxuskit.providers.claude import ClaudeProvider

        provider = ClaudeProvider(
            model="claude-sonnet-4-20250514", api_key="test-key", timeout=60.0
        )

        assert provider._connect_timeout == 60.0
        assert provider._read_timeout == 60.0

    def test_separate_connect_timeout(self):
        """Providers should support separate connect timeout."""
        from nxuskit.providers.claude import ClaudeProvider

        provider = ClaudeProvider(
            model="claude-sonnet-4-20250514", api_key="test-key", timeout=30.0, connect_timeout=5.0
        )

        assert provider._connect_timeout == 5.0
        assert provider._read_timeout == 30.0

    def test_separate_read_timeout(self):
        """Providers should support separate read timeout."""
        from nxuskit.providers.claude import ClaudeProvider

        provider = ClaudeProvider(
            model="claude-sonnet-4-20250514", api_key="test-key", timeout=30.0, read_timeout=90.0
        )

        assert provider._connect_timeout == 30.0
        assert provider._read_timeout == 90.0

    def test_both_separate_timeouts(self):
        """Providers should support separate connect and read timeouts."""
        from nxuskit.providers.claude import ClaudeProvider

        provider = ClaudeProvider(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            timeout=30.0,
            connect_timeout=5.0,
            read_timeout=120.0,
        )

        assert provider._connect_timeout == 5.0
        assert provider._read_timeout == 120.0

    @patch("requests.request")
    def test_timeout_passed_to_requests(self, mock_request):
        """Timeout should be passed as tuple to requests library."""
        from nxuskit.providers.claude import ClaudeProvider

        mock_response = MagicMock()
        mock_response.json.return_value = {
            "content": [{"type": "text", "text": "Response"}],
            "usage": {"input_tokens": 10, "output_tokens": 5},
        }
        mock_response.status_code = 200
        mock_request.return_value = mock_response

        provider = ClaudeProvider(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            timeout=30.0,
            connect_timeout=5.0,
            read_timeout=90.0,
        )

        provider.chat([Message.user("Hello")])

        # Verify timeout was passed as (connect, read) tuple
        assert mock_request.called
        call_kwargs = mock_request.call_args.kwargs
        assert call_kwargs["timeout"] == (5.0, 90.0)

    @patch("requests.request")
    def test_default_timeout_tuple(self, mock_request):
        """Default timeouts should be passed as tuple."""
        from nxuskit.providers.claude import ClaudeProvider

        mock_response = MagicMock()
        mock_response.json.return_value = {
            "content": [{"type": "text", "text": "Response"}],
            "usage": {"input_tokens": 10, "output_tokens": 5},
        }
        mock_response.status_code = 200
        mock_request.return_value = mock_response

        provider = ClaudeProvider(model="claude-sonnet-4-20250514", api_key="test-key")

        provider.chat([Message.user("Hello")])

        # Default should be (30.0, 30.0)
        call_kwargs = mock_request.call_args.kwargs
        assert call_kwargs["timeout"] == (30.0, 30.0)


class TestTimeoutAcrossAllProviders:
    """Test timeout support across all providers."""

    def test_claude_timeout_support(self):
        """Claude provider should support timeout parameters."""
        from nxuskit.providers.claude import ClaudeProvider

        provider = ClaudeProvider(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            connect_timeout=10.0,
            read_timeout=60.0,
        )

        assert provider._connect_timeout == 10.0
        assert provider._read_timeout == 60.0

    def test_openai_timeout_support(self):
        """OpenAI provider should support timeout parameters."""
        from nxuskit.providers.openai import OpenAIProvider

        provider = OpenAIProvider(
            model="gpt-4o", api_key="test-key", connect_timeout=8.0, read_timeout=120.0
        )

        assert provider._connect_timeout == 8.0
        assert provider._read_timeout == 120.0

    def test_ollama_timeout_support(self):
        """Ollama provider should support timeout parameters."""
        from nxuskit.providers.ollama import OllamaProvider

        provider = OllamaProvider(model="llama3.2", connect_timeout=15.0, read_timeout=300.0)

        assert provider._connect_timeout == 15.0
        assert provider._read_timeout == 300.0

    def test_groq_timeout_support(self):
        """Groq provider should support timeout parameters."""
        from nxuskit.providers.groq import GroqProvider

        provider = GroqProvider(
            model="llama-3.3-70b-versatile",
            api_key="test-key",
            connect_timeout=6.0,
            read_timeout=180.0,
        )

        assert provider._connect_timeout == 6.0
        assert provider._read_timeout == 180.0

    def test_mistral_timeout_support(self):
        """Mistral provider should support timeout parameters."""
        from nxuskit.providers.mistral import MistralProvider

        provider = MistralProvider(model="mistral-large-latest", api_key="test-key", timeout=45.0)

        assert provider._connect_timeout == 45.0
        assert provider._read_timeout == 45.0

    def test_factory_provides_timeout_parameters(self):
        """Factory methods should accept timeout parameters."""
        providers = [
            Provider.claude(
                model="claude-sonnet-4-20250514",
                api_key="key",
                connect_timeout=5.0,
                read_timeout=60.0,
            ),
            Provider.openai(model="gpt-4o", api_key="key", connect_timeout=8.0, read_timeout=120.0),
            Provider.ollama(model="llama3.2", connect_timeout=15.0, read_timeout=300.0),
            Provider.groq(model="llama-3.3-70b-versatile", api_key="key", connect_timeout=6.0),
            Provider.mistral(model="mistral-large-latest", api_key="key", timeout=45.0),
        ]

        # Verify first few providers
        assert providers[0]._connect_timeout == 5.0
        assert providers[0]._read_timeout == 60.0

        assert providers[1]._connect_timeout == 8.0
        assert providers[1]._read_timeout == 120.0

        assert providers[2]._connect_timeout == 15.0
        assert providers[2]._read_timeout == 300.0

        assert providers[3]._connect_timeout == 6.0
        assert providers[4]._connect_timeout == 45.0


class TestTimeoutEdgeCases:
    """Test edge cases for timeout configuration."""

    def test_zero_timeout(self):
        """Providers should accept zero timeout (immediate timeout)."""
        from nxuskit.providers.claude import ClaudeProvider

        provider = ClaudeProvider(model="claude-sonnet-4-20250514", api_key="test-key", timeout=0.0)

        assert provider._connect_timeout == 0.0
        assert provider._read_timeout == 0.0

    def test_very_large_timeout(self):
        """Providers should accept very large timeout values."""
        from nxuskit.providers.claude import ClaudeProvider

        provider = ClaudeProvider(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            timeout=3600.0,  # 1 hour
        )

        assert provider._connect_timeout == 3600.0
        assert provider._read_timeout == 3600.0

    def test_float_timeout_values(self):
        """Providers should accept fractional timeout values."""
        from nxuskit.providers.claude import ClaudeProvider

        provider = ClaudeProvider(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            connect_timeout=0.5,
            read_timeout=2.5,
        )

        assert provider._connect_timeout == 0.5
        assert provider._read_timeout == 2.5

    def test_connect_timeout_none_uses_total(self):
        """connect_timeout=None should fall back to total timeout."""
        from nxuskit.providers.claude import ClaudeProvider

        provider = ClaudeProvider(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            timeout=45.0,
            connect_timeout=None,
            read_timeout=120.0,
        )

        assert provider._connect_timeout == 45.0  # Falls back to timeout
        assert provider._read_timeout == 120.0

    def test_read_timeout_none_uses_total(self):
        """read_timeout=None should fall back to total timeout."""
        from nxuskit.providers.claude import ClaudeProvider

        provider = ClaudeProvider(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            timeout=45.0,
            connect_timeout=10.0,
            read_timeout=None,
        )

        assert provider._connect_timeout == 10.0
        assert provider._read_timeout == 45.0  # Falls back to timeout

    def test_both_separate_timeouts_none_uses_total(self):
        """Both None should use total timeout."""
        from nxuskit.providers.claude import ClaudeProvider

        provider = ClaudeProvider(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            timeout=50.0,
            connect_timeout=None,
            read_timeout=None,
        )

        assert provider._connect_timeout == 50.0
        assert provider._read_timeout == 50.0


class TestTimeoutWithStreaming:
    """Test timeout behavior with streaming requests."""

    @patch("requests.request")
    def test_timeout_passed_in_streaming_request(self, mock_request):
        """Timeout should be passed in streaming requests."""
        from nxuskit.providers.claude import ClaudeProvider

        mock_response = MagicMock()
        mock_response.iter_lines.return_value = [
            b'event: content_block_delta\ndata: {"type":"content_block_delta","delta":{"type":"text_delta","text":"Hello"}}',
            b'event: message_stop\ndata: {"type":"message_stop"}',
        ]
        mock_response.status_code = 200
        mock_request.return_value = mock_response

        provider = ClaudeProvider(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            connect_timeout=4.0,
            read_timeout=180.0,
        )

        list(provider.chat_stream([Message.user("Hello")]))

        # Verify timeout was passed
        assert mock_request.called
        call_kwargs = mock_request.call_args.kwargs
        assert call_kwargs["timeout"] == (4.0, 180.0)
        assert call_kwargs["stream"] is True


class TestTimeoutDocumentation:
    """Test that timeout parameters are documented."""

    def test_factory_claude_timeout_in_signature(self):
        """Claude factory should have timeout parameters."""
        import inspect

        from nxuskit.providers.factory import Provider

        sig = inspect.signature(Provider.claude)
        assert "timeout" in sig.parameters
        assert "connect_timeout" in sig.parameters
        assert "read_timeout" in sig.parameters

    def test_factory_openai_timeout_in_signature(self):
        """OpenAI factory should have timeout parameters."""
        import inspect

        from nxuskit.providers.factory import Provider

        sig = inspect.signature(Provider.openai)
        assert "timeout" in sig.parameters
        assert "connect_timeout" in sig.parameters
        assert "read_timeout" in sig.parameters

    def test_factory_ollama_timeout_in_signature(self):
        """Ollama factory should have timeout parameters."""
        import inspect

        from nxuskit.providers.factory import Provider

        sig = inspect.signature(Provider.ollama)
        assert "timeout" in sig.parameters
        assert "connect_timeout" in sig.parameters
        assert "read_timeout" in sig.parameters

    def test_all_factory_methods_have_timeout_parameters(self):
        """All factory methods should have timeout parameters."""
        import inspect

        from nxuskit.providers.factory import Provider

        factory_methods = [
            "claude",
            "openai",
            "ollama",
            "groq",
            "mistral",
            "fireworks",
            "together",
            "openrouter",
            "perplexity",
            "lmstudio",
        ]

        for method_name in factory_methods:
            method = getattr(Provider, method_name)
            sig = inspect.signature(method)
            assert "timeout" in sig.parameters, f"{method_name} missing timeout"
            assert "connect_timeout" in sig.parameters, f"{method_name} missing connect_timeout"
            assert "read_timeout" in sig.parameters, f"{method_name} missing read_timeout"
