# nxusKit v2.0.0 — Upgrade, licensing diagnosis, and recovery

The initial supported Pro launch capability is Solver. This guide distinguishes
local runtime diagnostics from purchase, activation and recovery service
behavior. It does not promise that an untested portal journey has succeeded.

## Migrate from v1.0.5 to v2.0.0

Keep your v1.0.5 installation, native library, bindings, and license
configuration available for recovery. Install v2.0.0 in a separate directory
and select its CLI by absolute path. Use v2.0.0 headers, bindings, and native
library together. The v2.0.0 bundle for macOS requires Apple Silicon. On
an Intel Mac, keep your existing working setup or use a supported v2.0.0
platform.

- **Rust.** Source builds and Rust consumers require Rust 1.94 or later.
  `ChatRequest.seed` changed from `Option<u64>` to `Option<i64>`. Convert
  existing `u64` values with `i64::try_from` and handle out-of-range values.
  Prefer SDK constructors or defaults when building chat requests, responses,
  and stream chunks. Rust struct literals must initialize the new fields.
- **Go and Python.** Use the matching v2.0.0 binding from the bundle or public
  source. Update Go unkeyed literals for changed response and stream types to
  use named fields. In Python, install into a new virtual environment. The
  GitHub release does not publish a v2.0.0 Go module or Python package to an index.
- **Streaming.** Use the SDK completion helper where available. In structured
  output, check terminality independently of a recognized finish reason. An
  absent or unfamiliar provider reason can still end a stream. Handle errors
  separately from successful completion.
- **License.** Start with the credential-free [local first call](first-call.md).
  Pro Solver requires a valid v2 Pro entitlement. New Pro purchases remain
  paused. Review your existing entitlement before activating v2. After
  activation, `license status --json` reports local status. Confirm the token
  is valid and run the Solver check for the operation you need.

Keep v1.0.5 selected until the v2.0.0 install and the operations your project
uses pass. If you need to return to v1.0.5, select its original toolkit and
matching native library and bindings together.

## Inspect before changing state

```bash
nxuskit-cli license status --json
```

Inspect `auth`, `license` and `licensing`; exit 0 alone is not entitlement proof.
Keep the complete structured error from the intended Solver operation. CLI
entitlement denial and the native `feature_unavailable` error need not have the
same envelope or message. Do not parse English messages as stable API codes.

| Diagnosis | Meaning | Recovery boundary |
| --- | --- | --- |
| No token / license required | No usable token reached the verifier | Follow the owned entitlement's activation instructions |
| Invalid token | Token parsing/signature or claims validation failed | Replace only through the approved issuer; never edit/sign it locally |
| Expired token | Runtime token expiry was reached | Obtain a valid token through the approved service; maintenance dates are not token expiry |
| Edition insufficient | The binary does not contain the requested Pro implementation | Install the exact selected Pro package; a token cannot add missing code |
| Wrong subject/machine or unauthorized feature | Token scope does not authorize this operation | Resolve the entitlement/device scope with the service owner |
| Historical-v1 deployment version ceiling | A legacy runtime exceeds its token ceiling; strict deployment-v2 rejects ceiling claims | Use a legacy SDK version the token covers; do not treat a ceiling token as a v2 deployment token |
| Trial inactive/expired or issuance failed | Trial policy or service issuance did not grant usable access | Follow current service policy; no fixed-duration/resume promise is inferred here |

Token resolution distinguishes explicit API input, environment, configured/local
token files and embedded/sidecar deployment inputs. Do not assume one wrapper's
input order applies to every binding; consult the
[License Activation Guide](license-activation-guide.md). A cached login is not
a license token, and neither is an activation HTTP acknowledgment.

## Shipped activation command

```bash
nxuskit-cli license login
nxuskit-cli license activate --key "$NXUSKIT_PURCHASE_ID" --accept-eula --json
nxuskit-cli license status --json
```

These commands have network/state effects and require an owned entitlement and
the separately qualified service. `--accept-eula` belongs to activation, not
global CLI status. A same-machine retry or sync must follow service instructions;
do not invent retries, seat capacity or immediate recovery guarantees.

Strict deployment-v2 tokens carry `sdk_release_line`; line 2 covers the v2
major line. Developer-token expiry/machine claims and historical-v1 deployment
ceilings are separate from Annual/Perpetual maintenance/support terms. Offline verification
still enforces signature, expiry, subject and authorization. It does not prove
immediate online revocation or that a future renewal has issued a replacement.
New assisted deployment-token issuance and replacement are
deferred until after the v2.0.0 GA release, with no announced date.
Already-issued tokens remain subject to the same offline validation checks. See
[Tier Comparison](tier-comparison.md) for the initial offer and support scope.

## Migration and limits

Keep the exact old package/token evidence while moving to a selected v2 package.
Do not relabel a v1 or qualification archive as v2 final distribution. Check ABI
and SDK versions, native library discovery and the intended entitled operation.
Use the [historical CLIPS migration guide](migration-guide.md) only when
moving from the pre-v0.9.1 CLIPS environment API.

Community workflows remain available without a Pro token. No live purchase,
seat, renewal, recovery, key-rotation or positive Solver result is established
by this local diagnostic guide.
