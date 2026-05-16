# nxuskit C ABI Reference

All functions are declared in `nxuskit.h`. The ABI uses opaque handles and JSON
strings for all data exchange. Every function is thread-safe unless noted.

## Version

### `nxuskit_version`

```c
const char *nxuskit_version(void);
```

Returns the library version string (e.g., `"0.9.4"`). The returned pointer is
static and valid for the process lifetime. Never returns NULL.

## Provider Lifecycle

### `nxuskit_create_provider`

```c
struct NxuskitProvider *nxuskit_create_provider(const char *config_json);
```

Creates a provider from a JSON configuration string.

**Parameters:**
- `config_json` — JSON string with at minimum `{"provider_type": "..."}`.
  See [Provider Reference](../providers/cloud-llms/) for provider-specific fields.

**Returns:** Opaque provider handle, or NULL on failure. On failure, call
`nxuskit_last_error()` for the error message.

**Ownership:** Caller owns the returned handle. Must call `nxuskit_free_provider()`
when done.

**Config JSON fields:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `provider_type` | string | Yes | Provider identifier (see providers list) |
| `api_key` | string | Varies | API key for cloud providers |
| `model` | string | No | Default model name |
| `base_url` | string | No | Custom API endpoint |
| `timeout_ms` | integer | No | Request timeout in milliseconds |

**Example:**
```c
const char *config = "{\"provider_type\":\"openai\",\"api_key\":\"sk-...\"}";
struct NxuskitProvider *p = nxuskit_create_provider(config);
if (!p) {
    fprintf(stderr, "Error: %s\n", nxuskit_last_error());
}
```

### `nxuskit_free_provider`

```c
void nxuskit_free_provider(struct NxuskitProvider *provider);
```

Frees a provider handle. Safe to call with NULL (no-op).

**Parameters:**
- `provider` — Handle from `nxuskit_create_provider()`, or NULL.

## Synchronous Chat

### `nxuskit_chat`

```c
struct NxuskitResponse *nxuskit_chat(
    struct NxuskitProvider *provider,
    const char *request_json
);
```

Sends a synchronous chat request. Blocks until the response is complete.

**Parameters:**
- `provider` — Provider handle (must not be NULL)
- `request_json` — JSON chat request string

**Returns:** Response handle, or NULL on failure.

**Ownership:** Caller owns the returned handle. Must call `nxuskit_free_response()`.

**Request JSON fields:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `model` | string | Yes | Model identifier |
| `messages` | array | Yes | Array of `{role, content}` objects |
| `temperature` | float | No | Sampling temperature (0.0–2.0) |
| `max_tokens` | integer | No | Maximum tokens in response |
| `top_p` | float | No | Nucleus sampling parameter |
| `stop` | array | No | Stop sequences |
| `stream` | boolean | No | Must be `false` for sync (default) |
| `response_format` | object | No | Response format constraints |

**Message object:**

| Field | Type | Description |
|-------|------|-------------|
| `role` | string | `"system"`, `"user"`, or `"assistant"` |
| `content` | string | Message content |

### `nxuskit_response_json`

```c
const char *nxuskit_response_json(const struct NxuskitResponse *response);
```

Returns the JSON string from a response handle.

**Parameters:**
- `response` — Response handle from `nxuskit_chat()` (must not be NULL)

**Returns:** JSON string pointer. Valid only while the response handle exists.
Do **not** free this pointer — it is owned by the response handle.

**Response JSON fields:**

| Field | Type | Description |
|-------|------|-------------|
| `content` | string | The model's response text |
| `model` | string | Model used |
| `provider` | string | Provider name |
| `usage` | object | Token usage (if available) |
| `usage.prompt_tokens` | integer | Input tokens |
| `usage.completion_tokens` | integer | Output tokens |
| `usage.total_tokens` | integer | Total tokens |
| `finish_reason` | string | Why generation stopped |
| `warnings` | array | Provider warnings (if any) |

### `nxuskit_free_response`

```c
void nxuskit_free_response(struct NxuskitResponse *response);
```

Frees a response handle. Safe to call with NULL.

## Streaming Chat

### Callback Types

```c
typedef int32_t (*NxuskitStreamCallback)(
    const char *chunk_json,
    void *user_data
);

typedef void (*NxuskitStreamDoneCallback)(
    const char *final_json,
    void *user_data
);
```

**`NxuskitStreamCallback`** is called for each streaming chunk. Return 0 to
continue, non-zero to request cancellation.

**`NxuskitStreamDoneCallback`** is called exactly once when streaming completes
(success, error, or cancellation).

**Important:** Callbacks fire from a **background thread**. The caller must
ensure `user_data` is thread-safe.

### `nxuskit_chat_stream`

```c
struct NxuskitStream *nxuskit_chat_stream(
    struct NxuskitProvider *provider,
    const char *request_json,
    NxuskitStreamCallback on_chunk,
    NxuskitStreamDoneCallback on_done,
    void *user_data
);
```

Starts a streaming chat request. Returns immediately; chunks arrive via callbacks.

**Parameters:**
- `provider` — Provider handle (must not be NULL)
- `request_json` — JSON chat request string
- `on_chunk` — Called for each chunk (from background thread)
- `on_done` — Called once when streaming ends (from background thread)
- `user_data` — Opaque pointer passed to both callbacks

**Returns:** Stream handle, or NULL on failure.

**Chunk JSON fields:**

| Field | Type | Description |
|-------|------|-------------|
| `delta` | string | Incremental text content |
| `index` | integer | Chunk sequence number (0-based) |
| `thinking` | string | Chain-of-thought reasoning (if enabled, optional) |
| `finish_reason` | string | Why generation stopped (set on final chunk, optional) |
| `usage` | object | Token usage statistics (typically only on final chunk, optional) |
| `tool_calls` | array | Tool call deltas (if applicable, optional) |

**Done JSON fields:** Same as synchronous response JSON, plus optional `error`:

| Field | Type | Description |
|-------|------|-------------|
| `error.error_type` | string | Error category |
| `error.message` | string | Error description |

### `nxuskit_cancel_stream`

```c
void nxuskit_cancel_stream(struct NxuskitStream *stream);
```

Cancels a streaming request. Blocks until all pending callbacks have completed.
After this call returns, no further callbacks will fire.

Safe to call with NULL.

### `nxuskit_free_stream`

```c
void nxuskit_free_stream(struct NxuskitStream *stream);
```

Frees a stream handle. Must be called after the stream completes or is cancelled.
Safe to call with NULL.

**Typical lifecycle:**
```c
stream = nxuskit_chat_stream(provider, request, on_chunk, on_done, data);
// ... callbacks fire ...
// After on_done fires (or if you want to cancel):
nxuskit_cancel_stream(stream);   // optional — ensures no more callbacks
nxuskit_free_stream(stream);     // required — frees resources
```

## Model Discovery

### `nxuskit_list_models`

```c
char *nxuskit_list_models(struct NxuskitProvider *provider);
```

Returns a JSON array of available models.

**Parameters:**
- `provider` — Provider handle (must not be NULL)

**Returns:** JSON string (caller-owned), or NULL on failure.

**Ownership:** Caller must free the returned string with `nxuskit_free_string()`.

**Response format:**
```json
[
  {"id": "gpt-4o", "name": "GPT-4o"},
  {"id": "gpt-4o-mini", "name": "GPT-4o Mini"}
]
```

## Error Handling

### `nxuskit_last_error`

```c
const char *nxuskit_last_error(void);
```

Returns the last error message for the **calling thread**. Returns NULL if no
error has occurred on this thread.

**Thread-local:** Each thread has its own error state. Calling any `nxuskit_*`
function may overwrite the previous error on that thread.

**Lifetime:** The returned pointer is valid until the next `nxuskit_*` call on
the same thread.

## Memory Management

### `nxuskit_free_string`

```c
void nxuskit_free_string(char *ptr);
```

Frees a caller-owned string returned by any `nxuskit_*` function that returns
`char*` (e.g., `nxuskit_list_models()`, `nxuskit_clips_facts_list()`,
`nxuskit_clips_eval()`). Safe to call with NULL.

**Important:** Only use this for strings documented as "caller-owned". Do not
use it for strings from `nxuskit_response_json()` (those are owned by the
response handle) or `nxuskit_version()` (static).

## Ownership Summary

| Function | Returns | Owned By | Free With |
|----------|---------|----------|-----------|
| `nxuskit_version()` | `const char*` | Library (static) | Never free |
| `nxuskit_create_provider()` | `NxuskitProvider*` | Caller | `nxuskit_free_provider()` |
| `nxuskit_chat()` | `NxuskitResponse*` | Caller | `nxuskit_free_response()` |
| `nxuskit_response_json()` | `const char*` | Response handle | Freed with response |
| `nxuskit_chat_stream()` | `NxuskitStream*` | Caller | `nxuskit_free_stream()` |
| `nxuskit_list_models()` | `char*` | Caller | `nxuskit_free_string()` |
| `nxuskit_last_error()` | `const char*` | Thread-local | Never free |

## CLIPS Session API

Direct access to the CLIPS 6.4.2 rule engine via session-based handles — create
sessions, load rules, assert facts, run inference, and inspect results without
going through the provider/chat abstraction.

All CLIPS operations use opaque `uint64_t` session handles (generational indices
that prevent use-after-free). Return conventions:
- `int32_t` functions return 0 on success, -1 on error
- `char*` functions return a JSON string (caller-owned, free with `nxuskit_free_string()`) or NULL on error
- `bool` functions return the queried value; check `nxuskit_last_error()` on unexpected results

### Session Lifecycle

#### `nxuskit_clips_session_create`

```c
uint64_t nxuskit_clips_session_create(void);
```

Creates a new isolated CLIPS session. Returns a non-zero session handle, or 0 on
failure (check `nxuskit_last_error()`).

#### `nxuskit_clips_session_destroy`

```c
void nxuskit_clips_session_destroy(uint64_t session);
```

Destroys a session and frees all resources. No-op if the handle is invalid.

#### `nxuskit_clips_session_reset`

```c
int32_t nxuskit_clips_session_reset(uint64_t session);
```

Resets the session: clears all facts, preserves rules, restores initial-fact.

#### `nxuskit_clips_session_clear`

```c
int32_t nxuskit_clips_session_clear(uint64_t session);
```

Clears everything (facts, rules, templates) — returns the session to a pristine state.

#### `nxuskit_clips_session_info`

```c
char *nxuskit_clips_session_info(uint64_t session);
```

Returns session metadata as a JSON object (module count, rule count, fact count, etc.).

### Loading Constructs

#### `nxuskit_clips_session_load_file`

```c
int32_t nxuskit_clips_session_load_file(uint64_t session, const char *path);
```

Loads constructs from a `.clp` file.

#### `nxuskit_clips_session_load_string`

```c
int32_t nxuskit_clips_session_load_string(uint64_t session, const char *constructs);
```

Loads constructs from an in-memory string.

**Example:**
```c
uint64_t s = nxuskit_clips_session_create();
nxuskit_clips_session_load_string(s,
    "(deftemplate sensor (slot name) (slot value))");
```

#### `nxuskit_clips_session_load_binary`

```c
int32_t nxuskit_clips_session_load_binary(uint64_t session, const char *path);
```

Loads a pre-compiled binary image (`.bin` file created with `save_binary`).

#### `nxuskit_clips_session_save_binary`

```c
int32_t nxuskit_clips_session_save_binary(uint64_t session, const char *path);
```

Saves the current session state as a binary image for fast reloading.

#### `nxuskit_clips_session_build`

```c
int32_t nxuskit_clips_session_build(uint64_t session, const char *construct);
```

Builds a single CLIPS construct (deftemplate, defrule, etc.) from a string.

#### `nxuskit_clips_session_batch`

```c
int32_t nxuskit_clips_session_batch(uint64_t session, const char *path);
```

Executes a batch file of CLIPS commands.

#### `nxuskit_clips_session_load_json`

```c
int32_t nxuskit_clips_session_load_json(uint64_t session, const char *json);
```

Loads constructs from a JSON specification (templates, rules, facts in one call).

### Session Cache

#### `nxuskit_clips_session_preload`

```c
int32_t nxuskit_clips_session_preload(const char *name, const char *rules_json);
```

Preloads a named session into the cache. The session is created, rules are loaded,
and the session is stored for later retrieval via `get_cached`.

#### `nxuskit_clips_session_get_cached`

```c
uint64_t nxuskit_clips_session_get_cached(const char *name);
```

Returns a cached session handle by name. Returns 0 if not found.

#### `nxuskit_clips_session_cache_remove`

```c
int32_t nxuskit_clips_session_cache_remove(const char *name);
```

Removes a named session from the cache and destroys it.

### Fact Operations

#### `nxuskit_clips_fact_assert_string`

```c
int64_t nxuskit_clips_fact_assert_string(uint64_t session, const char *fact_string);
```

Asserts a fact using CLIPS syntax. Returns the fact index (>= 0) on success, or -1 on error.

**Example:**
```c
int64_t idx = nxuskit_clips_fact_assert_string(s,
    "(sensor (name \"temp-1\") (value 150))");
```

#### `nxuskit_clips_fact_assert_structured`

```c
int64_t nxuskit_clips_fact_assert_structured(
    uint64_t session,
    const char *template_name,
    const char *slots_json
);
```

Asserts a fact using a JSON slot specification. Returns the fact index or -1.

**Example:**
```c
int64_t idx = nxuskit_clips_fact_assert_structured(s, "sensor",
    "{\"name\":\"temp-1\",\"value\":150}");
```

#### `nxuskit_clips_fact_retract`

```c
int32_t nxuskit_clips_fact_retract(uint64_t session, int64_t fact_index);
```

Retracts a fact by its index.

#### `nxuskit_clips_fact_retract_by_template`

```c
int32_t nxuskit_clips_fact_retract_by_template(uint64_t session, const char *template_name);
```

Retracts all facts of a given template.

#### `nxuskit_clips_fact_exists`

```c
bool nxuskit_clips_fact_exists(uint64_t session, int64_t fact_index);
```

Returns true if the fact index refers to an existing fact.

#### `nxuskit_clips_fact_get_slot`

```c
char *nxuskit_clips_fact_get_slot(uint64_t session, int64_t fact_index, const char *slot_name);
```

Returns a slot value as a type-tagged JSON string (e.g., `{"type":"integer","value":42}`).

**Slot JSON types:**

| CLIPS Type | JSON `type` | JSON `value` |
|------------|-------------|--------------|
| INTEGER | `"integer"` | number |
| FLOAT | `"float"` | number |
| STRING | `"string"` | string |
| SYMBOL | `"symbol"` | string |
| MULTIFIELD | `"multifield"` | array of typed values |

#### `nxuskit_clips_fact_slot_values`

```c
char *nxuskit_clips_fact_slot_values(uint64_t session, int64_t fact_index);
```

Returns all slot values as a JSON object.

#### `nxuskit_clips_fact_pp_form`

```c
char *nxuskit_clips_fact_pp_form(uint64_t session, int64_t fact_index);
```

Returns the pretty-printed CLIPS representation of a fact.

#### `nxuskit_clips_fact_index`

```c
int64_t nxuskit_clips_fact_index(uint64_t session, int64_t fact_index);
```

Returns the canonical CLIPS fact index, or -1 on error.

#### `nxuskit_clips_facts_list`

```c
char *nxuskit_clips_facts_list(uint64_t session);
```

Returns all facts as a JSON array.

#### `nxuskit_clips_facts_by_template`

```c
char *nxuskit_clips_facts_by_template(uint64_t session, const char *template_name);
```

Returns all facts matching a template as a JSON array.

#### `nxuskit_clips_fact_duplication_get` / `set`

```c
bool nxuskit_clips_fact_duplication_get(uint64_t session);
int32_t nxuskit_clips_fact_duplication_set(uint64_t session, bool allow);
```

Query or set whether duplicate facts are allowed.

### Template Operations

#### `nxuskit_clips_template_exists`

```c
bool nxuskit_clips_template_exists(uint64_t session, const char *name);
```

Returns true if the named template exists in the session.

#### `nxuskit_clips_template_list`

```c
char *nxuskit_clips_template_list(uint64_t session);
```

Returns all template names as a JSON array.

#### `nxuskit_clips_template_slot_names`

```c
char *nxuskit_clips_template_slot_names(uint64_t session, const char *template_name);
```

Returns slot names for a template as a JSON array.

#### `nxuskit_clips_template_slot_info`

```c
char *nxuskit_clips_template_slot_info(uint64_t session, const char *template_name);
```

Returns detailed slot information (types, defaults, constraints) as JSON.

#### `nxuskit_clips_template_facts`

```c
char *nxuskit_clips_template_facts(uint64_t session, const char *template_name);
```

Returns all facts of a template as a JSON array.

#### `nxuskit_clips_template_pp_form`

```c
char *nxuskit_clips_template_pp_form(uint64_t session, const char *template_name);
```

Returns the pretty-printed CLIPS definition of a template.

### Inference Engine

#### `nxuskit_clips_session_run`

```c
int64_t nxuskit_clips_session_run(uint64_t session, int64_t limit);
```

Runs inference. Pass `limit = -1` to run until the agenda is exhausted.
Returns the number of rules fired, or -1 on error.

#### `nxuskit_clips_session_halt`

```c
int32_t nxuskit_clips_session_halt(uint64_t session);
```

Signals the session to halt inference (thread-safe — can be called from another thread).

#### `nxuskit_clips_agenda_size`

```c
int64_t nxuskit_clips_agenda_size(uint64_t session);
```

Returns the number of activations on the agenda.

#### `nxuskit_clips_agenda_clear`

```c
int32_t nxuskit_clips_agenda_clear(uint64_t session);
```

Removes all activations from the agenda.

#### `nxuskit_clips_agenda_reorder`

```c
int32_t nxuskit_clips_agenda_reorder(uint64_t session);
```

Reorders the agenda using the current conflict resolution strategy.

#### `nxuskit_clips_strategy_get` / `set`

```c
char *nxuskit_clips_strategy_get(uint64_t session);
int32_t nxuskit_clips_strategy_set(uint64_t session, const char *strategy);
```

Get or set the conflict resolution strategy. Valid values: `"depth"`, `"breadth"`,
`"simplicity"`, `"complexity"`, `"lex"`, `"mea"`, `"random"`.

#### `nxuskit_clips_salience_mode_get` / `set`

```c
char *nxuskit_clips_salience_mode_get(uint64_t session);
int32_t nxuskit_clips_salience_mode_set(uint64_t session, const char *mode);
```

Get or set the salience evaluation mode.

### Rule Operations

#### `nxuskit_clips_rule_exists`

```c
bool nxuskit_clips_rule_exists(uint64_t session, const char *name);
```

Returns true if the named rule exists.

#### `nxuskit_clips_rule_list`

```c
char *nxuskit_clips_rule_list(uint64_t session);
```

Returns all rule names as a JSON array.

#### `nxuskit_clips_rule_times_fired`

```c
int64_t nxuskit_clips_rule_times_fired(uint64_t session, const char *rule_name);
```

Returns how many times a rule has fired, or -1 on error.

#### `nxuskit_clips_rule_breakpoint_set` / `remove` / `has_breakpoint`

```c
int32_t nxuskit_clips_rule_breakpoint_set(uint64_t session, const char *rule_name);
int32_t nxuskit_clips_rule_breakpoint_remove(uint64_t session, const char *rule_name);
bool nxuskit_clips_rule_has_breakpoint(uint64_t session, const char *rule_name);
```

Manage breakpoints on rules for debugging.

#### `nxuskit_clips_rule_refresh`

```c
int32_t nxuskit_clips_rule_refresh(uint64_t session, const char *rule_name);
```

Refreshes a rule, placing its activations back on the agenda.

#### `nxuskit_clips_rule_pp_form`

```c
char *nxuskit_clips_rule_pp_form(uint64_t session, const char *rule_name);
```

Returns the pretty-printed CLIPS definition of a rule.

#### `nxuskit_clips_rule_delete`

```c
int32_t nxuskit_clips_rule_delete(uint64_t session, const char *rule_name);
```

Deletes a rule from the session.

### Module Operations

#### `nxuskit_clips_module_exists`

```c
bool nxuskit_clips_module_exists(uint64_t session, const char *name);
```

Returns true if the named module exists.

#### `nxuskit_clips_module_list`

```c
char *nxuskit_clips_module_list(uint64_t session);
```

Returns all module names as a JSON array.

#### `nxuskit_clips_module_current_get` / `set`

```c
char *nxuskit_clips_module_current_get(uint64_t session);
int32_t nxuskit_clips_module_current_set(uint64_t session, const char *name);
```

Get or set the current module.

#### `nxuskit_clips_focus_push` / `get` / `pop` / `clear`

```c
int32_t nxuskit_clips_focus_push(uint64_t session, const char *module_name);
char *nxuskit_clips_focus_get(uint64_t session);
int32_t nxuskit_clips_focus_pop(uint64_t session);
int32_t nxuskit_clips_focus_clear(uint64_t session);
```

Manage the module focus stack (controls which module's rules are eligible to fire).

### Global Variables

#### `nxuskit_clips_global_exists`

```c
bool nxuskit_clips_global_exists(uint64_t session, const char *name);
```

Returns true if the named defglobal exists.

#### `nxuskit_clips_global_list`

```c
char *nxuskit_clips_global_list(uint64_t session);
```

Returns all global variable names as a JSON array.

#### `nxuskit_clips_global_get_value` / `set_value`

```c
char *nxuskit_clips_global_get_value(uint64_t session, const char *name);
int32_t nxuskit_clips_global_set_value(uint64_t session, const char *name, const char *value_json);
```

Get or set a global variable value. Values use the type-tagged JSON format.

#### `nxuskit_clips_reset_globals_get` / `set`

```c
bool nxuskit_clips_reset_globals_get(uint64_t session);
int32_t nxuskit_clips_reset_globals_set(uint64_t session, bool reset);
```

Query or set whether globals are reset when `session_reset` is called.

### Evaluation

#### `nxuskit_clips_eval`

```c
char *nxuskit_clips_eval(uint64_t session, const char *expression);
```

Evaluates a CLIPS expression and returns the result as a type-tagged JSON string.

#### `nxuskit_clips_function_call`

```c
char *nxuskit_clips_function_call(uint64_t session, const char *function_name, const char *args);
```

Calls a CLIPS function by name with optional arguments string.

### Debugging

#### `nxuskit_clips_watch` / `unwatch`

```c
int32_t nxuskit_clips_watch(uint64_t session, const char *item);
int32_t nxuskit_clips_unwatch(uint64_t session, const char *item);
```

Enable or disable tracing for a watch item. Valid items: `"facts"`, `"rules"`,
`"activations"`, `"compilations"`, `"statistics"`, `"globals"`, `"focus"`, `"all"`.

#### `nxuskit_clips_dribble_on` / `off`

```c
int32_t nxuskit_clips_dribble_on(uint64_t session, const char *path);
int32_t nxuskit_clips_dribble_off(uint64_t session);
```

Start or stop logging all CLIPS I/O to a file.

### CLIPS Ownership Summary

| Function | Returns | Owned By | Free With |
|----------|---------|----------|-----------|
| `nxuskit_clips_session_create()` | `uint64_t` | Session registry | `nxuskit_clips_session_destroy()` |
| `nxuskit_clips_session_get_cached()` | `uint64_t` | Session cache | `nxuskit_clips_session_cache_remove()` |
| `nxuskit_clips_session_info()` | `char*` | Caller | `nxuskit_free_string()` |
| `nxuskit_clips_fact_get_slot()` | `char*` | Caller | `nxuskit_free_string()` |
| `nxuskit_clips_fact_slot_values()` | `char*` | Caller | `nxuskit_free_string()` |
| `nxuskit_clips_fact_pp_form()` | `char*` | Caller | `nxuskit_free_string()` |
| `nxuskit_clips_facts_list()` | `char*` | Caller | `nxuskit_free_string()` |
| `nxuskit_clips_facts_by_template()` | `char*` | Caller | `nxuskit_free_string()` |
| `nxuskit_clips_template_list()` | `char*` | Caller | `nxuskit_free_string()` |
| `nxuskit_clips_template_slot_names()` | `char*` | Caller | `nxuskit_free_string()` |
| `nxuskit_clips_template_slot_info()` | `char*` | Caller | `nxuskit_free_string()` |
| `nxuskit_clips_template_facts()` | `char*` | Caller | `nxuskit_free_string()` |
| `nxuskit_clips_template_pp_form()` | `char*` | Caller | `nxuskit_free_string()` |
| `nxuskit_clips_rule_list()` | `char*` | Caller | `nxuskit_free_string()` |
| `nxuskit_clips_rule_pp_form()` | `char*` | Caller | `nxuskit_free_string()` |
| `nxuskit_clips_module_list()` | `char*` | Caller | `nxuskit_free_string()` |
| `nxuskit_clips_module_current_get()` | `char*` | Caller | `nxuskit_free_string()` |
| `nxuskit_clips_focus_get()` | `char*` | Caller | `nxuskit_free_string()` |
| `nxuskit_clips_global_list()` | `char*` | Caller | `nxuskit_free_string()` |
| `nxuskit_clips_global_get_value()` | `char*` | Caller | `nxuskit_free_string()` |
| `nxuskit_clips_strategy_get()` | `char*` | Caller | `nxuskit_free_string()` |
| `nxuskit_clips_salience_mode_get()` | `char*` | Caller | `nxuskit_free_string()` |
| `nxuskit_clips_eval()` | `char*` | Caller | `nxuskit_free_string()` |
| `nxuskit_clips_function_call()` | `char*` | Caller | `nxuskit_free_string()` |

### Complete Example

```c
#include "nxuskit.h"
#include <stdio.h>

int main(void) {
    // Create session
    uint64_t s = nxuskit_clips_session_create();
    if (!s) {
        fprintf(stderr, "Error: %s\n", nxuskit_last_error());
        return 1;
    }

    // Load rules
    nxuskit_clips_session_load_string(s,
        "(deftemplate sensor (slot name (type STRING)) (slot value (type INTEGER)))");
    nxuskit_clips_session_load_string(s,
        "(defrule high-temp"
        "    (sensor (name ?n) (value ?v&:(> ?v 100)))"
        "    =>"
        "    (printout t \"ALERT: \" ?n \" = \" ?v crlf))");

    // Assert facts and run
    nxuskit_clips_session_reset(s);
    nxuskit_clips_fact_assert_string(s, "(sensor (name \"temp-1\") (value 150))");
    int64_t fired = nxuskit_clips_session_run(s, -1);
    printf("Rules fired: %lld\n", fired);

    // Inspect results
    char *facts = nxuskit_clips_facts_list(s);
    printf("Facts: %s\n", facts);
    nxuskit_free_string(facts);

    // Cleanup
    nxuskit_clips_session_destroy(s);
    return 0;
}
```

## Error Types

Errors returned in response JSON or via `nxuskit_last_error()`:

| Error Type | Description |
|------------|-------------|
| `configuration` | Invalid config, missing API key, version mismatch |
| `invalid_request` | Malformed request JSON, missing required fields |
| `authentication` | Invalid or expired API key |
| `rate_limit` | Provider rate limit exceeded |
| `provider` | Provider-side error (server error, model not found) |
| `timeout` | Request timed out |
| `stream` | Streaming error (connection lost, cancelled) |
| `internal` | Unexpected internal error |
