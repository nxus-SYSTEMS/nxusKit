# nxusKit v2.0.0 — Licensing diagnosis and recovery

The initial supported Pro launch capability is Solver. This guide distinguishes
local runtime diagnostics from purchase, activation and recovery service
behavior. It does not promise that an untested portal journey has succeeded.

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
Follow the [Migration Guide](migration-guide.md) for its explicitly versioned
API changes; historical migration sections are not new v2 API guarantees.

Community workflows remain available without a Pro token. No live purchase,
seat, renewal, recovery, key-rotation or positive Solver result is established
by this local diagnostic guide.
