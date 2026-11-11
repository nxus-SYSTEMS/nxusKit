# nxusKit License Activation Guide

## Overview

nxusKit Pro features require a valid license token. This guide covers the
full lifecycle: authentication, activation, deployment, renewal, and
deactivation.

## Quick Start

These are shipped command contracts, not a claim that a purchaser activation
journey has been qualified. Live purchase/portal activation and Pro operations
require their separate tested service and entitlement inputs.

```bash
# 1. Authenticate with your nxus.systems account
nxuskit-cli license login

# 2. Activate with your Purchase ID / Activation Key from My Products
nxuskit-cli license activate --key "$NXUSKIT_PURCHASE_ID" --accept-eula --json

# EULA acceptance is recorded by that activation command, not a global flag.

# 3. Inspect local status; exit 0 alone is not proof of Pro entitlement.
nxuskit-cli license status --json

# 4. Verify the intended entitled Pro operation separately.
```

## EULA Acceptance

The CLI's activation path enforces acceptance of the End User License
Agreement (EULA). Acceptance is recorded once per machine and persists across
sessions. This describes the runtime gate, not the full legal terms of use.
`--accept-eula` is an activation option, not a global option or an option on
`license status`/`call`.

### Interactive (TTY)

When activating in an interactive terminal without a recorded acceptance,
the CLI prompts you to accept the EULA:

```
By proceeding you accept the nxus.SYSTEMS EULA.
Full text: https://nxus.systems/legal/eula

Accept? (y/N):
```

Type `y` and press Enter to accept. Acceptance is stored as a JSON record at
`~/.config/nxuskit/eula_accepted` (mode `0600` on Unix).

### Non-interactive (CI/CD)

During non-TTY activation (CI runners, Docker, scripts), the interactive prompt
is suppressed. Missing acceptance without `--accept-eula` fails before
authentication/activation; a diagnostic status call does not perform activation.

**Option 1 — `--accept-eula` flag** (recommended for CI):

```bash
nxuskit-cli license activate --key "$NXUSKIT_PURCHASE_ID" --accept-eula --json
```

The flag belongs to `license activate` (and the legacy top-level `activate`
alias). It is not accepted on every subcommand. Read the EULA before supplying
it. A diagnostic status check is separate and needs no acceptance flag:

```bash
nxuskit-cli license status --json
```

Status exits 0 even with no/invalid license. Inspect its `auth`, `license` and
`licensing` fields and verify the intended gated operation; neither status
exit 0 nor an activation HTTP acknowledgment alone proves valid entitlement.

Do not pre-write the literal string `accepted`: it is not a valid acceptance
record and the CLI rejects it. Let the activation gate record acceptance.

### Acceptance File

| Path | `~/.config/nxuskit/eula_accepted` |
|------|-----------------------------------|
| Permissions | `0600` on Unix; platform permission behavior differs on Windows |
| Content | JSON with nonempty `eula_version`, `accepted_at`, and `method` (`flag` or `interactive`) |
| Created by | Interactive prompt or `--accept-eula` flag |

---

## Licensing Environments

Release builds default to the production licensing API:

```
https://nxus.systems/licensing-api/v1
```

The release verifier embeds an immutable ES256 public-key bundle. A named
`kid` selects its exact key; v2 deployment tokens require a nonempty `kid`.
Standard users do not supply an ambient public-key file or signing key.

Development and test lanes may opt into a non-production endpoint with
`NXUSKIT_LICENSE_SERVER` and may label that lane with
`NXUSKIT_LICENSE_ENVIRONMENT`. These overrides are visible in
`nxuskit-cli license status --json` under `licensing.endpoint`,
`licensing.environment`, and `licensing.signing_keys`; they are not silent
fallbacks.

## Offline-First Entitlements

After activation, Pro-gated entitlement checks validate the cached token
locally: signature, expiry, machine binding, product id, edition, and
entitlement claims are checked without contacting the licensing API. The
licensing API is contacted for authentication, activation, deactivation and
explicit refresh/sync/recovery. A local validation failure does not itself
authorize or automatically trigger a network refresh.

---

## Token Types

The SDK distinguishes an authentication session from runtime license families.
Expiry and machine/seat terms are issued by the service; the SDK enforces the
validated claims rather than guaranteeing a commercial duration or seat count.

| Token | Storage | Purpose | Expiry | Machine-bound? |
|-------|---------|---------|--------|----------------|
| **Auth session** | `~/.config/nxuskit/auth.json` | Authenticates you with the licensing service; not a runtime license | Session expiry supplied by service | No runtime machine claim |
| **Developer** | `~/.nxuskit/license.token` | Local developer runtime license | Token `exp` | Machine claim checked when present; seats service-defined |
| **Deployment** | `NXUSKIT_LICENSE_TOKEN` env var | Authorizes customer shipping/runtime use of products that embed or depend on nxusKit | Never expires | No (org-scoped) |
| **Trial** | `~/.nxuskit/license.token` | Trial runtime license; issuance and activation are distinct | Token `exp`; unactivated trials suspend after the grace period | Machine claim checked when present |
| **Real Purchase** | `~/.nxuskit/license.token` | Legacy/fixture-compatible claim family | Token `exp` | Machine claim checked when present |
| **Leased** | `~/.nxuskit/license.token` or `NXUSKIT_LICENSE_TOKEN` | Internal automation license | Token `exp`, not a guaranteed 72-hour duration | Machine claim checked when present |

For CI automations that need a working Pro license but also need routine
revocation control, prefer a leased activation key over a long-lived deployment
token. Re-run `nxuskit-cli license activate --key
"$NXUSKIT_PURCHASE_ID" --accept-eula` before the issued token expires.
Server-side issuance/revocation policy requires Licensing confirmation;
offline runtime validity remains bounded by the validated token's `exp`.

## Step-by-Step Activation

### 1. Authenticate

Before activating a license, you must authenticate with your nxus.systems
account:

```bash
nxuskit-cli license login
```

This opens your browser to the nxus.systems login page. After logging in,
enter the code shown in your terminal. The auth token is stored at
`~/.config/nxuskit/auth.json` and used automatically for subsequent commands.

Check auth status:

```bash
nxuskit-cli license status
```

### 2. Activate

With authentication complete, activate your license. First-time activations
must accept the EULA:

```bash
nxuskit-cli license activate --key "$NXUSKIT_PURCHASE_ID" --accept-eula --json
```

The `--accept-eula` flag is recorded in
`~/.config/nxuskit/eula_accepted` (mode `0600`); subsequent activations on
the same machine do not need to repeat it. The `--json` flag is recommended
for scripting — see the JSON output schema in §3.

On success you will see (text mode):

```
Activated. <seats_used>/<seats_total> machines used.
License stored at: ~/.nxuskit/license.token
```

The returned token is stored locally; status and Pro-gated operations perform
local validation. **Activation uses an extended 30s client timeout
(`EXTENDED_TIMEOUT_SECS = 30`)** to absorb production cold-starts; the
service/proxy timeout and real purchaser availability require separate
Licensing evidence, not this SDK source inspection.

#### Activation for an eligible existing purchase

New Pro purchases are paused during the SDK rollout. CE needs neither login
nor activation. If My Products provides a compatible activation key for an
eligible existing purchase, use it with the downloaded SDK:

```bash
nxuskit-cli license activate --key "$NXUSKIT_PURCHASE_ID" --accept-eula --json
```

The storefront's current issued family, seat limits and subscription terms
must be verified with Licensing/Website. SDK support for `developer` and the
legacy/fixture `real_purchase` family does not itself prove issuer behavior.
Customer shipping/runtime use belongs to the separate `deployment` token flow.
`leased` tokens are designed for CI/automation where revocation control matters. Re-activate before the issued `exp`; an offline
validator cannot observe a later server-side revocation by itself.

### 3. Verify

Check your license status at any time:

```bash
nxuskit-cli license status
```

Output includes local token status, effective edition and available token
type/source/days remaining, plus licensing environment, endpoint and signing-key
metadata. It is not a service seat-count or subscription-expiry lookup.

For JSON output (useful in scripts):

```bash
nxuskit-cli license status --json
```

Status is diagnostic: its top-level JSON objects are `auth`, `license`, and
`licensing`, not a provider-call `response`/`result` envelope. It can exit 0
with no license or an invalid token. Successful activation JSON acknowledges
the HTTP operation and includes `success`, `seats_used`, `seats_total`, and
`token_path`; local signature/claims and the intended Pro operation still need
verification. Auth/transport activation failures use `success:false`/`error`
and exit 1; EULA validation has its separate validation envelope/exit 5.
Do not log account identifiers, token paths or raw token bytes in shared output.
The explicit refresh command is `license sync --json`, not `license refresh`.

### 4. Verify the initial Pro Solver capability

With a valid compatible token and a binary containing the intended Pro feature,
verify Solver separately. The initial supported Pro launch capability is Solver;
other compiled domains are not initial launch support promises. This command
requires a real compatible entitlement and your problem file; it is not a
credential-free status probe or a completed purchaser walkthrough:

```bash
nxuskit-cli solver solve --input "$NXUSKIT_SOLVER_PROBLEM" --format json
```

## Trial Activation

To request a Pro trial, first register for an account and authenticate:

```bash
nxuskit-cli license login
nxuskit-cli license activate --trial --accept-eula --json
```

This requests trial issuance and stores the returned token; inspect `success`,
`days_remaining` and `message`. Trial activation is a separate activation-code
flow: `nxuskit-cli activate --trial --code "$NXUSKIT_TRIAL_CODE" --accept-eula --json`.
Unactivated trials suspend after seven days; activated trials remain subject
to `exp`, machine validation and compiled feature availability. Service trial
duration/access policy is not established by this guide.

## Deployment Tokens

Deployment tokens are designed for production, CI/CD, and containerized
environments. They have no expiry and no machine binding.

New assisted deployment-token issuance and replacement are
deferred until after the v2.0.0 GA release, with no announced date.
Already-issued tokens continue to be validated offline under the existing
signature, release-line, and entitlement checks. Developer activation is unchanged.

### Setup

If you already hold a compatible deployment token, set it as an environment
variable:

```bash
export NXUSKIT_LICENSE_TOKEN="<deployment_token>"
```

This works for:
- CI/CD pipelines (GitHub Actions, GitLab CI, Jenkins)
- Docker containers
- Kubernetes pods
- Production servers
- Serverless functions

### SDK Release Line

Strict v2 deployment tokens require `sdk_release_line` (at least 2). A line-2
token covers well-formed `2.minor.patch` SDK versions, not `3.0.0` or malformed
versions. Legacy `sdk_version_ceiling` deployment claims are rejected, not a
fallback. A token that does not cover the selected release line does not grant
access to that release.

## Token Resolution Chain

The SDK resolves tokens from multiple sources in this precedence order:

| Priority | Source | Use Case |
|----------|--------|----------|
| 1 (highest) | `NXUSKIT_LICENSE_TOKEN` env var | CI/CD, containers |
| 2 | Build-time embedded `NXUSKIT_DEPLOYMENT_TOKEN` | Redistributable applications |
| 3 | `nxuskit-license.token` adjacent to the executable | Sidecar runtime input |
| 4 | `~/.nxuskit/license.token` file | Local development |
| 5 (lowest) | API parameter | Explicit / programmatic input |

The first nonempty resolved input is validated. An invalid higher-priority
token blocks resolution; it does not silently fall through to a lower source.
Remove/correct the unintended override explicitly. Static/dynamic linking
does not change this source order.

## Multiple Machines

The service returns `seats_used`/`seats_total`; do not assume three seats for
every purchase. Each machine must satisfy the issued token's machine binding:

```bash
# Activate on machine 1
nxuskit-cli license activate --key "$NXUSKIT_PURCHASE_ID" --accept-eula --json

# Activate on machine 2
nxuskit-cli license activate --key "$NXUSKIT_PURCHASE_ID" --accept-eula --json

# If the service says no seats remain, deactivate an eligible machine first:
nxuskit-cli deactivate
```

## Renewal

For an otherwise valid expiring token with 0 through 7 whole days remaining,
the SDK emits at most one process-local warning. Logging configuration may
affect visibility; this is not a guaranteed notification service:

```
Pro license expires in 7 days. Renew: nxus.systems/account
```

After expiry, full token validation fails and the guarded operation's
command/API-specific error handling applies. Status can still exit 0 with
`invalid`; a universal literal error message is not promised.

Community features continue working without interruption.

## Deactivation

To free a machine activation slot:

```bash
nxuskit-cli deactivate
```

On successful service acknowledgment, the workstation token file is removed
and returned seat counts are reported. This contacts `/deactivate`; merely
deleting a local file does not prove a seat was released. Environment,
embedded and sidecar tokens are separate sources and are not removed.

To remove the locally cached auth session (not server-side revocation):

```bash
nxuskit-cli license logout
```

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `License installation required.` | No valid license token found | Run `nxuskit-cli license login`, then `nxuskit-cli license activate --key "$NXUSKIT_PURCHASE_ID" --accept-eula --json` |
| Expired token / invalid status | Issued token expiry reached | Confirm renewal/refresh eligibility with Licensing; inspect the guarded operation's error |
| `EditionInsufficient` | Binary lacks the required edition capability | Select the matching v2.0.0 SDK bundle; Pro Solver also requires a valid entitlement |
| `IncompatibleReleaseLine` | SDK major line differs from v2 deployment claim | Use an SDK release the existing token covers; new assisted issuance and replacement are deferred until after the v2.0.0 GA release |
| `FeatureUnavailable` | Multiple possible causes | Run `nxuskit-cli license status` for details |
| Auth token expired | Service-issued session expiry reached | Run `nxuskit-cli license login` again |
