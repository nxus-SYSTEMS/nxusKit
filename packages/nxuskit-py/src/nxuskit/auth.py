"""Auth helper — credential storage, resolution, and status.

Provides set/remove/resolve/status operations for provider API keys.
Credential resolution follows deterministic precedence:
  explicit > env var > OS credential store > none

Uses the `keyring` library for OS credential store access. Falls back
to file-based storage when the store is unavailable.
"""

from __future__ import annotations

import os
import stat
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

try:
    import keyring as _keyring
except ImportError:
    _keyring = None  # type: ignore[assignment]


# ── Types ─────────────────────────────────────────────────────────


@dataclass
class ProviderAuthMetadata:
    """Authentication metadata for a provider."""

    provider_id: str
    display_name: str
    env_var_name: str
    auth_required: bool
    dashboard_url: Optional[str] = None
    oauth_capable: bool = False
    auth_methods: list[str] = field(default_factory=list)
    credential_service_name: str = ""


@dataclass
class AuthResolution:
    """Credential resolution result."""

    provider_id: str
    source: str  # "explicit", "env", "store", "none"
    has_credential: bool


@dataclass
class AuthStatus:
    """Auth status for a provider."""

    provider_id: str
    status: str  # "authenticated_explicit/env/store", "not_authenticated", "not_required"
    masked_preview: Optional[str] = None
    source: Optional[str] = None
    dashboard_url: Optional[str] = None


# ── Provider Registry ─────────────────────────────────────────────

_PROVIDERS: list[ProviderAuthMetadata] = [
    ProviderAuthMetadata(
        "openai",
        "OpenAI / GPT",
        "OPENAI_API_KEY",
        True,
        "https://platform.openai.com/api-keys",
        False,
        ["api_key"],
        "nxuskit-openai",
    ),
    ProviderAuthMetadata(
        "claude",
        "Anthropic / Claude",
        "ANTHROPIC_API_KEY",
        True,
        "https://console.anthropic.com/settings/keys",
        False,
        ["api_key"],
        "nxuskit-claude",
    ),
    ProviderAuthMetadata(
        "groq",
        "Groq",
        "GROQ_API_KEY",
        True,
        "https://console.groq.com/keys",
        False,
        ["api_key"],
        "nxuskit-groq",
    ),
    ProviderAuthMetadata(
        "xai",
        "xAI Grok",
        "XAI_API_KEY",
        True,
        "https://console.x.ai/team/default/api-keys",
        False,
        ["api_key"],
        "nxuskit-xai",
    ),
    ProviderAuthMetadata(
        "ollama", "Ollama", "OLLAMA_HOST", False, None, False, [], "nxuskit-ollama"
    ),
    ProviderAuthMetadata(
        "lm-studio", "LM Studio", "LM_STUDIO_HOST", False, None, False, [], "nxuskit-lm-studio"
    ),
    ProviderAuthMetadata(
        "mistral",
        "Mistral AI",
        "MISTRAL_API_KEY",
        True,
        "https://console.mistral.ai/api-keys",
        False,
        ["api_key"],
        "nxuskit-mistral",
    ),
    ProviderAuthMetadata(
        "fireworks",
        "Fireworks AI",
        "FIREWORKS_API_KEY",
        True,
        "https://fireworks.ai/account/api-keys",
        False,
        ["api_key"],
        "nxuskit-fireworks",
    ),
    ProviderAuthMetadata(
        "together",
        "Together AI",
        "TOGETHER_API_KEY",
        True,
        "https://api.together.ai/settings/api-keys",
        False,
        ["api_key"],
        "nxuskit-together",
    ),
    ProviderAuthMetadata(
        "openrouter",
        "OpenRouter",
        "OPENROUTER_API_KEY",
        True,
        "https://openrouter.ai/settings/keys",
        False,
        ["api_key"],
        "nxuskit-openrouter",
    ),
    ProviderAuthMetadata(
        "perplexity",
        "Perplexity",
        "PERPLEXITY_API_KEY",
        True,
        "https://www.perplexity.ai/settings/api",
        False,
        ["api_key"],
        "nxuskit-perplexity",
    ),
]

_PROVIDER_MAP: dict[str, ProviderAuthMetadata] = {p.provider_id: p for p in _PROVIDERS}


def _lookup(provider_id: str) -> ProviderAuthMetadata:
    meta = _PROVIDER_MAP.get(provider_id)
    if meta is None:
        raise ValueError(f"unknown provider: {provider_id}")
    return meta


# ── Credential Store Backend ──────────────────────────────────────


def _keyring_set(service: str, key: str) -> bool:
    if _keyring is None:
        return False
    try:
        _keyring.set_password(service, "default", key)
        return True
    except Exception:
        return False


def _keyring_get(service: str) -> Optional[str]:
    if _keyring is None:
        return None
    try:
        return _keyring.get_password(service, "default")
    except Exception:
        return None


def _keyring_delete(service: str) -> bool:
    if _keyring is None:
        return False
    try:
        _keyring.delete_password(service, "default")
        return True
    except Exception:
        return False


# ── File-based Fallback ───────────────────────────────────────────


def _credential_dir() -> Path:
    env_dir = os.environ.get("NXUSKIT_CREDENTIALS_DIR")
    if env_dir:
        return Path(env_dir)
    home = Path.home()
    return home / ".nxuskit" / "credentials"


def _file_set(service: str, key: str) -> None:
    d = _credential_dir()
    d.mkdir(parents=True, exist_ok=True)
    p = d / f"{service}.key"
    p.write_text(key)
    p.chmod(stat.S_IRUSR | stat.S_IWUSR)  # 0600


def _file_get(service: str) -> Optional[str]:
    p = _credential_dir() / f"{service}.key"
    if not p.exists():
        return None
    return p.read_text().strip()


def _file_delete(service: str) -> bool:
    p = _credential_dir() / f"{service}.key"
    if not p.exists():
        return False
    p.unlink()
    return True


# ── Masked Preview ────────────────────────────────────────────────


def masked_preview(key: str) -> str:
    """Generate a masked preview: first 3 + '...' + last 4 chars."""
    if len(key) < 10:
        return "****"
    return f"{key[:3]}...{key[-4:]}"


# ── Public API ────────────────────────────────────────────────────


def auth_set_credential(provider_id: str, api_key: str) -> None:
    """Store a credential for a provider."""
    meta = _lookup(provider_id)
    if not meta.auth_required:
        raise ValueError(f"provider '{provider_id}' does not require authentication")

    if not _keyring_set(meta.credential_service_name, api_key):
        _file_set(meta.credential_service_name, api_key)


def auth_remove_credential(provider_id: str) -> None:
    """Remove a stored credential for a provider."""
    meta = _lookup(provider_id)
    kr = _keyring_delete(meta.credential_service_name)
    fi = _file_delete(meta.credential_service_name)
    if not kr and not fi:
        raise ValueError(f"no credential found for '{provider_id}'")


def auth_resolve(provider_id: str, explicit_key: Optional[str] = None) -> AuthResolution:
    """Resolve a credential using deterministic precedence."""
    meta = _lookup(provider_id)

    if not meta.auth_required:
        return AuthResolution(provider_id, "none", False)

    if explicit_key is not None:
        return AuthResolution(provider_id, "explicit", True)

    if os.environ.get(meta.env_var_name):
        return AuthResolution(provider_id, "env", True)

    if _keyring_get(meta.credential_service_name) is not None:
        return AuthResolution(provider_id, "store", True)
    if _file_get(meta.credential_service_name) is not None:
        return AuthResolution(provider_id, "store", True)

    return AuthResolution(provider_id, "none", False)


def auth_status(provider_id: str) -> AuthStatus:
    """Get auth status for a single provider."""
    meta = _lookup(provider_id)

    if not meta.auth_required:
        return AuthStatus(provider_id, "not_required", dashboard_url=meta.dashboard_url)

    val = os.environ.get(meta.env_var_name)
    if val:
        return AuthStatus(
            provider_id, "authenticated_env", masked_preview(val), "env", meta.dashboard_url
        )

    val = _keyring_get(meta.credential_service_name)
    if val is None:
        val = _file_get(meta.credential_service_name)
    if val is not None:
        return AuthStatus(
            provider_id, "authenticated_store", masked_preview(val), "store", meta.dashboard_url
        )

    return AuthStatus(provider_id, "not_authenticated", dashboard_url=meta.dashboard_url)


def auth_status_all() -> list[AuthStatus]:
    """Get auth status for all known providers."""
    return [auth_status(p.provider_id) for p in _PROVIDERS]


def auth_providers() -> list[ProviderAuthMetadata]:
    """Get metadata for all known providers."""
    return list(_PROVIDERS)
