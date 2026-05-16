"""LM Studio provider implementation."""

import os
from typing import Optional

from nxuskit.providers.openai_compatible import OpenAICompatibleProvider


class LMStudioProvider(OpenAICompatibleProvider):
    """Provider for LM Studio models (local deployment)."""

    DEFAULT_API_URL = "http://localhost:1234"

    def __init__(
        self,
        model: str,
        api_url: Optional[str] = None,
        timeout: float = 30.0,
        connect_timeout: Optional[float] = None,
        read_timeout: Optional[float] = None,
    ):
        """Initialize LM Studio provider.

        LM Studio is a local deployment and does not require an API key.
        """
        if api_url is None:
            api_url = os.getenv("LMSTUDIO_BASE_URL", self.DEFAULT_API_URL)

        # No API key for local deployment
        super().__init__(
            model,
            api_key=None,
            api_url=api_url,
            timeout=timeout,
            connect_timeout=connect_timeout,
            read_timeout=read_timeout,
        )

    @property
    def provider_name(self) -> str:
        """Get provider name."""
        return "lmstudio"

    def _build_headers(self) -> dict:
        """Build request headers for LM Studio API."""
        return {
            "content-type": "application/json",
        }
