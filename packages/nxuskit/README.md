# nxuskit

`nxuskit` is the Rust wrapper for the nxusKit SDK. It exposes the public
native API through either dynamic or static linking.

Requires Rust 1.94 or newer; earlier compilers are no longer supported. Static
Rust linking also requires a compiler-compatible native SDK build. Historical
native archives retain their actual compiler provenance; changing this support
declaration does not rebuild them.

## Features

| Flag | Default | Description |
|---|---:|---|
| `dynamic-link` | yes | Load `libnxuskit` at runtime. |
| `static-link` | no | Link the SDK library at compile time. |

Public builds use the package-owned immutable ES256 public-key bundle for
offline token verification. Verification selects an exact key identifier and
fails closed for missing, unknown, malformed, or unsupported inputs. No
runtime key override is available in release builds.

## Native library discovery

Set `NXUSKIT_LIB_DIR` to a directory containing `libnxuskit`, or set
`NXUSKIT_SDK_DIR` to an SDK root whose `lib/` directory contains the library.
Use absolute paths when configuring either variable.

If neither variable is set, the platform library search path is used.

## Platform support

| Platform | Architecture | Library |
|---|---|---|
| Linux | x86_64 | `libnxuskit.so` |
| macOS | aarch64 | `libnxuskit.dylib` |
| Windows | x86_64 | `nxuskit.dll` |
