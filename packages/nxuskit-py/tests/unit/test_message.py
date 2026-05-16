"""Unit tests for Message class with builder pattern."""

from nxuskit import ImageSource, ImageSourceType, Message, Role


class TestMessageFactory:
    """Tests for Message factory methods."""

    def test_message_user_factory(self):
        """Message.user() should create a user message."""
        msg = Message.user("Hello")
        assert msg.role == Role.USER
        assert msg.content == "Hello"

    def test_message_assistant_factory(self):
        """Message.assistant() should create an assistant message."""
        msg = Message.assistant("Hi there")
        assert msg.role == Role.ASSISTANT
        assert msg.content == "Hi there"

    def test_message_system_factory(self):
        """Message.system() should create a system message."""
        msg = Message.system("You are helpful")
        assert msg.role == Role.SYSTEM
        assert msg.content == "You are helpful"

    def test_message_factories_with_empty_content(self):
        """Message factories should accept empty strings."""
        user_msg = Message.user("")
        assert user_msg.content == ""
        assert user_msg.role == Role.USER


class TestMessageBuilder:
    """Tests for Message builder pattern methods."""

    def test_with_image_url(self):
        """Message.with_image_url() should add image URL."""
        msg = Message.user("What's in this?").with_image_url("https://example.com/img.jpg")
        assert msg.content == "What's in this?"
        assert len(msg.images) == 1
        assert msg.images[0].source_type == ImageSourceType.URL
        assert msg.images[0].data == "https://example.com/img.jpg"

    def test_with_image_base64(self):
        """Message.with_image_base64() should add base64 image."""
        b64_data = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=="
        msg = Message.user("Describe").with_image_base64(b64_data)
        assert len(msg.images) == 1
        assert msg.images[0].source_type == ImageSourceType.BASE64
        assert msg.images[0].data == b64_data

    def test_with_image_file(self):
        """Message.with_image_file() should add file path image."""
        msg = Message.user("What's this?").with_image_file("/path/to/image.jpg")
        assert len(msg.images) == 1
        assert msg.images[0].source_type == ImageSourceType.FILEPATH
        assert msg.images[0].data == "/path/to/image.jpg"

    def test_multiple_images(self):
        """Message should support multiple images."""
        msg = (
            Message.user("Multiple images")
            .with_image_url("https://example.com/1.jpg")
            .with_image_url("https://example.com/2.jpg")
            .with_image_file("/local/image.png")
        )
        assert len(msg.images) == 3
        assert msg.images[0].data == "https://example.com/1.jpg"
        assert msg.images[1].data == "https://example.com/2.jpg"
        assert msg.images[2].data == "/local/image.png"

    def test_builder_returns_message(self):
        """Builder methods should return Message for chaining."""
        msg = Message.user("test")
        result = msg.with_image_url("http://example.com/img.jpg")
        assert isinstance(result, Message)
        assert result is msg  # Should return self


class TestMessageInitialization:
    """Tests for direct Message instantiation."""

    def test_message_with_role_and_content(self):
        """Message should be creatable with role and content."""
        msg = Message(role=Role.USER, content="Hello")
        assert msg.role == Role.USER
        assert msg.content == "Hello"

    def test_message_with_no_images(self):
        """Message should have empty images list by default."""
        msg = Message(role=Role.ASSISTANT, content="Response")
        assert msg.images == []

    def test_message_with_images_list(self):
        """Message should accept initial images list."""
        imgs = [ImageSource(ImageSourceType.URL, "http://example.com/img.jpg")]
        msg = Message(role=Role.USER, content="Text", images=imgs)
        assert len(msg.images) == 1
        assert msg.images[0].source_type == ImageSourceType.URL

    def test_message_content_types(self):
        """Message content should store string values."""
        msg = Message.user("String content")
        assert isinstance(msg.content, str)
        assert msg.content == "String content"


class TestMessageEdgeCases:
    """Tests for Message edge cases."""

    def test_very_long_content(self):
        """Message should handle very long content."""
        long_content = "x" * 100000
        msg = Message.user(long_content)
        assert len(msg.content) == 100000

    def test_content_with_special_characters(self):
        """Message should preserve special characters."""
        special = 'Hello\n\t"quoted"\n🚀 emoji\n中文'
        msg = Message.user(special)
        assert msg.content == special

    def test_unicode_in_content(self):
        """Message should handle unicode properly."""
        unicode_text = "你好世界 مرحبا العالم"
        msg = Message.user(unicode_text)
        assert msg.content == unicode_text

    def test_empty_message_content(self):
        """Message should allow empty content."""
        msg = Message.user("")
        assert msg.content == ""
        assert msg.role == Role.USER
