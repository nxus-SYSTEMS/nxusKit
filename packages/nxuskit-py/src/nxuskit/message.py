"""Message class with builder pattern for constructing chat messages."""

from dataclasses import dataclass, field
from typing import List

from nxuskit.types import ImageSource, ImageSourceType, Role


@dataclass
class Message:
    """Represents a message in a conversation."""

    role: Role
    content: str
    images: List[ImageSource] = field(default_factory=list)

    @staticmethod
    def user(content: str) -> "Message":
        """Create a user message."""
        return Message(role=Role.USER, content=content)

    @staticmethod
    def assistant(content: str) -> "Message":
        """Create an assistant message."""
        return Message(role=Role.ASSISTANT, content=content)

    @staticmethod
    def system(content: str) -> "Message":
        """Create a system message."""
        return Message(role=Role.SYSTEM, content=content)

    def with_image_url(self, url: str) -> "Message":
        """Add an image from URL to this message."""
        self.images.append(ImageSource(ImageSourceType.URL, url))
        return self

    def with_image_base64(self, data: str) -> "Message":
        """Add a base64-encoded image to this message."""
        self.images.append(ImageSource(ImageSourceType.BASE64, data))
        return self

    def with_image_file(self, path: str) -> "Message":
        """Add an image from file path to this message."""
        self.images.append(ImageSource(ImageSourceType.FILEPATH, path))
        return self
