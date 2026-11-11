#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ -d "${ROOT_DIR}/internal" ]]; then
  echo "public CE boundary check: internal checkout detected; export script enforces public boundary"
  exit 0
fi

fail() {
  echo "ERROR: $*" >&2
  exit 1
}

sha256_file() {
  local path="$1"
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$path" | awk '{print $1}'
  elif command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$path" | awk '{print $1}'
  else
    fail "no SHA-256 utility available for public verifier-resource validation"
  fi
}

required_files=(
  "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/zen_stub.rs"
  "packages/nxuskit-engine/crates/nxuskit-cli/src/commands/solver_stub.rs"
  "packages/nxuskit-engine/crates/nxuskit-cli/src/commands/zen_stub.rs"
)
for path in "${required_files[@]}"; do
  [[ -f "${ROOT_DIR}/${path}" ]] || fail "required CE stub is missing: ${path}"
done

verifier_resource_dir="${ROOT_DIR}/packages/nxuskit-engine/crates/nxuskit-core/resources/licensing"
verifier_manifest="${verifier_resource_dir}/production-verifier-bundle-v1.json"
verifier_schema="${verifier_resource_dir}/production-verifier-bundle-v1.schema.json"
verifier_pem="${verifier_resource_dir}/production-verifier-bundle-v1/version-1.pem"
for path in "$verifier_manifest" "$verifier_schema" "$verifier_pem"; do
  [[ -f "$path" && ! -L "$path" ]] || fail "required public verifier resource is missing or not a regular file: ${path#"${ROOT_DIR}/"}"
done
[[ "$(sha256_file "$verifier_manifest")" == "ac569d0499cd994728f20c5f68e61af167dc2229648b770460a4e694e9410931" ]] \
  || fail "public verifier manifest hash mismatch"
[[ "$(sha256_file "$verifier_schema")" == "fb95abeb2cc3f4af3433d4d39ff4453b1d6c70599ed18fc1b6d65a045006c50f" ]] \
  || fail "public verifier schema hash mismatch"
[[ "$(sha256_file "$verifier_pem")" == "f5a48e001977f7b44a9f77f4ea38ae3d9707f5d49aca4968bd923eac38edfdaf" ]] \
  || fail "public verifier PEM hash mismatch"
[[ "$(head -n 1 "$verifier_pem")" == "-----BEGIN PUBLIC KEY-----" ]] \
  || fail "public verifier PEM does not have the required public-key boundary"
[[ "$(tail -n 1 "$verifier_pem")" == "-----END PUBLIC KEY-----" ]] \
  || fail "public verifier PEM does not have the required public-key boundary"
if grep -q "PRIVATE KEY" "$verifier_pem"; then
  fail "private-key material is present in the public verifier resource"
fi
openssl pkey -pubin -in "$verifier_pem" -noout >/dev/null 2>&1 \
  || fail "public verifier PEM does not parse"

forbidden_paths=(
  "internal"
  "specs"
  "packages/nxuskit-engine/crates/nxuskit-engine/src/channel"
  "packages/nxuskit-engine/crates/nxuskit-engine/src/tool_catalog"
  "packages/nxuskit-engine/crates/nxuskit-engine/src/capability_truth"
  "packages/nxuskit-engine/crates/nxuskit-engine/src/clips_bridge"
  "packages/nxuskit-engine/crates/nxuskit-engine/src/peeler_pipeline"
  "packages/nxuskit-engine/crates/nxuskit-engine/docs/provider-channel.md"
  "packages/nxuskit-engine/crates/nxuskit-engine/benches/provider_channel_dispatch.rs"
  "packages/nxuskit-engine/crates/nxuskit-engine/tests/channel_envelope.rs"
  "packages/nxuskit-engine/crates/nxuskit-engine/tests/channel_dispatch.rs"
  "packages/nxuskit-engine/crates/nxuskit-engine/tests/tool_catalog_dispatch.rs"
  "packages/nxuskit-engine/crates/nxuskit-engine/tests/tool_catalog_composition.rs"
  "packages/nxuskit-engine/crates/nxuskit-engine/tests/capability_truth.rs"
  "packages/nxuskit-engine/crates/nxuskit-engine/tests/clips_bridge_contract.rs"
  "packages/nxuskit-engine/crates/nxuskit-engine/tests/testutil/provider_channel.rs"
  "packages/nxuskit-engine/crates/nxuskit-engine/tests/fixtures/provider_channel"
  "packages/nxuskit-engine/crates/nxuskit-engine/tests/fixtures/clips_bridge"
  "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/z3"
  "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/zen"
  "packages/nxuskit-engine/crates/nxuskit-cli/src/commands/solver.rs"
  "packages/nxuskit-engine/crates/nxuskit-cli/src/commands/zen.rs"
  "sdk-packaging/conformance"
  "sdk-packaging/docs/providers/z3-solver.md"
  "docs/user/providers/z3-solver.md"
)
for path in "${forbidden_paths[@]}"; do
  [[ ! -e "${ROOT_DIR}/${path}" ]] || fail "Pro/private payload present in public CE tree: ${path}"
done

if find "${ROOT_DIR}/packages/nxuskit-engine/crates/nxuskit-engine/tests" \
    -type f -name 'peeler_pipeline_*.rs' 2>/dev/null | grep -q .; then
  find "${ROOT_DIR}/packages/nxuskit-engine/crates/nxuskit-engine/tests" \
    -type f -name 'peeler_pipeline_*.rs' 2>/dev/null >&2 || true
  fail "B-Team Peeler pipeline tests are present in public CE tree"
fi

forbidden_cargo_terms=(
  "zen-engine"
  "dep:z3"
  "^z3 ="
  "z3-sys"
  "nxus-licensing-client"
  "nxus-licensing.git"
)
for term in "${forbidden_cargo_terms[@]}"; do
  if grep -R -n \
      --include="Cargo.toml" --include="Cargo.lock" \
      "${term}" "${ROOT_DIR}" 2>/dev/null | grep -q .; then
    grep -R -n \
      --include="Cargo.toml" --include="Cargo.lock" \
      "${term}" "${ROOT_DIR}" 2>/dev/null >&2 || true
    fail "Pro/private Cargo term found in public CE tree: ${term}"
  fi
done

forbidden_source_terms=(
  "zen-engine"
  "nxus-licensing-client"
  "nxus-licensing.git"
  "z3-sys"
  "InternalSolverSession"
  "ConstraintInput"
  "Z3Provider"
  "providers::z3"
  "decisionTableNode"
  "JSON Decision Model"
  "produce_unsat_core"
  "unsat_core"
)
for term in "${forbidden_source_terms[@]}"; do
  if grep -R -n \
      --include="*.rs" --include="*.go" --include="*.py" --include="*.sh" \
      --exclude="assert-public-ce-clean.sh" \
      "${term}" "${ROOT_DIR}" 2>/dev/null | grep -q .; then
    grep -R -n \
      --include="*.rs" --include="*.go" --include="*.py" --include="*.sh" \
      --exclude="assert-public-ce-clean.sh" \
      "${term}" "${ROOT_DIR}" 2>/dev/null >&2 || true
    fail "Pro/private source term found in public CE tree: ${term}"
  fi
done

forbidden_ab_terms=(
  "ProviderChannel"
  "ToolCatalog"
  "CapabilityTruthSnapshot"
  "CeCapabilityView"
  "RequestEnvelope"
  "ResultEnvelope"
  "EntitlementSource"
  "peeler_pipeline"
  "bteam-internal"
)
for term in "${forbidden_ab_terms[@]}"; do
  if grep -R -n \
      --include="*.rs" --include="*.md" --include="*.clp" --include="*.json" \
      --exclude="assert-public-ce-clean.sh" \
      --exclude="ce_boundary_helpers.rs" \
      --exclude="ce_boundary_seeded_leaks.rs" \
      --exclude="ce_facade_external_tool.rs" \
      --exclude="forbidden_tool_symbols.txt" \
      --exclude-dir="ce_boundary" \
      "${term}" "${ROOT_DIR}/packages/nxuskit-engine" "${ROOT_DIR}/packages/nxuskit" 2>/dev/null | grep -q .; then
    grep -R -n \
      --include="*.rs" --include="*.md" --include="*.clp" --include="*.json" \
      --exclude="assert-public-ce-clean.sh" \
      --exclude="ce_boundary_helpers.rs" \
      --exclude="ce_boundary_seeded_leaks.rs" \
      --exclude="ce_facade_external_tool.rs" \
      --exclude="forbidden_tool_symbols.txt" \
      --exclude-dir="ce_boundary" \
      "${term}" "${ROOT_DIR}/packages/nxuskit-engine" "${ROOT_DIR}/packages/nxuskit" 2>/dev/null >&2 || true
    fail "A/B internal term found in public CE tree: ${term}"
  fi
done

core_build_rs="${ROOT_DIR}/packages/nxuskit-engine/crates/nxuskit-core/build.rs"
if [[ -f "${core_build_rs}" ]]; then
  grep -q "Public CE builds only support NXUSKIT_EDITION=oss" "${core_build_rs}" \
    || fail "public CE build.rs does not reject non-OSS editions"
  if grep -n '"pro" => &\["' "${core_build_rs}" >/dev/null || \
     grep -n '"enterprise" => &\["' "${core_build_rs}" >/dev/null; then
    grep -n '"pro" => &\["\|"enterprise" => &\["' "${core_build_rs}" >&2 || true
    fail "public CE build.rs still embeds Pro/Enterprise fallback catalog features"
  fi
fi

solver_sdk="${ROOT_DIR}/packages/nxuskit-engine/crates/nxuskit-core/src/solver_sdk.rs"
if [[ -f "${solver_sdk}" ]]; then
  grep -q "Public CE solver ABI stubs" "${solver_sdk}" \
    || fail "public CE solver ABI was not replaced by unavailable stubs"
  if grep -n 'InternalSolverSession\|ConstraintDef\|ConstraintInput\|Z3Options\|providers::z3' "${solver_sdk}" >/dev/null; then
    grep -n 'InternalSolverSession\|ConstraintDef\|ConstraintInput\|Z3Options\|providers::z3' "${solver_sdk}" >&2 || true
    fail "public CE solver ABI still references solver implementation internals"
  fi
fi

commands_mod="${ROOT_DIR}/packages/nxuskit-engine/crates/nxuskit-cli/src/commands/mod.rs"
if [[ -f "${commands_mod}" ]] && grep -n 'cfg(feature = "pro-engines")' "${commands_mod}" >/dev/null; then
  grep -n 'cfg(feature = "pro-engines")' "${commands_mod}" >&2 || true
  fail "public CE CLI command module still lets pro-engines switch to removed implementation files"
fi

forbidden_doc_spec_terms=(
  "SMT-LIB"
  "ConstraintInput"
  "JSON Decision Model"
  "fixture-set envelope"
  "produce_unsat_core"
  "unsat_core"
  "decisionTableNode"
)
for term in "${forbidden_doc_spec_terms[@]}"; do
  if grep -R -n \
      --include="*.md" --include="*.mdx" \
      "${term}" "${ROOT_DIR}" 2>/dev/null | grep -q .; then
    grep -R -n \
      --include="*.md" --include="*.mdx" \
      "${term}" "${ROOT_DIR}" 2>/dev/null >&2 || true
    fail "detailed Pro behavioral-spec term found in public CE docs: ${term}"
  fi
done

forbidden_public_roadmap_terms=(
  "deferral register"
  "deferral registry"
  "deferred-v"
  "v0.9.4-deferral-register"
  "### Deferred"
  "**Status**: Deferred"
  "deferred from v"
  "deferred to v"
  "deferred beyond v"
  "Deferred indefinitely"
)
for term in "${forbidden_public_roadmap_terms[@]}"; do
  if grep -F -R -n \
      --include="*.md" --include="*.mdx" \
      "${term}" "${ROOT_DIR}" 2>/dev/null | grep -q .; then
    grep -F -R -n \
      --include="*.md" --include="*.mdx" \
      "${term}" "${ROOT_DIR}" 2>/dev/null >&2 || true
    fail "forbidden public Markdown term found in public CE docs: ${term}"
  fi
done

if [[ "${NXUSKIT_PUBLIC_CE_SKIP_CARGO_TREE:-0}" == "1" ]]; then
  echo "public CE boundary check: cargo tree dependency check skipped"
elif command -v cargo >/dev/null 2>&1; then
  tree_output="$(
    cargo tree \
      --manifest-path "${ROOT_DIR}/packages/nxuskit-engine/Cargo.toml" \
      -p nxuskit-cli \
      --no-default-features 2>/dev/null || true
  )"
  if printf '%s\n' "${tree_output}" | grep -E '(^|[[:space:]])(z3|z3-sys|zen-engine|nxus-licensing-client) v' >/dev/null; then
    printf '%s\n' "${tree_output}" >&2
    fail "CE CLI dependency graph includes a Pro/private dependency"
  fi
fi

echo "public CE boundary check: clean"
