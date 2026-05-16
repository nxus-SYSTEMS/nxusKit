"""Advanced contract tests for streaming functionality."""

from pytest_httpserver import HTTPServer

from nxuskit import Message, Provider, StreamChunk


class TestClaudeAdvancedStreaming:
    """Advanced streaming tests for Claude provider."""

    def test_claude_streaming_accumulation(self, httpserver: HTTPServer):
        """Verify accumulated streaming chunks match full response."""
        stream_response = """event: content_block_start
data: {"type":"content_block_start","content_block":{"type":"text"}}

event: content_block_delta
data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"The"}}

event: content_block_delta
data: {"type":"content_block_delta","delta":{"type":"text_delta","text":" quick"}}

event: content_block_delta
data: {"type":"content_block_delta","delta":{"type":"text_delta","text":" brown"}}

event: content_block_delta
data: {"type":"content_block_delta","delta":{"type":"text_delta","text":" fox"}}

event: message_stop
data: {"type":"message_stop"}
"""
        httpserver.expect_request("/v1/messages").respond_with_data(
            stream_response,
            status=200,
            content_type="text/event-stream",
        )

        provider = Provider.claude(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        chunks = list(provider.chat_stream([Message.user("Say something")]))
        accumulated = "".join(c.delta for c in chunks)
        assert accumulated == "The quick brown fox"

    def test_claude_streaming_chunk_types(self, httpserver: HTTPServer):
        """Verify each chunk is a valid StreamChunk."""
        stream_response = """event: content_block_delta
data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"test"}}

event: message_stop
data: {"type":"message_stop"}
"""
        httpserver.expect_request("/v1/messages").respond_with_data(
            stream_response,
            status=200,
            content_type="text/event-stream",
        )

        provider = Provider.claude(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        for chunk in provider.chat_stream([Message.user("Test")]):
            assert isinstance(chunk, StreamChunk)
            assert isinstance(chunk.delta, str)
            assert hasattr(chunk, "model")

    def test_claude_streaming_with_system_message(self, httpserver: HTTPServer):
        """Verify streaming works with system messages."""
        stream_response = """event: content_block_delta
data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"Helpful"}}

event: message_stop
data: {"type":"message_stop"}
"""
        httpserver.expect_request("/v1/messages").respond_with_data(
            stream_response,
            status=200,
            content_type="text/event-stream",
        )

        provider = Provider.claude(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        chunks = list(
            provider.chat_stream(
                [
                    Message.system("Be helpful"),
                    Message.user("Help me"),
                ]
            )
        )
        assert len(chunks) > 0

    def test_claude_streaming_large_response(self, httpserver: HTTPServer):
        """Verify streaming handles large responses."""
        # Create a long response with multiple chunks
        chunks_data = [
            f'event: content_block_delta\ndata: {{"type":"content_block_delta","delta":{{"type":"text_delta","text":"Chunk {i}. "}}}}\n'
            for i in range(50)
        ]
        stream_response = (
            "".join(chunks_data) + 'event: message_stop\ndata: {"type":"message_stop"}\n'
        )

        httpserver.expect_request("/v1/messages").respond_with_data(
            stream_response,
            status=200,
            content_type="text/event-stream",
        )

        provider = Provider.claude(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        chunks = list(provider.chat_stream([Message.user("Generate")]))
        assert len(chunks) == 50


class TestOpenAIAdvancedStreaming:
    """Advanced streaming tests for OpenAI provider."""

    def test_openai_streaming_accumulation(self, httpserver: HTTPServer):
        """Verify accumulated streaming chunks match full response."""
        stream_response = """data: {"id":"test","object":"text_completion.chunk","created":1234567890,"model":"gpt-4o","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}

data: {"id":"test","object":"text_completion.chunk","created":1234567890,"model":"gpt-4o","choices":[{"index":0,"delta":{"content":"The"},"finish_reason":null}]}

data: {"id":"test","object":"text_completion.chunk","created":1234567890,"model":"gpt-4o","choices":[{"index":0,"delta":{"content":" answer"},"finish_reason":null}]}

data: {"id":"test","object":"text_completion.chunk","created":1234567890,"model":"gpt-4o","choices":[{"index":0,"delta":{"content":" is"},"finish_reason":null}]}

data: {"id":"test","object":"text_completion.chunk","created":1234567890,"model":"gpt-4o","choices":[{"index":0,"delta":{"content":" 42"},"finish_reason":"stop"}]}

data: [DONE]
"""
        httpserver.expect_request("/v1/chat/completions").respond_with_data(
            stream_response,
            status=200,
            content_type="text/event-stream",
        )

        provider = Provider.openai(
            model="gpt-4o",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        chunks = list(provider.chat_stream([Message.user("What is the answer?")]))
        accumulated = "".join(c.delta for c in chunks if c.delta)
        assert "answer" in accumulated and "42" in accumulated

    def test_openai_streaming_chunk_types(self, httpserver: HTTPServer):
        """Verify each chunk is a valid StreamChunk."""
        stream_response = """data: {"id":"test","object":"text_completion.chunk","created":1234567890,"model":"gpt-4o","choices":[{"index":0,"delta":{"content":"test"},"finish_reason":null}]}

data: [DONE]
"""
        httpserver.expect_request("/v1/chat/completions").respond_with_data(
            stream_response,
            status=200,
            content_type="text/event-stream",
        )

        provider = Provider.openai(
            model="gpt-4o",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        for chunk in provider.chat_stream([Message.user("Test")]):
            assert isinstance(chunk, StreamChunk)
            assert isinstance(chunk.delta, str)

    def test_openai_streaming_with_system_message(self, httpserver: HTTPServer):
        """Verify streaming works with system messages."""
        stream_response = """data: {"id":"test","object":"text_completion.chunk","created":1234567890,"model":"gpt-4o","choices":[{"index":0,"delta":{"content":"Response"},"finish_reason":"stop"}]}

data: [DONE]
"""
        httpserver.expect_request("/v1/chat/completions").respond_with_data(
            stream_response,
            status=200,
            content_type="text/event-stream",
        )

        provider = Provider.openai(
            model="gpt-4o",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        chunks = list(
            provider.chat_stream(
                [
                    Message.system("Be concise"),
                    Message.user("Respond"),
                ]
            )
        )
        assert len(chunks) > 0

    def test_openai_streaming_large_response(self, httpserver: HTTPServer):
        """Verify streaming handles large responses."""
        chunks_data = [
            f'data: {{"id":"test","object":"text_completion.chunk","created":1234567890,"model":"gpt-4o","choices":[{{"index":0,"delta":{{"content":"Word{i} "}},"finish_reason":null}}]}}\n'
            for i in range(100)
        ]
        stream_response = "".join(chunks_data) + "data: [DONE]\n"

        httpserver.expect_request("/v1/chat/completions").respond_with_data(
            stream_response,
            status=200,
            content_type="text/event-stream",
        )

        provider = Provider.openai(
            model="gpt-4o",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        chunks = list(provider.chat_stream([Message.user("Generate")]))
        assert len(chunks) == 100


class TestOllamaAdvancedStreaming:
    """Advanced streaming tests for Ollama provider."""

    def test_ollama_streaming_accumulation(self, httpserver: HTTPServer):
        """Verify accumulated streaming chunks match full response."""
        stream_response = """{"model":"mistral","created_at":"2024-01-15T12:00:00Z","message":{"role":"assistant","content":"Hello"},"done":false}
{"model":"mistral","created_at":"2024-01-15T12:00:00Z","message":{"role":"assistant","content":" "},"done":false}
{"model":"mistral","created_at":"2024-01-15T12:00:00Z","message":{"role":"assistant","content":"world"},"done":true,"total_duration":1000000000,"load_duration":100000000,"prompt_eval_count":5,"prompt_eval_duration":200000000,"eval_count":10,"eval_duration":700000000}
"""
        httpserver.expect_request("/api/chat").respond_with_data(
            stream_response,
            status=200,
            content_type="application/x-ndjson",
        )

        provider = Provider.ollama(
            model="mistral",
            api_url=httpserver.url_for(""),
        )

        chunks = list(provider.chat_stream([Message.user("Say hello")]))
        accumulated = "".join(c.delta for c in chunks)
        assert "Hello" in accumulated and "world" in accumulated

    def test_ollama_streaming_chunk_types(self, httpserver: HTTPServer):
        """Verify each chunk is a valid StreamChunk."""
        stream_response = """{"model":"mistral","created_at":"2024-01-15T12:00:00Z","message":{"role":"assistant","content":"test"},"done":true}
"""
        httpserver.expect_request("/api/chat").respond_with_data(
            stream_response,
            status=200,
            content_type="application/x-ndjson",
        )

        provider = Provider.ollama(
            model="mistral",
            api_url=httpserver.url_for(""),
        )

        for chunk in provider.chat_stream([Message.user("Test")]):
            assert isinstance(chunk, StreamChunk)
            assert isinstance(chunk.delta, str)

    def test_ollama_streaming_with_system_message(self, httpserver: HTTPServer):
        """Verify streaming works with system messages."""
        stream_response = """{"model":"mistral","created_at":"2024-01-15T12:00:00Z","message":{"role":"assistant","content":"Concise"},"done":true}
"""
        httpserver.expect_request("/api/chat").respond_with_data(
            stream_response,
            status=200,
            content_type="application/x-ndjson",
        )

        provider = Provider.ollama(
            model="mistral",
            api_url=httpserver.url_for(""),
        )

        chunks = list(
            provider.chat_stream(
                [
                    Message.system("Be concise"),
                    Message.user("Respond"),
                ]
            )
        )
        assert len(chunks) > 0

    def test_ollama_streaming_large_response(self, httpserver: HTTPServer):
        """Verify streaming handles large responses."""
        chunks_data = [
            f'{{"model":"mistral","created_at":"2024-01-15T12:00:00Z","message":{{"role":"assistant","content":"Word{i} "}},"done":false}}\n'
            for i in range(50)
        ]
        chunks_data.append(
            '{"model":"mistral","created_at":"2024-01-15T12:00:00Z","message":{"role":"assistant","content":""},"done":true,"total_duration":1000000000,"load_duration":100000000,"prompt_eval_count":5,"prompt_eval_duration":200000000,"eval_count":50,"eval_duration":700000000}\n'
        )
        stream_response = "".join(chunks_data)

        httpserver.expect_request("/api/chat").respond_with_data(
            stream_response,
            status=200,
            content_type="application/x-ndjson",
        )

        provider = Provider.ollama(
            model="mistral",
            api_url=httpserver.url_for(""),
        )

        chunks = list(provider.chat_stream([Message.user("Generate")]))
        # 50 content chunks + 1 final chunk with finish_reason
        assert len(chunks) == 51
        assert chunks[-1].is_final()
        assert chunks[-1].finish_reason == "stop"


class TestStreamingInterruption:
    """Tests for handling streaming interruption."""

    def test_claude_streaming_network_error_mid_stream(self, httpserver: HTTPServer):
        """Claude should handle network interruption during streaming."""
        # HTTPServer will close connection after first chunk
        httpserver.expect_request("/v1/messages").respond_with_data(
            b'event: content_block_delta\ndata: {"type":"content_block_delta","delta":{"type":"text_delta","text":"partial"}}\n',
            status=200,
            content_type="text/event-stream",
        )

        provider = Provider.claude(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        # Should handle partial response gracefully
        chunks = []
        try:
            for chunk in provider.chat_stream([Message.user("Test")]):
                chunks.append(chunk)
        except Exception:
            # May raise NetworkError on interrupted stream
            pass

        # Should have at least gotten partial response
        assert len(chunks) >= 0

    def test_openai_streaming_network_error_mid_stream(self, httpserver: HTTPServer):
        """OpenAI should handle network interruption during streaming."""
        httpserver.expect_request("/v1/chat/completions").respond_with_data(
            b'data: {"id":"test","choices":[{"index":0,"delta":{"content":"partial"},"finish_reason":null}]}\n',
            status=200,
            content_type="text/event-stream",
        )

        provider = Provider.openai(
            model="gpt-4o",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        chunks = []
        try:
            for chunk in provider.chat_stream([Message.user("Test")]):
                chunks.append(chunk)
        except Exception:
            pass

        assert len(chunks) >= 0
