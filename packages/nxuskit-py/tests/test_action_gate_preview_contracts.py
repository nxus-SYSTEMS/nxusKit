"""Schema-only contract checks for the Action Gate Preview."""

from __future__ import annotations

import json
from pathlib import Path

RESULT_SCHEMA = (
    Path(__file__).resolve().parents[3]
    / "specs"
    / "118-v2-action-gate-python-preview"
    / "contracts"
    / "action-gate-preview-result.schema.json"
)


def test_nullable_result_handoff_ref_reuses_the_frozen_reference_contract() -> None:
    schema = json.loads(RESULT_SCHEMA.read_text())
    handoff = schema["properties"]["review_handoff_ref"]

    assert handoff == {
        "anyOf": [
            {"$ref": "#/$defs/ref"},
            {"type": "null"},
        ]
    }
