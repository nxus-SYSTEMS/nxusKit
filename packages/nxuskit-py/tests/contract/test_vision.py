"""Contract tests for vision/image input capabilities."""

import base64
import os
import tempfile

from pytest_httpserver import HTTPServer

from nxuskit import Message, Provider

# Sample 1x1 pixel JPEG for testing
SAMPLE_JPEG_BASE64 = "/9j/4AAQSkZJRgABAQEAYABgAAD/2wBDAAgGBgcGBQgHBwcJCQgKDBQNDAsLDBkSEw8UHRofHh0aHBwgJC4nICIsIxwcKDcpLDAxNDQ0Hyc5PTgyPC4zNDL/2wBDAQkJCQwLDBgNDRgyIRwhMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjL/wAARCAABAAEDASIAAhEBAxEB/8QAFQABAQAAAAAAAAAAAAAAAAAAAAv/xAAUEAEAAAAAAAAAAAAAAAAAAAAA/8VAFQEBAQAAAAAAAAAAAAAAAAAAAAX/xAAUEQEAAAAAAAAAAAAAAAAAAAAA/9oADAMBAAIRAxEAPwCwAA0A/9k="


class TestClaudeVision:
    """Contract tests for Claude vision capabilities."""

    def test_claude_chat_with_image_url(self, httpserver: HTTPServer):
        """Claude should accept images via URL in messages."""
        mock_response = {
            "id": "msg_test123",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "This image shows a blue square."}],
            "model": "claude-sonnet-4-20250514",
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 150, "output_tokens": 25},
        }
        httpserver.expect_request(
            "/v1/messages",
            method="POST",
        ).respond_with_json(mock_response, status=200)

        provider = Provider.claude(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        response = provider.chat(
            [Message.user("What's in this image?").with_image_url("https://example.com/image.jpg")]
        )

        assert response.content == "This image shows a blue square."
        assert "150" in str(response.usage.input_tokens)

    def test_claude_chat_with_image_base64(self, httpserver: HTTPServer):
        """Claude should accept base64-encoded images."""
        mock_response = {
            "id": "msg_test456",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "I can see the image."}],
            "model": "claude-sonnet-4-20250514",
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 200, "output_tokens": 10},
        }
        httpserver.expect_request(
            "/v1/messages",
            method="POST",
        ).respond_with_json(mock_response, status=200)

        provider = Provider.claude(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        response = provider.chat(
            [Message.user("Describe this image").with_image_base64(SAMPLE_JPEG_BASE64)]
        )

        assert response.content == "I can see the image."

    def test_claude_chat_with_image_file(self, httpserver: HTTPServer):
        """Claude should accept images from file paths."""
        mock_response = {
            "id": "msg_test789",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "File-based image loaded."}],
            "model": "claude-sonnet-4-20250514",
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 175, "output_tokens": 8},
        }
        httpserver.expect_request(
            "/v1/messages",
            method="POST",
        ).respond_with_json(mock_response, status=200)

        # Create a temporary image file
        with tempfile.NamedTemporaryFile(suffix=".jpg", delete=False) as f:
            # Write JPEG header and minimal JPEG data
            f.write(base64.b64decode(SAMPLE_JPEG_BASE64))
            temp_path = f.name

        try:
            provider = Provider.claude(
                model="claude-sonnet-4-20250514",
                api_key="test-key",
                api_url=httpserver.url_for(""),
            )

            response = provider.chat([Message.user("What do you see?").with_image_file(temp_path)])

            assert response.content == "File-based image loaded."
        finally:
            os.unlink(temp_path)

    def test_claude_chat_with_multiple_images(self, httpserver: HTTPServer):
        """Claude should accept multiple images in one message."""
        mock_response = {
            "id": "msg_multi",
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "Two images detected."}],
            "model": "claude-sonnet-4-20250514",
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 300, "output_tokens": 10},
        }
        httpserver.expect_request(
            "/v1/messages",
            method="POST",
        ).respond_with_json(mock_response, status=200)

        provider = Provider.claude(
            model="claude-sonnet-4-20250514",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        response = provider.chat(
            [
                Message.user("Compare these images")
                .with_image_url("https://example.com/image1.jpg")
                .with_image_url("https://example.com/image2.jpg")
            ]
        )

        assert response.content == "Two images detected."

    def test_claude_streaming_with_image(self, httpserver: HTTPServer):
        """Claude should support streaming with images."""
        stream_response = """event: content_block_delta
data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"The"}}

event: content_block_delta
data: {"type":"content_block_delta","delta":{"type":"text_delta","text":" image"}}

event: content_block_delta
data: {"type":"content_block_delta","delta":{"type":"text_delta","text":" shows"}}

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
                [Message.user("Analyze this").with_image_url("https://example.com/image.jpg")]
            )
        )

        assert len(chunks) >= 3
        accumulated = "".join(c.delta for c in chunks)
        assert "image" in accumulated


class TestOpenAIVision:
    """Contract tests for OpenAI vision capabilities."""

    def test_openai_chat_with_image_url(self, httpserver: HTTPServer):
        """OpenAI should accept images via URL."""
        mock_response = {
            "id": "chatcmpl-test",
            "object": "chat.completion",
            "created": 1234567890,
            "model": "gpt-4-vision",
            "choices": [
                {
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": "The image contains a dog.",
                    },
                    "finish_reason": "stop",
                }
            ],
            "usage": {
                "prompt_tokens": 200,
                "completion_tokens": 15,
                "total_tokens": 215,
            },
        }
        httpserver.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_json(mock_response, status=200)

        provider = Provider.openai(
            model="gpt-4-vision",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        response = provider.chat(
            [Message.user("What animal is this?").with_image_url("https://example.com/dog.jpg")]
        )

        assert response.content == "The image contains a dog."

    def test_openai_chat_with_image_base64(self, httpserver: HTTPServer):
        """OpenAI should accept base64-encoded images."""
        mock_response = {
            "id": "chatcmpl-test2",
            "object": "chat.completion",
            "created": 1234567890,
            "model": "gpt-4-vision",
            "choices": [
                {
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": "Image processed.",
                    },
                    "finish_reason": "stop",
                }
            ],
            "usage": {
                "prompt_tokens": 250,
                "completion_tokens": 10,
                "total_tokens": 260,
            },
        }
        httpserver.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_json(mock_response, status=200)

        provider = Provider.openai(
            model="gpt-4-vision",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        response = provider.chat(
            [Message.user("Process this").with_image_base64(SAMPLE_JPEG_BASE64)]
        )

        assert response.content == "Image processed."

    def test_openai_chat_with_image_file(self, httpserver: HTTPServer):
        """OpenAI should accept images from file paths."""
        mock_response = {
            "id": "chatcmpl-test3",
            "object": "chat.completion",
            "created": 1234567890,
            "model": "gpt-4-vision",
            "choices": [
                {
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": "File image received.",
                    },
                    "finish_reason": "stop",
                }
            ],
            "usage": {
                "prompt_tokens": 180,
                "completion_tokens": 12,
                "total_tokens": 192,
            },
        }
        httpserver.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_json(mock_response, status=200)

        with tempfile.NamedTemporaryFile(suffix=".jpg", delete=False) as f:
            f.write(base64.b64decode(SAMPLE_JPEG_BASE64))
            temp_path = f.name

        try:
            provider = Provider.openai(
                model="gpt-4-vision",
                api_key="test-key",
                api_url=httpserver.url_for(""),
            )

            response = provider.chat([Message.user("Analyze").with_image_file(temp_path)])

            assert response.content == "File image received."
        finally:
            os.unlink(temp_path)

    def test_openai_chat_with_multiple_images(self, httpserver: HTTPServer):
        """OpenAI should support multiple images."""
        mock_response = {
            "id": "chatcmpl-test4",
            "object": "chat.completion",
            "created": 1234567890,
            "model": "gpt-4-vision",
            "choices": [
                {
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": "Comparing both images.",
                    },
                    "finish_reason": "stop",
                }
            ],
            "usage": {
                "prompt_tokens": 350,
                "completion_tokens": 15,
                "total_tokens": 365,
            },
        }
        httpserver.expect_request(
            "/v1/chat/completions",
            method="POST",
        ).respond_with_json(mock_response, status=200)

        provider = Provider.openai(
            model="gpt-4-vision",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        response = provider.chat(
            [
                Message.user("Which is bigger?")
                .with_image_url("https://example.com/small.jpg")
                .with_image_url("https://example.com/large.jpg")
            ]
        )

        assert response.content == "Comparing both images."

    def test_openai_streaming_with_image(self, httpserver: HTTPServer):
        """OpenAI should support streaming with images."""
        stream_response = """data: {"id":"test","choices":[{"index":0,"delta":{"content":"I"},"finish_reason":null}]}

data: {"id":"test","choices":[{"index":0,"delta":{"content":" see"},"finish_reason":null}]}

data: {"id":"test","choices":[{"index":0,"delta":{"content":" details"},"finish_reason":"stop"}]}

data: [DONE]
"""
        httpserver.expect_request("/v1/chat/completions").respond_with_data(
            stream_response,
            status=200,
            content_type="text/event-stream",
        )

        provider = Provider.openai(
            model="gpt-4-vision",
            api_key="test-key",
            api_url=httpserver.url_for(""),
        )

        chunks = list(
            provider.chat_stream(
                [Message.user("Describe").with_image_url("https://example.com/image.jpg")]
            )
        )

        assert len(chunks) >= 3
        accumulated = "".join(c.delta for c in chunks if c.delta)
        assert "see" in accumulated


class TestOllamaVision:
    """Contract tests for Ollama vision capabilities."""

    def test_ollama_chat_with_image_base64(self, httpserver: HTTPServer):
        """Ollama should accept base64-encoded images."""
        mock_response = {
            "model": "llava",
            "created_at": "2024-01-15T12:00:00Z",
            "message": {
                "role": "assistant",
                "content": "I can see this is an image.",
            },
            "done": True,
            "total_duration": 2000000000,
            "load_duration": 200000000,
            "prompt_eval_count": 150,
            "prompt_eval_duration": 1200000000,
            "eval_count": 15,
            "eval_duration": 600000000,
        }
        httpserver.expect_request(
            "/api/chat",
            method="POST",
        ).respond_with_json(mock_response, status=200)

        provider = Provider.ollama(
            model="llava",
            api_url=httpserver.url_for(""),
        )

        response = provider.chat(
            [Message.user("What's in this image?").with_image_base64(SAMPLE_JPEG_BASE64)]
        )

        assert response.content == "I can see this is an image."

    def test_ollama_chat_with_image_file(self, httpserver: HTTPServer):
        """Ollama should accept images from file paths."""
        mock_response = {
            "model": "llava",
            "created_at": "2024-01-15T12:00:00Z",
            "message": {
                "role": "assistant",
                "content": "File image loaded.",
            },
            "done": True,
            "total_duration": 1800000000,
            "load_duration": 180000000,
            "prompt_eval_count": 140,
            "prompt_eval_duration": 1100000000,
            "eval_count": 10,
            "eval_duration": 520000000,
        }
        httpserver.expect_request(
            "/api/chat",
            method="POST",
        ).respond_with_json(mock_response, status=200)

        with tempfile.NamedTemporaryFile(suffix=".jpg", delete=False) as f:
            f.write(base64.b64decode(SAMPLE_JPEG_BASE64))
            temp_path = f.name

        try:
            provider = Provider.ollama(
                model="llava",
                api_url=httpserver.url_for(""),
            )

            response = provider.chat([Message.user("Analyze this").with_image_file(temp_path)])

            assert response.content == "File image loaded."
        finally:
            os.unlink(temp_path)

    def test_ollama_streaming_with_image(self, httpserver: HTTPServer):
        """Ollama should support streaming with images."""
        stream_response = """{"model":"llava","created_at":"2024-01-15T12:00:00Z","message":{"role":"assistant","content":"Vision"},"done":false}
{"model":"llava","created_at":"2024-01-15T12:00:00Z","message":{"role":"assistant","content":" analysis"},"done":true,"total_duration":1500000000,"load_duration":150000000,"prompt_eval_count":100,"prompt_eval_duration":900000000,"eval_count":10,"eval_duration":450000000}
"""
        httpserver.expect_request("/api/chat").respond_with_data(
            stream_response,
            status=200,
            content_type="application/x-ndjson",
        )

        provider = Provider.ollama(
            model="llava",
            api_url=httpserver.url_for(""),
        )

        chunks = list(
            provider.chat_stream(
                [Message.user("Analyze image").with_image_base64(SAMPLE_JPEG_BASE64)]
            )
        )

        assert len(chunks) >= 2
        accumulated = "".join(c.delta for c in chunks)
        assert "Vision" in accumulated
