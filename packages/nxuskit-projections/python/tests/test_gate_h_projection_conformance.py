from __future__ import annotations

import json
import unittest
from pathlib import Path

from nxuskit_projection_alpha.v2_contracts import (
    BoundaryViolation,
    classify_contract_v2,
    collect_core_ref_kinds,
    find_boundary_violation,
    parse_contract_v2,
)


ROOT = Path(__file__).resolve().parents[4]
FIXTURE_ROOT = ROOT / "packages/nxuskit/tests/fixtures/contract_v2"
PROVENANCE = ROOT / "packages/nxuskit-projections/provenance/v2_projection_alpha.json"


def load_json(path: Path) -> object:
    return json.loads(path.read_text(encoding="utf-8"))


class GateHProjectionConformance(unittest.TestCase):
    def test_positive_fixture_corpus_parses_with_pydantic_projection(self) -> None:
        seen: set[str] = set()
        for path in sorted((FIXTURE_ROOT / "positive").glob("*.json")):
            value = load_json(path)
            model = parse_contract_v2(value)
            seen.add(classify_contract_v2(value))
            self.assertEqual(getattr(model, "contract_version"), "2.0-alpha", path.name)

        self.assertEqual(
            seen,
            {
                "CapabilityCatalogEntry",
                "OperationAccepted",
                "OperationEnvelope",
                "OperationEvent",
                "OperationResult",
            },
        )

    def test_operation_and_capability_fixtures_cover_required_core_refs(self) -> None:
        operation_refs = set(
            collect_core_ref_kinds(load_json(FIXTURE_ROOT / "positive/operation-envelope.json"))
        )
        capability_refs = set(
            collect_core_ref_kinds(
                load_json(FIXTURE_ROOT / "positive/capability-catalog-entry.json")
            )
        )
        self.assertEqual(
            operation_refs | capability_refs,
            {
                "ArtifactRef",
                "BatchRef",
                "CapabilityRef",
                "FactSetRef",
                "PacketRef",
                "PlanRef",
                "RouteRef",
                "SchemaRef",
            },
        )

    def test_negative_fixture_corpus_reports_reference_boundary_violation(self) -> None:
        expected_keys = {
            "blob-payload.json": "blob_payload",
            "bulk-rows.json": "rows",
            "file-body.json": "file_body",
            "inline-sql.json": "inline_sql",
            "persona-content.json": "private_persona_ref",
            "record-batch.json": "record_batch",
            "workflow-pack-content.json": "workflow_pack_content_ref",
        }
        for path in sorted((FIXTURE_ROOT / "negative").glob("*.json")):
            violation = find_boundary_violation(load_json(path))
            self.assertIsInstance(violation, BoundaryViolation, path.name)
            assert violation is not None
            self.assertEqual(violation.key, expected_keys[path.name], path.name)

    def test_projection_provenance_records_fixture_source_and_gate_locks(self) -> None:
        provenance = load_json(PROVENANCE)
        self.assertEqual(provenance["artifact"], "v2_projection_conformance_alpha")
        self.assertEqual(provenance["classification"], "preview")
        self.assertEqual(provenance["source"]["fixture_root"], FIXTURE_ROOT.relative_to(ROOT).as_posix())
        self.assertEqual(provenance["scope_locks"]["gate_g"], "locked")
        self.assertEqual(
            set(provenance["ref_required_keys"]),
            {
                "ArtifactRef",
                "BatchRef",
                "CapabilityRef",
                "FactSetRef",
                "PacketRef",
                "PlanRef",
                "RouteRef",
                "SchemaRef",
            },
        )
        self.assertIn(
            "packages/nxuskit/tests/fixtures/contract_v2/positive/operation-envelope.json",
            provenance["fixture_hashes"],
        )


if __name__ == "__main__":
    unittest.main()
