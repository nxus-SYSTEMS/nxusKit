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

required_files=(
  "packages/nxuskit-engine/crates/nxuskit-engine/src/providers/zen_stub.rs"
  "packages/nxuskit-engine/crates/nxuskit-cli/src/commands/solver_stub.rs"
  "packages/nxuskit-engine/crates/nxuskit-cli/src/commands/zen_stub.rs"
)
for path in "${required_files[@]}"; do
  [[ -f "${ROOT_DIR}/${path}" ]] || fail "required CE stub is missing: ${path}"
done

forbidden_paths=(
  "internal"
  "specs"
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
