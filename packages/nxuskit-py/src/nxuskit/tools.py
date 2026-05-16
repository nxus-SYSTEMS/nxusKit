"""Tool/function calling types for the nxusKit Python SDK.

These types define the canonical tool calling contract, matching the
OpenAI-compatible schema used across all nxusKit wrappers.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


@dataclass
class FunctionDefinition:
    """A function that can be called by the model."""

    name: str
    description: str | None = None
    parameters: dict[str, Any] | None = None


@dataclass
class ToolDefinition:
    """A tool available for the model to call."""

    type: str = "function"
    function: FunctionDefinition = field(default_factory=FunctionDefinition)

    @staticmethod
    def create(
        name: str,
        description: str | None = None,
        parameters: dict[str, Any] | None = None,
    ) -> "ToolDefinition":
        """Create a tool definition for a function."""
        return ToolDefinition(
            type="function",
            function=FunctionDefinition(
                name=name,
                description=description,
                parameters=parameters,
            ),
        )

    def to_dict(self) -> dict[str, Any]:
        """Serialize to dict for JSON encoding."""
        result: dict[str, Any] = {"type": self.type, "function": {"name": self.function.name}}
        if self.function.description is not None:
            result["function"]["description"] = self.function.description
        if self.function.parameters is not None:
            result["function"]["parameters"] = self.function.parameters
        return result


@dataclass
class FunctionCall:
    """Function invocation details from the model."""

    name: str
    arguments: str  # JSON-encoded


@dataclass
class ToolCall:
    """A tool call requested by the model."""

    id: str
    type: str  # always "function"
    function: FunctionCall

    @staticmethod
    def from_dict(d: dict[str, Any]) -> "ToolCall":
        """Deserialize from dict."""
        return ToolCall(
            id=d["id"],
            type=d.get("type", "function"),
            function=FunctionCall(
                name=d["function"]["name"],
                arguments=d["function"]["arguments"],
            ),
        )


@dataclass
class ToolResultMessage:
    """Result of executing a tool, sent back to the model."""

    tool_call_id: str
    content: str
    role: str = "tool"

    def to_dict(self) -> dict[str, Any]:
        """Serialize to dict for JSON encoding."""
        return {
            "role": self.role,
            "tool_call_id": self.tool_call_id,
            "content": self.content,
        }


# ── Tool Choice helpers ───────────────────────────────────────────


def tool_choice_auto() -> str:
    """Return a tool_choice value of 'auto'."""
    return "auto"


def tool_choice_none() -> str:
    """Return a tool_choice value of 'none'."""
    return "none"


def tool_choice_required() -> str:
    """Return a tool_choice value of 'required'."""
    return "required"


def tool_choice_named(function_name: str) -> dict[str, Any]:
    """Return a tool_choice that forces a specific function."""
    return {
        "type": "function",
        "function": {"name": function_name},
    }
