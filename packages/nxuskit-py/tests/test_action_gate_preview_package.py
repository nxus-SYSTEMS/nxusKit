"""Fresh-install conformance test for the local Action Gate Python Preview.

This module is copied to a temporary directory and run by the package receipt.
It intentionally has no project-relative imports: the installed distribution and
the explicitly supplied candidate native library are the only SDK inputs.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


class ActionGatePreviewPackageTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.fixture_dir = _required_directory("NXUSKIT_ACTION_GATE_FIXTURE_DIR")
        cls.candidate_library = _required_file("NXUSKIT_ACTION_GATE_TEST_LIB")
        cls.source_package = _required_directory("NXUSKIT_ACTION_GATE_SOURCE_PACKAGE")
        os.environ["NXUSKIT_LIB_PATH"] = str(cls.candidate_library)

        import nxuskit
        from nxuskit._ffi import _lib_path
        from nxuskit.preview.action_gate import ActionGate, ActionGateRequest

        cls.nxuskit = nxuskit
        cls.loaded_library = Path(_lib_path).resolve()
        cls.action_gate = ActionGate
        cls.action_gate_request = ActionGateRequest

    def test_installed_distribution_uses_the_explicit_candidate_library(self) -> None:
        self.assertEqual(self.nxuskit.__version__, "2.0.0")
        self.assertEqual(self.loaded_library, self.candidate_library)
        self.assertFalse(Path(self.nxuskit.__file__).resolve().is_relative_to(self.source_package))
        self.assertEqual(self.action_gate.capability()["native_sdk_version"], "2.0.0")

    def test_primary_fixtures_match_the_authoritative_native_result(self) -> None:
        for name in ("allow.json", "deny.json", "review-required.json"):
            with self.subTest(fixture=name):
                fixture = json.loads((self.fixture_dir / name).read_text())
                request = self.action_gate_request.from_mapping(fixture["request"])
                self.assertEqual(
                    normalize(self.action_gate.evaluate(request).to_mapping()),
                    normalize(fixture["expected_result"]),
                )

    def test_missing_explicit_library_fails_without_a_global_sdk_fallback(self) -> None:
        with tempfile.TemporaryDirectory(prefix="nxuskit-action-gate-no-lib-") as scratch:
            environment = {
                key: value
                for key, value in os.environ.items()
                if key
                not in {
                    "NXUSKIT_LIB_DIR",
                    "NXUSKIT_SDK_DIR",
                    "NXUSKIT_LIB_PATH",
                    "PYTHONPATH",
                    "DYLD_LIBRARY_PATH",
                    "LD_LIBRARY_PATH",
                }
            }
            environment["HOME"] = scratch
            outcome = subprocess.run(
                [
                    sys.executable,
                    "-c",
                    "from nxuskit.preview.action_gate import ActionGate; ActionGate.capability()",
                ],
                cwd=scratch,
                env=environment,
                text=True,
                capture_output=True,
                check=False,
            )

        self.assertNotEqual(outcome.returncode, 0)
        self.assertNotIn(str(self.source_package), outcome.stdout + outcome.stderr)


def _required_file(name: str) -> Path:
    path = Path(os.environ[name]).resolve()
    if not path.is_file():
        raise RuntimeError(f"{name} must identify a readable candidate library")
    return path


def _required_directory(name: str) -> Path:
    path = Path(os.environ[name]).resolve()
    if not path.is_dir():
        raise RuntimeError(f"{name} must identify a readable directory")
    return path


def normalize(value: object) -> object:
    if isinstance(value, dict):
        return {key: normalize(item) for key, item in value.items()}
    if isinstance(value, list):
        return sorted((normalize(item) for item in value), key=repr)
    return value


if __name__ == "__main__":
    unittest.main(verbosity=2)
