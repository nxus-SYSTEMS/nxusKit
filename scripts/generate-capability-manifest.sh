#!/usr/bin/env bash
# Generate capability-manifest.json from a built libnxuskit binary.
#
# Usage:
#   ./scripts/generate-capability-manifest.sh <lib-dir> [--edition <edition>] [--output <path>]
#
# Arguments:
#   <lib-dir>        Directory containing libnxuskit.{dylib,so,dll}
#   --edition <ed>   Override edition field (default: read from binary)
#   --output <path>  Output file path (default: stdout)
#
# Requires: cc (C compiler), jq

set -euo pipefail

LIB_DIR=""
EDITION_OVERRIDE=""
OUTPUT=""
ARCH_FLAG=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --edition)   EDITION_OVERRIDE="$2"; shift 2 ;;
        --output|-o) OUTPUT="$2"; shift 2 ;;
        --arch)      ARCH_FLAG="-arch $2"; shift 2 ;;
        --help|-h)
            sed -n '2,/^$/p' "$0" | sed 's/^# \?//'
            exit 0
            ;;
        *)
            if [[ -z "$LIB_DIR" ]]; then
                LIB_DIR="$1"
            else
                echo "Error: unexpected argument: $1" >&2
                exit 1
            fi
            shift
            ;;
    esac
done

if [[ -z "$LIB_DIR" ]]; then
    echo "Error: <lib-dir> argument is required" >&2
    echo "Usage: $0 <lib-dir> [--edition <edition>] [--output <path>]" >&2
    exit 1
fi

if [[ ! -d "$LIB_DIR" ]]; then
    echo "Error: library directory not found: $LIB_DIR" >&2
    exit 1
fi

# Check dependencies
if ! command -v jq &>/dev/null; then
    echo "Error: jq is required but not installed" >&2
    exit 1
fi

TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

# Write probe program
cat > "$TMPDIR/caps_probe.c" << 'EOF'
#include <stdio.h>
#include <stdlib.h>

extern const char* nxuskit_capabilities(void);
extern void nxuskit_free_string(char* ptr);

int main(void) {
    char* caps = (char*)nxuskit_capabilities();
    if (caps) {
        printf("%s\n", caps);
        nxuskit_free_string(caps);
        return 0;
    }
    fprintf(stderr, "nxuskit_capabilities() returned NULL\n");
    return 1;
}
EOF

# Compile
cc $ARCH_FLAG -o "$TMPDIR/caps_probe" "$TMPDIR/caps_probe.c" \
    -L"$LIB_DIR" -lnxuskit \
    -Wl,-rpath,"$LIB_DIR" 2>/dev/null \
    || cc $ARCH_FLAG -o "$TMPDIR/caps_probe" "$TMPDIR/caps_probe.c" \
        -L"$LIB_DIR" -lnxuskit

# Run probe
CAPS_JSON=$(DYLD_LIBRARY_PATH="$LIB_DIR" LD_LIBRARY_PATH="$LIB_DIR" "$TMPDIR/caps_probe")

# Detect platform and architecture
case "$(uname -s)" in
    Darwin) PLATFORM="macos" ;;
    Linux)  PLATFORM="linux" ;;
    MINGW*|MSYS*|CYGWIN*) PLATFORM="windows" ;;
    *)      PLATFORM="unknown" ;;
esac

if [[ -n "$ARCH_FLAG" ]]; then
    # Use explicitly provided arch (e.g. cross-compiling x86_64 on arm64 runner)
    ARCH="${ARCH_FLAG#-arch }"
else
    case "$(uname -m)" in
        x86_64|amd64)  ARCH="x86_64" ;;
        aarch64|arm64) ARCH="aarch64" ;;
        *)             ARCH="unknown" ;;
    esac
fi

# Enrich with platform, arch, and optional edition override
if [[ -n "$EDITION_OVERRIDE" ]]; then
    RESULT=$(echo "$CAPS_JSON" | jq \
        --arg platform "$PLATFORM" \
        --arg arch "$ARCH" \
        --arg edition "$EDITION_OVERRIDE" \
        '. + {platform: $platform, arch: $arch, edition: $edition}')
else
    RESULT=$(echo "$CAPS_JSON" | jq \
        --arg platform "$PLATFORM" \
        --arg arch "$ARCH" \
        '. + {platform: $platform, arch: $arch}')
fi

# Output
if [[ -n "$OUTPUT" ]]; then
    echo "$RESULT" > "$OUTPUT"
    echo "Generated: $OUTPUT" >&2
else
    echo "$RESULT"
fi
