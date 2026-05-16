"""Tests for the FFI provider layer.

These tests require libnxuskit to be available. They exercise provider
creation, synchronous chat, and response parsing through the FFI bridge.

Run with: pytest tests/test_ffi_provider.py -v
"""

import pytest

# Guard: skip all tests if nxuskit library is not available
try:
    from nxuskit._ffi_errors import ConfigError
    from nxuskit._ffi_provider import FFIProvider, create_ffi_provider
    from nxuskit._ffi_types import ChatResponse

    HAS_NXUSKIT = True
except (OSError, Exception):
    HAS_NXUSKIT = False

pytestmark = pytest.mark.skipif(
    not HAS_NXUSKIT,
    reason="nxuskit shared library not available",
)


class TestCreateProvider:
    """Test provider creation via FFI."""

    def test_create_mock_provider(self):
        with create_ffi_provider({"provider_type": "mock"}) as p:
            assert isinstance(p, FFIProvider)

    def test_create_loopback_provider(self):
        with create_ffi_provider({"provider_type": "loopback"}) as p:
            assert isinstance(p, FFIProvider)

    def test_missing_provider_type_raises_config_error(self):
        with pytest.raises(ConfigError, match="provider_type"):
            create_ffi_provider({})

    def test_invalid_provider_type_raises_config_error(self):
        with pytest.raises(ConfigError):
            create_ffi_provider({"provider_type": "nonexistent_xyz"})

    def test_context_manager_closes_provider(self):
        p = create_ffi_provider({"provider_type": "mock"})
        with p:
            pass
        assert p._closed

    def test_double_close_is_safe(self):
        p = create_ffi_provider({"provider_type": "mock"})
        p.close()
        p.close()  # Should not raise


class TestSyncChat:
    """Test synchronous chat via FFI."""

    def test_chat_with_mock_returns_response(self):
        with create_ffi_provider({"provider_type": "mock"}) as p:
            resp = p.chat(
                {
                    "model": "test",
                    "messages": [{"role": "user", "content": "hello"}],
                }
            )
            assert isinstance(resp, ChatResponse)
            assert isinstance(resp.content, str)

    def test_chat_with_loopback_echoes_content(self):
        with create_ffi_provider({"provider_type": "loopback"}) as p:
            resp = p.chat(
                {
                    "model": "echo",
                    "messages": [{"role": "user", "content": "echo this back"}],
                }
            )
            assert "echo this back" in resp.content

    def test_chat_after_close_raises(self):
        p = create_ffi_provider({"provider_type": "mock"})
        p.close()
        with pytest.raises(ConfigError, match="closed"):
            p.chat({"model": "test", "messages": [{"role": "user", "content": "hi"}]})

    def test_chat_response_has_model_field(self):
        with create_ffi_provider({"provider_type": "mock"}) as p:
            resp = p.chat(
                {
                    "model": "test-model",
                    "messages": [{"role": "user", "content": "hello"}],
                }
            )
            assert isinstance(resp.model, str)
