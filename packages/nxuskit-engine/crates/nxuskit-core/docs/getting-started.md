# nxuskit SDK — Getting Started

## Downloading SDK Releases

The nxuskit SDK is distributed as pre-built shared libraries attached to
GitHub Releases in the private `nxus-SYSTEMS/nxusKit` repository. Access
requires authentication.

### Prerequisites

- [GitHub CLI](https://cli.github.com/) (`gh`) installed and authenticated
- Repository read access (granted via organization membership or fine-grained PAT)

### Download via GitHub CLI

```bash
# Authenticate (one-time)
gh auth login

# Download the latest SDK for your platform
gh release download --repo nxus-SYSTEMS/nxusKit \
  --pattern "nxuskit-sdk-*-linux-x86_64.tar.gz"   # Linux
  # --pattern "nxuskit-sdk-*-macos-arm64.tar.gz"   # macOS Apple Silicon
  # --pattern "nxuskit-sdk-*-windows-x86_64.zip"   # Windows

# Download a specific version
gh release download sdk-v0.7.0 --repo nxus-SYSTEMS/nxusKit \
  --pattern "nxuskit-sdk-*"

# Verify checksum
sha256sum -c nxuskit-sdk-*.sha256
```

### Download via Personal Access Token (PAT)

For CI systems or scripts that can't use `gh`:

1. Create a fine-grained PAT at https://github.com/settings/personal-access-tokens
   - **Repository access**: Select `nxus-SYSTEMS/nxusKit`
   - **Permissions**: Contents → Read-only
2. Use the token with the GitHub API:

```bash
# Set your token
export GH_TOKEN="github_pat_..."

# List available SDK releases
curl -H "Authorization: Bearer $GH_TOKEN" \
  "https://api.github.com/repos/nxus-SYSTEMS/nxusKit/releases?per_page=5" \
  | jq '.[].tag_name'

# Download a specific asset (get asset ID from release)
curl -L -H "Authorization: Bearer $GH_TOKEN" \
  -H "Accept: application/octet-stream" \
  "https://api.github.com/repos/nxus-SYSTEMS/nxusKit/releases/assets/{ASSET_ID}" \
  -o nxuskit-sdk.tar.gz
```

### External Partner Access

For external partners who need SDK access:

1. The repository admin adds the partner's GitHub account as an **outside
   collaborator** with **Read** access, or invites them to a team with
   repository read permissions.
2. The partner creates a fine-grained PAT scoped to `nxus-SYSTEMS/nxusKit`
   with Contents read-only permission.
3. The partner uses the PAT-based download method above.

## SDK Bundle Contents

```
nxuskit-sdk-{version}-{platform}/
├── include/
│   └── nxuskit.h          # C header — all API declarations
└── lib/
    ├── libnxuskit.so      # Shared library (Linux)
    │   libnxuskit.dylib   # Shared library (macOS)
    │   nxuskit.dll        # Shared library (Windows)
    ├── libnxuskit.a       # Static library (Linux/macOS)
    │   nxuskit.lib        # Static library (Windows)
    └── nxuskit.dll.lib    # Import library (Windows only)
```

## Linking

### GCC / Clang (Linux / macOS)

```bash
# Dynamic linking
cc -I sdk/include -o myapp myapp.c -L sdk/lib -lnxuskit -Wl,-rpath,sdk/lib

# Static linking
cc -I sdk/include -o myapp myapp.c sdk/lib/libnxuskit.a -lpthread -ldl -lm
```

### MSVC (Windows)

```
cl /I sdk\include myapp.c /link sdk\lib\nxuskit.lib
```

### CGo (Go)

```go
// #cgo CFLAGS: -I${SRCDIR}/sdk/include
// #cgo linux LDFLAGS: -L${SRCDIR}/sdk/lib -lnxuskit -Wl,-rpath,${SRCDIR}/sdk/lib
// #cgo darwin LDFLAGS: -L${SRCDIR}/sdk/lib -lnxuskit -Wl,-rpath,${SRCDIR}/sdk/lib
// #cgo windows LDFLAGS: -L${SRCDIR}/sdk/lib -lnxuskit
// #include "nxuskit.h"
import "C"
```

### Python (cffi)

```python
from cffi import FFI
ffi = FFI()
ffi.cdef(open("sdk/include/nxuskit.h").read())
lib = ffi.dlopen("sdk/lib/libnxuskit.so")  # or .dylib / .dll
```

## Quick Example (C)

```c
#include <stdio.h>
#include "nxuskit.h"

int main(void) {
    printf("nxuskit version: %s\n", nxuskit_version());

    // Create a provider
    NxuskitProvider *p = nxuskit_create_provider(
        "{\"provider_type\":\"openai\",\"api_key\":\"sk-...\",\"model\":\"gpt-4\"}");
    if (!p) {
        fprintf(stderr, "Error: %s\n", nxuskit_last_error());
        return 1;
    }

    // Chat
    NxuskitResponse *r = nxuskit_chat(p,
        "{\"model\":\"gpt-4\",\"messages\":[{\"role\":\"user\",\"content\":\"Hello!\"}]}");
    if (r) {
        printf("Response: %s\n", nxuskit_response_json(r));
        nxuskit_free_response(r);
    }

    nxuskit_free_provider(p);
    return 0;
}
```
