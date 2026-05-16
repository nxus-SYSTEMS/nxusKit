# Limit-Hitting Examples

Demonstrates Community-tier limit behavior with upgrade messaging.

## Examples

### Session Limit (`session_limit`)

Creates CLIPS sessions in a loop until the Community-tier concurrent session
limit is hit. Displays the error message showing the tier name, current limit,
and upgrade URL.

```bash
cargo run --bin session_limit
```

### Rule Limit (`rule_limit`)

Creates a single CLIPS session and loads rules until the per-session rule limit
is hit. Displays the error with limit details and upgrade path.

```bash
cargo run --bin rule_limit
```

## Expected Output

Both examples demonstrate that when Community-tier limits are reached, the SDK
provides clear error messages including:

- The current tier name (e.g., "community")
- The numeric limit that was reached
- An upgrade URL for higher limits

This behavior helps users understand their current tier and how to upgrade.
