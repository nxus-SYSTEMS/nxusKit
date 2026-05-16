# CLIPS Excluded Capabilities & Workarounds

Four CLIPS capabilities are intentionally excluded from the Session API.
This document explains why and provides working alternatives.

## 1. User-Defined Functions (UDFs)

**What it is:** CLIPS allows registering C functions callable from rule RHS actions
(`(my-custom-fn ?arg)`).

**Why excluded:** UDFs require raw C function pointers — exposing this through the
session-based FFI would bypass safety guarantees and create ABI fragility.

**Workaround:** Use `nxuskit_clips_eval()` to call built-in CLIPS functions, or
encode custom logic as rules:

```c
// Instead of a UDF that computes weighted score:
//   (my-weighted-score ?base ?multiplier)

// Use a rule that computes and asserts the result:
nxuskit_clips_session_load_string(s,
    "(defrule compute-weighted-score"
    "    (input (base ?b) (multiplier ?m))"
    "    =>"
    "    (assert (result (score (* ?b ?m)))))");
```

For complex custom functions, pre-compute values in your host language and assert
them as facts:

```c
// Compute in C, assert as fact
double score = my_complex_calculation(base, mult);
char buf[256];
snprintf(buf, sizeof(buf), "(result (score %f))", score);
nxuskit_clips_fact_assert_string(s, buf);
```

## 2. I/O Routers

**What it is:** CLIPS uses I/O routers to redirect `printout`, `read`, and other
I/O operations to custom handlers.

**Why excluded:** Router registration requires persistent C callbacks with
environment-specific state — incompatible with the session handle model.

**Workaround:** Use dribble logging to capture output:

```c
// Capture all CLIPS output to a file
nxuskit_clips_dribble_on(s, "/tmp/clips-output.log");
nxuskit_clips_session_run(s, -1);
nxuskit_clips_dribble_off(s);
// Read /tmp/clips-output.log for captured output
```

For structured output, use facts instead of `printout`:

```c
// Instead of: (printout t "Result: " ?value crlf)
// Use:        (assert (output (message (str-cat "Result: " ?value))))

// Then query output facts:
char *outputs = nxuskit_clips_facts_by_template(s, "output");
// Parse JSON array of output facts
nxuskit_free_string(outputs);
```

## 3. Periodic Functions

**What it is:** CLIPS supports registering functions that execute between rule
firings (e.g., for progress callbacks or heartbeat checks).

**Why excluded:** Requires raw function pointers called from within the CLIPS
engine loop — cannot be safely exposed through FFI session handles.

**Workaround:** Use batch `run()` with a step limit and poll between batches:

```c
// Instead of a periodic callback, run in controlled batches
int64_t total_fired = 0;
while (true) {
    int64_t fired = nxuskit_clips_session_run(s, 100);  // 100 rules per batch
    if (fired <= 0) break;
    total_fired += fired;

    // Your "periodic" logic here:
    printf("Progress: %lld rules fired\n", total_fired);

    // Check if we should halt
    if (should_stop()) {
        nxuskit_clips_session_halt(s);
        break;
    }
}
```

For thread-safe cancellation from another thread:

```c
// Thread 1: run inference
int64_t fired = nxuskit_clips_session_run(s, -1);

// Thread 2: signal halt after timeout
sleep(5);
nxuskit_clips_session_halt(s);  // Thread-safe
```

## 4. External Addresses

**What it is:** CLIPS external addresses allow storing opaque C pointers as slot
values, enabling rules to reference host-language objects.

**Why excluded:** External address values are raw `void*` pointers — they cannot
be safely serialized through JSON and would create dangling pointer risks across
session boundaries.

**Workaround:** Use string or integer keys as indirect references:

```c
// Instead of storing a pointer to a connection object:
//   (connection (handle <ExternalAddress-0x7f...>))

// Store an integer key and maintain a lookup table in your host code:
nxuskit_clips_session_load_string(s,
    "(deftemplate connection (slot handle (type INTEGER)) (slot status (type SYMBOL)))");

// In C: maintain a mapping
int handle_id = register_connection(conn);  // Your lookup table
char buf[128];
snprintf(buf, sizeof(buf), "(connection (handle %d) (status active))", handle_id);
nxuskit_clips_fact_assert_string(s, buf);

// When a rule fires referencing the handle, look up the real object:
// (defrule process-connection
//     (connection (handle ?h) (status active))
//     =>
//     (assert (connection-result (handle ?h) (processed TRUE))))
```

## Summary

| Capability | Status | Workaround Pattern |
|------------|--------|-------------------|
| User-Defined Functions | Excluded | Encode as rules or pre-compute in host language |
| I/O Routers | Excluded | Dribble logging or output facts |
| Periodic Functions | Excluded | Batch `run()` with step limit + polling |
| External Addresses | Excluded | Integer/string keys with host-side lookup table |

All four exclusions are by design — they involve raw C pointers or callbacks that
cannot be safely exposed through the session-handle FFI boundary. The workarounds
provide equivalent functionality using the session API's safe data exchange patterns.
