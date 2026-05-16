"""In-memory mock provider for testing nxuskit streaming and logprob behaviour."""

from typing import Iterator, List, Optional

from nxuskit.types import (
    ProviderCapabilities,
    StreamChunk,
    StreamLogprobsDelta,
    TokenUsage,
)


class MockProvider:
    """Minimal mock provider for CE-public tests.

    Yields pre-configured :class:`StreamChunk` sequences without any network
    I/O.  Use ``streaming_logprobs`` to inject per-chunk logprob deltas.

    Example::

        from nxuskit.mock import MockProvider
        from nxuskit.types import StreamLogprobsDelta, TokenLogprob

        delta = StreamLogprobsDelta(content=[TokenLogprob(token="Hi", logprob=-0.01)])
        provider = MockProvider(
            chunks=["Hi", " there"],
            streaming_logprobs=[[delta, None]],
        )
        for chunk in provider.chat_stream([]):
            if chunk.logprobs:
                print(chunk.logprobs.content[0].token)
    """

    def __init__(
        self,
        chunks: Optional[List[str]] = None,
        streaming_logprobs: Optional[List[List[Optional[StreamLogprobsDelta]]]] = None,
    ) -> None:
        """
        Args:
            chunks: Text deltas to emit per stream call. Defaults to a
                three-chunk "Mock stream response" sequence.
            streaming_logprobs: Outer list is per-stream-call; inner list is
                per-chunk logprob delta (``None`` = no logprob for that chunk).
                When provided, ``SupportsStreamingLogprobs`` is ``True``.
        """
        self._chunks = chunks or ["Mock ", "stream ", "response"]
        self._streaming_logprobs: List[List[Optional[StreamLogprobsDelta]]] = (
            streaming_logprobs or []
        )
        self._call_index = 0

    @property
    def provider_name(self) -> str:
        return "mock"

    def get_capabilities(self) -> ProviderCapabilities:
        return ProviderCapabilities(
            supports_streaming=True,
            supports_logprobs=True,
            supports_streaming_logprobs=bool(self._streaming_logprobs),
        )

    def chat_stream(self, messages: list, **_kwargs: object) -> Iterator[StreamChunk]:
        """Yield pre-configured chunks, injecting logprob deltas if configured."""
        call_idx = self._call_index
        self._call_index += 1

        logprobs_seq: List[Optional[StreamLogprobsDelta]] = []
        if call_idx < len(self._streaming_logprobs):
            logprobs_seq = self._streaming_logprobs[call_idx]

        for i, text in enumerate(self._chunks):
            lp: Optional[StreamLogprobsDelta] = None
            if i < len(logprobs_seq):
                lp = logprobs_seq[i]

            is_last = i == len(self._chunks) - 1
            yield StreamChunk(
                delta=text,
                finish_reason="stop" if is_last else None,
                usage=TokenUsage(input_tokens=5, output_tokens=3) if is_last else None,
                logprobs=lp,
            )
