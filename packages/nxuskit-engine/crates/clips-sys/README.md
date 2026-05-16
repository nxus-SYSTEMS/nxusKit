# clips-sys

Full FFI bindings to the CLIPS expert system shell with safe Rust wrappers.

## What is CLIPS?

CLIPS (C Language Integrated Production System) is a rule-based expert system shell created by NASA in 1985. It provides:

- **Forward-chaining inference engine** - Pattern matching and rule execution
- **Fact-based knowledge representation** - Structured facts with templates
- **Procedural programming** - Functions and imperative constructs
- **Object-oriented programming** - COOL (CLIPS Object-Oriented Language)

A word about CLIPS: During its development at NASA from 1985 to 1996, the
primary CLIPS contributors were: Robert Savely, who conceived and championed
the project; Chris Culbert, who managed the project; Gary Riley and Brian
Dantes, who were the lead developers; and Frank Lopez, who developed the first
version. Since leaving NASA in 1996, Gary Riley has maintained CLIPS as public
domain software.

## Features

- **Full FFI bindings** - Complete bindings to CLIPS 6.4 C API
- **Safe Rust wrappers** - Thread-safe, idiomatic Rust interface
- **Static linking** - No runtime dependencies on CLIPS library
- **Comprehensive coverage** - Facts, rules, templates, modules, instances, and more

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
clips-sys = { path = "../clips-sys" }
```

### Building from Source

This crate requires the CLIPS source code:

1. Download CLIPS 6.4 from https://sourceforge.net/projects/clipsrules/files/CLIPS/6.4/
2. Extract the source files to `clips-source/` directory
3. Build with `cargo build`

CLIPS 6.4.2 is licensed under **MIT No Attribution (MIT-0)**. See: https://www.clipsrules.net/

## Quick Start

```rust
use clips_sys::{ClipsEnvironment, ClipsValue};

fn main() -> clips_sys::Result<()> {
    // Create environment
    let env = ClipsEnvironment::new()?;

    // Define a template
    env.build(r#"
        (deftemplate patient
            (slot name (type STRING))
            (slot age (type INTEGER))
            (multislot symptoms))
    "#)?;

    // Define a rule
    env.build(r#"
        (defrule elderly-care
            (patient (name ?n) (age ?a&:(>= ?a 65)))
            =>
            (assert (care-level (patient ?n) (level high))))
    "#)?;

    // Assert facts
    env.assert_string("(patient (name \"John\") (age 70) (symptoms fever cough))")?;

    // Run inference
    let result = env.run(None)?;
    println!("Rules fired: {}", result.rules_fired);

    // Query results
    for fact in env.facts() {
        let fact = fact?;
        println!("{}", fact.pp_form());
    }

    Ok(())
}
```

## API Overview

### Environment

```rust
let env = ClipsEnvironment::new()?;    // Create
env.load("rules.clp")?;                 // Load from file
env.build("(defrule ...)")?;            // Build construct
env.reset()?;                           // Reset (retract facts)
env.clear()?;                           // Clear all constructs
```

### Facts

```rust
// Assert from string
let fact = env.assert_string("(person (name \"Alice\"))")?;

// Use FactBuilder
let mut builder = env.fact_builder("person")?;
builder.put_string("name", "Bob")?;
let fact = builder.assert()?;

// Query facts
for fact in env.facts() {
    let fact = fact?;
    let values = fact.slot_values()?;
    println!("{:?}", values);
}
```

### Rules

```rust
// Find and inspect rules
if let Some(rule) = env.find_rule("my-rule")? {
    println!("Fired {} times", rule.times_fired());
    rule.set_breakpoint();
}

// Run with limit
let result = env.run(Some(100))?;  // Max 100 rule firings
```

### Template Introspection (v0.6.0+)

Query template slot constraints for schema generation:

```rust
if let Some(template) = env.find_deftemplate("patient")? {
    // Get slot names
    let slots = template.slot_names()?;

    for slot in &slots {
        // Get slot type constraints
        if let Some(types) = template.slot_types(slot)? {
            println!("{}: {:?}", slot, types);  // ["STRING", "SYMBOL"]
        }

        // Get default value
        if let Some(default) = template.slot_default_value(slot)? {
            println!("  default: {:?}", default);
        }

        // Get allowed values (for SYMBOL slots)
        if let Some(allowed) = template.slot_allowed_values(slot)? {
            println!("  allowed: {:?}", allowed);
        }

        // Get cardinality for multislots
        if let Some((min, max)) = template.slot_cardinality(slot)? {
            println!("  cardinality: {} to {}", min, max);
        }

        // Get range for numeric slots
        if let Some((min, max)) = template.slot_range(slot)? {
            println!("  range: {} to {}", min, max);
        }
    }
}
```

### Debugging

```rust
env.watch(WatchItem::Rules)?;       // Watch rule firings
env.watch(WatchItem::Facts)?;       // Watch fact changes
env.watch(WatchItem::Activations)?; // Watch agenda changes
```

## Thread Safety

`ClipsEnvironment` is `Send + Sync` and can be safely shared between threads. Internal access is synchronized via `parking_lot::Mutex`.

```rust
use std::sync::Arc;
use std::thread;

let env = Arc::new(ClipsEnvironment::new()?);

let env_clone = env.clone();
thread::spawn(move || {
    env_clone.assert_string("(fact from thread)")?;
    Ok::<(), clips_sys::ClipsError>(())
});
```

## CLIPS Resources

- [CLIPS Official Site](https://www.clipsrules.net/)
- [User's Guide (PDF)](https://www.clipsrules.net/documentation/v640/ug.pdf)
- [Reference Manual (PDF)](https://www.clipsrules.net/documentation/v640/bpg.pdf)
- [Advanced Programming Guide (PDF)](https://www.clipsrules.net/documentation/v640/apg.pdf)

## License

This crate is licensed under MIT OR Apache-2.0. CLIPS itself is licensed under MIT No Attribution (MIT-0).
See the vendored license at `internal/CLIPS/clips_core_source_642/readme.txt`.
