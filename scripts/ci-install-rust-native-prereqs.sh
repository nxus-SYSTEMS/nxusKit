#!/usr/bin/env bash
# Ensure native tools required by Rust feature builds are available on CI.

set -euo pipefail

required_tools=(cmake)
apt_packages=(build-essential cmake pkg-config clang libclang-dev llvm-dev)

if [[ "$(uname -s)" == "Linux" ]]; then
  required_tools+=(pkg-config gcc g++ make clang llvm-config)
fi

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

missing=()
for tool in "${required_tools[@]}"; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    missing+=("$tool")
  fi
done

libclang_dir="$(find_libclang_dir || true)"
if [[ -z "$libclang_dir" ]]; then
  missing+=("libclang")
fi

if [[ ${#missing[@]} -gt 0 ]]; then
  if command -v apt-get >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
    sudo apt-get update
    sudo apt-get install -y "${apt_packages[@]}"

    missing=()
    for tool in "${required_tools[@]}"; do
      if ! command -v "$tool" >/dev/null 2>&1; then
        missing+=("$tool")
      fi
    done

    libclang_dir="$(find_libclang_dir || true)"
    if [[ -z "$libclang_dir" ]]; then
      missing+=("libclang")
    fi
  fi
fi

if [[ ${#missing[@]} -gt 0 ]]; then
  echo "Missing required Rust native build prerequisites: ${missing[*]}" >&2
  echo "Install runner packages: ${apt_packages[*]}" >&2
  echo "If libclang is installed in a non-standard location, set LIBCLANG_PATH to its lib directory." >&2
  exit 1
fi

if [[ -n "$libclang_dir" ]]; then
  export LIBCLANG_PATH="$libclang_dir"
  if [[ -n "${GITHUB_ENV:-}" ]]; then
    printf 'LIBCLANG_PATH=%s\n' "$libclang_dir" >> "$GITHUB_ENV"
  fi
  echo "LIBCLANG_PATH=$libclang_dir"
fi

echo "Rust native build prerequisites are available."
