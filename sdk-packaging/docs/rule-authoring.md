# Rule Authoring Guide

This guide explains how to write, test, and deploy custom CLIPS rules with the nxusKit SDK.

## CLIPS Rule Syntax

nxusKit uses CLIPS 6.4, a forward-chaining inference engine. Rules follow the pattern:

```clips
(defrule rule-name
    "Documentation string"
    ;; LHS: conditions (pattern matching)
    (template-name (slot-name ?variable))
    (test (< ?variable 100))
    =>
    ;; RHS: actions (assertions, side effects)
    (assert (alert
        (alert-type "threshold_exceeded")
        (severity "warning")
        (message (str-cat "Value " ?variable " is out of range"))
        (recommendation "Check your input data")
        (entity-id ?id)
        (rule-name "rule-name")
        (module-name "data-qc"))))
```

## Defining Templates

Templates define the fact schemas your rules operate on. Define them in shared template files loaded before any module rules:

```clips
;;; shared/000-core.clp — Core templates

(deftemplate input-data
    "A single data record to evaluate"
    (slot record-id (type INTEGER))
    (slot value (type FLOAT))
    (slot category (type STRING))
    (slot confidence (type FLOAT) (range 0.0 1.0)))

(deftemplate threshold-config
    "Configurable thresholds for QC checks"
    (slot value-min (type FLOAT))
    (slot value-max (type FLOAT))
    (slot confidence-min (type FLOAT) (default 0.5)))

(deftemplate alert
    "Output: a raised alert from rule inference"
    (slot alert-type (type STRING))
    (slot severity (type STRING))           ;; "critical", "high", "warning", "info"
    (slot message (type STRING))
    (slot recommendation (type STRING))
    (slot entity-id (type INTEGER))
    (slot rule-name (type STRING))
    (slot module-name (type STRING)))
```

## Modules

CLIPS modules provide namespace isolation for rules. Each module groups related rules:

```clips
;;; In data-qc/bounds-check.clp
(defmodule data-qc (export ?ALL))

(defrule bounds-check
    "Flag records outside configured bounds"
    (threshold-config (value-min ?vmin) (value-max ?vmax))
    (input-data (record-id ?rid) (value ?v&:(or (< ?v ?vmin) (> ?v ?vmax))))
    =>
    (assert (alert
        (alert-type "out_of_bounds")
        (severity "high")
        (message (str-cat "Record " ?rid " value " ?v " outside [" ?vmin "-" ?vmax "]"))
        (recommendation "Verify input data or adjust thresholds")
        (entity-id ?rid)
        (rule-name "bounds-check")
        (module-name "data-qc"))))
```

## Directory Structure

Organize rules with shared templates loaded first, then per-module rule files:

```
rules/
  shared/                          # Shared templates (loaded first, alphabetically)
    000-core.clp                   #   input-data, threshold-config, alert
    010-domain.clp                 #   Additional domain-specific templates
  data-qc/                         # Data quality checks
    bounds-check.clp
    confidence-check.clp
  classification/                  # Classification rules
    category-classifier.clp
  custom/                          # User-defined rules
    my-custom-check.clp
```

## CLIPS Integration Paths

nxusKit provides two ways to use CLIPS:

- **Provider chat** — CLIPS as a standard chat provider. Send `ClipsInput` JSON
  as the user message; receive `ClipsOutput` JSON in the response content.
  Best for request/response workflows and cross-language portability.
- **Session API** — Direct engine access via `ClipsSession` (Rust, Go, Python)
  or the C ABI (`nxuskit_clips_session_*`). Best for interactive, multi-step
  rule authoring, debugging, and fine-grained fact manipulation.

This guide focuses on the **provider chat** path. For the session API, see the
[API Reference](api-reference.md#clips-session-api).

## ClipsInput JSON Reference

The user message JSON must conform to the `ClipsInput` schema. Unknown fields
are rejected (the engine uses strict deserialization).

```json
{
  "facts": [
    {"template": "sensor", "values": {"name": "temp-1", "value": 150}}
  ],
  "templates": [
    {"name": "alert", "slots": [{"name": "type", "type": "STRING"}, {"name": "severity", "type": "STRING"}]}
  ],
  "rules": [
    {"name": "high-temp", "source": "(defrule high-temp (sensor (value ?v&:(> ?v 100))) => (assert (alert (type \"over-threshold\") (severity \"high\"))))"}
  ],
  "config": {
    "max_rules": 1000,
    "include_trace": true,
    "derived_only_new": true
  },
  "focus": ["data-qc"],
  "globals": {"*threshold*": 100}
}
```

All fields are optional. The minimal valid input is `{}` (empty object).

| Field | Type | Description |
|-------|------|-------------|
| `facts` | array of `{template, values}` | Facts to assert before running inference |
| `templates` | array of `{name, slots}` | Templates to create (if not in rule base) |
| `rules` | array of `{name, source}` or `{name, conditions, actions}` | Rules to create programmatically |
| `config` | object | Request-level overrides (see below) |
| `focus` | array of strings | Module focus stack (controls which rules fire) |
| `globals` | object | Global variable values to set |
| `command` | string | Special command: `"reset"`, `"clear"`, `"retract"` |
| `modules` | array of `{name, doc, imports}` | Modules to create |
| `policy_id` | string | Cache key for session reuse |

**Config fields:**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `max_rules` | integer | -1 (unlimited) | Maximum rules to fire |
| `include_trace` | boolean | false | Include rule firing trace in output |
| `derived_only_new` | boolean | false | Only return newly derived facts |
| `output_templates` | array of strings | all | Only return facts matching these templates |

## Rule Loading

nxusKit's CLIPS provider loads rules through the `ClipsInput` configuration. Rules can be loaded from:

1. **Text strings** — CLIPS source passed directly via `rules_text`
2. **File paths** — `.clp` files loaded at runtime via `rules` array
3. **Binary images** — Pre-compiled `.bin` files via `binary_rules`

### Loading Order

1. Shared templates are loaded first (alphabetically by filename)
2. Module rules are loaded next, in the order specified by the `focus` configuration
3. User override rules are loaded last (taking precedence)

### Rust Example

```rust
use nxuskit::{AsyncProvider, ChatRequest, Message, NxuskitProvider, ProviderConfig};

let config = ProviderConfig {
    provider_type: "clips".into(),
    model: Some("/path/to/rules".into()),
    ..Default::default()
};
let provider = NxuskitProvider::new(config)?;

let request = ChatRequest::new("clips")
    .with_message(Message::user(r#"{"facts": [{"template": "input-data", "values": {"record-id": 1, "value": 150.0}}]}"#))
    .with_provider_options(serde_json::json!({
        "focus": ["data-qc"],
        "derived_only_new": true
    }));

let response = provider.chat(request).await?;
println!("Alerts: {}", response.content);
```

### Go Example

```go
import "github.com/nxus-SYSTEMS/nxusKit/packages/nxuskit-go"

config := nxuskit-go.ProviderConfig{
    ProviderType: "clips",
    Model:        strPtr("/path/to/rules"),
}
provider, _ := nxuskit-go.NewProvider(config)

request := nxuskit-go.NewChatRequest("clips").
    AddMessage(nxuskit-go.UserMessage(`{"facts": [{"template": "input-data", "values": {"record-id": 1, "value": 150.0}}]}`))
request.ProviderOptions = map[string]interface{}{
    "focus":            []string{"data-qc"},
    "derived_only_new": true,
}

response, _ := provider.Chat(ctx, request)
fmt.Println("Alerts:", response.Content)
```

### Python Example

```python
from nxuskit._ffi_provider import create_ffi_provider

provider = create_ffi_provider({
    "provider_type": "clips",
    "model": "/path/to/rules",
})

response = provider.chat({
    "model": "clips",
    "messages": [
        {
            "role": "user",
            "content": '{"facts": [{"template": "input-data", "values": {"record-id": 1, "value": 150.0}}]}',
        },
    ],
    "provider_options": {
        "focus": ["data-qc"],
        "derived_only_new": True,
    },
})
print("Alerts:", response.content)
```

## Writing Custom Rules

### 1. Create a Rule File

Place your `.clp` file in the appropriate module directory:

```
/path/to/my-rules/data-qc/my-custom-check.clp
```

### 2. Reference Shared Templates

Do NOT redefine templates. Use templates from the shared `shared/*.clp` files:

```clips
;;; my-custom-check.clp
;;; Custom confidence check for strict environments

(defrule strict-confidence-check
    "Flag records with confidence below 0.8"
    (input-data (record-id ?rid) (confidence ?c&:(< ?c 0.8)))
    =>
    (assert (alert
        (alert-type "low_confidence")
        (severity "warning")
        (message (str-cat "Record " ?rid " confidence " ?c " below threshold 0.8"))
        (recommendation "Review data source quality")
        (entity-id ?rid)
        (rule-name "strict-confidence-check")
        (module-name "data-qc"))))
```

### 3. Use Configurable Thresholds

Reference the `threshold-config` fact instead of hard-coding values:

```clips
(defrule configurable-bounds-check
    "Flag records outside configured bounds"
    (threshold-config (value-min ?vmin) (value-max ?vmax))
    (input-data (record-id ?rid) (value ?v&:(or (< ?v ?vmin) (> ?v ?vmax))))
    =>
    (assert (alert
        (alert-type "value_out_of_bounds")
        (severity "high")
        (message (str-cat "Record " ?rid " value " ?v " outside [" ?vmin "-" ?vmax "]"))
        (recommendation "Check data or adjust threshold configuration")
        (entity-id ?rid)
        (rule-name "configurable-bounds-check")
        (module-name "data-qc"))))
```

### 4. Naming Conventions

- **File names**: `NNN-descriptive-name.clp` (NNN = numeric prefix for load order)
- **Rule names**: `kebab-case`, descriptive of what is being checked
- **Alert types**: `snake_case`, machine-readable identifiers
- **Module names**: `kebab-case`, matching the directory name

## Testing Custom Rules

### Rust

```rust
use nxuskit::{AsyncProvider, ChatRequest, Message, MockProvider};

// Unit test with MockProvider (no SDK binary required)
#[tokio::test]
async fn test_with_mock() {
    let provider = MockProvider::new(r#"{"alerts": [{"type": "low_confidence"}]}"#);
    let request = ChatRequest::new("clips")
        .with_message(Message::user("test input"));
    let response = provider.chat(request).await.unwrap();
    assert!(response.content.contains("low_confidence"));
}

// Integration test with real CLIPS engine (requires SDK binary)
#[tokio::test]
#[ignore = "requires libnxuskit runtime"]
async fn test_with_clips_engine() {
    use nxuskit::{NxuskitProvider, ProviderConfig};

    let config = ProviderConfig {
        provider_type: "clips".into(),
        model: Some("tests/rules".into()),
        ..Default::default()
    };
    let provider = NxuskitProvider::new(config).unwrap();
    let request = ChatRequest::new("clips")
        .with_message(Message::user(r#"{"facts": [{"template": "input-data", "values": {"record-id": 1, "value": 999.0}}]}"#));
    let response = provider.chat(request).await.unwrap();
    assert!(response.content.contains("out_of_bounds"));
}
```

### Go

```go
func TestRulesWithMock(t *testing.T) {
    provider := nxuskit-go.NewMockProvider(
        nxuskit-go.WithResponse(`{"alerts": [{"type": "low_confidence"}]}`),
    )
    req := nxuskit-go.NewChatRequest("clips").
        AddMessage(nxuskit-go.UserMessage("test input"))
    resp, err := provider.Chat(context.Background(), req)
    require.NoError(t, err)
    assert.Contains(t, resp.Content, "low_confidence")
}
```

### Python

```python
import pytest
from nxuskit import Message
from nxuskit.mock import MockProvider

def test_rules_with_mock():
    provider = MockProvider(chunks=["low_confidence alert fired"])
    chunks = list(provider.chat_stream([Message.user("test input")]))
    assert "low_confidence" in "".join(chunk.delta for chunk in chunks)
```

## Debugging Rules

### Enable Tracing

Set the `RUST_LOG` environment variable when using the Rust SDK:

```bash
RUST_LOG=nxuskit=debug cargo test -- --nocapture
```

This shows:
- Which rule files are loaded
- Module loading order and file counts
- Warnings for syntax errors

### Inspect CLIPS Facts

Access the CLIPS session API directly (Rust SDK):

```rust
use nxuskit::ClipsSession;

let session = ClipsSession::create()?;
session.load_string("(deftemplate input-data (slot record-id (type INTEGER)) (slot value (type FLOAT)))")?;
session.load_string("(deftemplate alert (slot alert-type (type STRING)) (slot severity (type STRING)))")?;
session.load_string(r#"(defrule check (input-data (record-id ?rid) (value ?v&:(> ?v 100.0))) =>
    (assert (alert (alert-type "over-threshold") (severity "high"))))"#)?;
session.reset()?;
session.assert_string(r#"(input-data (record-id 1) (value 150.0))"#)?;
session.run(-1)?;

// List facts by template
let facts_json = session.facts_by_template("alert")?;
println!("Alert facts: {}", facts_json);

// Drop destroys the session automatically
```

### Step Limit Debugging

If inference hits the step limit, check for rule cycles. Common causes:

- Rules that assert facts matching their own LHS (infinite loop)
- Missing `not` patterns allowing rules to fire repeatedly
- Very large fact sets with cross-product patterns

Increase the step limit if needed:

```rust
session.run(500000)?;  // Pass -1 to run until agenda exhausted
```

## Best Practices

1. **Keep rules simple** — One check per rule. Complex logic should be split across multiple rules.
2. **Use configurable thresholds** — Reference threshold facts instead of hard-coding values.
3. **Document every rule** — Use the CLIPS documentation string for a brief description.
4. **Test in isolation** — Load only shared templates and the single rule file under test.
5. **Use meaningful names** — Rule names should describe what is checked, not how.
6. **Set appropriate salience** — Use `(declare (salience N))` to control firing order when needed.
