# nxusKit v2.0.0 — Community and Pro

Community Edition is free, open source and useful without a Pro entitlement.
Released CE features remain available under their open-source license.
Use CE from the v2.0.0 SDK bundle or build it from the public source.

The initial supported Pro launch capability is **Solver**. Compiled capability
manifests may also expose ZEN, MCP, plugin loading or advanced CLIPS; their
presence is not a launch support promise or proof of authorization. Enterprise
catalog entries are planning inputs, not an offered v2.0.0 GA tier.

## Distribution and support scope

| Edition | Distribution | Initial scope |
| --- | --- | --- |
| Community | Public source or the public SDK bundle | Community workflows without an SDK license token |
| Pro | The same public SDK bundle with a valid paid entitlement | Entitled Solver workflows |

Pro targets are Linux x86_64, macOS ARM64 and Windows x86_64. macOS x86_64 is
unsupported starting with v2.0.0. Follow [Getting Started](getting-started.md)
to select the exact archive and verify its checksum. A package-index install
is not a substitute for the selected native bundle.

## Commercial terms and runtime validity

New Pro purchases are paused. Details about v2.0.0 Launch Party Pricing will
follow. The public download does not change any existing paid rights, and
older prices in the bundled guides are not a current offer.

Runtime validity and maintenance/support are separate. Strict deployment-v2
tokens carry `sdk_release_line`; line 2 covers the v2 major line. Developer-token
expiry and machine claims are separate. Legacy deployment version ceilings are
historical-v1 behavior, not accepted strict deployment-v2 claims. Token expiry,
signature, subject/machine and feature
authorization must still pass. A support-lapsed perpetual purchase is not by
itself proof that a token expired, nor permission to ignore an expired token.

Developer-seat capacity follows the owned entitlement and service policy.
There is no unlimited-instance/no-seat-fee promise here. New assisted
deployment-token issuance and replacement are
deferred until after the v2.0.0 GA release, with no announced date. This does
not change offline validation of already-issued tokens. Trial duration,
renewal, recovery and purchaser/seat behavior require
tested service contracts; do not infer them from static catalog limits.

## Checking authorization

Use the [License Activation Guide](license-activation-guide.md) for the shipped
login/activation/status commands. `license status --json` can exit 0 when no
token is installed. A Pro binary or a capability flag does not grant access:
verify the intended entitled Solver operation separately. Purchase/activation
walkthroughs remain dependent on the qualified Licensing and portal service.
