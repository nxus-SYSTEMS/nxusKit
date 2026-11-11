#!/usr/bin/env python3
"""Generate Gate H alpha projection validators from v2 fixture evidence."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
FIXTURE_ROOT = ROOT / "packages/nxuskit/tests/fixtures/contract_v2"
PROJECTION_ROOT = ROOT / "packages/nxuskit-projections"
PY_OUT = PROJECTION_ROOT / "python/nxuskit_projection_alpha/v2_contracts.py"
TS_OUT = PROJECTION_ROOT / "typescript/src/v2_contracts.zod.ts"
PROVENANCE_OUT = PROJECTION_ROOT / "provenance/v2_projection_alpha.json"

CONTRACT_VERSION = "2.0-alpha"
REF_ORDER = [
    "ArtifactRef",
    "BatchRef",
    "FactSetRef",
    "PlanRef",
    "PacketRef",
    "SchemaRef",
    "CapabilityRef",
    "RouteRef",
]


def load_json(path: Path) -> object:
    return json.loads(path.read_text(encoding="utf-8"))


def classify_fixture(path: Path, value: object) -> str:
    if not isinstance(value, dict):
        return "unknown"
    if "vocabulary" in value and "capability_ref" in value:
        return "CapabilityCatalogEntry"
    if "event_id" in value:
        return "OperationEvent"
    if "output_refs" in value or ("result_ref" in value and "completed_at" in value):
        return "OperationResult"
    if "status" in value and ("accepted_at" in value or "validation_errors" in value):
        return "OperationAccepted"
    if "actor" in value and "execution_policy" in value:
        return "OperationEnvelope"
    raise ValueError(f"cannot classify positive fixture {path}")


def collect_positive_groups() -> dict[str, list[dict[str, object]]]:
    groups: dict[str, list[dict[str, object]]] = {}
    for path in sorted((FIXTURE_ROOT / "positive").glob("*.json")):
        value = load_json(path)
        if not isinstance(value, dict):
            raise ValueError(f"positive fixture must be an object: {path}")
        kind = classify_fixture(path, value)
        groups.setdefault(kind, []).append(value)
    return groups


def required_keys_for(group: list[dict[str, object]]) -> list[str]:
    common = set(group[0].keys())
    for value in group[1:]:
        common &= set(value.keys())
    return sorted(common)


def classify_ref_value(value: object) -> str | None:
    if not isinstance(value, dict):
        return None
    kind = value.get("kind")
    if kind == "artifact" and "artifact_id" in value:
        return "ArtifactRef"
    if kind == "batch" and "batch_id" in value:
        return "BatchRef"
    if kind == "fact_set" and "fact_set_id" in value:
        return "FactSetRef"
    if kind == "packet" and "packet_id" in value:
        return "PacketRef"
    if "schema_id" in value and "schema_kind" in value:
        return "SchemaRef"
    if "plan_id" in value and "plan_kind" in value:
        return "PlanRef"
    if "capability_id" in value and "capability_version" in value:
        return "CapabilityRef"
    if "route_id" in value and "locality" in value:
        return "RouteRef"
    return None


def collect_ref_groups_from_value(
    value: object, groups: dict[str, list[dict[str, object]]]
) -> None:
    if isinstance(value, dict):
        ref_name = classify_ref_value(value)
        if ref_name is not None:
            groups.setdefault(ref_name, []).append(value)
        for nested in value.values():
            collect_ref_groups_from_value(nested, groups)
    elif isinstance(value, list):
        for nested in value:
            collect_ref_groups_from_value(nested, groups)


def collect_ref_groups(groups: dict[str, list[dict[str, object]]]) -> dict[str, list[dict[str, object]]]:
    ref_groups: dict[str, list[dict[str, object]]] = {}
    for group in groups.values():
        for value in group:
            collect_ref_groups_from_value(value, ref_groups)
    missing = [name for name in REF_ORDER if name not in ref_groups]
    if missing:
        raise ValueError(f"missing required ref evidence: {', '.join(missing)}")
    return {name: ref_groups[name] for name in REF_ORDER}


def fixture_hashes() -> dict[str, str]:
    hashes: dict[str, str] = {}
    for path in sorted(FIXTURE_ROOT.glob("*/*.json")):
        rel = path.relative_to(ROOT).as_posix()
        hashes[rel] = "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()
    return hashes


def render_python_model(name: str, required_keys: list[str]) -> str:
    lines = [f"class {name}(ContractBase):"]
    for key in required_keys:
        if key == "contract_version":
            lines.append(f'    {key}: Literal["{CONTRACT_VERSION}"]')
        else:
            lines.append(f"    {key}: Any")
    if len(lines) == 1:
        lines.append("    pass")
    return "\n".join(lines)


def render_python_ref_required(ref_groups: dict[str, list[dict[str, object]]]) -> str:
    lines = ["REF_REQUIRED_KEYS = {"]
    for name in REF_ORDER:
        keys = tuple(required_keys_for(ref_groups[name]))
        lines.append(f"    {json.dumps(name)}: {keys!r},")
    lines.append("}")
    return "\n".join(lines)


def render_python(groups: dict[str, list[dict[str, object]]], ref_groups: dict[str, list[dict[str, object]]]) -> str:
    model_blocks = [
        render_python_model(name, required_keys_for(groups[name])) for name in sorted(groups)
    ]
    model_section = "\n\n\n".join(model_blocks)
    model_names = sorted(groups)
    ref_required = render_python_ref_required(ref_groups)
    return f'''# Generated by tools/projection/generate_v2_projection_alpha.py
# Source fixtures: packages/nxuskit/tests/fixtures/contract_v2

from __future__ import annotations

from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, model_validator


CONTRACT_VERSION = "{CONTRACT_VERSION}"

BULK_KEYS = {{
    "record_batch",
    "record_batches",
    "row_json",
    "rows",
    "inline_sql",
    "sql_text",
    "sql",
    "file_contents",
    "file_content",
    "file_bytes",
    "blob",
    "payload",
    "body",
}}
INLINE_SUFFIXES = ("_body", "_blob", "_payload", "_bytes", "_contents")
PRIVATE_TERMS = (
    "persona",
    "workflow_pack",
    "providerchannel",
    "provider_channel",
    "toolcatalog",
    "tool_catalog",
    "private_route",
    "protected_route",
    "pro_private_route",
)

{ref_required}


class BoundaryViolation(ValueError):
    def __init__(self, kind: str, path: str, key: str) -> None:
        self.kind = kind
        self.path = path
        self.key = key
        super().__init__(f"{{kind}} at {{path}}: {{key}}")


class RefViolation(ValueError):
    def __init__(self, ref_name: str, path: str, missing_key: str) -> None:
        self.ref_name = ref_name
        self.path = path
        self.missing_key = missing_key
        super().__init__(f"{{ref_name}} missing {{missing_key}} at {{path}}")


def find_boundary_violation(value: Any, path: str = "$") -> BoundaryViolation | None:
    if isinstance(value, dict):
        for key, nested in value.items():
            normalized = str(key).lower()
            if normalized in BULK_KEYS:
                return BoundaryViolation("bulk_data_key", path, str(key))
            if normalized.endswith(INLINE_SUFFIXES):
                return BoundaryViolation("inline_body_key", path, str(key))
            if any(term in normalized for term in PRIVATE_TERMS):
                return BoundaryViolation("protected_content_key", path, str(key))
            violation = find_boundary_violation(nested, f"{{path}}.{{key}}")
            if violation is not None:
                return violation
    elif isinstance(value, list):
        for index, nested in enumerate(value):
            violation = find_boundary_violation(nested, f"{{path}}[{{index}}]")
            if violation is not None:
                return violation
    elif isinstance(value, str):
        normalized = value.lower()
        if value.startswith("data:") and ";base64," in value:
            return BoundaryViolation("data_url_value", path, "data_url")
        if any(term in normalized for term in PRIVATE_TERMS):
            return BoundaryViolation("protected_content_key", path, "value")
    return None


def classify_ref_contract(value: Any) -> str | None:
    if not isinstance(value, dict):
        return None
    kind = value.get("kind")
    if kind == "artifact" and "artifact_id" in value:
        return "ArtifactRef"
    if kind == "batch" and "batch_id" in value:
        return "BatchRef"
    if kind == "fact_set" and "fact_set_id" in value:
        return "FactSetRef"
    if kind == "packet" and "packet_id" in value:
        return "PacketRef"
    if "schema_id" in value and "schema_kind" in value:
        return "SchemaRef"
    if "plan_id" in value and "plan_kind" in value:
        return "PlanRef"
    if "capability_id" in value and "capability_version" in value:
        return "CapabilityRef"
    if "route_id" in value and "locality" in value:
        return "RouteRef"
    return None


def find_core_ref_violation(value: Any, path: str = "$") -> RefViolation | None:
    if isinstance(value, dict):
        ref_name = classify_ref_contract(value)
        if ref_name is not None:
            for key in REF_REQUIRED_KEYS[ref_name]:
                if key not in value:
                    return RefViolation(ref_name, path, key)
        for key, nested in value.items():
            violation = find_core_ref_violation(nested, f"{{path}}.{{key}}")
            if violation is not None:
                return violation
    elif isinstance(value, list):
        for index, nested in enumerate(value):
            violation = find_core_ref_violation(nested, f"{{path}}[{{index}}]")
            if violation is not None:
                return violation
    return None


def collect_core_ref_kinds(value: Any) -> list[str]:
    seen: set[str] = set()

    def visit(nested: Any) -> None:
        if isinstance(nested, dict):
            ref_name = classify_ref_contract(nested)
            if ref_name is not None:
                seen.add(ref_name)
            for child in nested.values():
                visit(child)
        elif isinstance(nested, list):
            for child in nested:
                visit(child)

    visit(value)
    return sorted(seen)


def validate_reference_only(value: Any) -> None:
    violation = find_boundary_violation(value)
    if violation is not None:
        raise violation
    ref_violation = find_core_ref_violation(value)
    if ref_violation is not None:
        raise ref_violation


class ContractBase(BaseModel):
    model_config = ConfigDict(extra="allow")

    @model_validator(mode="after")
    def _reference_only_boundary(self) -> "ContractBase":
        validate_reference_only(self.model_dump(mode="json"))
        return self


{model_section}


MODEL_BY_NAME = {{
{chr(10).join(f'    "{name}": {name},' for name in model_names)}
}}


def classify_contract_v2(value: Any) -> str:
    if not isinstance(value, dict):
        raise ValueError("v2 contract projection expects a JSON object")
    if "vocabulary" in value and "capability_ref" in value:
        return "CapabilityCatalogEntry"
    if "event_id" in value:
        return "OperationEvent"
    if "output_refs" in value or ("result_ref" in value and "completed_at" in value):
        return "OperationResult"
    if "status" in value and ("accepted_at" in value or "validation_errors" in value):
        return "OperationAccepted"
    if "actor" in value and "execution_policy" in value:
        return "OperationEnvelope"
    raise ValueError("unknown v2 contract projection shape")


def parse_contract_v2(value: Any) -> ContractBase:
    model_name = classify_contract_v2(value)
    return MODEL_BY_NAME[model_name].model_validate(value)
'''


def render_ts_schema(name: str, required_keys: list[str]) -> str:
    fields = []
    for key in required_keys:
        if key == "contract_version":
            fields.append(f'  {json.dumps(key)}: z.literal("{CONTRACT_VERSION}")')
        else:
            fields.append(f"  {json.dumps(key)}: z.unknown()")
    field_block = ",\n".join(fields)
    return (
        f"export const {name}Schema = "
        f"withBoundary(z.object({{\n{field_block}\n}}).passthrough());"
    )


def render_ts_ref_required(ref_groups: dict[str, list[dict[str, object]]]) -> str:
    lines = ["export const refRequiredKeys = {"]
    for name in REF_ORDER:
        keys = ", ".join(json.dumps(key) for key in required_keys_for(ref_groups[name]))
        lines.append(f"  {name}: [{keys}],")
    lines.append("} as const;")
    return "\n".join(lines)


def render_typescript(groups: dict[str, list[dict[str, object]]], ref_groups: dict[str, list[dict[str, object]]]) -> str:
    schema_blocks = [
        render_ts_schema(name, required_keys_for(groups[name])) for name in sorted(groups)
    ]
    names = sorted(groups)
    registry = "\n".join(f"  {name}: {name}Schema," for name in names)
    ref_required = render_ts_ref_required(ref_groups)
    return f'''// Generated by tools/projection/generate_v2_projection_alpha.py
// Source fixtures: packages/nxuskit/tests/fixtures/contract_v2

import {{ z }} from "zod";

export const CONTRACT_VERSION = "{CONTRACT_VERSION}";

export type BoundaryViolation = {{
  kind: "bulk_data_key" | "inline_body_key" | "protected_content_key" | "data_url_value";
  path: string;
  key: string;
}};

export type RefViolation = {{
  refName: CoreRefKind;
  path: string;
  missingKey: string;
}};

const BULK_KEYS = new Set([
  "record_batch",
  "record_batches",
  "row_json",
  "rows",
  "inline_sql",
  "sql_text",
  "sql",
  "file_contents",
  "file_content",
  "file_bytes",
  "blob",
  "payload",
  "body",
]);
const INLINE_SUFFIXES = ["_body", "_blob", "_payload", "_bytes", "_contents"];
const PRIVATE_TERMS = [
  "persona",
  "workflow_pack",
  "providerchannel",
  "provider_channel",
  "toolcatalog",
  "tool_catalog",
  "private_route",
  "protected_route",
  "pro_private_route",
];

{ref_required}

export type CoreRefKind = keyof typeof refRequiredKeys;

export function findBoundaryViolation(value: unknown, path = "$"): BoundaryViolation | undefined {{
  if (value && typeof value === "object" && !Array.isArray(value)) {{
    for (const [key, nested] of Object.entries(value as Record<string, unknown>)) {{
      const normalized = key.toLowerCase();
      if (BULK_KEYS.has(normalized)) return {{ kind: "bulk_data_key", path, key }};
      if (INLINE_SUFFIXES.some((suffix) => normalized.endsWith(suffix))) {{
        return {{ kind: "inline_body_key", path, key }};
      }}
      if (PRIVATE_TERMS.some((term) => normalized.includes(term))) {{
        return {{ kind: "protected_content_key", path, key }};
      }}
      const nestedViolation = findBoundaryViolation(nested, `${{path}}.${{key}}`);
      if (nestedViolation) return nestedViolation;
    }}
  }} else if (Array.isArray(value)) {{
    for (const [index, nested] of value.entries()) {{
      const nestedViolation = findBoundaryViolation(nested, `${{path}}[${{index}}]`);
      if (nestedViolation) return nestedViolation;
    }}
  }} else if (typeof value === "string") {{
    const normalized = value.toLowerCase();
    if (value.startsWith("data:") && value.includes(";base64,")) {{
      return {{ kind: "data_url_value", path, key: "data_url" }};
    }}
    if (PRIVATE_TERMS.some((term) => normalized.includes(term))) {{
      return {{ kind: "protected_content_key", path, key: "value" }};
    }}
  }}
  return undefined;
}}

export function classifyRefContract(value: Record<string, unknown>): CoreRefKind | undefined {{
  const kind = value.kind;
  if (kind === "artifact" && "artifact_id" in value) return "ArtifactRef";
  if (kind === "batch" && "batch_id" in value) return "BatchRef";
  if (kind === "fact_set" && "fact_set_id" in value) return "FactSetRef";
  if (kind === "packet" && "packet_id" in value) return "PacketRef";
  if ("schema_id" in value && "schema_kind" in value) return "SchemaRef";
  if ("plan_id" in value && "plan_kind" in value) return "PlanRef";
  if ("capability_id" in value && "capability_version" in value) return "CapabilityRef";
  if ("route_id" in value && "locality" in value) return "RouteRef";
  return undefined;
}}

export function findCoreRefViolation(value: unknown, path = "$"): RefViolation | undefined {{
  if (value && typeof value === "object" && !Array.isArray(value)) {{
    const object = value as Record<string, unknown>;
    const refName = classifyRefContract(object);
    if (refName) {{
      for (const key of refRequiredKeys[refName]) {{
        if (!(key in object)) return {{ refName, path, missingKey: key }};
      }}
    }}
    for (const [key, nested] of Object.entries(object)) {{
      const nestedViolation = findCoreRefViolation(nested, `${{path}}.${{key}}`);
      if (nestedViolation) return nestedViolation;
    }}
  }} else if (Array.isArray(value)) {{
    for (const [index, nested] of value.entries()) {{
      const nestedViolation = findCoreRefViolation(nested, `${{path}}[${{index}}]`);
      if (nestedViolation) return nestedViolation;
    }}
  }}
  return undefined;
}}

export function collectCoreRefKinds(value: unknown): CoreRefKind[] {{
  const seen = new Set<CoreRefKind>();
  const visit = (nested: unknown): void => {{
    if (nested && typeof nested === "object" && !Array.isArray(nested)) {{
      const object = nested as Record<string, unknown>;
      const refName = classifyRefContract(object);
      if (refName) seen.add(refName);
      for (const child of Object.values(object)) visit(child);
    }} else if (Array.isArray(nested)) {{
      for (const child of nested) visit(child);
    }}
  }};
  visit(value);
  return [...seen].sort();
}}

function withBoundary<T extends z.ZodTypeAny>(schema: T): T {{
  return schema.superRefine((value, context) => {{
    const violation = findBoundaryViolation(value);
    if (violation) {{
      context.addIssue({{
        code: z.ZodIssueCode.custom,
        message: `${{violation.kind}} at ${{violation.path}}: ${{violation.key}}`,
      }});
    }}
    const refViolation = findCoreRefViolation(value);
    if (refViolation) {{
      context.addIssue({{
        code: z.ZodIssueCode.custom,
        message: `${{refViolation.refName}} missing ${{refViolation.missingKey}} at ${{refViolation.path}}`,
      }});
    }}
  }}) as T;
}}

{chr(10).join(schema_blocks)}

export const schemaByName = {{
{registry}
}} as const;

export function classifyContractV2(value: unknown): keyof typeof schemaByName {{
  if (!value || typeof value !== "object" || Array.isArray(value)) {{
    throw new Error("v2 contract projection expects a JSON object");
  }}
  const object = value as Record<string, unknown>;
  if ("vocabulary" in object && "capability_ref" in object) return "CapabilityCatalogEntry";
  if ("event_id" in object) return "OperationEvent";
  if ("output_refs" in object || ("result_ref" in object && "completed_at" in object)) {{
    return "OperationResult";
  }}
  if ("status" in object && ("accepted_at" in object || "validation_errors" in object)) {{
    return "OperationAccepted";
  }}
  if ("actor" in object && "execution_policy" in object) return "OperationEnvelope";
  throw new Error("unknown v2 contract projection shape");
}}

export function parseContractV2(value: unknown): unknown {{
  const name = classifyContractV2(value);
  return schemaByName[name].parse(value);
}}
'''


def main() -> None:
    groups = collect_positive_groups()
    ref_groups = collect_ref_groups(groups)
    for path in [PY_OUT.parent, TS_OUT.parent, PROVENANCE_OUT.parent]:
        path.mkdir(parents=True, exist_ok=True)

    PY_OUT.write_text(render_python(groups, ref_groups), encoding="utf-8")
    TS_OUT.write_text(render_typescript(groups, ref_groups), encoding="utf-8")
    provenance = {
        "artifact": "v2_projection_conformance_alpha",
        "contract_version": CONTRACT_VERSION,
        "classification": "preview",
        "source": {
            "fixture_root": FIXTURE_ROOT.relative_to(ROOT).as_posix(),
            "schema_cli": "nxuskit-cli contract-v2 schema emit",
        },
        "generated": {
            "python": PY_OUT.relative_to(ROOT).as_posix(),
            "typescript": TS_OUT.relative_to(ROOT).as_posix(),
        },
        "fixture_hashes": fixture_hashes(),
        "model_required_keys": {
            name: required_keys_for(groups[name]) for name in sorted(groups)
        },
        "ref_required_keys": {
            name: required_keys_for(ref_groups[name]) for name in REF_ORDER
        },
        "scope_locks": {
            "gate_g": "locked",
            "public_docs_finalization": "locked",
            "go_openapi_mcp_utcp": "locked",
        },
    }
    PROVENANCE_OUT.write_text(
        json.dumps(provenance, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


if __name__ == "__main__":
    main()
