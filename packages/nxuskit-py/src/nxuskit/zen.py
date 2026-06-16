"""Public CE ZEN wrapper stub."""

from __future__ import annotations

import asyncio
from typing import Any


class ZenError(RuntimeError):
    """Raised when a Pro ZEN API is requested from public CE."""


class ZenLibraryNotFoundError(RuntimeError):
    """Compatibility placeholder for public CE imports."""


def zen_evaluate(model: dict[str, Any], input_data: dict[str, Any]) -> dict[str, Any]:
    raise ZenError("ZEN evaluation is a Pro capability and is not shipped in public CE")


async def zen_evaluate_async(model: dict[str, Any], input_data: dict[str, Any]) -> dict[str, Any]:
    return await asyncio.to_thread(zen_evaluate, model, input_data)
