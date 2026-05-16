#!/usr/bin/env bash
# Configure persistent native build caches for self-hosted CI runners.

set -euo pipefail

sanitize_component() {
  local value="$1"
  value="${value:-unknown}"
  printf '%s' "$value" |
    tr '[:upper:]' '[:lower:]' |
    tr -cs 'a-z0-9._-' '_' |
    sed -e 's/^_*//' -e 's/_*$//'
}

repo_component() {
  local repo=""

  if [[ -n "${GITHUB_REPOSITORY:-}" ]]; then
    repo="$(basename "$GITHUB_REPOSITORY")"
  elif command -v git >/dev/null 2>&1; then
    repo="$(basename "$(git rev-parse --show-toplevel 2>/dev/null || pwd)")"
  else
    repo="$(basename "$PWD")"
  fi

  sanitize_component "$repo"
}

rust_release_host_component() {
  local release="unknown"
  local host="unknown"
  local rustv

  if command -v rustc >/dev/null 2>&1; then
    rustv="$(rustc -vV 2>/dev/null || true)"
    release="$(printf '%s\n' "$rustv" | awk '/^release:/ {print $2; exit}')"
    host="$(printf '%s\n' "$rustv" | awk '/^host:/ {print $2; exit}')"
  fi

  sanitize_component "rust-${release:-unknown}-${host:-unknown}"
}

if [[ -n "${NXUS_CI_CACHE_ROOT:-}" ]]; then
  CACHE_ROOT="$NXUS_CI_CACHE_ROOT"
elif [[ -n "${NXUSKIT_CI_CACHE_ROOT:-}" ]]; then
  CACHE_ROOT="$NXUSKIT_CI_CACHE_ROOT"
  echo "WARNING: NXUSKIT_CI_CACHE_ROOT is deprecated; use NXUS_CI_CACHE_ROOT." >&2
else
  CACHE_ROOT="$HOME/.cache/nxus-ci"
fi

REPO_COMPONENT="$(repo_component)"
OS_COMPONENT="$(sanitize_component "${RUNNER_OS:-$(uname -s)}")"
ARCH_COMPONENT="$(sanitize_component "${RUNNER_ARCH:-$(uname -m)}")"
RUST_COMPONENT="$(rust_release_host_component)"
BUCKET_COMPONENT="$(sanitize_component "${NXUS_CI_CACHE_BUCKET:-ci}")"

CARGO_TARGET_CACHE="$CACHE_ROOT/cargo-target/$REPO_COMPONENT/$OS_COMPONENT/$ARCH_COMPONENT/$RUST_COMPONENT/$BUCKET_COMPONENT"
SCCACHE_CACHE="$CACHE_ROOT/sccache"
SCCACHE_SIZE="${SCCACHE_CACHE_SIZE:-20G}"
CLIPS_CACHE="${CLIPS_LIB_CACHE_ROOT:-$CACHE_ROOT/clips}"

mkdir -p "$CACHE_ROOT" "$CARGO_TARGET_CACHE" "$SCCACHE_CACHE" "$CLIPS_CACHE"

append_env() {
  local name="$1"
  local value="$2"

  export "${name}=${value}"
  if [[ -n "${GITHUB_ENV:-}" ]]; then
    printf '%s=%s\n' "$name" "$value" >> "$GITHUB_ENV"
  fi
}

dir_has_libclang() {
  local dir="$1"
  [[ -n "$dir" && -d "$dir" ]] || return 1
  compgen -G "$dir/libclang.so" >/dev/null ||
    compgen -G "$dir/libclang.so.*" >/dev/null ||
    compgen -G "$dir/libclang-*.so" >/dev/null ||
    compgen -G "$dir/libclang-*.so.*" >/dev/null ||
    compgen -G "$dir/libclang.dylib" >/dev/null
}

find_libclang_dir() {
  local dir

  if [[ -n "${LIBCLANG_PATH:-}" ]] && dir_has_libclang "$LIBCLANG_PATH"; then
    printf '%s\n' "$LIBCLANG_PATH"
    return 0
  fi

  if command -v llvm-config >/dev/null 2>&1; then
    dir="$(llvm-config --libdir 2>/dev/null || true)"
    if dir_has_libclang "$dir"; then
      printf '%s\n' "$dir"
      return 0
    fi
  fi

  if command -v ldconfig >/dev/null 2>&1; then
    dir="$(ldconfig -p 2>/dev/null | awk '/libclang\.so/{print $NF; exit}' | xargs dirname 2>/dev/null || true)"
    if dir_has_libclang "$dir"; then
      printf '%s\n' "$dir"
      return 0
    fi
  fi

  for dir in \
    /usr/lib/llvm-*/lib \
    /usr/lib/x86_64-linux-gnu \
    /usr/lib64 \
    /usr/local/opt/llvm/lib \
    /opt/homebrew/opt/llvm/lib \
    /Library/Developer/CommandLineTools/usr/lib; do
    if dir_has_libclang "$dir"; then
      printf '%s\n' "$dir"
      return 0
    fi
  done

  return 1
}

append_env NXUS_CI_CACHE_ROOT "$CACHE_ROOT"
append_env CARGO_TARGET_DIR "$CARGO_TARGET_CACHE"
append_env SCCACHE_DIR "$SCCACHE_CACHE"
append_env SCCACHE_CACHE_SIZE "$SCCACHE_SIZE"
append_env CLIPS_LIB_CACHE_ROOT "$CLIPS_CACHE"

if command -v sccache >/dev/null 2>&1; then
  append_env RUSTC_WRAPPER sccache
  append_env CMAKE_C_COMPILER_LAUNCHER sccache
  append_env CMAKE_CXX_COMPILER_LAUNCHER sccache
  append_env CC "sccache cc"
  append_env CXX "sccache c++"
fi

libclang_dir="$(find_libclang_dir || true)"
if [[ -n "$libclang_dir" ]]; then
  append_env LIBCLANG_PATH "$libclang_dir"
fi

echo "Configured nxusKit CI cache root: $CACHE_ROOT"
echo "CARGO_TARGET_DIR=$CARGO_TARGET_CACHE"
echo "SCCACHE_DIR=$SCCACHE_CACHE"
echo "SCCACHE_CACHE_SIZE=$SCCACHE_SIZE"
echo "CLIPS_LIB_CACHE_ROOT=$CLIPS_CACHE"
if [[ -n "${RUSTC_WRAPPER:-}" ]]; then
  echo "RUSTC_WRAPPER=$RUSTC_WRAPPER"
  echo "CMAKE_C_COMPILER_LAUNCHER=$CMAKE_C_COMPILER_LAUNCHER"
  echo "CMAKE_CXX_COMPILER_LAUNCHER=$CMAKE_CXX_COMPILER_LAUNCHER"
  echo "CC=$CC"
  echo "CXX=$CXX"
fi
if [[ -n "${LIBCLANG_PATH:-}" ]]; then
  echo "LIBCLANG_PATH=$LIBCLANG_PATH"
fi
