# nxusKit License Activation Guide

## Overview

nxusKit Pro features require a valid license token. This guide covers the
full lifecycle: authentication, activation, deployment, renewal, and
deactivation.

## Quick Start

```bash
# 1. Authenticate with your nxus.systems account
nxuskit-cli license login

# 2. Activate with your Purchase ID / Activation Key from My Products
nxuskit-cli license activate --key <purchase_id> --accept-eula

# 3. Accept the End User License Agreement (required once per machine)
nxuskit-cli --accept-eula <command>
# Or pre-accept for CI/CD (see EULA Acceptance below)

# 4. Verify activation
nxuskit-cli license status

# 5. Use Pro features — they just work
```

## EULA Acceptance

nxusKit requires acceptance of the End User License Agreement (EULA) before
use. Acceptance is recorded once per machine and persists across sessions.

### Interactive (TTY)

When running in an interactive terminal, the CLI prompts you to accept the EULA
on first use:

```
nxusKit End User License Agreement
...
Do you accept the EULA? [y/N]:
```

Type `y` and press Enter to accept. Acceptance is stored at
`~/.config/nxuskit/eula_accepted` (mode `0600`).

### Non-interactive (CI/CD)

In non-TTY environments (CI runners, Docker, scripts), the interactive prompt is
suppressed and the CLI exits with an error if the EULA has not been pre-accepted.

**Option 1 — `--accept-eula` flag** (recommended for CI):

```bash
nxuskit-cli --accept-eula license status
```

The flag is accepted on any subcommand. In GitHub Actions:

```yaml
- name: Verify license
  run: nxuskit-cli --accept-eula license status
  env:
    NXUSKIT_LICENSE_TOKEN: ${{ secrets.NXUSKIT_LICENSE_TOKEN }}
```

**Option 2 — Pre-write the acceptance file**:

Create the file before invoking the CLI (useful in container image builds):

```bash
mkdir -p ~/.config/nxuskit
echo "accepted" > ~/.config/nxuskit/eula_accepted
chmod 0600 ~/.config/nxuskit/eula_accepted
```

### Acceptance File

| Path | `~/.config/nxuskit/eula_accepted` |
|------|-----------------------------------|
| Permissions | `0600` (owner read/write only) |
| Content | `accepted` (literal string) |
| Created by | Interactive prompt or `--accept-eula` flag |

---

## Licensing Environments

Release builds default to the production licensing API:

```
https://nxus.systems/licensing-api/v1
```

The production ES256 public key is embedded in release builds. Standard users do not configure the key or endpoint.

Development and test lanes may opt into a non-production endpoint with
`NXUSKIT_LICENSE_SERVER` and may label that lane with
`NXUSKIT_LICENSE_ENVIRONMENT`. These overrides are visible in
`nxuskit-cli license status --json` under `licensing.endpoint`,
`licensing.environment`, and `licensing.signing_key`; they are not silent
fallbacks.

## Offline-First Entitlements

After activation, Pro-gated entitlement checks validate the cached token
locally: signature, expiry, machine binding, product id, edition, and
entitlement claims are checked without contacting the licensing API. The
licensing API is contacted for activation, explicit refresh/sync, or recovery
from a local validation failure.

---

## Token Types

The SDK manages five token shapes:

| Token | Storage | Purpose | Expiry | Machine-bound? |
|-------|---------|---------|--------|----------------|
| **Auth** | `~/.config/nxuskit/auth.json` | Authenticates you with the licensing service | 30 days | No |
| **Developer** | `~/.nxuskit/license.token` | Authorizes purchased Pro SDK features for local developer workstations after storefront checkout | Subscription period | Yes (up to 3 machines) |
| **Deployment** | `NXUSKIT_LICENSE_TOKEN` env var | Authorizes customer shipping/runtime use of products that embed or depend on nxusKit | Never expires | No (org-scoped) |
| **Real Purchase** | `~/.nxuskit/license.token` | Backward-compatible SDK fixture/claim shape; production storefront activations currently issue Developer tokens | Subscription period | Yes |
| **Leased** | `~/.nxuskit/license.token` or `NXUSKIT_LICENSE_TOKEN` | Internal CI/automation license that can be revoked server-side | Short lease, default 72 hours | Yes |

For CI automations that need a working Pro license but also need routine
revocation control, prefer a leased activation key over a long-lived deployment
token. Re-run `nxuskit-cli license activate --key
<leased_purchase_id> --accept-eula` on the runner before the 72-hour lease
expires. If the lease is blocked server-side, reactivation fails and any
already-issued token lapses at its normal `exp`.

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
nxuskit-cli license activate --key <purchase_id> --accept-eula --json
```

The `--accept-eula` flag is recorded in
`~/.config/nxuskit/eula_accepted` (mode `0600`); subsequent activations on
the same machine do not need to repeat it. The `--json` flag is recommended
for scripting — see the JSON output schema in §3.

On success you will see (text mode):

```
Activated. 1/3 machines used.
Token stored: ~/.nxuskit/license.token
```

The license token is stored locally and validated on each SDK
initialization. **Activation uses an extended 30s client timeout
(`EXTENDED_TIMEOUT_SECS = 30`)** to absorb production cold-starts; the
matching proxy timeout extension is in
`nxus_device_auth/services/proxy_client.py::EXTENDED_TIMEOUT_PATHS`.

#### Real-purchase activation (production)

After purchasing a license at `https://nxus.systems/shop`, your purchase
IDs appear at `https://nxus.systems/my/products`. Each card shows the
exact CLI to run:

```bash
nxuskit-cli license activate --key nxus_purchase_<id> --accept-eula --json
```

Production storefront purchases currently issue `developer` license tokens:
machine-bound, seat-indexed, and valid through the commercial subscription
period. The SDK also accepts the legacy/fixture `real_purchase` claim shape for
compatibility, but it is not the production storefront issuer contract.
Customer shipping/runtime use belongs to the separate `deployment` token flow.
`leased` tokens are designed for CI/automation where revocation control matters — re-activate before the default 72-hour expiry;
blocking the lease server-side prevents future activations and existing tokens
lapse at `exp`.

### 3. Verify

Check your license status at any time:

```bash
nxuskit-cli license status
```

Output includes token type, edition, expiry date, machine count, licensing
environment, endpoint, and signing-key metadata.

For JSON output (useful in scripts):

```bash
nxuskit-cli license status --json
```

### 4. Use Pro Features

Once activated, Pro features work transparently:

```python
# Python — ZEN decision tables (Pro)
from nxuskit import zen_evaluate
result = zen_evaluate(table_path, input_data)

# Python — Solver (Pro)
from nxuskit import SolverConfig
```

```rust
// Rust — ZEN evaluation (Pro)
let result = nxuskit::zen_evaluate(&table, &input)?;
```

```go
// Go — Solver (Pro)
session, err := nxuskit.NewSolverSession(config)
```

## Trial Activation

To start a 30-day Pro trial, first register for an account and authenticate:

```bash
nxuskit-cli license login
nxuskit-cli license activate --trial
```

The trial provides full Pro-tier access for 30 days.

## Deployment Tokens

Deployment tokens are designed for production, CI/CD, and containerized
environments. They have no expiry and no machine binding.

### Setup

Set the deployment token as an environment variable:

```bash
export NXUSKIT_LICENSE_TOKEN="<deployment_token>"
```

This works for:
- CI/CD pipelines (GitHub Actions, GitLab CI, Jenkins)
- Docker containers
- Kubernetes pods
- Production servers
- Serverless functions

### Version Ceiling

Deployment tokens include a **version ceiling** (e.g., `1.0`). The token
is valid for any SDK version at or below that ceiling:

- Token ceiling `1.0` → works with v1.0.0 and patch releases such as v1.0.5
- Token ceiling `1.0` → does NOT work with v1.1.0+

When you upgrade the SDK past the ceiling, you will see:

```
Deployment token covers up to v1.0.x. Update your deployment token for v1.1+ support.
```

Organizations with active support subscriptions receive updated deployment
tokens when new major.minor versions are released.

## Token Resolution Chain

The SDK resolves tokens from multiple sources in this precedence order:

| Priority | Source | Use Case |
|----------|--------|----------|
| 1 (highest) | `NXUSKIT_LICENSE_TOKEN` env var | CI/CD, containers |
| 2 | `~/.nxuskit/license.token` file | Local development |
| 3 (lowest) | API parameter | Embedded / programmatic |

The first valid token found is used. This order is the same for static and
dynamic linking modes.

## Multiple Machines

Each developer license supports up to 3 machine activations:

```bash
# Activate on machine 1
nxuskit-cli license activate --key <purchase_id>
# → Activated. 1/3 machines used.

# Activate on machine 2
nxuskit-cli license activate --key <purchase_id>
# → Activated. 2/3 machines used.

# If all 3 slots are used, deactivate one first:
nxuskit-cli deactivate
# → Deactivated. 2/3 machines used.
```

## Renewal

When your subscription approaches expiry (7 days out), the SDK logs a
once-per-session reminder:

```
Pro license expires in 7 days. Renew at your account dashboard.
```

After expiry, Pro features return:

```
License installation required.
```

Community features continue working without interruption.

## Deactivation

To free a machine activation slot:

```bash
nxuskit-cli deactivate
```

The local token file is removed and the activation count is decremented.

To revoke your auth session:

```bash
nxuskit-cli license logout
```

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `License installation required.` | No valid license token found | Run `nxuskit-cli license login`, then `nxuskit-cli license activate --key <id>` |
| `LicenseExpired` | Subscription lapsed | Renew at your account dashboard |
| `EditionInsufficient` | Community binary | Download Pro binary |
| `VersionCeilingExceeded` | SDK upgraded past token ceiling | Request updated deployment token |
| `FeatureUnavailable` | Multiple possible causes | Run `nxuskit-cli license status` for details |
| Auth token expired | 30-day auth session ended | Run `nxuskit-cli license login` again |
