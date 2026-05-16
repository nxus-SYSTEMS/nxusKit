# Migration Guide: ClipsEnvironment → ClipsSession

This guide covers migrating from the legacy `ClipsEnvironment` / `nxuskit_clips_env_*` API
to the new `ClipsSession` / `nxuskit_clips_session_*` API introduced in SDK v0.9.1.

## What Changed

| Aspect | Old API | New API |
|--------|---------|---------|
| Handle type | Opaque pointer (`NxuskitClipsEnv*`) | `uint64_t` session handle |
| Safety | Manual pointer management | Generational index (use-after-free protection) |
| Fact builder | Separate `NxuskitClipsFactBuilder` type | `nxuskit_clips_fact_assert_structured()` (JSON slots) |
| Fact iteration | Pointer chain (`first_fact` → `fact_next`) | `nxuskit_clips_facts_by_template()` (JSON array) |
| Template lookup | `nxuskit_clips_find_template()` → pointer | `nxuskit_clips_template_exists()` / `template_list()` |
| Thread safety | Not guaranteed | Session registry with RwLock; `session_halt()` for cross-thread signalling |

## Who Needs to Migrate

- **Chat provider users** (`NxuskitProvider` / `ChatRequest`): **No changes needed.** The Chat provider interface is unchanged.
- **Direct C ABI users** (`nxuskit_clips_env_*` functions): **Must migrate.** Old functions are removed.
- **Rust SDK users** (`nxuskit::ClipsEnvironment`): **Must migrate** to `nxuskit::ClipsSession`.

## C ABI Migration

### Session Lifecycle

```c
// OLD
struct NxuskitClipsEnv *env = nxuskit_clips_env_create();
nxuskit_clips_env_load_file(env, "rules.clp");
nxuskit_clips_env_reset(env);
nxuskit_clips_env_run(env, -1);
nxuskit_clips_env_destroy(env);

// NEW
uint64_t s = nxuskit_clips_session_create();
nxuskit_clips_session_load_file(s, "rules.clp");
nxuskit_clips_session_reset(s);
nxuskit_clips_session_run(s, -1);
nxuskit_clips_session_destroy(s);
```

### Asserting Facts

```c
// OLD — string-based
nxuskit_clips_env_assert_string(env, "(sensor (name \"t1\") (value 42))");

// OLD — fact builder
struct NxuskitClipsFactBuilder *fb = nxuskit_clips_fb_create(env, "sensor");
nxuskit_clips_fb_put_string(fb, "name", "t1");
nxuskit_clips_fb_put_integer(fb, "value", 42);
nxuskit_clips_fb_assert(fb);

// NEW — string-based (unchanged pattern)
nxuskit_clips_fact_assert_string(s, "(sensor (name \"t1\") (value 42))");

// NEW — structured (replaces fact builder)
nxuskit_clips_fact_assert_structured(s, "sensor",
    "{\"name\":\"t1\",\"value\":42}");
```

### Querying Facts

```c
// OLD — pointer iteration
struct NxuskitClipsTemplate *tmpl = nxuskit_clips_find_template(env, "sensor");
struct NxuskitClipsFact *fact = nxuskit_clips_template_first_fact(tmpl);
while (fact) {
    char *slot = nxuskit_clips_fact_get_slot(fact, "value");
    // use slot...
    nxuskit_free_string(slot);
    struct NxuskitClipsFact *next = nxuskit_clips_fact_next(fact);
    nxuskit_clips_fact_destroy(fact);
    fact = next;
}
nxuskit_clips_template_destroy(tmpl);

// NEW — JSON array
char *facts = nxuskit_clips_facts_by_template(s, "sensor");
// facts is a JSON array of fact objects — parse with your JSON library
nxuskit_free_string(facts);
```

## Rust SDK Migration

### Basic Usage

```rust
// OLD
use nxuskit::ClipsEnvironment;

let env = ClipsEnvironment::new()?;
env.load_from_string("(deftemplate sensor (slot name) (slot value))")?;
env.reset()?;
env.assert_string("(sensor (name \"t1\") (value 42))")?;
env.run(None)?;
if let Some(tmpl) = env.find_template("sensor")? {
    for fact in tmpl.facts().flatten() {
        println!("{:?}", fact.get_slot("value")?);
    }
}

// NEW
use nxuskit::ClipsSession;

let session = ClipsSession::create()?;
session.load_string("(deftemplate sensor (slot name) (slot value))")?;
session.reset()?;
session.assert_string("(sensor (name \"t1\") (value 42))")?;
session.run(-1)?;
let facts = session.facts_by_template("sensor")?;
println!("{}", facts);
// session is destroyed on drop
```

### FBP (Fact-Based Processing) Pattern

```rust
use nxuskit::ClipsSession;

// Create a persistent session for iterative processing
let session = ClipsSession::create()?;
session.load_file("rules/shared/000-core.clp")?;
session.load_file("rules/data-qc/bounds-check.clp")?;

// Cycle 1: Load initial data
session.reset()?;
session.assert_string(r#"(input-data (record-id 1) (value 150.0))"#)?;
session.run(-1)?;
let alerts = session.facts_by_template("alert")?;

// Cycle 2: New data, same rules (facts persist unless reset)
session.assert_string(r#"(input-data (record-id 2) (value 200.0))"#)?;
session.run(-1)?;
let more_alerts = session.facts_by_template("alert")?;
```

### LKS (Load-知識-Solve) Pattern

```rust
use nxuskit::ClipsSession;

// Load once, solve many times
let session = ClipsSession::create()?;
session.load_file("expert-system.clp")?;

for case in cases {
    session.reset()?;  // Clear facts, keep rules
    session.assert_string(&format!("(case-data (id {}) (symptoms {}))",
        case.id, case.symptoms))?;
    session.run(-1)?;
    let diagnosis = session.facts_by_template("diagnosis")?;
    println!("Case {}: {}", case.id, diagnosis);
}
```

## New Capabilities (No Old Equivalent)

These features are only available in the Session API:

| Feature | Function | Description |
|---------|----------|-------------|
| Session halt | `session_halt()` | Thread-safe inference cancellation |
| Session cache | `session_preload()` / `get_cached()` | Named session caching |
| Fact retraction | `fact_retract()` / `fact_retract_by_template()` | Targeted fact removal |
| Rule debugging | `rule_breakpoint_set()` / `rule_times_fired()` | Rule-level debugging |
| Global variables | `global_get_value()` / `global_set_value()` | Defglobal access |
| Watch/dribble | `watch()` / `dribble_on()` | Tracing and logging |
| Strategy control | `strategy_get()` / `strategy_set()` | Conflict resolution |
| Module focus | `focus_push()` / `focus_pop()` | Module execution control |
| Structured assert | `fact_assert_structured()` | JSON-based fact assertion |
| Binary save/load | `session_save_binary()` / `load_binary()` | Fast environment serialization |
