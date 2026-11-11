#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

SDK_CONFORMANCE_DIR="${NXUSKIT_SDK_CONFORMANCE_DIR:-}"
if [[ -z "${SDK_CONFORMANCE_DIR}" ]]; then
  if [[ -f "${ROOT_DIR}/sdk-packaging/conformance/examples_manifest.json" ]]; then
    SDK_CONFORMANCE_DIR="${ROOT_DIR}/sdk-packaging/conformance"
  else
    SDK_CONFORMANCE_DIR="${ROOT_DIR}/conformance"
  fi
fi

AUTH_CONFORMANCE_DIR="${NXUSKIT_EXAMPLES_CONFORMANCE_DIR:-}"
if [[ -z "${AUTH_CONFORMANCE_DIR}" && -n "${NXUSKIT_EXAMPLES_REPO:-}" ]]; then
  AUTH_CONFORMANCE_DIR="${NXUSKIT_EXAMPLES_REPO}/conformance"
fi
if [[ -z "${AUTH_CONFORMANCE_DIR}" ]]; then
  for candidate in \
    "${ROOT_DIR}/../nxusKit-examples/conformance"; do
    if [[ -f "${candidate}/examples_manifest.json" ]]; then
      AUTH_CONFORMANCE_DIR="${candidate}"
      break
    fi
  done
fi

if [[ ! -f "${SDK_CONFORMANCE_DIR}/examples_manifest.json" ]]; then
  echo "ERROR: SDK examples manifest missing: ${SDK_CONFORMANCE_DIR}/examples_manifest.json" >&2
  exit 1
fi

if [[ -z "${AUTH_CONFORMANCE_DIR}" || ! -f "${AUTH_CONFORMANCE_DIR}/examples_manifest.json" ]]; then
  if [[ "${NXUSKIT_REQUIRE_EXAMPLES_FRESHNESS:-0}" == "1" ]]; then
    echo "ERROR: authoritative examples conformance directory not found." >&2
    echo "Set NXUSKIT_EXAMPLES_REPO or NXUSKIT_EXAMPLES_CONFORMANCE_DIR." >&2
    exit 1
  fi
  echo "examples conformance freshness: skipped; authoritative examples repo not found"
  exit 0
fi

python3 - "${SDK_CONFORMANCE_DIR}" "${AUTH_CONFORMANCE_DIR}" <<'PY'
import json
import sys
from pathlib import Path

sdk = Path(sys.argv[1])
auth = Path(sys.argv[2])

def load_json(path):
    with path.open("r", encoding="utf-8") as f:
        return json.load(f)

def fail(message):
    print(f"ERROR: {message}", file=sys.stderr)
    sys.exit(1)

sdk_manifest = load_json(sdk / "examples_manifest.json")
auth_manifest = load_json(auth / "examples_manifest.json")

sdk_examples = {entry["name"]: entry for entry in sdk_manifest.get("examples", [])}
auth_examples = {entry["name"]: entry for entry in auth_manifest.get("examples", [])}

sdk_names = set(sdk_examples)
auth_names = set(auth_examples)
if len(sdk_examples) != len(auth_examples) or sdk_names != auth_names:
    missing = sorted(auth_names - sdk_names)
    extra = sorted(sdk_names - auth_names)
    parts = [f"sdk_count={len(sdk_examples)}", f"authoritative_count={len(auth_examples)}"]
    if missing:
        parts.append(f"missing={missing}")
    if extra:
        parts.append(f"extra={extra}")
    fail("SDK examples manifest differs from authoritative manifest: " + "; ".join(parts))

hash_mismatches = []
for name in sorted(auth_names):
    sdk_hash = sdk_examples[name].get("content_hash")
    auth_hash = auth_examples[name].get("content_hash")
    if sdk_hash and auth_hash and sdk_hash != auth_hash:
        hash_mismatches.append(name)
if hash_mismatches:
    fail(f"SDK examples content_hash mismatches: {hash_mismatches}")

sdk_tiers_path = sdk / "example-tiers.json"
auth_tiers_path = auth / "example-tiers.json"
if sdk_tiers_path.is_file() and auth_tiers_path.is_file():
    sdk_tiers = load_json(sdk_tiers_path).get("examples", {})
    auth_tiers = load_json(auth_tiers_path).get("examples", {})
    if set(sdk_tiers) != set(auth_tiers):
        missing = sorted(set(auth_tiers) - set(sdk_tiers))
        extra = sorted(set(sdk_tiers) - set(auth_tiers))
        parts = []
        if missing:
            parts.append(f"missing_tiers={missing}")
        if extra:
            parts.append(f"extra_tiers={extra}")
        fail("SDK example tier keys differ from authoritative tier map: " + "; ".join(parts))
    value_mismatches = [
        name for name in sorted(auth_tiers)
        if sdk_tiers.get(name) != auth_tiers.get(name)
    ]
    if value_mismatches:
        fail(f"SDK example tier values differ: {value_mismatches}")

sdk_groups_path = sdk / "example-groups.json"
auth_groups_path = auth / "example-groups.json"
if sdk_groups_path.is_file() and auth_groups_path.is_file():
    if load_json(sdk_groups_path) != load_json(auth_groups_path):
        fail("SDK example-groups.json differs from authoritative example-groups.json")

print(f"examples conformance freshness: clean ({len(sdk_examples)} examples)")
PY
