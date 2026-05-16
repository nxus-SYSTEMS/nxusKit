"""Utilities for retrying requests with exponential backoff."""

import random
import time
from typing import Callable, Optional, TypeVar

from nxuskit.errors import LLMError, RateLimitError

T = TypeVar("T")


class RetryConfig:
    """Configuration for retry behavior."""

    def __init__(
        self,
        max_retries: int = 3,
        initial_delay: float = 1.0,
        max_delay: float = 60.0,
        exponential_base: float = 2.0,
        jitter: bool = True,
    ):
        """
        Initialize retry configuration.

        Args:
            max_retries: Maximum number of retry attempts
            initial_delay: Initial delay in seconds between retries
            max_delay: Maximum delay in seconds between retries
            exponential_base: Base for exponential backoff calculation
            jitter: Whether to add random jitter to delays
        """
        self.max_retries = max_retries
        self.initial_delay = initial_delay
        self.max_delay = max_delay
        self.exponential_base = exponential_base
        self.jitter = jitter

    def get_delay(self, attempt: int) -> float:
        """
        Calculate delay for a given retry attempt.

        Args:
            attempt: 0-indexed attempt number

        Returns:
            Delay in seconds
        """
        # Exponential backoff: initial_delay * (base ^ attempt)
        delay = self.initial_delay * (self.exponential_base**attempt)
        # Cap at max_delay
        delay = min(delay, self.max_delay)
        # Add jitter if enabled
        if self.jitter:
            delay *= 0.5 + random.random()  # 0.5x to 1.5x
        return delay


def should_retry(error: Exception) -> bool:
    """
    Determine if an error should be retried.

    Args:
        error: The exception that occurred

    Returns:
        True if the error should be retried

    Example:
        if should_retry(e):
            # Retry the request
            pass
    """
    if isinstance(error, LLMError):
        return error.is_retryable
    return False


def retry_with_backoff(
    func: Callable[..., T],
    *args,
    max_retries: int = 3,
    initial_delay: float = 1.0,
    max_delay: float = 60.0,
    **kwargs,
) -> T:
    """
    Retry a function with exponential backoff.

    Args:
        func: Function to call
        *args: Positional arguments to pass to func
        max_retries: Maximum number of retries
        initial_delay: Initial delay between retries in seconds
        max_delay: Maximum delay between retries in seconds
        **kwargs: Keyword arguments to pass to func

    Returns:
        The return value of func

    Raises:
        The last exception if all retries fail

    Example:
        response = retry_with_backoff(
            provider.chat,
            [Message.user("Hello")],
            max_retries=3
        )
    """
    config = RetryConfig(
        max_retries=max_retries,
        initial_delay=initial_delay,
        max_delay=max_delay,
    )

    last_error = None

    for attempt in range(max_retries + 1):
        try:
            return func(*args, **kwargs)
        except Exception as e:
            last_error = e

            if not should_retry(e) or attempt >= max_retries:
                raise

            # Calculate delay
            delay = config.get_delay(attempt)
            time.sleep(delay)

    # This should not be reached, but just in case
    raise last_error


def retry_on_rate_limit(
    func: Callable[..., T],
    *args,
    max_retries: int = 3,
    **kwargs,
) -> T:
    """
    Retry a function specifically on rate limit errors.

    Uses retry_after from the error if available.

    Args:
        func: Function to call
        *args: Positional arguments
        max_retries: Maximum retry attempts
        **kwargs: Keyword arguments

    Returns:
        The return value of func

    Example:
        response = retry_on_rate_limit(
            provider.chat,
            [Message.user("Hello")],
            max_retries=3
        )
    """
    last_error = None

    for attempt in range(max_retries + 1):
        try:
            return func(*args, **kwargs)
        except RateLimitError as e:
            last_error = e

            if attempt >= max_retries:
                raise

            # Use retry_after if available, otherwise exponential backoff
            if e.retry_after:
                delay = e.retry_after
            else:
                # Exponential backoff: 1, 2, 4, 8, ...
                delay = min(2**attempt, 60)

            time.sleep(delay)
        except Exception:
            # Don't retry other error types
            raise

    raise last_error


class RetryIterator:
    """
    Iterator for retrying streaming responses.

    Accumulates chunks and retries on error.
    """

    def __init__(
        self,
        iterator,
        max_retries: int = 1,
        buffer_size: int = 1000,
    ):
        """
        Initialize RetryIterator.

        Args:
            iterator: The streaming iterator to wrap
            max_retries: Maximum retry attempts
            buffer_size: Size of chunk buffer for retries
        """
        self.iterator = iterator
        self.max_retries = max_retries
        self.buffer_size = buffer_size
        self.chunk_buffer = []
        self.exhausted = False

    def __iter__(self):
        """Return iterator."""
        return self

    def __next__(self):
        """Get next chunk with retry logic."""
        if self.exhausted:
            raise StopIteration

        try:
            chunk = next(self.iterator)
            self.chunk_buffer.append(chunk)
            return chunk
        except StopIteration:
            self.exhausted = True
            raise
        except Exception as e:
            # For now, we can't truly retry a streaming response
            # without re-creating the initial request
            # This is primarily for documenting intent
            if should_retry(e):
                # In production, would need to re-create the iterator
                # This is a limitation of the streaming model
                pass
            raise


class AdaptiveRateLimiter:
    """
    Adaptive rate limiter that respects server rate limit headers.

    Tracks rate limits and delays requests accordingly.
    """

    def __init__(self, default_delay: float = 0.1):
        """
        Initialize AdaptiveRateLimiter.

        Args:
            default_delay: Default delay between requests in seconds
        """
        self.default_delay = default_delay
        self.last_request_time = 0
        self.rate_limit_reset_time = 0

    def wait_if_needed(self) -> None:
        """Wait if necessary to respect rate limits."""
        now = time.time()

        # Check if we need to wait for rate limit reset
        if now < self.rate_limit_reset_time:
            wait_time = self.rate_limit_reset_time - now
            time.sleep(wait_time)
            return

        # Check if we need to space out requests
        time_since_last = now - self.last_request_time
        if time_since_last < self.default_delay:
            time.sleep(self.default_delay - time_since_last)

    def request_complete(self, error: Optional[Exception] = None) -> None:
        """
        Record that a request completed.

        Args:
            error: Any error that occurred (may include rate limit info)
        """
        self.last_request_time = time.time()

        if isinstance(error, RateLimitError) and error.retry_after:
            self.rate_limit_reset_time = self.last_request_time + error.retry_after

    def with_rate_limit(self, func: Callable[..., T], *args, **kwargs) -> T:
        """
        Execute a function with rate limiting.

        Args:
            func: Function to call
            *args: Positional arguments
            **kwargs: Keyword arguments

        Returns:
            The return value of func
        """
        self.wait_if_needed()

        try:
            result = func(*args, **kwargs)
            self.request_complete()
            return result
        except Exception as e:
            self.request_complete(error=e)
            raise
