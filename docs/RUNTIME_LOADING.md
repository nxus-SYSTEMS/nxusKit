# Runtime Library Loading — Cross-SDK Search Order

This document describes how each nxusKit SDK locates the native shared library
at runtime. The search order is consistent across SDKs where applicable.

## Search Order Priority Table

| Priority | Rust | Python | Go |
|----------|------|--------|----|
| 1 | `NXUSKIT_LIB_DIR` | `NXUSKIT_LIB_DIR` | `NXUSKIT_LIB_DIR` (rpath) |
| 2 | `NXUSKIT_SDK_DIR/lib` | `NXUSKIT_SDK_DIR/lib` | `NXUSKIT_SDK_DIR/lib` (rpath) |
| 3 | Platform defaults | `NXUSKIT_LIB_PATH` (**deprecated**) | System linker |
| 4 | — | `~/.nxuskit/sdk/current/lib/` | — |
| 5 | — | Bundled `libs/` (wheel) | — |
| 6 | — | System library path | — |

## Environment Variables

- **`NXUSKIT_LIB_DIR`** — Directory containing the shared library. Highest priority across all SDKs.
- **`NXUSKIT_SDK_DIR`** — SDK root directory. The library is expected at `$NXUSKIT_SDK_DIR/lib/`.
- **`NXUSKIT_LIB_PATH`** — (**Deprecated**) Exact path to the library file. Python only. Emits a `DeprecationWarning` when used.

## Migration from `NXUSKIT_LIB_PATH`

`NXUSKIT_LIB_PATH` is deprecated in v0.9.2 and will be removed in a future
release. To migrate:

```bash
# Before (deprecated):
export NXUSKIT_LIB_PATH=/path/to/libnxuskit.dylib

# After (recommended):
export NXUSKIT_LIB_DIR=/path/to

# Or use the SDK root:
export NXUSKIT_SDK_DIR=/path/to/sdk  # expects lib/ subdirectory
```

## SDK-Specific Notes

### Rust
Uses compile-time linking. The library path is resolved at build time via
`cargo` and `build.rs`. Environment variables affect the linker search path.

### Python
Uses runtime discovery via `_find_library()` in `nxuskit/_ffi.py`. The search
is fully dynamic and supports the most resolution strategies. Validated by
`scripts/validate-runtime-loading.sh`.

### Go
Uses `cgo` with rpath-based linking. The `NXUSKIT_LIB_DIR` and
`NXUSKIT_SDK_DIR` environment variables are embedded as rpath hints at build
time. Runtime resolution falls back to the system dynamic linker.

## Validation

Run the cross-language validation script:

```bash
scripts/validate-runtime-loading.sh
```

This runs the Python FFI discovery tests and displays the search-order table.
