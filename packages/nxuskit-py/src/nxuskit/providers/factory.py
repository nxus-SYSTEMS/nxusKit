"""Factory class for creating LLM provider instances.

When ``use_ffi=True`` the factory returns a synchronous protocol adapter backed by
the nxuskit C ABI shared library.  When ``use_ffi=False`` (the default) the
factory returns the original Python-native HTTP providers.

The environment variable ``NXUSKIT_USE_FFI=1`` can also be used to default
all providers to C ABI routing without changing code.
"""

from __future__ import annotations

import os
from typing import TYPE_CHECKING, Any, Optional

if TYPE_CHECKING:
    from nxuskit.provider import AsyncLLMProvider

from nxuskit.provider import LLMProvider
from nxuskit.providers.claude import ClaudeProvider
from nxuskit.providers.fireworks import FireworksProvider
from nxuskit.providers.groq import GroqProvider
from nxuskit.providers.lmstudio import LMStudioProvider
from nxuskit.providers.mistral import MistralProvider
from nxuskit.providers.ollama import OllamaProvider
from nxuskit.providers.openai import OpenAIProvider
from nxuskit.providers.openrouter import OpenRouterProvider
from nxuskit.providers.perplexity import PerplexityProvider
from nxuskit.providers.together import TogetherProvider
from nxuskit.providers.xai import XaiProvider

# Type alias: factory methods return either native or FFI providers.
AnyProvider = LLMProvider


def _should_use_ffi(use_ffi: Optional[bool]) -> bool:
    """Determine whether to use the C ABI (FFI) path.

    Priority: explicit parameter > NXUSKIT_USE_FFI env var > False.
    """
    if use_ffi is not None:
        return use_ffi
    return os.environ.get("NXUSKIT_USE_FFI", "").lower() in ("1", "true", "yes")


def _make_ffi_provider(
    provider_type: str,
    model: str,
    api_key: Optional[str] = None,
    api_url: Optional[str] = None,
    timeout: float = 30.0,
    license_key: Optional[str] = None,
    *,
    declared_profile: Optional[str] = None,
) -> AnyProvider:
    """Create a provider via the nxuskit C ABI shared library.

    Lazy-imports the FFI module so cffi is only required when actually used.
    """
    from nxuskit._ffi_provider import create_ffi_provider
    from nxuskit._ffi_sync_provider import SyncFFIProvider

    config: dict[str, Any] = {
        "provider_type": provider_type,
        "model": model,
        "timeout_ms": int(timeout * 1000),
    }
    if api_key is not None:
        config["api_key"] = api_key
    if api_url is not None:
        config["base_url"] = api_url
    if license_key is not None:
        config["license_key"] = license_key

    return SyncFFIProvider(
        create_ffi_provider(config, declared_profile=declared_profile),
        model,
        provider_type,
        declared_profile=declared_profile,
    )


class Provider:
    """Factory for creating LLM provider instances.

    Each static method accepts an optional ``use_ffi`` parameter:

    * ``True``  — use the nxuskit C ABI shared library (all domains routed
      through Rust engine).
    * ``False`` — use the native Python HTTP implementation (default).
    * ``None``  — fall back to ``NXUSKIT_USE_FFI`` environment variable.
    """

    @staticmethod
    def create_async(
        provider_type: str,
        model: str,
        *,
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        max_in_flight: int = 4,
        use_ffi: bool = True,
        profile: Optional[str] = None,
    ) -> "AsyncLLMProvider":
        """Construct an async owner; existing calls retain FFI routing.

        Explicit use_ffi=False selects the listed compatible native async profiles.
        Other native profiles fail explicitly; no fallback or live-call guarantee.
        """
        if not use_ffi:
            if provider_type not in (
                "openai",
                "openrouter",
                "perplexity",
                "groq",
                "xai",
                "fireworks",
                "mistral",
                "together",
                "lmstudio",
                "ollama",
                "claude",
            ):
                raise ValueError(
                    "native async currently supports only openai, openrouter, perplexity, "
                    "groq, xai, fireworks, mistral, together, lmstudio, ollama, claude"
                )
            from nxuskit.providers.async_openai import NativeAsyncOpenAIProvider

            return NativeAsyncOpenAIProvider(
                model,
                provider_type=provider_type,
                api_key=api_key,
                api_url=api_url,
                timeout=timeout,
                max_in_flight=max_in_flight,
                profile=profile,
            )
        from nxuskit._ffi_async_provider import AsyncFFIProvider

        return AsyncFFIProvider(
            provider_type,
            model,
            api_key=api_key,
            api_url=api_url,
            timeout=timeout,
            max_in_flight=max_in_flight,
            declared_profile=profile,
        )

    @staticmethod
    def claude(
        model: str = "claude-sonnet-4-20250514",
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
        use_ffi: Optional[bool] = None,
        license_key: Optional[str] = None,
    ) -> AnyProvider:
        """Create a Claude provider instance."""
        if _should_use_ffi(use_ffi):
            return _make_ffi_provider("claude", model, api_key, api_url, timeout, license_key)
        return ClaudeProvider(
            model=model,
            api_key=api_key,
            api_url=api_url,
            timeout=timeout,
            connect_timeout=connect_timeout,
            read_timeout=read_timeout,
        )

    @staticmethod
    def openai(
        model: str = "gpt-4o",
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
        use_ffi: Optional[bool] = None,
        license_key: Optional[str] = None,
    ) -> AnyProvider:
        """Create an OpenAI provider instance."""
        if _should_use_ffi(use_ffi):
            return _make_ffi_provider("openai", model, api_key, api_url, timeout, license_key)
        return OpenAIProvider(
            model=model,
            api_key=api_key,
            api_url=api_url,
            timeout=timeout,
            connect_timeout=connect_timeout,
            read_timeout=read_timeout,
        )

    @staticmethod
    def ollama(
        model: str = "llama3.2",
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
        use_ffi: Optional[bool] = None,
        license_key: Optional[str] = None,
        *,
        profile: Optional[str] = None,
    ) -> AnyProvider:
        """Create an Ollama provider instance."""
        OllamaProvider._validate_profile(profile)
        if _should_use_ffi(use_ffi):
            return _make_ffi_provider(
                "ollama",
                model,
                None,
                api_url,
                timeout,
                license_key,
                declared_profile=profile,
            )
        return OllamaProvider(
            model=model,
            api_url=api_url,
            timeout=timeout,
            connect_timeout=connect_timeout,
            read_timeout=read_timeout,
            profile=profile,
        )

    @staticmethod
    def groq(
        model: str = "llama-3.3-70b-versatile",
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
        use_ffi: Optional[bool] = None,
        license_key: Optional[str] = None,
    ) -> AnyProvider:
        """Create a Groq provider instance."""
        if _should_use_ffi(use_ffi):
            return _make_ffi_provider("groq", model, api_key, api_url, timeout, license_key)
        return GroqProvider(
            model=model,
            api_key=api_key,
            api_url=api_url,
            timeout=timeout,
            connect_timeout=connect_timeout,
            read_timeout=read_timeout,
        )

    @staticmethod
    def xai(
        model: str = "grok-4",
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
        use_ffi: Optional[bool] = None,
        license_key: Optional[str] = None,
    ) -> AnyProvider:
        """Create an xAI Grok provider instance."""
        if _should_use_ffi(use_ffi):
            return _make_ffi_provider("xai", model, api_key, api_url, timeout, license_key)
        return XaiProvider(
            model=model,
            api_key=api_key,
            api_url=api_url,
            timeout=timeout,
            connect_timeout=connect_timeout,
            read_timeout=read_timeout,
        )

    @staticmethod
    def mistral(
        model: str = "mistral-large-latest",
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
        use_ffi: Optional[bool] = None,
        license_key: Optional[str] = None,
    ) -> AnyProvider:
        """Create a Mistral provider instance."""
        if _should_use_ffi(use_ffi):
            return _make_ffi_provider("mistral", model, api_key, api_url, timeout, license_key)
        return MistralProvider(
            model=model,
            api_key=api_key,
            api_url=api_url,
            timeout=timeout,
            connect_timeout=connect_timeout,
            read_timeout=read_timeout,
        )

    @staticmethod
    def fireworks(
        model: str = "accounts/fireworks/models/llama-v3p1-70b-instruct",
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
        use_ffi: Optional[bool] = None,
        license_key: Optional[str] = None,
    ) -> AnyProvider:
        """Create a Fireworks provider instance."""
        if _should_use_ffi(use_ffi):
            return _make_ffi_provider("fireworks", model, api_key, api_url, timeout, license_key)
        return FireworksProvider(
            model=model,
            api_key=api_key,
            api_url=api_url,
            timeout=timeout,
            connect_timeout=connect_timeout,
            read_timeout=read_timeout,
        )

    @staticmethod
    def together(
        model: str = "meta-llama/Llama-3.3-70B-Instruct-Turbo",
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
        use_ffi: Optional[bool] = None,
        license_key: Optional[str] = None,
    ) -> AnyProvider:
        """Create a Together provider instance."""
        if _should_use_ffi(use_ffi):
            return _make_ffi_provider("together", model, api_key, api_url, timeout, license_key)
        return TogetherProvider(
            model=model,
            api_key=api_key,
            api_url=api_url,
            timeout=timeout,
            connect_timeout=connect_timeout,
            read_timeout=read_timeout,
        )

    @staticmethod
    def openrouter(
        model: str = "anthropic/claude-sonnet-4-20250514",
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
        use_ffi: Optional[bool] = None,
        license_key: Optional[str] = None,
    ) -> AnyProvider:
        """Create an OpenRouter provider instance."""
        if _should_use_ffi(use_ffi):
            return _make_ffi_provider("openrouter", model, api_key, api_url, timeout, license_key)
        return OpenRouterProvider(
            model=model,
            api_key=api_key,
            api_url=api_url,
            timeout=timeout,
            connect_timeout=connect_timeout,
            read_timeout=read_timeout,
        )

    @staticmethod
    def perplexity(
        model: str = "llama-3.1-sonar-small-128k-online",
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
        use_ffi: Optional[bool] = None,
        license_key: Optional[str] = None,
    ) -> AnyProvider:
        """Create a Perplexity provider instance."""
        if _should_use_ffi(use_ffi):
            return _make_ffi_provider("perplexity", model, api_key, api_url, timeout, license_key)
        return PerplexityProvider(
            model=model,
            api_key=api_key,
            api_url=api_url,
            timeout=timeout,
            connect_timeout=connect_timeout,
            read_timeout=read_timeout,
        )

    @staticmethod
    def lmstudio(
        model: str = "local-model",
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
        use_ffi: Optional[bool] = None,
        license_key: Optional[str] = None,
    ) -> AnyProvider:
        """Create an LM Studio provider instance."""
        if _should_use_ffi(use_ffi):
            return _make_ffi_provider("lmstudio", model, None, api_url, timeout, license_key)
        return LMStudioProvider(
            model=model,
            api_url=api_url,
            timeout=timeout,
            connect_timeout=connect_timeout,
            read_timeout=read_timeout,
        )
