# nxuskit Shared Libraries

Place the nxuskit shared library for your platform here:

- **Linux**: `libnxuskit.so`
- **macOS**: `libnxuskit.dylib`
- **Windows**: `nxuskit.dll`

## From SDK Release

```bash
# Download the SDK for your platform
gh release download --repo nxus-SYSTEMS/nxusKit \
  --pattern "nxuskit-sdk-*-linux-x86_64.tar.gz"

# Extract and copy the shared library
tar xzf nxuskit-sdk-*.tar.gz
cp nxuskit-sdk-*/lib/libnxuskit.so packages/nxuskit-py/src/nxuskit/libs/
```

## From Local Build

```bash
# Build nxuskit-core
cargo build --release -p nxuskit-core

# Copy the shared library (adjust path for your CARGO_TARGET_DIR)
cp target/release/libnxuskit_core.so packages/nxuskit-py/src/nxuskit/libs/libnxuskit.so
```

## Alternative: Environment Variable

Set `NXUSKIT_LIB_PATH` to the full path of the shared library:

```bash
export NXUSKIT_LIB_PATH=/path/to/libnxuskit.so
```
