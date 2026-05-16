"""nxuskit - Pure Python library mirroring nxusKit Rust API."""

from nxuskit._ffi_errors import (
    EditionInsufficientError,
    FeatureUnavailableError,
    LicenseExpiredError,
    LicenseRequiredError,
    NxuskitError,
)
from nxuskit._version import __author__, __license__, __version__
from nxuskit.errors import (
    AuthenticationError,
    LLMError,
    NetworkError,
    ProviderError,
    RateLimitError,
    TimeoutError,
)
from nxuskit.message import Message
from nxuskit.provider import LLMProvider
from nxuskit.providers import Provider
from nxuskit.retry import (
    AdaptiveRateLimiter,
    RetryConfig,
    RetryIterator,
    retry_on_rate_limit,
    retry_with_backoff,
    should_retry,
)
from nxuskit.security import (
    SecurityIssue,
    SecuritySeverity,
    SecurityValidationResult,
    SecurityValidator,
)
from nxuskit.solver_types import (
    ConstraintDef,
    ConstraintType,
    DomainDef,
    MultiObjectiveMode,
    ObjectiveDef,
    ObjectiveDirection,
    SessionStatus,
    SolverCapabilities,
    SolverConfig,
    SolveResult,
    SolverExplanation,
    SolverStats,
    SolverValue,
    SolveStatus,
    VariableDef,
    VariableType,
)
from nxuskit.streaming import (
    StreamBuffer,
    collect_stream,
    stream_to_file,
    stream_with_callback,
)
from nxuskit.tools import (
    FunctionCall,
    FunctionDefinition,
    ToolCall,
    ToolDefinition,
    ToolResultMessage,
    tool_choice_auto,
    tool_choice_named,
    tool_choice_none,
    tool_choice_required,
)
from nxuskit.types import (
    PUBLIC_CAPABILITY_FIELDS,
    CapabilityStatus,
    ChatRequest,
    ChatResponse,
    ImageSource,
    ImageSourceType,
    LogprobsData,
    ManifestPublicationPosture,
    ModelInfo,
    PublicCapabilityManifest,
    PublicProviderCapability,
    ResponseFormat,
    Role,
    StreamChunk,
    TokenLogprob,
    TokenUsage,
    TopLogprob,
)
from nxuskit.vision import (
    ImageLoader,
    add_images_to_message,
    detect_image_type,
    image_to_data_url,
    is_base64,
    is_valid_url,
    load_image_base64,
)

# FFI-dependent modules are imported lazily to allow pure-Python usage
# (unit tests, type inspection) without the native library present.
# These modules load libnxuskit at import time via _ffi.py.
_FFI_NAMES = {
    # auth_oauth
    "OAuthResult",
    "OAuthStatus",
    "oauth_start",
    "oauth_status",
    "oauth_revoke",
    # clips
    "ClipsSession",
    "ClipsError",
    # license
    "ActivationResult",
    "LicenseResolution",
    "TokenInfo",
    "TrialResult",
    "license_activate",
    "license_deactivate",
    "license_machine_id",
    "license_resolve",
    "license_trial_activate",
    "license_trial_issue",
    "license_validate",
    # solver (stream chunk requires _solver_ffi)
    "SolverStreamChunk",
    # zen
    "zen_evaluate",
    "zen_evaluate_async",
}

_FFI_MODULE_MAP = {
    "OAuthResult": "nxuskit.auth_oauth",
    "OAuthStatus": "nxuskit.auth_oauth",
    "oauth_start": "nxuskit.auth_oauth",
    "oauth_status": "nxuskit.auth_oauth",
    "oauth_revoke": "nxuskit.auth_oauth",
    "ClipsSession": "nxuskit.clips",
    "ClipsError": "nxuskit.clips",
    "ActivationResult": "nxuskit.license",
    "LicenseResolution": "nxuskit.license",
    "TokenInfo": "nxuskit.license",
    "TrialResult": "nxuskit.license",
    "license_activate": "nxuskit.license",
    "license_deactivate": "nxuskit.license",
    "license_machine_id": "nxuskit.license",
    "license_resolve": "nxuskit.license",
    "license_trial_activate": "nxuskit.license",
    "license_trial_issue": "nxuskit.license",
    "license_validate": "nxuskit.license",
    "SolverStreamChunk": "nxuskit.solver",
    "zen_evaluate": "nxuskit.zen",
    "zen_evaluate_async": "nxuskit.zen",
}


def __getattr__(name: str):
    if name in _FFI_NAMES:
        import importlib

        module = importlib.import_module(_FFI_MODULE_MAP[name])
        value = getattr(module, name)
        # Cache in module namespace for subsequent access
        globals()[name] = value
        return value
    raise AttributeError(f"module 'nxuskit' has no attribute {name!r}")


__all__ = [
    "__version__",
    "__author__",
    "__license__",
    # Types
    "Role",
    "ImageSourceType",
    "ImageSource",
    "TokenUsage",
    "ChatRequest",
    "ChatResponse",
    "StreamChunk",
    "ModelInfo",
    "ResponseFormat",
    "CapabilityStatus",
    "ManifestPublicationPosture",
    "PUBLIC_CAPABILITY_FIELDS",
    "PublicProviderCapability",
    "PublicCapabilityManifest",
    "LogprobsData",
    "TokenLogprob",
    "TopLogprob",
    # Message
    "Message",
    # Errors
    "LLMError",
    "AuthenticationError",
    "RateLimitError",
    "NetworkError",
    "ProviderError",
    "TimeoutError",
    # FFI / entitlement errors
    "NxuskitError",
    "FeatureUnavailableError",
    "LicenseRequiredError",
    "LicenseExpiredError",
    "EditionInsufficientError",
    # Provider protocol
    "LLMProvider",
    # Provider factory
    "Provider",
    # Streaming utilities
    "collect_stream",
    "stream_with_callback",
    "stream_to_file",
    "StreamBuffer",
    # Vision utilities
    "load_image_base64",
    "detect_image_type",
    "is_valid_url",
    "is_base64",
    "add_images_to_message",
    "image_to_data_url",
    "ImageLoader",
    # Retry utilities
    "RetryConfig",
    "should_retry",
    "retry_with_backoff",
    "retry_on_rate_limit",
    "RetryIterator",
    "AdaptiveRateLimiter",
    # Solver types
    "SolverStreamChunk",
    "VariableType",
    "VariableDef",
    "DomainDef",
    "ConstraintType",
    "ConstraintDef",
    "ObjectiveDirection",
    "ObjectiveDef",
    "MultiObjectiveMode",
    "SolverConfig",
    "SolveStatus",
    "SolverValue",
    "SolverStats",
    "SolverExplanation",
    "SolveResult",
    "SolverCapabilities",
    "SessionStatus",
    # Tool calling
    "ToolDefinition",
    "FunctionDefinition",
    "ToolCall",
    "FunctionCall",
    "ToolResultMessage",
    "tool_choice_auto",
    "tool_choice_none",
    "tool_choice_required",
    "tool_choice_named",
    # CLIPS Session (FFI-dependent, lazy-loaded)
    "ClipsSession",
    "ClipsError",
    # License management (FFI-dependent, lazy-loaded)
    "ActivationResult",
    "LicenseResolution",
    "TokenInfo",
    "TrialResult",
    "license_activate",
    "license_deactivate",
    "license_machine_id",
    "license_resolve",
    "license_trial_activate",
    "license_trial_issue",
    "license_validate",
    # OAuth authentication (FFI-dependent, lazy-loaded)
    "OAuthResult",
    "OAuthStatus",
    "oauth_start",
    "oauth_status",
    "oauth_revoke",
    # ZEN evaluation (FFI-dependent, lazy-loaded)
    "zen_evaluate",
    "zen_evaluate_async",
    # Security validation
    "SecurityValidator",
    "SecurityValidationResult",
    "SecurityIssue",
    "SecuritySeverity",
]
