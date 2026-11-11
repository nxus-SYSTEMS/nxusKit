"""Exception types for nxuskit."""

from typing import Optional


class LLMError(Exception):
    """Base exception for all LLM-related errors."""

    def __init__(
        self,
        message: str,
        status_code: Optional[int] = None,
        provider: Optional[str] = None,
        model: Optional[str] = None,
    ):
        """Initialize LLMError."""
        super().__init__(message)
        self.status_code = status_code
        self.provider = provider
        self.model = model

    @property
    def is_retryable(self) -> bool:
        """Whether this error suggests a retry is appropriate."""
        return False


class AuthenticationError(LLMError):
    """Raised when authentication fails (e.g., invalid API key)."""

    @property
    def is_retryable(self) -> bool:
        """Authentication errors are not retryable."""
        return False


class RateLimitError(LLMError):
    """Raised when rate limit is exceeded."""

    def __init__(
        self,
        message: str,
        status_code: Optional[int] = None,
        provider: Optional[str] = None,
        model: Optional[str] = None,
        retry_after: Optional[float] = None,
        retry_after_ms: Optional[int] = None,
    ):
        """Initialize RateLimitError with canonical and legacy retry metadata."""
        super().__init__(message, status_code, provider, model)
        if retry_after_ms is None and retry_after is not None:
            retry_after_ms = int(retry_after * 1000)
        elif retry_after is None and retry_after_ms is not None:
            retry_after = retry_after_ms / 1000
        self.code = "rate_limited"
        self.retry_after = retry_after
        self.retry_after_ms = retry_after_ms

    @property
    def is_retryable(self) -> bool:
        """Rate limit errors are retryable."""
        return True


class NetworkError(LLMError):
    """Raised when network communication fails."""

    @property
    def is_retryable(self) -> bool:
        """Network errors are retryable."""
        return True


class TimeoutError(LLMError):
    """Raised when a request times out."""

    @property
    def is_retryable(self) -> bool:
        """Timeout errors are retryable (with potentially longer timeout)."""
        return True


class ProviderError(LLMError):
    """Raised for provider-specific errors."""

    @property
    def is_retryable(self) -> bool:
        """Provider errors may be retryable depending on status code."""
        if self.status_code is None:
            return False
        # 5xx errors are typically retryable
        return 500 <= self.status_code < 600
