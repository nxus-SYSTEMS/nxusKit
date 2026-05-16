"""PEP 561 type stubs for the nxuskit package."""

from dataclasses import dataclass
from enum import Enum
from typing import Any, Iterator, Optional, Protocol, Union

# ── Core Types ──────────────────────────────────────────────────

class CapabilityStatus(str, Enum):
    SUPPORTED: str
    UNSUPPORTED: str
    RECOGNIZED: str
    PROVIDER_SPECIFIC: str
    FUTURE: str
    UNKNOWN: str

class ManifestPublicationPosture(str, Enum):
    SPLIT: str

PUBLIC_CAPABILITY_FIELDS: tuple[str, ...]

@dataclass
class PublicProviderCapability:
    name: str
    display_name: str
    last_reviewed_on: str
    provider_status: str
    capabilities: dict[str, CapabilityStatus]
    def to_dict(self) -> dict[str, Any]: ...

@dataclass
class PublicCapabilityManifest:
    schema_version: str
    posture: ManifestPublicationPosture
    providers: list[PublicProviderCapability]
    def to_dict(self) -> dict[str, Any]: ...

@dataclass
class TokenUsage:
    input_tokens: int
    output_tokens: int
    total_tokens: int
    @property
    def prompt_tokens(self) -> int: ...
    @property
    def completion_tokens(self) -> int: ...

@dataclass
class ChatResponse:
    content: Optional[str]
    usage: TokenUsage
    model: str
    finish_reason: Optional[str]
    tool_calls: Optional[list]
    provider: Optional[str]
    warnings: list[str]
    @property
    def stop_reason(self) -> Optional[str]: ...

@dataclass
class StreamChunk:
    delta: str
    model: Optional[str]
    finish_reason: Optional[str]
    thinking: Optional[str]
    usage: Optional[TokenUsage]
    tool_calls: Optional[list]
    def is_final(self) -> bool: ...
    def has_thinking(self) -> bool: ...
    def has_tool_calls(self) -> bool: ...

@dataclass
class ModelInfo:
    id: str
    name: str
    description: Optional[str]
    size_bytes: Optional[int]
    context_window: Optional[int]
    provider: str
    metadata: dict[str, str]
    def supports_vision(self) -> bool: ...
    def modalities(self) -> list[str]: ...
    def max_images(self) -> Optional[int]: ...
    @classmethod
    def from_dict(cls, data: dict) -> "ModelInfo": ...

@dataclass
class ImageSource:
    source_type: Any  # ImageSourceType enum
    data: str
    media_type: Optional[str]

# ── Message ─────────────────────────────────────────────────────

@dataclass
class Message:
    role: Any  # Role enum
    content: str
    images: list[ImageSource]
    @staticmethod
    def user(content: str) -> "Message": ...
    @staticmethod
    def assistant(content: str) -> "Message": ...
    @staticmethod
    def system(content: str) -> "Message": ...
    def with_image_url(self, url: str) -> "Message": ...
    def with_image_base64(self, data: str) -> "Message": ...
    def with_image_file(self, path: str) -> "Message": ...

# ── Tool Calling ────────────────────────────────────────────────

@dataclass
class FunctionDefinition:
    name: str
    description: str
    parameters: dict

@dataclass
class ToolDefinition:
    type: str
    function: FunctionDefinition
    @staticmethod
    def create(name: str, description: str, parameters: dict) -> "ToolDefinition": ...
    def to_dict(self) -> dict: ...

@dataclass
class FunctionCall:
    name: str
    arguments: str

@dataclass
class ToolCall:
    id: str
    type: str
    function: FunctionCall
    @classmethod
    def from_dict(cls, data: dict) -> "ToolCall": ...

def tool_choice_auto() -> str: ...
def tool_choice_none() -> str: ...
def tool_choice_required() -> str: ...
def tool_choice_named(name: str) -> dict: ...

# ── Provider Protocol ───────────────────────────────────────────

class LLMProvider(Protocol):
    @property
    def model(self) -> str: ...
    @property
    def provider_name(self) -> str: ...
    def chat(
        self,
        messages: list[Message],
        *,
        model: Optional[str] = ...,
        temperature: Optional[float] = ...,
        max_tokens: Optional[int] = ...,
        top_p: Optional[float] = ...,
        stop: Optional[Union[str, list[str]]] = ...,
        response_format: Optional[Any] = ...,
        tools: Optional[list[ToolDefinition]] = ...,
        tool_choice: Optional[Any] = ...,
    ) -> ChatResponse: ...
    def chat_stream(
        self,
        messages: list[Message],
        *,
        model: Optional[str] = ...,
        temperature: Optional[float] = ...,
        max_tokens: Optional[int] = ...,
        top_p: Optional[float] = ...,
        stop: Optional[Union[str, list[str]]] = ...,
        response_format: Optional[Any] = ...,
        tools: Optional[list[ToolDefinition]] = ...,
        tool_choice: Optional[Any] = ...,
    ) -> Iterator[StreamChunk]: ...
    def list_models(self) -> list[ModelInfo]: ...

# ── Provider Factory ────────────────────────────────────────────

class Provider:
    @staticmethod
    def claude(*, model: str = ..., api_key: Optional[str] = ..., **kwargs: Any) -> Any: ...
    @staticmethod
    def openai(*, model: str = ..., api_key: Optional[str] = ..., **kwargs: Any) -> Any: ...
    @staticmethod
    def ollama(*, model: str = ..., **kwargs: Any) -> Any: ...
    @staticmethod
    def groq(*, model: str = ..., api_key: Optional[str] = ..., **kwargs: Any) -> Any: ...
    @staticmethod
    def xai(*, model: str = ..., api_key: Optional[str] = ..., **kwargs: Any) -> Any: ...
    @staticmethod
    def mistral(*, model: str = ..., api_key: Optional[str] = ..., **kwargs: Any) -> Any: ...
    @staticmethod
    def fireworks(*, model: str = ..., api_key: Optional[str] = ..., **kwargs: Any) -> Any: ...
    @staticmethod
    def together(*, model: str = ..., api_key: Optional[str] = ..., **kwargs: Any) -> Any: ...
    @staticmethod
    def openrouter(*, model: str = ..., api_key: Optional[str] = ..., **kwargs: Any) -> Any: ...
    @staticmethod
    def perplexity(*, model: str = ..., api_key: Optional[str] = ..., **kwargs: Any) -> Any: ...
    @staticmethod
    def lmstudio(*, model: str = ..., **kwargs: Any) -> Any: ...

# ── Errors ──────────────────────────────────────────────────────

class LLMError(Exception):
    status_code: Optional[int]
    provider: Optional[str]
    model: Optional[str]
    @property
    def is_retryable(self) -> bool: ...

class AuthenticationError(LLMError): ...

class RateLimitError(LLMError):
    retry_after: Optional[float]

class NetworkError(LLMError): ...
class TimeoutError(LLMError): ...
class ProviderError(LLMError): ...

# FFI errors
class NxuskitError(Exception):
    error_type: str
    message: str
    provider: Optional[str]
    feature: Optional[str]

class ConfigError(NxuskitError): ...
class FeatureUnavailableError(NxuskitError): ...
class LicenseRequiredError(NxuskitError): ...
class LicenseExpiredError(NxuskitError): ...

class EditionInsufficientError(NxuskitError):
    required_edition: Optional[str]

# ── FFI Provider ────────────────────────────────────────────────

class FFIProvider:
    def chat(self, request: dict[str, Any]) -> ChatResponse: ...
    def stream(self, request: dict[str, Any]) -> Iterator[StreamChunk]: ...
    def list_models(self) -> list[ModelInfo]: ...
    def close(self) -> None: ...
    def __enter__(self) -> "FFIProvider": ...
    def __exit__(self, *args: Any) -> None: ...

def create_ffi_provider(config: dict[str, Any]) -> FFIProvider: ...
