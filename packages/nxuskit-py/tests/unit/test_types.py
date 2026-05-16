"""Unit tests for core types (Role, ImageSource, TokenUsage, ChatResponse, StreamChunk)."""

from nxuskit import ChatResponse, ImageSource, ImageSourceType, Role, StreamChunk, TokenUsage


class TestRole:
    """Tests for Role enum."""

    def test_role_user_exists(self):
        """Role.USER should exist."""
        assert hasattr(Role, "USER")

    def test_role_assistant_exists(self):
        """Role.ASSISTANT should exist."""
        assert hasattr(Role, "ASSISTANT")

    def test_role_system_exists(self):
        """Role.SYSTEM should exist."""
        assert hasattr(Role, "SYSTEM")

    def test_role_values(self):
        """Role enum values should be strings."""
        assert isinstance(Role.USER.value, str)
        assert isinstance(Role.ASSISTANT.value, str)
        assert isinstance(Role.SYSTEM.value, str)

    def test_role_string_representation(self):
        """Role values should convert to lowercase strings."""
        assert Role.USER.value == "user"
        assert Role.ASSISTANT.value == "assistant"
        assert Role.SYSTEM.value == "system"


class TestImageSourceType:
    """Tests for ImageSourceType enum."""

    def test_image_source_type_url_exists(self):
        """ImageSourceType.URL should exist."""
        assert hasattr(ImageSourceType, "URL")

    def test_image_source_type_base64_exists(self):
        """ImageSourceType.BASE64 should exist."""
        assert hasattr(ImageSourceType, "BASE64")

    def test_image_source_type_filepath_exists(self):
        """ImageSourceType.FILEPATH should exist."""
        assert hasattr(ImageSourceType, "FILEPATH")


class TestImageSource:
    """Tests for ImageSource dataclass."""

    def test_image_source_with_url(self):
        """ImageSource should be creatable with URL type."""
        img = ImageSource(source_type=ImageSourceType.URL, data="https://example.com/image.jpg")
        assert img.source_type == ImageSourceType.URL
        assert img.data == "https://example.com/image.jpg"

    def test_image_source_with_base64(self):
        """ImageSource should be creatable with BASE64 type."""
        img = ImageSource(
            source_type=ImageSourceType.BASE64,
            data="iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
        )
        assert img.source_type == ImageSourceType.BASE64
        assert len(img.data) > 0

    def test_image_source_with_filepath(self):
        """ImageSource should be creatable with FILEPATH type."""
        img = ImageSource(source_type=ImageSourceType.FILEPATH, data="/path/to/image.jpg")
        assert img.source_type == ImageSourceType.FILEPATH
        assert img.data == "/path/to/image.jpg"


class TestTokenUsage:
    """Tests for TokenUsage dataclass."""

    def test_token_usage_creation(self):
        """TokenUsage should be creatable with token counts."""
        usage = TokenUsage(input_tokens=100, output_tokens=50, total_tokens=150)
        assert usage.input_tokens == 100
        assert usage.output_tokens == 50
        assert usage.total_tokens == 150

    def test_token_usage_zero_values(self):
        """TokenUsage should accept zero values."""
        usage = TokenUsage(input_tokens=0, output_tokens=0, total_tokens=0)
        assert usage.input_tokens == 0
        assert usage.output_tokens == 0
        assert usage.total_tokens == 0


class TestChatResponse:
    """Tests for ChatResponse dataclass."""

    def test_chat_response_with_text_content(self):
        """ChatResponse should be creatable with text content."""
        usage = TokenUsage(input_tokens=10, output_tokens=20, total_tokens=30)
        response = ChatResponse(
            content="Hello, world!", usage=usage, model="claude-sonnet-4-20250514"
        )
        assert response.content == "Hello, world!"
        assert response.usage.total_tokens == 30
        assert response.model == "claude-sonnet-4-20250514"

    def test_chat_response_with_empty_content(self):
        """ChatResponse should accept empty content."""
        usage = TokenUsage(input_tokens=0, output_tokens=0, total_tokens=0)
        response = ChatResponse(content="", usage=usage, model="test-model")
        assert response.content == ""

    def test_chat_response_finish_reason(self):
        """ChatResponse should store finish_reason."""
        usage = TokenUsage(input_tokens=10, output_tokens=20, total_tokens=30)
        response = ChatResponse(
            content="Hello", usage=usage, model="test-model", finish_reason="end_turn"
        )
        assert response.finish_reason == "end_turn"

    def test_chat_response_optional_finish_reason(self):
        """ChatResponse should allow None finish_reason."""
        usage = TokenUsage(input_tokens=10, output_tokens=20, total_tokens=30)
        response = ChatResponse(
            content="Hello", usage=usage, model="test-model", finish_reason=None
        )
        assert response.finish_reason is None

    def test_chat_response_deprecated_stop_reason(self):
        """ChatResponse.stop_reason should be a deprecated alias for finish_reason."""
        import warnings

        usage = TokenUsage(input_tokens=10, output_tokens=20, total_tokens=30)
        response = ChatResponse(
            content="Hello", usage=usage, model="test-model", finish_reason="end_turn"
        )
        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            assert response.stop_reason == "end_turn"
            assert len(w) == 1
            assert issubclass(w[0].category, DeprecationWarning)


class TestStreamChunk:
    """Tests for StreamChunk dataclass."""

    def test_stream_chunk_with_delta(self):
        """StreamChunk should be creatable with delta content."""
        chunk = StreamChunk(delta="Hello ")
        assert chunk.delta == "Hello "

    def test_stream_chunk_empty_delta(self):
        """StreamChunk should accept empty delta."""
        chunk = StreamChunk(delta="")
        assert chunk.delta == ""

    def test_stream_chunk_with_model(self):
        """StreamChunk should store model name."""
        chunk = StreamChunk(delta="world", model="gpt-4o")
        assert chunk.delta == "world"
        assert chunk.model == "gpt-4o"

    def test_stream_chunk_with_finish_reason(self):
        """StreamChunk should store finish_reason."""
        chunk = StreamChunk(delta="final", finish_reason="end_turn")
        assert chunk.delta == "final"
        assert chunk.finish_reason == "end_turn"
        assert chunk.is_final()

    def test_stream_chunk_optional_fields(self):
        """StreamChunk should allow optional fields to be None."""
        chunk = StreamChunk(delta="text", model=None, finish_reason=None)
        assert chunk.delta == "text"
        assert chunk.model is None
        assert chunk.finish_reason is None
        assert not chunk.is_final()
