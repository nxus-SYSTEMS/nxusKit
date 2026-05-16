"""Unit tests for vision and image handling utilities."""

import base64
import os
import tempfile

from nxuskit import ImageSourceType, Message

# Sample 1x1 pixel JPEG
SAMPLE_JPEG_BASE64 = "/9j/4AAQSkZJRgABAQEAYABgAAD/2wBDAAgGBgcGBQgHBwcJCQgKDBQNDAsLDBkSEw8UHRofHh0aHBwgJC4nICIsIxwcKDcpLDAxNDQ0Hyc5PTgyPC4zNDL/2wBDAQkJCQwLDBgNDRgyIRwhMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjL/wAARCAABAAEDASIAAhEBAxEB/8QAFQABAQAAAAAAAAAAAAAAAAAAAAv/xAAUEAEAAAAAAAAAAAAAAAAAAAAA/8VAFQEBAQAAAAAAAAAAAAAAAAAAAAX/xAAUEQEAAAAAAAAAAAAAAAAAAAAA/9oADAMBAAIRAxEAPwCwAA0A/9k="


class TestMessageVisionBuilder:
    """Tests for message builder with image support."""

    def test_with_image_url_single(self):
        """Message should support adding a single URL image."""
        msg = Message.user("What's this?").with_image_url("https://example.com/img.jpg")

        assert len(msg.images) == 1
        assert msg.images[0].source_type == ImageSourceType.URL
        assert msg.images[0].data == "https://example.com/img.jpg"

    def test_with_image_url_multiple(self):
        """Message should support adding multiple URL images."""
        msg = (
            Message.user("Compare these")
            .with_image_url("https://example.com/img1.jpg")
            .with_image_url("https://example.com/img2.jpg")
        )

        assert len(msg.images) == 2
        assert all(img.source_type == ImageSourceType.URL for img in msg.images)
        assert msg.images[0].data == "https://example.com/img1.jpg"
        assert msg.images[1].data == "https://example.com/img2.jpg"

    def test_with_image_url_preserves_content(self):
        """Adding images should not modify message content."""
        content = "What's in this image?"
        msg = Message.user(content).with_image_url("https://example.com/img.jpg")

        assert msg.content == content
        assert len(msg.images) == 1

    def test_with_image_base64_single(self):
        """Message should support adding base64 images."""
        msg = Message.user("Decode this").with_image_base64(SAMPLE_JPEG_BASE64)

        assert len(msg.images) == 1
        assert msg.images[0].source_type == ImageSourceType.BASE64
        assert msg.images[0].data == SAMPLE_JPEG_BASE64

    def test_with_image_base64_multiple(self):
        """Message should support multiple base64 images."""
        msg = (
            Message.user("Two images")
            .with_image_base64(SAMPLE_JPEG_BASE64)
            .with_image_base64(SAMPLE_JPEG_BASE64)
        )

        assert len(msg.images) == 2
        assert all(img.source_type == ImageSourceType.BASE64 for img in msg.images)

    def test_with_image_file_single(self):
        """Message should support file path images."""
        with tempfile.NamedTemporaryFile(suffix=".jpg", delete=False) as f:
            f.write(base64.b64decode(SAMPLE_JPEG_BASE64))
            temp_path = f.name

        try:
            msg = Message.user("Local file").with_image_file(temp_path)

            assert len(msg.images) == 1
            assert msg.images[0].source_type == ImageSourceType.FILEPATH
            assert msg.images[0].data == temp_path
        finally:
            os.unlink(temp_path)

    def test_with_image_file_nonexistent(self):
        """Message should accept file path even if file doesn't exist yet."""
        fake_path = "/nonexistent/path/image.jpg"
        msg = Message.user("Test").with_image_file(fake_path)

        assert len(msg.images) == 1
        assert msg.images[0].data == fake_path

    def test_with_image_mixed_types(self):
        """Message should support mixing different image source types."""
        with tempfile.NamedTemporaryFile(suffix=".jpg", delete=False) as f:
            f.write(base64.b64decode(SAMPLE_JPEG_BASE64))
            temp_path = f.name

        try:
            msg = (
                Message.user("All types")
                .with_image_url("https://example.com/img.jpg")
                .with_image_base64(SAMPLE_JPEG_BASE64)
                .with_image_file(temp_path)
            )

            assert len(msg.images) == 3
            assert msg.images[0].source_type == ImageSourceType.URL
            assert msg.images[1].source_type == ImageSourceType.BASE64
            assert msg.images[2].source_type == ImageSourceType.FILEPATH
        finally:
            os.unlink(temp_path)

    def test_builder_chaining(self):
        """Builder methods should return self for chaining."""
        msg = Message.user("Test")
        result = msg.with_image_url("http://example.com/1.jpg")

        assert result is msg

    def test_image_order_preserved(self):
        """Images should be stored in the order they were added."""
        msg = Message.user("Test")
        msg.with_image_url("http://example.com/1.jpg")
        msg.with_image_url("http://example.com/2.jpg")
        msg.with_image_url("http://example.com/3.jpg")

        assert [img.data for img in msg.images] == [
            "http://example.com/1.jpg",
            "http://example.com/2.jpg",
            "http://example.com/3.jpg",
        ]


class TestImageFileHandling:
    """Tests for image file reading and encoding."""

    def test_image_file_path_types(self):
        """Should accept various image file path formats."""
        paths = [
            "image.jpg",
            "/absolute/path/image.png",
            "relative/path/image.gif",
            "~/home/image.bmp",
            "./local/image.webp",
        ]

        for path in paths:
            msg = Message.user("Test").with_image_file(path)
            assert msg.images[0].data == path

    def test_image_file_multiple_formats(self):
        """Should handle various image formats by filename."""
        formats = [".jpg", ".jpeg", ".png", ".gif", ".bmp", ".webp", ".tiff"]

        for fmt in formats:
            path = f"image{fmt}"
            msg = Message.user("Test").with_image_file(path)
            assert msg.images[0].data == path

    def test_image_base64_validation(self):
        """Base64 images should preserve data integrity."""
        # Test with a known base64 string
        test_data = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=="
        msg = Message.user("Test").with_image_base64(test_data)

        assert msg.images[0].data == test_data
        # Should be decodable
        decoded = base64.b64decode(test_data)
        assert len(decoded) > 0

    def test_image_url_validation(self):
        """URLs should be stored as-is."""
        urls = [
            "https://example.com/image.jpg",
            "http://example.com/image.png",
            "https://cdn.example.com/path/to/image.gif",
            "data:image/jpeg;base64,/9j/4AAQSkZJRg...",  # data URL
        ]

        for url in urls:
            msg = Message.user("Test").with_image_url(url)
            assert msg.images[0].data == url

    def test_real_image_file_handling(self):
        """Should properly handle real image files."""
        with tempfile.NamedTemporaryFile(suffix=".jpg", delete=False) as f:
            # Write actual JPEG data
            f.write(base64.b64decode(SAMPLE_JPEG_BASE64))
            temp_path = f.name

        try:
            # File should exist and be readable
            assert os.path.exists(temp_path)
            assert os.path.getsize(temp_path) > 0

            # Message should store the path
            msg = Message.user("Test").with_image_file(temp_path)
            assert msg.images[0].data == temp_path

            # Path should still be valid
            assert os.path.exists(msg.images[0].data)
        finally:
            os.unlink(temp_path)

    def test_large_image_paths(self):
        """Should handle paths with many directory levels."""
        deep_path = "/".join([f"dir{i}" for i in range(20)]) + "/image.jpg"
        msg = Message.user("Test").with_image_file(deep_path)

        assert msg.images[0].data == deep_path

    def test_special_characters_in_paths(self):
        """Should handle special characters in file paths."""
        paths = [
            "image with spaces.jpg",
            "image-with-dashes.jpg",
            "image_with_underscores.jpg",
            "image.multiple.dots.jpg",
        ]

        for path in paths:
            msg = Message.user("Test").with_image_file(path)
            assert msg.images[0].data == path


class TestVisionMessageTypes:
    """Tests for different message roles with images."""

    def test_user_message_with_image(self):
        """User messages should support images."""
        msg = Message.user("Analyze this").with_image_url("https://example.com/img.jpg")

        assert len(msg.images) == 1

    def test_assistant_message_with_image(self):
        """Assistant messages should support images (for context)."""
        msg = Message.assistant("I see this").with_image_url("https://example.com/img.jpg")

        assert len(msg.images) == 1

    def test_system_message_with_image(self):
        """System messages should support images."""
        msg = Message.system("Analyze images like this").with_image_url(
            "https://example.com/example.jpg"
        )

        assert len(msg.images) == 1

    def test_multi_image_vision_conversation(self):
        """Should support vision in multi-turn conversations."""
        messages = [
            Message.system("You are a vision expert"),
            Message.user("What's in this?").with_image_url("https://example.com/img1.jpg"),
            Message.assistant("I see a cat"),
            Message.user("Compare with this").with_image_url("https://example.com/img2.jpg"),
        ]

        assert len(messages) == 4
        assert len(messages[1].images) == 1
        assert len(messages[3].images) == 1
        assert len(messages[0].images) == 0
        assert len(messages[2].images) == 0
