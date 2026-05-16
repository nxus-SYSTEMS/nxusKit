"""Tests for nxuskit auth helper."""

# Add src to path for direct import
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent / "src"))

from nxuskit.auth import (
    _file_delete,
    _file_get,
    _file_set,
    auth_providers,
    auth_resolve,
    auth_set_credential,
    auth_status,
    auth_status_all,
    masked_preview,
)


class TestMaskedPreview:
    def test_normal(self):
        assert masked_preview("sk-proj-abc123xyz789") == "sk-...z789"

    def test_short(self):
        assert masked_preview("abc") == "****"

    def test_boundary(self):
        assert masked_preview("1234567890") == "123...7890"


class TestProviderRegistry:
    def test_known_providers(self):
        providers = auth_providers()
        assert len(providers) >= 5
        ids = [p.provider_id for p in providers]
        assert "openai" in ids
        assert "claude" in ids
        assert "groq" in ids
        assert "ollama" in ids

    def test_unique_ids(self):
        providers = auth_providers()
        ids = [p.provider_id for p in providers]
        assert len(ids) == len(set(ids))


class TestAuthResolve:
    def test_local_provider(self):
        res = auth_resolve("ollama")
        assert res.source == "none"
        assert not res.has_credential

    def test_explicit_wins(self):
        res = auth_resolve("openai", explicit_key="sk-test")
        assert res.source == "explicit"
        assert res.has_credential

    def test_env_wins(self, monkeypatch):
        monkeypatch.setenv("GROQ_API_KEY", "gsk_test12345678")
        res = auth_resolve("groq")
        assert res.source == "env"
        assert res.has_credential

    def test_unknown_provider(self):
        with pytest.raises(ValueError, match="unknown provider"):
            auth_resolve("nonexistent")


class TestAuthStatus:
    def test_local_provider(self):
        s = auth_status("ollama")
        assert s.status == "not_required"
        assert s.masked_preview is None

    def test_env_status(self, monkeypatch):
        monkeypatch.setenv("GROQ_API_KEY", "gsk_test12345678")
        s = auth_status("groq")
        assert s.status == "authenticated_env"
        assert s.masked_preview == "gsk...5678"
        assert s.source == "env"

    def test_unknown_provider(self):
        with pytest.raises(ValueError, match="unknown provider"):
            auth_status("nonexistent")


class TestAuthSetCredential:
    def test_local_rejected(self):
        with pytest.raises(ValueError, match="does not require"):
            auth_set_credential("ollama", "key")

    def test_unknown_rejected(self):
        with pytest.raises(ValueError, match="unknown provider"):
            auth_set_credential("nonexistent", "key")


class TestFileFallback:
    def test_roundtrip(self, monkeypatch, tmp_path):
        monkeypatch.setenv("NXUSKIT_CREDENTIALS_DIR", str(tmp_path))

        service = "nxuskit-test-py-roundtrip"
        _file_set(service, "test-api-key-py")
        assert _file_get(service) == "test-api-key-py"

        # Verify permissions
        path = tmp_path / f"{service}.key"
        mode = path.stat().st_mode & 0o777
        assert mode == 0o600, f"permissions = {oct(mode)}, want 0600"

        assert _file_delete(service)
        assert _file_get(service) is None


class TestAuthStatusAll:
    def test_returns_all(self):
        statuses = auth_status_all()
        assert len(statuses) >= 5
