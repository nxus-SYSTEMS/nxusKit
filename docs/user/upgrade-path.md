# nxusKit Upgrade Path — Error Messages & Resolution

## Overview

When a Pro feature is called without valid authorization, the SDK returns
a specific error type with an actionable message. This document maps every
error to its cause and resolution.

## Error Reference

### `FeatureUnavailable`

**Message**: varies by context (see sub-cases below)

This is the umbrella error returned when a Pro feature cannot be accessed.
The message text tells you exactly what to do.

---

### `LicenseRequired`

**Returned when**: A Pro feature is called but no valid license token was
found in the resolution chain (env var → file → API parameter).

**Message**:
```
License installation required.
```

**Resolution**:
1. Authenticate: `nxuskit-cli license login`
2. Activate your license: `nxuskit-cli license activate --key <purchase_id>`
3. For CI/CD: set `NXUSKIT_LICENSE_TOKEN` environment variable with your
   deployment token
4. To start a trial: `nxuskit-cli license login` then `nxuskit-cli license activate --trial`

---

### `LicenseExpired`

**Returned when**: The developer token's subscription period has ended.

**Message**:
```
License installation required.
```

**Resolution**:
1. Renew your subscription at your account dashboard
2. After renewal, run `nxuskit-cli license activate --key <purchase_id>` to get a
   fresh token
3. Community features continue working during the gap

---

### `EditionInsufficient`

**Returned when**: You have a valid token but the binary is the Community
edition, which does not include Pro code.

**Message**:
```
This feature requires Pro edition.
```

**Resolution**:
1. Download the Pro edition binary (requires authenticated access)
2. Replace your Community binary with the Pro binary
3. Your existing license token will be recognized automatically

---

### `VersionCeilingExceeded`

**Returned when**: A deployment token's version ceiling is lower than the
running SDK version.

**Message**:
```
Deployment token covers up to v{ceiling}. Update your deployment token for v{current}+ support.
```

**Resolution**:
1. If you have an active support subscription, request an updated
   deployment token from your account dashboard
2. Alternatively, pin the SDK version to stay within the token ceiling
3. Contact **support@nxus.systems** if you need help

---

### `TrialSuspended`

**Returned when**: A trial token was issued but not activated within
the 7-day grace period.

**Message**:
```
License installation required.
```

**Resolution**:
1. Run `nxuskit-cli license login` to authenticate
2. Run `nxuskit-cli license activate --trial` to resume the trial
3. This extends Pro access for the remainder of the 30-day trial period

---

### `TrialExpired`

**Returned when**: The 30-day trial period has ended.

**Message**:
```
License installation required.
```

**Resolution**:
1. Purchase a Pro license
2. Community features continue working without interruption
3. All Pro features will be restored immediately after activation

---

### `TrialIssuanceFailed`

**Returned when**: The SDK attempted to issue a trial token but could
not complete the operation.

**Message**:
```
License installation required.
```

**Resolution**:
1. Run `nxuskit-cli license login` to authenticate first
2. Then run `nxuskit-cli license activate --trial`
3. Community features remain available regardless

---

## Error Handling by Language

### Rust

```rust
use nxuskit::{FeatureUnavailableError, LicenseExpiredError, LicenseRequiredError};

match nxuskit::zen_evaluate(&table, &input) {
    Ok(result) => { /* success */ }
    Err(e) if e.is::<LicenseRequiredError>() => {
        eprintln!("{}", e);  // "License installation required."
    }
    Err(e) if e.is::<LicenseExpiredError>() => {
        eprintln!("{}", e);  // "License installation required."
    }
    Err(e) => { /* other errors */ }
}
```

### Python

```python
from nxuskit import zen_evaluate
from nxuskit import LicenseRequiredError, LicenseExpiredError, FeatureUnavailableError

try:
    result = zen_evaluate(table_path, input_data)
except LicenseRequiredError as e:
    print(e.message)  # "License installation required."
except LicenseExpiredError as e:
    print(e.message)  # "License installation required."
except FeatureUnavailableError as e:
    print(e.message)  # generic feature gate
```

### Go

```go
import "github.com/nxus-SYSTEMS/nxuskit-go"

result, err := nxuskit.ZenEvaluate(table, input)
if err != nil {
    switch {
    case errors.Is(err, nxuskit.ErrLicenseRequired):
        fmt.Println(err)  // "License installation required."
    case errors.Is(err, nxuskit.ErrLicenseExpired):
        fmt.Println(err)  // "License installation required."
    default:
        fmt.Println(err)
    }
}
```

## Quick Decision Tree

```
Pro feature called
  │
  ├─ No token found?          → "License installation required."
  ├─ Token expired?           → "License installation required."
  ├─ Community binary?        → EditionInsufficient → download Pro binary
  ├─ Version ceiling hit?     → VersionCeilingExceeded → update deployment token
  ├─ Trial not activated?     → "License installation required."
  ├─ Trial past 30 days?      → "License installation required."
  ├─ Can't reach service?     → "License installation required."
  └─ Valid token + Pro binary? → Success
```
