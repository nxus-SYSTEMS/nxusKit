# First Call with nxusKit 2.0.0 CE

This first call uses the local `loopback` fixture provider. It makes no live
LLM/provider request, requires no API key or license token, and proves only the
local CLI/request/response path. It is not a live-provider qualification or a
Pro activation proof.

## Select the exact local executable

For a CE source build, follow [Installation](getting-started.md) and set
`NXUSKIT_CLI` to the absolute path of that build's `nxuskit-cli`. On Windows,
the executable is `nxuskit-cli.exe`.

For a checksum-verified v2.0.0 SDK bundle, select its exact extracted
executable. This CE call needs no license, even in the Pro-capable bundle:

```bash
: "${NXUSKIT_SDK_DIR:?Set the absolute verified SDK bundle directory}"
export NXUSKIT_CLI="$NXUSKIT_SDK_DIR/bin/nxuskit-cli"
"$NXUSKIT_CLI" --version
```

Expected version output: `nxuskit-cli 2.0.0`. Do not substitute an ambient
`nxuskit-cli` from another installation. On Windows select
`bin/nxuskit-cli.exe` in the verified bundle.

## Run the credential-free local fixture

The example explicitly selects the fixture provider and `echo` model, so an
ambient provider default cannot redirect it to a live service. The local CE
call requires neither developer activation nor an EULA acceptance flag.
`--accept-eula` belongs to activation commands, not this call.

<!-- executable: ce-first-call -->
```bash
: "${NXUSKIT_CLI:?Set the absolute nxuskit-cli path from your selected source build or bundle}"
"$NXUSKIT_CLI" call --input - --format json <<'JSON'
{"prompt":"Hello from nxusKit CE","provider":"loopback","model":"echo"}
JSON
```

The command exits 0 and returns a JSON envelope. Its literal result fields are:

```json
{"content":"Hello from nxusKit CE","model":"echo","provider":"loopback"}
```

These fields occur under `response`, not at the envelope root. The envelope also
contains `trace_id`, timing/usage information and `request_metadata.command`
equal to `call`. Generated IDs/timestamps are not fixed expected values.

PowerShell users can run the equivalent stdin call:

```powershell
'{"prompt":"Hello from nxusKit CE","provider":"loopback","model":"echo"}' |
  & $env:NXUSKIT_CLI call --input - --format json
```

## Next steps and failure boundaries

- For a real cloud provider, configure that provider's credentials explicitly
  and consult [Authentication](auth-modes-by-provider.md). Such a call may be
  billable and is not part of this fixture.
- For all request fields and error envelopes, see the
  [CLI input reference](cli-input-reference.md).
- A missing executable means the selected source build/package path is wrong;
  do not fall back to a global installation. A nonzero exit or error envelope
  is not success, even if some output was printed.
- This CE call does not demonstrate Pro entitlement. `license status --json`
  is diagnostic, and a successful activation acknowledgment alone is not
  proof that a local token and intended Pro operation validate.

Language SDK examples are in [Getting Started](getting-started.md). Package
bindings must resolve their native library from the selected build/bundle,
not an unrelated global SDK installation.
