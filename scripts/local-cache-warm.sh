#!/usr/bin/env bash
# Warm local nxusKit build caches using the shared ~/.cache/nxus-ci root.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
ORIGINAL_ARGS=("$@")

usage() {
  cat <<'USAGE'
Usage: scripts/local-cache-warm.sh [options]

Options:
  --full-features  Also warm the nxuskit-engine optional-feature check path.
  --release        Also warm release builds for nxuskit-core and nxuskit-cli.
  --skip-go        Skip Go module/build-cache warmup.
  --skip-python    Skip Python compile warmup.
  --help           Show this help.

Defaults warm the CE/native Rust check path, nxuskit-cli check path, Rust wrapper
check path, Go module/build cache, and Python bytecode compile cache.
USAGE
}

FULL_FEATURES=0
RELEASE=0
SKIP_GO=0
SKIP_PYTHON=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --full-features)
      FULL_FEATURES=1
      shift
      ;;
    --release)
      RELEASE=1
      shift
      ;;
    --skip-go)
      SKIP_GO=1
      shift
      ;;
    --skip-python)
      SKIP_PYTHON=1
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ "${NXUSKIT_LOCAL_CACHE_ENV_ACTIVE:-}" != "1" ]]; then
  exec "$SCRIPT_DIR/local-cache-env.sh" -- "$SCRIPT_DIR/local-cache-warm.sh" "${ORIGINAL_ARGS[@]}"
fi

cd "$REPO_ROOT"

echo "Using local nxusKit caches:"
echo "  NXUS_CI_CACHE_ROOT=${NXUS_CI_CACHE_ROOT:-}"
echo "  CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-}"
echo "  CLIPS_LIB_CACHE_ROOT=${CLIPS_LIB_CACHE_ROOT:-}"
echo "  GOCACHE=${GOCACHE:-}"
echo "  GOMODCACHE=${GOMODCACHE:-}"
echo "  CARGO_NET_GIT_FETCH_WITH_CLI=${CARGO_NET_GIT_FETCH_WITH_CLI:-}"
if [[ -n "${RUSTC_WRAPPER:-}" ]]; then
  echo "  RUSTC_WRAPPER=$RUSTC_WRAPPER"
else
  echo "  RUSTC_WRAPPER=(disabled; install sccache to enable compiler caching)"
fi
echo ""

if [[ -x scripts/ci-install-rust-native-prereqs.sh ]]; then
  bash scripts/ci-install-rust-native-prereqs.sh
fi

if [[ -f scripts/clips-source-helper.sh ]]; then
  # shellcheck source=/dev/null
  source scripts/clips-source-helper.sh
fi

echo "==> Warm Rust CE/native check cache"
cargo check --manifest-path packages/nxuskit-engine/Cargo.toml -p nxuskit-core --no-default-features
cargo check --manifest-path packages/nxuskit-engine/Cargo.toml -p nxuskit-cli --no-default-features
cargo check --manifest-path packages/nxuskit/Cargo.toml

if [[ "$FULL_FEATURES" -eq 1 ]]; then
  features="${NXUSKIT_LOCAL_WARM_FEATURES:-blocking-api,stream-token-estimation}"
  echo "==> Warm Rust optional-feature engine check cache ($features)"
  cargo check --manifest-path packages/nxuskit-engine/Cargo.toml -p nxuskit-engine --features "$features"
  if grep -q '^licensing-client =' packages/nxuskit-engine/crates/nxuskit-core/Cargo.toml; then
    echo "==> Warm Rust licensing-client check cache"
    cargo check --manifest-path packages/nxuskit-engine/Cargo.toml -p nxuskit-core --features licensing-client
    cargo check --manifest-path packages/nxuskit-engine/Cargo.toml -p nxuskit-cli --features licensing-client
  fi
fi

if [[ "$RELEASE" -eq 1 ]]; then
  echo "==> Warm Rust release build cache"
  cargo build --release --manifest-path packages/nxuskit-engine/Cargo.toml -p nxuskit-core --no-default-features
  cargo build --release --manifest-path packages/nxuskit-engine/Cargo.toml -p nxuskit-cli --no-default-features
fi

if [[ "$SKIP_GO" -eq 0 ]]; then
  echo "==> Warm Go module and build cache"
  (
    cd packages/nxuskit-go
    go mod download
    go test ./... -run '^$'
  )
fi

if [[ "$SKIP_PYTHON" -eq 0 ]]; then
  python_bin="${PYTHON:-}"
  if [[ -z "$python_bin" ]]; then
    python_bin="$(command -v python3.13 2>/dev/null || command -v python3.12 2>/dev/null || command -v python3.11 2>/dev/null || command -v python3 2>/dev/null || true)"
  fi
  if [[ -n "$python_bin" ]]; then
    echo "==> Warm Python bytecode cache"
    "$python_bin" -m compileall -q packages/nxuskit-py/src
  else
    echo "WARNING: Python 3.11+ not found; skipping Python warmup" >&2
  fi
fi

echo ""
echo "Local cache warmup complete."
