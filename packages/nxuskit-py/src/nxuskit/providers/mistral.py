"""Mistral provider implementation."""

import os
from typing import Optional

from nxuskit.providers.openai_compatible import OpenAICompatibleProvider


class MistralProvider(OpenAICompatibleProvider):
    """Provider for Mistral models."""

    DEFAULT_API_URL = "https://api.mistral.ai"

    def __init__(
        self,
        model: str,
        api_key: Optional[str] = None,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
    ):
        """Initialize Mistral provider."""
        if api_key is None:
            api_key = os.getenv("MISTRAL_API_KEY")
        if api_url is None:
            api_url = self.DEFAULT_API_URL

        super().__init__(model, api_key, api_url, timeout, connect_timeout, read_timeout)

    @property
    def provider_name(self) -> str:
        """Get provider name."""
        return "mistral"

    def _build_headers(self) -> dict:
        """Build request headers for Mistral API."""
        return {
            "content-type": "application/json",
            "authorization": f"Bearer {self._api_key}",
        }
