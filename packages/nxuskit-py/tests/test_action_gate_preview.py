"""Executable local conformance tests for the private Action Gate Python Preview."""

from __future__ import annotations

import json
import os
import unittest
from pathlib import Path

FIXTURE_DIR = (
    Path(__file__).resolve().parents[3] / "specs" / "118-v2-action-gate-python-preview" / "fixtures"
)


class ActionGatePreviewTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        library = os.environ.get("NXUSKIT_ACTION_GATE_TEST_LIB")
        if not library:
            raise RuntimeError("NXUSKIT_ACTION_GATE_TEST_LIB is required for Action Gate ABI proof")
        os.environ["NXUSKIT_LIB_PATH"] = library

        import nxuskit
        from nxuskit.preview.action_gate import ActionGate, ActionGateError, ActionGateRequest

        cls.nxuskit = nxuskit
        cls.action_gate = ActionGate
        cls.action_gate_error = ActionGateError
        cls.action_gate_request = ActionGateRequest

    def test_preview_is_namespaced_and_never_production_authorized(self) -> None:
        self.assertNotIn("ActionGate", vars(self.nxuskit))
        capability = self.action_gate.capability()
        self.assertEqual(capability["namespace"], "nxuskit.preview.action_gate")
        self.assertEqual(capability["schema_version"], "nxuskit-action-gate-preview/v1")
        self.assertEqual(capability["native_sdk_version"], "2.0.0")
        self.assertFalse(capability["production_authorization"])
        self.assertFalse(capability["action_execution_authorized"])
        self.assertFalse(capability["public_projection_authorized"])

    def test_all_shared_fixtures_match_the_native_safe_projection(self) -> None:
        for fixture_path in sorted(FIXTURE_DIR.glob("*.json")):
            with self.subTest(fixture=fixture_path.name):
                fixture = json.loads(fixture_path.read_text())
                request = self.action_gate_request.from_mapping(fixture["request"])
                result = self.action_gate.evaluate(request)
                self.assertEqual(
                    normalize(result.to_mapping()),
                    normalize(fixture["expected_result"]),
                )

    def test_python_rejects_invalid_input_without_echoing_the_request(self) -> None:
        fixture = json.loads((FIXTURE_DIR / "allow.json").read_text())
        fixture["request"]["action"]["kind"] = "not-an-action"
        fixture["request"]["actor_context"]["actor_ref"] = "synthetic.actor.secret-value"
        with self.assertRaises(self.action_gate_error) as captured:
            self.action_gate_request.from_mapping(fixture["request"])
        self.assertNotIn("secret-value", str(captured.exception))

    def test_request_models_are_immutable(self) -> None:
        fixture = json.loads((FIXTURE_DIR / "allow.json").read_text())
        request = self.action_gate_request.from_mapping(fixture["request"])
        with self.assertRaises((AttributeError, TypeError)):
            request.case_ref = "synthetic.case.changed"  # type: ignore[misc]


def normalize(value: object) -> object:
    if isinstance(value, dict):
        return {key: normalize(item) for key, item in value.items()}
    if isinstance(value, list):
        return sorted((normalize(item) for item in value), key=repr)
    return value
