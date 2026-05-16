"""Unit tests for retry utilities."""

import time

import pytest

from nxuskit import AuthenticationError, NetworkError, RateLimitError
from nxuskit.retry import (
    AdaptiveRateLimiter,
    RetryConfig,
    RetryIterator,
    retry_on_rate_limit,
    retry_with_backoff,
    should_retry,
)


class TestRetryConfig:
    """Tests for RetryConfig."""

    def test_default_config(self):
        """RetryConfig should have sensible defaults."""
        config = RetryConfig()
        assert config.max_retries == 3
        assert config.initial_delay == 1.0

    def test_custom_config(self):
        """RetryConfig should accept custom values."""
        config = RetryConfig(max_retries=5, initial_delay=0.5)
        assert config.max_retries == 5
        assert config.initial_delay == 0.5

    def test_exponential_backoff(self):
        """RetryConfig should calculate exponential delays."""
        config = RetryConfig(initial_delay=1.0, exponential_base=2.0)
        assert config.get_delay(0) == pytest.approx(1.0, rel=0.5)  # 0.5-1.5x
        assert config.get_delay(1) == pytest.approx(2.0, rel=0.5)
        assert config.get_delay(2) == pytest.approx(4.0, rel=0.5)

    def test_delay_max_cap(self):
        """RetryConfig should cap delays at max_delay."""
        config = RetryConfig(initial_delay=1.0, max_delay=10.0, exponential_base=2.0)
        # At attempt 5, exponential would be 32, should be capped at 10
        delay = config.get_delay(5)
        assert delay <= 10.0 * 1.5  # Account for jitter


class TestShouldRetry:
    """Tests for should_retry function."""

    def test_should_retry_rate_limit(self):
        """should_retry should return True for RateLimitError."""
        error = RateLimitError("Rate limited")
        assert should_retry(error) is True

    def test_should_not_retry_auth_error(self):
        """should_retry should return False for AuthenticationError."""
        error = AuthenticationError("Invalid key")
        assert should_retry(error) is False

    def test_should_retry_network_error(self):
        """should_retry should return True for NetworkError."""
        error = NetworkError("Connection failed")
        assert should_retry(error) is True

    def test_should_not_retry_other_errors(self):
        """should_retry should return False for non-LLMError exceptions."""
        error = ValueError("Some error")
        assert should_retry(error) is False


class TestRetryWithBackoff:
    """Tests for retry_with_backoff function."""

    def test_success_first_try(self):
        """Should return result on first success."""
        call_count = {"count": 0}

        def success_func():
            call_count["count"] += 1
            return "success"

        result = retry_with_backoff(success_func, max_retries=3)
        assert result == "success"
        assert call_count["count"] == 1

    def test_retry_on_retryable_error(self):
        """Should retry on retryable errors."""
        call_count = {"count": 0}

        def fail_then_succeed():
            call_count["count"] += 1
            if call_count["count"] < 2:
                raise NetworkError("Network issue")
            return "success"

        result = retry_with_backoff(fail_then_succeed, max_retries=3)
        assert result == "success"
        assert call_count["count"] == 2

    def test_no_retry_on_non_retryable_error(self):
        """Should not retry on non-retryable errors."""
        call_count = {"count": 0}

        def fail_with_auth_error():
            call_count["count"] += 1
            raise AuthenticationError("Invalid key")

        with pytest.raises(AuthenticationError):
            retry_with_backoff(fail_with_auth_error, max_retries=3)

        assert call_count["count"] == 1

    def test_max_retries_exceeded(self):
        """Should raise error after max retries exceeded."""
        call_count = {"count": 0}

        def always_fail():
            call_count["count"] += 1
            raise NetworkError("Network issue")

        with pytest.raises(NetworkError):
            retry_with_backoff(always_fail, max_retries=2)

        assert call_count["count"] == 3  # initial + 2 retries

    def test_respects_initial_delay(self):
        """Should respect initial delay parameter."""
        start = time.time()

        def instant_fail():
            raise NetworkError("Test")

        with pytest.raises(NetworkError):
            retry_with_backoff(
                instant_fail,
                max_retries=1,
                initial_delay=0.05,
            )

        elapsed = time.time() - start
        # Should have waited at least around the initial delay
        # Be generous with tolerance due to system timing variations
        assert elapsed >= 0.02  # Much looser tolerance


class TestRetryOnRateLimit:
    """Tests for retry_on_rate_limit function."""

    def test_retries_on_rate_limit(self):
        """Should retry on RateLimitError."""
        call_count = {"count": 0}

        def fail_then_succeed():
            call_count["count"] += 1
            if call_count["count"] < 2:
                raise RateLimitError("Rate limited", retry_after=0.01)
            return "success"

        result = retry_on_rate_limit(fail_then_succeed, max_retries=3)
        assert result == "success"
        assert call_count["count"] == 2

    def test_uses_retry_after_from_error(self):
        """Should use retry_after from error if available."""

        def fail_once():
            raise RateLimitError("Rate limited", retry_after=0.05)

        start = time.time()
        with pytest.raises(RateLimitError):
            retry_on_rate_limit(fail_once, max_retries=1)
        elapsed = time.time() - start

        # Should have waited at least the retry_after time
        assert elapsed >= 0.04

    def test_no_retry_on_auth_error(self):
        """Should not retry on AuthenticationError."""

        def fail_with_auth():
            raise AuthenticationError("Invalid key")

        with pytest.raises(AuthenticationError):
            retry_on_rate_limit(fail_with_auth, max_retries=3)

    def test_passes_arguments(self):
        """Should pass arguments to function."""

        def func_with_args(a, b, c=None):
            return f"{a}-{b}-{c}"

        result = retry_on_rate_limit(func_with_args, "x", "y", c="z")
        assert result == "x-y-z"


class TestRetryIterator:
    """Tests for RetryIterator class."""

    def test_wraps_iterator(self):
        """RetryIterator should wrap and yield from iterator."""
        items = [1, 2, 3, 4, 5]
        iterator = RetryIterator(iter(items))

        results = list(iterator)
        assert results == items

    def test_buffers_chunks(self):
        """RetryIterator should buffer chunks."""
        items = [1, 2, 3]
        iterator = RetryIterator(iter(items), buffer_size=10)

        list(iterator)
        assert len(iterator.chunk_buffer) == 3

    def test_handles_exhaustion(self):
        """RetryIterator should handle iterator exhaustion."""
        items = [1, 2, 3]
        iterator = RetryIterator(iter(items))

        list(iterator)
        assert iterator.exhausted is True

        # Second iteration should immediately raise
        with pytest.raises(StopIteration):
            next(iterator)


class TestAdaptiveRateLimiter:
    """Tests for AdaptiveRateLimiter class."""

    def test_initialization(self):
        """AdaptiveRateLimiter should initialize properly."""
        limiter = AdaptiveRateLimiter(default_delay=0.1)
        assert limiter.default_delay == 0.1

    def test_no_wait_on_first_request(self):
        """Should not wait on first request."""
        limiter = AdaptiveRateLimiter()

        start = time.time()

        def quick_func():
            return "result"

        result = limiter.with_rate_limit(quick_func)
        elapsed = time.time() - start

        assert result == "result"
        assert elapsed < 0.05  # Should be fast, no wait

    def test_spaces_out_requests(self):
        """Should space out successive requests."""
        limiter = AdaptiveRateLimiter(default_delay=0.05)

        def quick_func():
            return "result"

        start = time.time()
        limiter.with_rate_limit(quick_func)
        limiter.with_rate_limit(quick_func)
        elapsed = time.time() - start

        # Should have waited at least the default_delay
        assert elapsed >= 0.04

    def test_respects_rate_limit_header(self):
        """Should respect rate limit info from error."""
        limiter = AdaptiveRateLimiter()

        def fail_func():
            raise RateLimitError("Rate limited", retry_after=0.05)

        try:
            limiter.with_rate_limit(fail_func)
        except RateLimitError:
            pass

        # Next request should wait for reset time
        start = time.time()

        def success_func():
            return "result"

        # This should trigger the wait
        result = limiter.with_rate_limit(success_func)
        elapsed = time.time() - start

        assert elapsed >= 0.04  # Account for initial delay + rate limit wait
        assert result == "result"

    def test_preserves_error_on_exception(self):
        """Should propagate errors while recording them."""
        limiter = AdaptiveRateLimiter()

        def fail_func():
            raise ValueError("Test error")

        with pytest.raises(ValueError):
            limiter.with_rate_limit(fail_func)
