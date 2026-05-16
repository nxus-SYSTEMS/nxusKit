#!/usr/bin/env bash
# Configure the local nxusKit build-cache environment.
#
# Usage:
#   eval "$(scripts/local-cache-env.sh --print)"
#   scripts/local-cache-env.sh -- make build
#   scripts/local-cache-env.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

usage() {
  cat <<'USAGE'
Usage: scripts/local-cache-env.sh [--print] [--summary] [-- COMMAND...]

Options:
  --print     Print shell exports for eval in the current shell.
  --summary   Print the configured cache paths. This is the default.
  --help      Show this help.
  -- COMMAND  Run COMMAND with the local cache environment.

Examples:
  eval "$(scripts/local-cache-env.sh --print)"
  scripts/local-cache-env.sh -- make build
  scripts/local-cache-env.sh -- cargo check --manifest-path packages/nxuskit-engine/Cargo.toml -p nxuskit-cli
USAGE
}

mode="summary"
if [[ $# -gt 0 ]]; then
  case "$1" in
    --print)
      mode="print"
      shift
      ;;
    --summary)
      mode="summary"
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    --)
      mode="run"
      shift
      ;;
    *)
      mode="run"
      ;;
  esac
fi

if [[ "$mode" != "run" && $# -gt 0 ]]; then
  echo "Unexpected arguments: $*" >&2
  usage >&2
  exit 2
fi

if [[ "$mode" == "run" && $# -eq 0 ]]; then
  echo "No command supplied after --." >&2
  usage >&2
  exit 2
fi

export NXUS_CI_CACHE_ROOT="${NXUS_CI_CACHE_ROOT:-$HOME/.cache/nxus-ci}"
export NXUS_CI_CACHE_BUCKET="${NXUS_CI_CACHE_BUCKET:-local}"
export GOCACHE="${NXUSKIT_LOCAL_GOCACHE:-$NXUS_CI_CACHE_ROOT/go-build}"
export GOMODCACHE="${NXUSKIT_LOCAL_GOMODCACHE:-$NXUS_CI_CACHE_ROOT/go-mod}"
export NXUSKIT_LOCAL_CACHE_ENV_ACTIVE=1
export CARGO_NET_GIT_FETCH_WITH_CLI="${CARGO_NET_GIT_FETCH_WITH_CLI:-true}"

mkdir -p "$NXUS_CI_CACHE_ROOT" "$GOCACHE" "$GOMODCACHE"


env_file="$(mktemp "${TMPDIR:-/tmp}/nxuskit-local-cache-env.XXXXXX")"
log_file="$(mktemp "${TMPDIR:-/tmp}/nxuskit-local-cache-log.XXXXXX")"
cleanup() {
  rm -f "$env_file" "$log_file"
}
trap cleanup EXIT

GITHUB_ENV="$env_file" bash "$SCRIPT_DIR/ci-configure-rust-cache.sh" >"$log_file"

while IFS= read -r line; do
  [[ -z "$line" || "$line" != *=* ]] && continue
  name="${line%%=*}"
  value="${line#*=}"
  export "$name=$value"
done < "$env_file"

print_export() {
  local name="$1"
  local value
  eval "value=\${$name-}"
  if [[ -n "$value" ]]; then
    printf 'export %s=%q\n' "$name" "$value"
  fi
}

if [[ "$mode" == "print" ]]; then
  for name in \
    NXUS_CI_CACHE_ROOT \
    NXUS_CI_CACHE_BUCKET \
    NXUSKIT_LOCAL_CACHE_ENV_ACTIVE \
    CARGO_NET_GIT_FETCH_WITH_CLI \
    CARGO_TARGET_DIR \
    CLIPS_LIB_CACHE_ROOT \
    SCCACHE_DIR \
    SCCACHE_CACHE_SIZE \
    RUSTC_WRAPPER \
    CMAKE_C_COMPILER_LAUNCHER \
    CMAKE_CXX_COMPILER_LAUNCHER \
    CC \
    CXX \
    LIBCLANG_PATH \
    GOCACHE \
    GOMODCACHE; do
    print_export "$name"
  done
  exit 0
fi

if [[ "$mode" == "run" ]]; then
  cd "$REPO_ROOT"
  exec "$@"
fi

echo "Configured local nxusKit cache environment"
echo "  NXUS_CI_CACHE_ROOT=$NXUS_CI_CACHE_ROOT"
echo "  NXUS_CI_CACHE_BUCKET=$NXUS_CI_CACHE_BUCKET"
echo "  CARGO_TARGET_DIR=$CARGO_TARGET_DIR"
echo "  CLIPS_LIB_CACHE_ROOT=$CLIPS_LIB_CACHE_ROOT"
echo "  GOCACHE=$GOCACHE"
echo "  GOMODCACHE=$GOMODCACHE"
echo "  CARGO_NET_GIT_FETCH_WITH_CLI=$CARGO_NET_GIT_FETCH_WITH_CLI"
echo "  SCCACHE_DIR=$SCCACHE_DIR"
if [[ -n "${RUSTC_WRAPPER:-}" ]]; then
  echo "  RUSTC_WRAPPER=$RUSTC_WRAPPER"
else
  echo "  RUSTC_WRAPPER=(disabled; install sccache to enable compiler caching)"
fi
if [[ -n "${LIBCLANG_PATH:-}" ]]; then
  echo "  LIBCLANG_PATH=$LIBCLANG_PATH"
fi
echo ""
echo "To use this environment in your current shell:"
echo '  eval "$(scripts/local-cache-env.sh --print)"'
