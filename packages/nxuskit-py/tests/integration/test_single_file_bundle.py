"""Integration tests for the single-file nxuskit bundle.

These tests verify that the bundled nxuskit_bundle.py file works identically
to the installed package version.

IMPORTANT: These tests use a pytest fixture to load the bundle module.
The bundle must NOT be loaded at module level, as it would override the
nxuskit package in sys.modules and break other tests.
"""

import importlib.util
import sys
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

# Path to the bundled module
bundle_path = Path(__file__).parent.parent.parent / "nxuskit_bundle.py"


@pytest.fixture
def bundle():
    """Load the bundle as a module for testing.

    This fixture loads the bundle into a separate module name to avoid
    conflicting with the installed nxuskit package.
    """
    if not bundle_path.exists():
        pytest.skip(f"Bundle file not found at {bundle_path}")

    # Load bundle with a unique name to avoid conflicts
    spec = importlib.util.spec_from_file_location("nxuskit_bundle_test", bundle_path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class TestBundleImport:
    """Test that the bundle can be imported."""

    def test_bundle_file_exists(self):
        """Bundle file should exist."""
        assert bundle_path.exists(), f"Bundle file not found at {bundle_path}"

    def test_bundle_size_reasonable(self):
        """Bundle should be reasonable size (< 500 KB)."""
        if not bundle_path.exists():
            pytest.skip("Bundle file not found")
        size_kb = bundle_path.stat().st_size / 1024
        assert size_kb < 500, f"Bundle size {size_kb:.1f} KB is too large"

    def test_import_bundle_module(self, bundle):
        """Bundle should be importable as Python module."""
        assert bundle is not None

    def test_provider_factory_available(self, bundle):
        """Provider factory should be available in bundle."""
        assert hasattr(bundle, "Provider")
        assert bundle.Provider is not None

    def test_message_class_available(self, bundle):
        """Message class should be available in bundle."""
        assert hasattr(bundle, "Message")
        assert bundle.Message is not None


class TestBundleProviders:
    """Test that all providers are available in bundle."""

    def test_claude_provider_available(self, bundle):
        """Claude provider should be instantiable from bundle."""
        provider = bundle.Provider.claude(model="test", api_key="test-key")
        assert provider is not None
        assert provider.provider_name == "claude"

    def test_openai_provider_available(self, bundle):
        """OpenAI provider should be instantiable from bundle."""
        provider = bundle.Provider.openai(model="test", api_key="test-key")
        assert provider is not None
        assert provider.provider_name == "openai"

    def test_ollama_provider_available(self, bundle):
        """Ollama provider should be instantiable from bundle."""
        provider = bundle.Provider.ollama(model="test")
        assert provider is not None
        assert provider.provider_name == "ollama"

    def test_groq_provider_available(self, bundle):
        """Groq provider should be instantiable from bundle."""
        provider = bundle.Provider.groq(model="test", api_key="test-key")
        assert provider is not None
        assert provider.provider_name == "groq"

    def test_xai_provider_available(self, bundle):
        """xAI Grok provider should be instantiable from bundle."""
        provider = bundle.Provider.xai(model="grok-4", api_key="test-key")
        assert provider is not None
        assert provider.provider_name == "xai"

    def test_mistral_provider_available(self, bundle):
        """Mistral provider should be instantiable from bundle."""
        provider = bundle.Provider.mistral(model="test", api_key="test-key")
        assert provider is not None
        assert provider.provider_name == "mistral"

    def test_all_providers_available(self, bundle):
        """All 11 providers should be available."""
        providers = [
            "claude",
            "openai",
            "ollama",
            "groq",
            "xai",
            "mistral",
            "fireworks",
            "together",
            "openrouter",
            "perplexity",
            "lmstudio",
        ]
        for provider_name in providers:
            assert hasattr(bundle.Provider, provider_name), (
                f"Provider {provider_name} not found in bundle"
            )


class TestBundleMessageAPI:
    """Test Message API in bundle."""

    def test_create_user_message(self, bundle):
        """Should be able to create user messages."""
        msg = bundle.Message.user("Hello")
        assert msg is not None
        assert msg.content == "Hello"

    def test_create_assistant_message(self, bundle):
        """Should be able to create assistant messages."""
        msg = bundle.Message.assistant("Response")
        assert msg is not None
        assert msg.content == "Response"

    def test_create_system_message(self, bundle):
        """Should be able to create system messages."""
        msg = bundle.Message.system("System prompt")
        assert msg is not None
        assert msg.content == "System prompt"


class TestBundleChatInterface:
    """Test chat interface in bundle."""

    @patch("requests.request")
    def test_chat_method_exists(self, mock_request, bundle):
        """Providers should have chat method."""
        mock_response = MagicMock()
        mock_response.json.return_value = {
            "content": [{"type": "text", "text": "Response"}],
            "usage": {"input_tokens": 10, "output_tokens": 5},
        }
        mock_response.status_code = 200
        mock_request.return_value = mock_response

        provider = bundle.Provider.claude(api_key="test-key")
        assert hasattr(provider, "chat")
        assert callable(provider.chat)

    @patch("requests.request")
    def test_chat_stream_method_exists(self, mock_request, bundle):
        """Providers should have chat_stream method."""
        mock_response = MagicMock()
        mock_response.iter_lines.return_value = [
            b'event: content_block_delta\ndata: {"type":"content_block_delta"}'
        ]
        mock_response.status_code = 200
        mock_request.return_value = mock_response

        provider = bundle.Provider.claude(api_key="test-key")
        assert hasattr(provider, "chat_stream")
        assert callable(provider.chat_stream)


class TestBundleErrorHandling:
    """Test error handling in bundle."""

    def test_authentication_error_available(self, bundle):
        """AuthenticationError should be available."""
        assert hasattr(bundle, "AuthenticationError")

    def test_rate_limit_error_available(self, bundle):
        """RateLimitError should be available."""
        assert hasattr(bundle, "RateLimitError")

    def test_network_error_available(self, bundle):
        """NetworkError should be available."""
        assert hasattr(bundle, "NetworkError")

    def test_provider_error_available(self, bundle):
        """ProviderError should be available."""
        assert hasattr(bundle, "ProviderError")


class TestBundleTypeAnnotations:
    """Test type annotations in bundle."""

    def test_chat_response_type_available(self, bundle):
        """ChatResponse type should be available."""
        assert hasattr(bundle, "ChatResponse")

    def test_token_usage_type_available(self, bundle):
        """TokenUsage type should be available."""
        assert hasattr(bundle, "TokenUsage")

    def test_stream_chunk_type_available(self, bundle):
        """StreamChunk type should be available."""
        assert hasattr(bundle, "StreamChunk")


class TestBundleParameterSupport:
    """Test that bundle providers support required parameters."""

    def test_stop_parameter_supported(self, bundle):
        """Providers should support stop parameter."""
        import inspect

        provider = bundle.Provider.claude(api_key="test-key")

        # Check method signature
        sig = inspect.signature(provider.chat)
        assert "stop" in sig.parameters

    def test_timeout_parameters_supported(self, bundle):
        """Providers should support timeout parameters."""
        provider = bundle.Provider.claude(
            api_key="test-key", connect_timeout=5.0, read_timeout=60.0
        )

        assert provider._connect_timeout == 5.0
        assert provider._read_timeout == 60.0

    @patch("requests.request")
    def test_temperature_parameter_supported(self, mock_request, bundle):
        """Providers should support temperature parameter."""
        mock_response = MagicMock()
        mock_response.json.return_value = {
            "content": [{"type": "text", "text": "Response"}],
            "usage": {"input_tokens": 10, "output_tokens": 5},
        }
        mock_response.status_code = 200
        mock_request.return_value = mock_response

        provider = bundle.Provider.claude(api_key="test-key")
        provider.chat([bundle.Message.user("Hello")], temperature=0.7)

        # Verify request was made with temperature
        call_kwargs = mock_request.call_args.kwargs
        json_data = call_kwargs["json"]
        assert "temperature" in json_data
        assert json_data["temperature"] == 0.7

    @patch("requests.request")
    def test_max_tokens_parameter_supported(self, mock_request, bundle):
        """Providers should support max_tokens parameter."""
        mock_response = MagicMock()
        mock_response.json.return_value = {
            "content": [{"type": "text", "text": "Response"}],
            "usage": {"input_tokens": 10, "output_tokens": 5},
        }
        mock_response.status_code = 200
        mock_request.return_value = mock_response

        provider = bundle.Provider.claude(api_key="test-key")
        provider.chat([bundle.Message.user("Hello")], max_tokens=100)

        # Verify request was made with max_tokens
        call_kwargs = mock_request.call_args.kwargs
        json_data = call_kwargs["json"]
        assert "max_tokens" in json_data
        assert json_data["max_tokens"] == 100


class TestBundleAPICompleteness:
    """Test that bundle has complete API parity with package."""

    def test_provider_factory_signature(self, bundle):
        """Provider factory methods should have correct signatures."""
        import inspect

        factory_methods = [
            "claude",
            "openai",
            "ollama",
            "groq",
            "xai",
            "mistral",
            "fireworks",
            "together",
            "openrouter",
            "perplexity",
            "lmstudio",
        ]

        for method_name in factory_methods:
            method = getattr(bundle.Provider, method_name)
            sig = inspect.signature(method)

            # All methods should have model parameter
            assert "model" in sig.parameters, f"{method_name} missing model param"

            # Most methods should have timeout parameters
            assert "timeout" in sig.parameters, f"{method_name} missing timeout"

    def test_message_api_completeness(self, bundle):
        """Message class should have all message types."""
        # Should have class methods for message creation
        assert hasattr(bundle.Message, "user")
        assert hasattr(bundle.Message, "assistant")
        assert hasattr(bundle.Message, "system")

    def test_public_api_exports(self, bundle):
        """Bundle should export required public APIs."""
        # Should have main classes
        assert hasattr(bundle, "Provider")
        assert hasattr(bundle, "Message")

        # Should have exception classes
        assert hasattr(bundle, "AuthenticationError")
        assert hasattr(bundle, "RateLimitError")
        assert hasattr(bundle, "NetworkError")
        assert hasattr(bundle, "ProviderError")

        # Should have type classes
        assert hasattr(bundle, "ChatResponse")
        assert hasattr(bundle, "TokenUsage")
        assert hasattr(bundle, "StreamChunk")


class TestBundleDocumentation:
    """Test that bundle includes documentation."""

    def test_bundle_has_docstring(self, bundle):
        """Bundle should have module docstring."""
        assert bundle.__doc__ is not None
        assert "nxusKit" in bundle.__doc__

    def test_provider_methods_have_docstrings(self, bundle):
        """Provider factory methods should have docstrings."""
        # Check a few methods
        assert bundle.Provider.claude.__doc__ is not None
        assert bundle.Provider.openai.__doc__ is not None
        assert bundle.Provider.ollama.__doc__ is not None

    def test_message_methods_have_docstrings(self, bundle):
        """Message class methods should have docstrings."""
        assert bundle.Message.user.__doc__ is not None
        assert bundle.Message.assistant.__doc__ is not None
        assert bundle.Message.system.__doc__ is not None
