"""Utilities for working with vision and image input."""

import base64
import os

from nxuskit.message import Message


def load_image_base64(file_path: str) -> str:
    """
    Load an image file and encode it to base64.

    Args:
        file_path: Path to the image file

    Returns:
        Base64-encoded image data

    Raises:
        FileNotFoundError: If the file doesn't exist
        IOError: If the file can't be read

    Example:
        b64 = load_image_base64("/path/to/image.jpg")
        msg = Message.user("What's this?").with_image_base64(b64)
    """
    if not os.path.exists(file_path):
        raise FileNotFoundError(f"Image file not found: {file_path}")

    with open(file_path, "rb") as f:
        image_data = f.read()

    return base64.b64encode(image_data).decode("utf-8")


def detect_image_type(file_path: str) -> str:
    """
    Detect image type from file extension.

    Args:
        file_path: Path to the image file

    Returns:
        Image MIME type (e.g., "image/jpeg", "image/png")

    Example:
        mime_type = detect_image_type("photo.jpg")  # Returns "image/jpeg"
    """
    ext = os.path.splitext(file_path)[1].lower()

    mime_types = {
        ".jpg": "image/jpeg",
        ".jpeg": "image/jpeg",
        ".png": "image/png",
        ".gif": "image/gif",
        ".webp": "image/webp",
        ".bmp": "image/bmp",
        ".tiff": "image/tiff",
        ".ico": "image/x-icon",
        ".svg": "image/svg+xml",
    }

    return mime_types.get(ext, "image/jpeg")  # Default to JPEG


def is_valid_url(string: str) -> bool:
    """
    Check if a string is a valid URL.

    Args:
        string: String to validate

    Returns:
        True if the string appears to be a URL

    Example:
        if is_valid_url(image_source):
            msg = msg.with_image_url(image_source)
    """
    return (
        string.startswith("http://")
        or string.startswith("https://")
        or string.startswith("data:image/")
    )


def is_base64(string: str) -> bool:
    """
    Check if a string appears to be base64-encoded.

    Args:
        string: String to validate

    Returns:
        True if the string looks like base64 data

    Example:
        if is_base64(image_data):
            msg = msg.with_image_base64(image_data)
    """
    if not string:
        return False

    # Check length (must be multiple of 4)
    if len(string) % 4 != 0:
        return False

    # Try to decode
    try:
        if isinstance(string, str):
            string_bytes = string.encode("utf-8")
        else:
            string_bytes = string

        base64.b64decode(string_bytes, validate=True)
        return True
    except Exception:
        return False


def add_images_to_message(
    msg: Message,
    image_sources: list,
) -> Message:
    """
    Add multiple images to a message, auto-detecting their type.

    Args:
        msg: Message to add images to
        image_sources: List of image sources (URLs, file paths, or base64 strings)

    Returns:
        Modified message with images added

    Example:
        images = [
            "https://example.com/photo.jpg",
            "/local/image.png",
            base64_data
        ]
        msg = add_images_to_message(Message.user("Analyze"), images)
    """
    for source in image_sources:
        if is_valid_url(source):
            msg = msg.with_image_url(source)
        elif is_base64(source):
            msg = msg.with_image_base64(source)
        elif os.path.exists(source):
            msg = msg.with_image_file(source)
        else:
            # Assume it's a file path even if it doesn't exist yet
            msg = msg.with_image_file(source)

    return msg


def image_to_data_url(file_path: str) -> str:
    """
    Convert an image file to a data URL.

    Args:
        file_path: Path to the image file

    Returns:
        Data URL string (e.g., "data:image/jpeg;base64,/9j/4AAQ...")

    Example:
        data_url = image_to_data_url("photo.jpg")
        msg = Message.user("What's this?").with_image_url(data_url)
    """
    mime_type = detect_image_type(file_path)
    b64_data = load_image_base64(file_path)
    return f"data:{mime_type};base64,{b64_data}"


class ImageLoader:
    """Utility class for loading and preparing images for vision models."""

    def __init__(self, cache_base64: bool = False):
        """
        Initialize ImageLoader.

        Args:
            cache_base64: Whether to cache base64 encodings
        """
        self.cache_base64 = cache_base64
        self._base64_cache = {}

    def load_as_base64(self, file_path: str) -> str:
        """
        Load image as base64, with optional caching.

        Args:
            file_path: Path to the image file

        Returns:
            Base64-encoded image data
        """
        if self.cache_base64 and file_path in self._base64_cache:
            return self._base64_cache[file_path]

        b64_data = load_image_base64(file_path)

        if self.cache_base64:
            self._base64_cache[file_path] = b64_data

        return b64_data

    def load_as_data_url(self, file_path: str) -> str:
        """
        Load image as data URL.

        Args:
            file_path: Path to the image file

        Returns:
            Data URL string
        """
        return image_to_data_url(file_path)

    def prepare_message(
        self,
        content: str,
        image_paths: list,
        use_data_urls: bool = False,
    ) -> Message:
        """
        Create a message with multiple images attached.

        Args:
            content: Message text
            image_paths: List of image file paths
            use_data_urls: If True, use data URLs; otherwise use base64

        Returns:
            Message with images attached

        Example:
            loader = ImageLoader()
            msg = loader.prepare_message(
                "Analyze these images",
                ["photo1.jpg", "photo2.png"]
            )
        """
        msg = Message.user(content)

        for image_path in image_paths:
            if use_data_urls:
                data_url = self.load_as_data_url(image_path)
                msg = msg.with_image_url(data_url)
            else:
                b64_data = self.load_as_base64(image_path)
                msg = msg.with_image_base64(b64_data)

        return msg

    def clear_cache(self) -> None:
        """Clear the base64 cache."""
        self._base64_cache.clear()
