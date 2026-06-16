"""Pythonic ZEN decision table evaluation wrapper over the nxusKit C ABI.

Provides a simple ``zen_evaluate`` function that evaluates a ZEN decision table
model against structured input and returns the evaluation result.

Example::

    from nxuskit.zen import zen_evaluate

    model = {
        "nodes": [...],
        "edges": [...],
    }
    result = zen_evaluate(model, {"age": 25, "income": 50000})
    print(result)
"""

from __future__ import annotations

import asyncio
import json
from typing import Any, Dict

from ._zen_ffi import ZenLibraryNotFoundError, last_error, read_and_free_string, zen_ffi

__all__ = [
    "zen_evaluate",
    "zen_evaluate_async",
    "ZenError",
    "ZenLibraryNotFoundError",
]


class ZenError(RuntimeError):
    """Base error for ZEN evaluation operations."""


def _get_lib():
    from ._zen_ffi import _get_lib as _gl

    return _gl()


def zen_evaluate(model: Dict[str, Any], input_data: Dict[str, Any]) -> Dict[str, Any]:
    """Evaluate a ZEN decision table model against structured input.

    Args:
        model: JDM content as a Python dict (nodes, edges, etc.).
        input_data: Evaluation input as a Python dict.

    Returns:
        Evaluation result as a Python dict.

    Raises:
        ZenError: If evaluation fails (invalid model, malformed input, etc.).
        ZenLibraryNotFoundError: If the nxuskit native library cannot be loaded.
    """
    lib = _get_lib()

    model_json = json.dumps(model).encode("utf-8")
    input_json = json.dumps(input_data).encode("utf-8")

    c_model = zen_ffi.new("char[]", model_json)
    c_input = zen_ffi.new("char[]", input_json)

    result_ptr = lib.nxuskit_zen_evaluate(c_model, c_input)

    if result_ptr == zen_ffi.NULL:
        err = last_error()
        raise ZenError(err if err else "zen_evaluate returned NULL")

    result_str = read_and_free_string(result_ptr)
    return json.loads(result_str)


async def zen_evaluate_async(model: Dict[str, Any], input_data: Dict[str, Any]) -> Dict[str, Any]:
    """Evaluate a ZEN model on a thread pool to avoid blocking the event loop.

    Args:
        model: JDM content as a Python dict.
        input_data: Evaluation input as a Python dict.

    Returns:
        Evaluation result as a Python dict.

    Raises:
        ZenError: If evaluation fails.
        ZenLibraryNotFoundError: If the nxuskit native library cannot be loaded.
    """
    loop = asyncio.get_running_loop()
    return await loop.run_in_executor(None, zen_evaluate, model, input_data)
