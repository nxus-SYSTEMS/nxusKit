"""Unit tests for vision helper functions."""

import base64
import os
import tempfile

import pytest

from nxuskit import Message
from nxuskit.vision import (
    ImageLoader,
    add_images_to_message,
    detect_image_type,
    image_to_data_url,
    is_base64,
    is_valid_url,
    load_image_base64,
)

SAMPLE_JPEG_BASE64 = "/9j/4AAQSkZJRgABAQEAYABgAAD/2wBDAAgGBgcGBQgHBwcJCQgKDBQNDAsLDBkSEw8UHRofHh0aHBwgJC4nICIsIxwcKDcpLDAxNDQ0Hyc5PTgyPC4zNDL/2wBDAQkJCQwLDBgNDRgyIRwhMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjL/wAARCAABAAEDASIAAhEBAxEB/8QAFQABAQAAAAAAAAAAAAAAAAAAAAv/xAAUEAEAAAAAAAAAAAAAAAAAAAAA/8VAFQEBAQAAAAAAAAAAAAAAAAAAAAX/xAAUEQEAAAAAAAAAAAAAAAAAAAAA/9oADAMBAAIRAxEAPwCwAA0A/9k="


class TestLoadImageBase64:
    """Tests for load_image_base64 function."""

    def test_load_real_image(self):
        """Should load and encode a real image file."""
        with tempfile.NamedTemporaryFile(suffix=".jpg", delete=False) as f:
            f.write(base64.b64decode(SAMPLE_JPEG_BASE64))
            temp_path = f.name

        try:
            b64_data = load_image_base64(temp_path)
            assert isinstance(b64_data, str)
            assert len(b64_data) > 0
            # Should be decodable
            decoded = base64.b64decode(b64_data)
            assert len(decoded) > 0
        finally:
            os.unlink(temp_path)

    def test_load_nonexistent_file(self):
        """Should raise FileNotFoundError for missing files."""
        with pytest.raises(FileNotFoundError):
            load_image_base64("/nonexistent/path/image.jpg")

    def test_load_preserves_data(self):
        """Loaded image should preserve original data."""
        original_data = base64.b64decode(SAMPLE_JPEG_BASE64)

        with tempfile.NamedTemporaryFile(suffix=".jpg", delete=False) as f:
            f.write(original_data)
            temp_path = f.name

        try:
            b64_data = load_image_base64(temp_path)
            decoded_data = base64.b64decode(b64_data)
            assert decoded_data == original_data
        finally:
            os.unlink(temp_path)


class TestDetectImageType:
    """Tests for detect_image_type function."""

    def test_common_formats(self):
        """Should detect common image formats."""
        assert detect_image_type("image.jpg") == "image/jpeg"
        assert detect_image_type("image.jpeg") == "image/jpeg"
        assert detect_image_type("image.png") == "image/png"
        assert detect_image_type("image.gif") == "image/gif"
        assert detect_image_type("image.webp") == "image/webp"
        assert detect_image_type("image.bmp") == "image/bmp"

    def test_case_insensitive(self):
        """Should handle uppercase extensions."""
        assert detect_image_type("image.JPG") == "image/jpeg"
        assert detect_image_type("image.PNG") == "image/png"
        assert detect_image_type("IMAGE.JPEG") == "image/jpeg"

    def test_unknown_format(self):
        """Should default to JPEG for unknown formats."""
        assert detect_image_type("image.xyz") == "image/jpeg"
        assert detect_image_type("image") == "image/jpeg"

    def test_full_paths(self):
        """Should work with full file paths."""
        assert detect_image_type("/path/to/image.jpg") == "image/jpeg"
        assert detect_image_type("~/home/image.png") == "image/png"


class TestIsValidUrl:
    """Tests for is_valid_url function."""

    def test_http_urls(self):
        """Should recognize HTTP URLs."""
        assert is_valid_url("http://example.com/image.jpg")
        assert is_valid_url("http://cdn.example.com/path/image.png")

    def test_https_urls(self):
        """Should recognize HTTPS URLs."""
        assert is_valid_url("https://example.com/image.jpg")
        assert is_valid_url("https://secure.example.com/images/photo.png")

    def test_data_urls(self):
        """Should recognize data URLs."""
        assert is_valid_url("data:image/jpeg;base64,/9j/4AAQ...")

    def test_non_urls(self):
        """Should reject non-URL strings."""
        assert not is_valid_url("image.jpg")
        assert not is_valid_url("/path/to/image.jpg")
        assert not is_valid_url("ftp://example.com/image.jpg")
        assert not is_valid_url("just text")

    def test_empty_string(self):
        """Should reject empty string."""
        assert not is_valid_url("")


class TestIsBase64:
    """Tests for is_base64 function."""

    def test_valid_base64(self):
        """Should recognize valid base64 strings."""
        assert is_base64("aGVsbG8gd29ybGQ=")
        assert is_base64(SAMPLE_JPEG_BASE64)

    def test_invalid_base64(self):
        """Should reject invalid base64."""
        assert not is_base64("not@base64!")
        assert not is_base64("image.jpg")
        assert not is_base64("http://example.com")

    def test_empty_string(self):
        """Should reject empty string."""
        assert not is_base64("")

    def test_padding_requirement(self):
        """Base64 must have valid padding."""
        # Valid base64 (multiple of 4)
        assert is_base64("SGVsbG8=")
        # Valid base64 without padding
        assert is_base64("SGVs")
        # Invalid base64 characters
        assert not is_base64("SGV@#$%")


class TestAddImagesToMessage:
    """Tests for add_images_to_message function."""

    def test_add_url_images(self):
        """Should add URL images."""
        msg = Message.user("Test")
        sources = [
            "https://example.com/1.jpg",
            "https://example.com/2.jpg",
        ]
        result = add_images_to_message(msg, sources)

        assert len(result.images) == 2

    def test_add_base64_images(self):
        """Should add base64 images."""
        msg = Message.user("Test")
        sources = [SAMPLE_JPEG_BASE64, SAMPLE_JPEG_BASE64]
        result = add_images_to_message(msg, sources)

        assert len(result.images) == 2

    def test_add_file_images(self):
        """Should add file path images."""
        with tempfile.NamedTemporaryFile(suffix=".jpg", delete=False) as f:
            f.write(base64.b64decode(SAMPLE_JPEG_BASE64))
            temp_path = f.name

        try:
            msg = Message.user("Test")
            sources = [temp_path]
            result = add_images_to_message(msg, sources)

            assert len(result.images) == 1
        finally:
            os.unlink(temp_path)

    def test_add_mixed_images(self):
        """Should handle mixed image sources."""
        msg = Message.user("Test")
        sources = [
            "https://example.com/image.jpg",
            SAMPLE_JPEG_BASE64,
        ]
        result = add_images_to_message(msg, sources)

        assert len(result.images) == 2

    def test_empty_sources(self):
        """Should handle empty source list."""
        msg = Message.user("Test")
        result = add_images_to_message(msg, [])

        assert len(result.images) == 0


class TestImageToDataUrl:
    """Tests for image_to_data_url function."""

    def test_create_data_url(self):
        """Should create valid data URL."""
        with tempfile.NamedTemporaryFile(suffix=".jpg", delete=False) as f:
            f.write(base64.b64decode(SAMPLE_JPEG_BASE64))
            temp_path = f.name

        try:
            data_url = image_to_data_url(temp_path)
            assert data_url.startswith("data:image/jpeg;base64,")
            assert len(data_url) > 30
        finally:
            os.unlink(temp_path)

    def test_different_formats(self):
        """Should use correct MIME type for format."""
        with tempfile.TemporaryDirectory() as tmpdir:
            # Test PNG
            png_path = os.path.join(tmpdir, "image.png")
            with open(png_path, "wb") as f:
                f.write(b"PNG fake data")

            data_url = image_to_data_url(png_path)
            assert "data:image/png;base64," in data_url


class TestImageLoader:
    """Tests for ImageLoader class."""

    def test_load_as_base64(self):
        """ImageLoader should load images as base64."""
        loader = ImageLoader()

        with tempfile.NamedTemporaryFile(suffix=".jpg", delete=False) as f:
            f.write(base64.b64decode(SAMPLE_JPEG_BASE64))
            temp_path = f.name

        try:
            b64_data = loader.load_as_base64(temp_path)
            assert isinstance(b64_data, str)
            assert len(b64_data) > 0
        finally:
            os.unlink(temp_path)

    def test_load_as_data_url(self):
        """ImageLoader should load images as data URLs."""
        loader = ImageLoader()

        with tempfile.NamedTemporaryFile(suffix=".jpg", delete=False) as f:
            f.write(base64.b64decode(SAMPLE_JPEG_BASE64))
            temp_path = f.name

        try:
            data_url = loader.load_as_data_url(temp_path)
            assert data_url.startswith("data:image/jpeg;base64,")
        finally:
            os.unlink(temp_path)

    def test_caching(self):
        """ImageLoader should cache base64 when enabled."""
        loader = ImageLoader(cache_base64=True)

        with tempfile.NamedTemporaryFile(suffix=".jpg", delete=False) as f:
            f.write(base64.b64decode(SAMPLE_JPEG_BASE64))
            temp_path = f.name

        try:
            b64_1 = loader.load_as_base64(temp_path)
            b64_2 = loader.load_as_base64(temp_path)

            # Should return same cached value
            assert b64_1 is b64_2
        finally:
            os.unlink(temp_path)

    def test_no_caching(self):
        """ImageLoader should not cache when disabled."""
        loader = ImageLoader(cache_base64=False)

        with tempfile.NamedTemporaryFile(suffix=".jpg", delete=False) as f:
            f.write(base64.b64decode(SAMPLE_JPEG_BASE64))
            temp_path = f.name

        try:
            b64_1 = loader.load_as_base64(temp_path)
            b64_2 = loader.load_as_base64(temp_path)

            # Should return equal but different objects
            assert b64_1 == b64_2
        finally:
            os.unlink(temp_path)

    def test_prepare_message_with_base64(self):
        """ImageLoader should prepare messages with base64 images."""
        loader = ImageLoader()

        with tempfile.NamedTemporaryFile(suffix=".jpg", delete=False) as f:
            f.write(base64.b64decode(SAMPLE_JPEG_BASE64))
            temp_path = f.name

        try:
            msg = loader.prepare_message("Analyze", [temp_path], use_data_urls=False)

            assert msg.content == "Analyze"
            assert len(msg.images) == 1
        finally:
            os.unlink(temp_path)

    def test_prepare_message_with_data_urls(self):
        """ImageLoader should prepare messages with data URLs."""
        loader = ImageLoader()

        with tempfile.NamedTemporaryFile(suffix=".jpg", delete=False) as f:
            f.write(base64.b64decode(SAMPLE_JPEG_BASE64))
            temp_path = f.name

        try:
            msg = loader.prepare_message("Analyze", [temp_path], use_data_urls=True)

            assert msg.content == "Analyze"
            assert len(msg.images) == 1
            assert "data:image" in msg.images[0].data
        finally:
            os.unlink(temp_path)

    def test_clear_cache(self):
        """ImageLoader should clear cache."""
        loader = ImageLoader(cache_base64=True)

        with tempfile.NamedTemporaryFile(suffix=".jpg", delete=False) as f:
            f.write(base64.b64decode(SAMPLE_JPEG_BASE64))
            temp_path = f.name

        try:
            loader.load_as_base64(temp_path)
            assert len(loader._base64_cache) > 0

            loader.clear_cache()
            assert len(loader._base64_cache) == 0
        finally:
            os.unlink(temp_path)
