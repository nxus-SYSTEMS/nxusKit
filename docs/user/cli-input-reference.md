# CLI Input Format Reference

Single source of truth for every Level 1 `nxuskit-cli` command's input schema,
with local loopback examples where the command can succeed without a semantic
LLM response.

All commands accept `--input -` for stdin and `--format json` (default).
Output is wrapped in a `ResponseEnvelope` with `trace_id`, `request_metadata`,
and timing fields.

Loopback examples that are intended to execute locally use explicit loopback
model names. In nxusKit CLI v1.0.0, do not rely on an omitted model resolving
to `"default"` for the `loopback` provider; use `"echo"` for plain echo tests
or `"echo-json-native"` when you need a loopback model that advertises native
JSON support. Commands that require a semantic structured answer, such as
`judge select`, should use a configured non-loopback LLM provider because
loopback models echo the prompt.

---

## Table of Contents

- [call](#call)
- [zen eval](#zen-eval)
- [zen validate](#zen-validate)
- [zen test](#zen-test)
- [solver solve](#solver-solve)
  - [Solver Format Disambiguation](#solver-format-disambiguation)
- [solver what-if](#solver-what-if)
- [clips eval](#clips-eval)
- [clips session](#clips-session)
  - [clips session create](#clips-session-create)
  - [clips session list](#clips-session-list)
  - [clips session destroy](#clips-session-destroy)
- [provider list](#provider-list)
- [provider info](#provider-info)
- [bn infer](#bn-infer)
- [bn learn](#bn-learn)
- [bn evidence](#bn-evidence)
- [pipeline run](#pipeline-run)
- [artifact merge](#artifact-merge)
- [artifact summarize](#artifact-summarize)
- [packet validate](#packet-validate)
- [tool-loop run](#tool-loop-run)
- [judge select](#judge-select)
- [branch fork](#branch-fork)
- [branch compare](#branch-compare)
- [Error responses](#error-responses)

---

### `call`

LLM invocation. Accepts either `prompt` (single-turn) or `messages` (multi-turn).

**Input schema (`CallInput`):**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `prompt` | string | no | Single-turn user prompt (convenience shorthand) |
| `messages` | array of `{role, content}` | no | Multi-turn conversation messages |
| `system` | string | no | System message prepended to the conversation |
| `provider` | string | no | Provider name (default: `loopback`, or `$NXUSKIT_PROVIDER`) |
| `model` | string | no | Model identifier. For `loopback`, use a valid loopback model such as `"echo"` or `"echo-json-native"`; do not rely on the implicit `"default"` model in v1.0.0. |
| `tool_definitions` | array of JSON objects | no | Tool/function schemas passed to the LLM |
| `tool_choice` | JSON value | no | Provider-compatible tool choice policy passed with `tool_definitions` |
| `response_format` | object | no | Response format constraint: `{"type":"text"}`, `{"type":"json_object"}`, or `{"type":"json_schema","schema":{...}}` |
| `thinking_mode` | string | no | Thinking policy: `auto`, `enabled`, `disabled`, or `omit` |
| `max_tokens` | u32 | no | Maximum output tokens |
| `temperature` | f32 | no | Sampling temperature |
| `stream` | bool | no | Enable streaming (JSONL output) |

At least one of `prompt` or `messages` should be provided.

**Example:**

```bash
echo '{"prompt": "Hello", "provider": "loopback", "model": "echo"}' \
  | nxuskit-cli call --input - --format json
```

**Common errors:**

- `Invalid call input JSON: missing field ...` -- Fix: ensure the input is valid JSON with at least `prompt` or `messages`.
- `Unknown provider "xyz"` -- Fix: use a valid provider name (`loopback`, `openai`, `claude`, etc.) or set `$NXUSKIT_PROVIDER`.

---

### `zen eval`

ZEN decision-table evaluation is a Pro capability. Public CE builds expose command discovery and entitlement errors only; the implementation is not shipped in CE source or release bundles. See the Pro-labeled product documentation for licensed usage details.

### `zen validate`

ZEN decision-model validation is a Pro capability. Public CE builds expose command discovery and entitlement errors only; detailed input/output contracts are published in Pro-labeled documentation.

### `zen test`

ZEN fixture testing is a Pro capability. Public CE builds expose command discovery and entitlement errors only; detailed fixture contracts are published in Pro-labeled documentation.

### `solver solve`

Constraint solving is a Pro capability. Public CE builds expose command discovery and entitlement errors only; solver implementation details, input formats, and output contracts are published in Pro-labeled documentation.

### `clips eval`

CLIPS rule engine evaluation.

**Input schema (`ClipsEvalInput`):**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `rules` | string | **yes** | CLIPS rule definitions (defrule, deftemplate, etc.) |
| `facts` | array of strings | no | Initial facts to assert (default: `[]`) |

> **Newline escaping:** CLIPS rules contain newlines. In JSON strings you must
> use `\n` for line breaks. When piping from a shell, use `$'...'` quoting or
> a heredoc to embed literal newlines, then let `jq` handle escaping.

**Example:**

```bash
echo '{
  "rules": "(defrule greet (person (name ?n)) => (assert (greeting (message (str-cat \"Hello \" ?n)))))",
  "facts": ["(person (name \"World\"))"]
}' | nxuskit-cli clips eval --input - --format json
```

**Multi-line rules (using jq for safe escaping):**

```bash
RULES=$(cat <<'CLIPS'
(deftemplate person (slot name))
(deftemplate greeting (slot message))
(defrule greet
  (person (name ?n))
  =>
  (assert (greeting (message (str-cat "Hello " ?n)))))
CLIPS
)
jq -n --arg rules "$RULES" '{"rules": $rules, "facts": ["(person (name \"World\"))"]}' \
  | nxuskit-cli clips eval --input - --format json
```

**Common errors:**

- `Failed to load CLIPS rules: ...` -- Fix: check CLIPS syntax. Common issues: unbalanced parentheses, missing `=>` in defrule.
- `Failed to assert fact '(foo)': ...` -- Fix: if using deftemplates, facts must match the template signature (e.g. `(person (name "x"))` not `(person "x")`).
- `Invalid CLIPS eval input: missing field "rules"` -- Fix: `rules` is required. Use `"rules": ""` for an empty ruleset.

---

### `bn infer`

Bayesian network inference via variable elimination.

**Input schema (`BnInferInput`):**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `network` | `NetworkDef` | **yes** | Bayesian network structure and CPDs |
| `evidence` | map of string to string | no | Observed variable states (default: `{}`) |
| `query_nodes` | array of strings | **yes** | Variables to compute posterior probabilities for |
| `algorithm` | string | no | Inference algorithm (default: `"variable_elimination"`) |

**`NetworkDef` fields:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `nodes` | array of `NodeDef` | **yes** | Variable definitions with states |
| `edges` | array of `EdgeDef` | **yes** | Directed edges (parent to child) |
| `cpds` | map of string to `CpdDef` | **yes** | Conditional probability distributions |

**`NodeDef` fields:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | **yes** | Variable name |
| `states` | array of strings | **yes** | Possible states for this variable |

**`EdgeDef` fields:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `from` | string | **yes** | Parent node name |
| `to` | string | **yes** | Child node name |

**`CpdDef` fields:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `probabilities` | array of f64 | **yes** | Flat probability table (row-major order over parent states) |

**Example:**

```bash
echo '{
  "network": {
    "nodes": [
      {"name": "Rain", "states": ["yes", "no"]},
      {"name": "Sprinkler", "states": ["on", "off"]},
      {"name": "GrassWet", "states": ["wet", "dry"]}
    ],
    "edges": [
      {"from": "Rain", "to": "GrassWet"},
      {"from": "Sprinkler", "to": "GrassWet"}
    ],
    "cpds": {
      "Rain": {"probabilities": [0.2, 0.8]},
      "Sprinkler": {"probabilities": [0.4, 0.6]},
      "GrassWet": {"probabilities": [0.99, 0.01, 0.9, 0.1, 0.8, 0.2, 0.0, 1.0]}
    }
  },
  "evidence": {"GrassWet": "wet"},
  "query_nodes": ["Rain"]
}' | nxuskit-cli bn infer --input - --format json
```

**Common errors:**

- `Invalid variable name '...': ...` -- Fix: node names must be non-empty alphanumeric identifiers.
- `Failed to set CPD for '...': ...` -- Fix: probability array length must equal the product of the node's state count and all parent nodes' state counts.
- `Invalid BN inference input: missing field "network"` -- Fix: `network` and `query_nodes` are both required.

---

### `bn learn`

Parameter learning: estimate the conditional probability tables (CPDs) of a
network from a CSV dataset, given the network *skeleton* (variables + edges, no
CPDs). The learned network is BIF-exportable. Community edition.

**Input schema (`BnLearnInput`):**

| Field | Type | Required | Description |
|---|---|---|---|
| `network` | object | **yes** | Network skeleton: `{"nodes": [{"name","states"}], "edges": [{"from","to"}]}` -- NO `cpds` |
| `data_file` | string | **yes** | Path to the CSV dataset; column headers must map to variable names |
| `learner` | string | no (default `"mle"`) | `"mle"` (Maximum Likelihood + Laplace smoothing) or `"bayesian"` (Dirichlet prior) |
| `pseudocount` | number | no (default `1.0`) | Laplace pseudocount; for `bayesian`, the default Dirichlet alpha |

**Output (`--format json`):** `{"result": {"learned_cpts": {var: [probs]}, "bif": "<BIF text>", "num_rows": N, "num_variables": M, "learner": "mle", "elapsed_ms": ...}}`.

**Example:**

```bash
echo '{
  "network": {
    "nodes": [
      {"name":"Rain","states":["yes","no"]},
      {"name":"Sprinkler","states":["on","off"]},
      {"name":"WetGrass","states":["yes","no"]}
    ],
    "edges": [{"from":"Rain","to":"WetGrass"},{"from":"Sprinkler","to":"WetGrass"}]
  },
  "data_file": "/abs/path/to/training.csv",
  "learner": "mle",
  "pseudocount": 0.0
}' | nxuskit-cli bn learn --input - --format json
```

**Common errors:**

- `Training CSV not found: ...` -- Fix: `data_file` must be a path to an existing CSV file.
- `Failed to load dataset '...': ...` -- Fix: CSV column headers must match the network's variable names; cell values must match declared states (empty / `?` cells are treated as missing).
- `Unknown learner '...'. Valid: mle, bayesian` -- Fix: use one of the two supported learners.

**Excluded from v1.0.0:** structure *search* (`hill_climb` / `k2`) is engine-only, not a CLI surface; streaming; team-runtime lineage.

---

### `bn evidence`

Validate and normalize an observations map against a fully-specified network
(same `network` shape as `bn infer`). Returns the validated observations or a
structured validation error naming the offending variable/state. Community edition.

**Input schema (`BnEvidenceInput`):**

| Field | Type | Required | Description |
|---|---|---|---|
| `network` | object | **yes** | `{"nodes": [...], "edges": [...], "cpds": {...}}` (same as `bn infer`) |
| `observations` | object | no | `{var: state, ...}` -- each is validated against the network |

**Output (`--format json`):** `{"result": {"valid": true, "evidence": {var: state}, "observation_count": N, "elapsed_ms": ...}}`.

**Example:**

```bash
echo '{
  "network": {
    "nodes": [{"name":"Rain","states":["yes","no"]},{"name":"WetGrass","states":["yes","no"]}],
    "edges": [{"from":"Rain","to":"WetGrass"}],
    "cpds": {"Rain":{"probabilities":[0.2,0.8]},"WetGrass":{"probabilities":[0.9,0.1,0.1,0.9]}}
  },
  "observations": {"Rain":"yes"}
}' | nxuskit-cli bn evidence --input - --format json
```

**Common errors:**

- `Invalid observation Rain=maybe: ...` (exit 5, `validation`) -- Fix: the observed state must be one of the variable's declared states.
- `Invalid observation variable '...': ...` -- Fix: the variable must exist in the network.

---

### `pipeline run`

Sequential multi-stage pipeline execution. Accepts YAML or JSON.

**Input schema (`PipelineDefinition`):**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | **yes** | Pipeline name |
| `stages` | array of `Stage` | **yes** | Ordered list of stages to execute |

**`Stage` fields:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | **yes** | Stage identifier |
| `type` | string | **yes** | Stage type: `"llm"`, `"clips_eval"`, `"zen_eval"`, `"solver_solve"`, `"bn_infer"` |
| `config` | JSON object | no | Stage-specific configuration (passed to the engine) |
| `output_key` | string | no | Bind stage output to a named key for `{{key}}` interpolation in later stages |

For `llm` stages, `config` accepts `prompt`, `provider`, and `model`.
For `clips_eval` stages, `config` accepts `rules` and `facts`.
For other stage types, `config` mirrors the respective command's input schema.

Stages execute sequentially. Each stage receives the previous stage's output.
String values in `config` support `{{key}}` interpolation from `output_key` bindings.
If a stage fails, all subsequent stages are marked `"skipped"`.

**Example:**

```bash
echo '{
  "name": "demo-pipeline",
  "stages": [
    {
      "name": "generate",
      "type": "llm",
      "config": {"prompt": "Say hello", "provider": "loopback", "model": "echo"},
      "output_key": "llm_result"
    },
    {
      "name": "evaluate",
      "type": "llm",
      "config": {"prompt": "Summarize: {{llm_result}}", "provider": "loopback", "model": "echo"}
    }
  ]
}' | nxuskit-cli pipeline run --input - --format json
```

**Common errors:**

- `Invalid pipeline definition: ...` -- Fix: ensure input is valid YAML or JSON with `name` and `stages` fields.
- `Unknown stage type: xyz` -- Fix: valid stage types are `llm`, `clips_eval`, `zen_eval`, `solver_solve`, `bn_infer`.
- `PipelineStageFailed { stage: "...", detail: ... }` -- Fix: check the `stages[].result.message` field in the output for the root cause.

---

### `artifact merge`

Deep-merge multiple JSON artifact files with conflict detection.

This command takes multiple `--input` flags (not stdin JSON). Each input must be a
JSON object (not an array or scalar).

**CLI arguments:**

| Argument | Type | Required | Description |
|----------|------|----------|-------------|
| `--input` | string (repeatable) | **yes** (>= 2) | Paths to JSON files to merge |
| `--merge-strategy` | string | no | Conflict resolution: `"error"` (default), `"first"`, `"last"` |

No JSON input schema -- inputs are arbitrary JSON objects read from files.

**Example:**

```bash
echo '{"a": 1, "b": {"x": 10}}' > /tmp/art1.json
echo '{"b": {"y": 20}, "c": 3}' > /tmp/art2.json

nxuskit-cli artifact merge \
  --input /tmp/art1.json --input /tmp/art2.json \
  --format json
```

**Common errors:**

- `artifact merge requires at least 2 input files` -- Fix: provide at least two `--input` paths.
- `MergeConflict { paths: ["b.x"] }` -- Fix: two files have different values at the same key path. Use `--merge-strategy first` or `--merge-strategy last` to resolve.
- `'...' is not a JSON object` -- Fix: each input file must contain a JSON object (`{...}`), not an array or scalar.

---

### `artifact summarize`

Summarize a JSON artifact's structure and estimated token cost.

**CLI arguments:**

| Argument | Type | Required | Description |
|----------|------|----------|-------------|
| `--input` | string | **yes** | Path to JSON file or `-` for stdin |

No JSON input schema -- the input is any valid JSON value.

**Output fields:**

| Field | Type | Description |
|-------|------|-------------|
| `field_count` | u32 | Total number of fields (recursive) |
| `top_level_keys` | array of strings | Keys at the root level |
| `estimated_tokens` | u32 | Rough token estimate (byte length / 4) |

**Example:**

```bash
echo '{"name": "test", "data": {"x": 1, "y": 2}}' \
  | nxuskit-cli artifact summarize --input - --format json
```

**Common errors:**

- `Invalid artifact JSON: ...` -- Fix: input must be valid JSON.

---

### `packet validate`

Validate a JSON document against a JSON Schema.

**CLI arguments:**

| Argument | Type | Required | Description |
|----------|------|----------|-------------|
| `--input` | string | **yes** | Packet data file (JSON) or `-` for stdin |
| `--schema` | string | **yes** | Path to a JSON Schema file |

No custom input schema -- the packet is any JSON value validated against the
provided JSON Schema.

**Example:**

```bash
echo '{"type": "string"}' > /tmp/schema.json
echo '"hello"' | nxuskit-cli packet validate --input - --schema /tmp/schema.json --format json
```

**Output fields:**

| Field | Type | Description |
|-------|------|-------------|
| `valid` | bool | `true` if the packet conforms to the schema |
| `errors` | array of `{path, message, keyword}` | Validation errors (empty when valid) |

**Common errors:**

- `SchemaNotFound { path: "..." }` -- Fix: the `--schema` path must point to an existing file.
- `Invalid JSON Schema: ...` -- Fix: ensure the schema file is a valid JSON Schema draft.
- Exits with code 1 when validation fails (output still written to stdout).

---

### `tool-loop run`

Iterative tool-augmented LLM loop. The model is called repeatedly until it
converges (stops requesting tool calls) or hits `max_iterations`.

**Input schema (`ToolLoopInput`):**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `prompt` | string | **yes** | Initial user prompt |
| `provider` | string | no | Provider name (default: `loopback` or `$NXUSKIT_PROVIDER`) |
| `model` | string | no | Model identifier. For `loopback`, use a valid loopback model such as `"echo"`; do not rely on the implicit `"default"` model in v1.0.0. |
| `max_iterations` | u32 | no | Maximum loop iterations (default: `10`) |
| `tools` | array of strings | no | Tool adapter names: `"file_reader"`, `"calculator"`, `"web_search"` |
| `tool_configs` | JSON object | no | Per-tool configuration |
| `tool_definitions` | array of JSON objects | no | Function/tool schemas passed to the LLM for function calling |

Built-in tool adapters:

- `file_reader` -- reads a file, expects `{"path": "..."}` arguments
- `calculator` -- evaluates a math expression, expects `{"expression": "..."}` arguments
- `web_search` -- searches the web, expects `{"query": "..."}` arguments (MCP-gated)

**Example:**

```bash
echo '{
  "prompt": "What is 2 + 2?",
  "provider": "loopback",
  "model": "echo",
  "tools": ["calculator"],
  "max_iterations": 5
}' | nxuskit-cli tool-loop run --input - --format json
```

**Common errors:**

- `Invalid tool-loop input: missing field "prompt"` -- Fix: `prompt` is the only required field.
- `Unknown tool adapter: xyz` -- Fix: valid adapters are `file_reader`, `calculator`, `web_search`.
- `Entitlement check failed: mcp` -- Fix: the `mcp` tool adapter is Pro-gated.

---

### `judge select`

LLM-based candidate selection. Sends candidates and criteria to an LLM and
parses a structured JSON response with scores and reasoning.

**Input schema (`JudgeSelectInput`):**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `candidates` | array of `Candidate` | **yes** | Candidates to evaluate |
| `criteria` | string | no | Evaluation criteria description |
| `provider` | string | no | Provider name (default: `"loopback"`) |
| `model` | string | no | Model identifier. Use a configured LLM model; `loopback` echoes the judge prompt and can fail structured parsing. |

**`Candidate` fields:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | **yes** | Unique candidate identifier |
| `content` | string | **yes** | Candidate text to evaluate |

**Example:**

```bash
echo '{
  "candidates": [
    {"id": "a", "content": "The answer is 42."},
    {"id": "b", "content": "The answer is approximately 42.0."}
  ],
  "criteria": "accuracy and conciseness",
  "provider": "your-provider",
  "model": "your-json-capable-model"
}' | nxuskit-cli judge select --input - --format json
```

**Common errors:**

- `Invalid judge select input: missing field "candidates"` -- Fix: `candidates` array is required with at least one entry.
- `Failed to parse judge response as structured JSON: ...` -- Fix: the LLM must return a JSON object with `selected_id`, `reasoning`, and `scores`. The loopback provider may not produce valid judge output; use a real LLM provider for meaningful results.

---

### `branch fork`

Fan out a single prompt to multiple models concurrently.

**Input schema (`BranchForkInput`):**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `prompt` | string | **yes** | Prompt sent to all models |
| `models` | array of strings | **yes** | Model identifiers to invoke in parallel |
| `provider` | string | no | Provider name (default: `"loopback"`) |
| `system` | string | no | System message prepended to each request |

Alternatively, models can be specified via the `--models` CLI flag as a
comma-separated list, which overrides the `models` field in the JSON input.

**Example:**

```bash
echo '{
  "prompt": "Explain recursion in one sentence.",
  "models": ["echo", "echo-json-native"],
  "provider": "loopback"
}' | nxuskit-cli branch fork --input - --format json
```

**Using `--models` flag:**

```bash
echo '{"prompt": "Explain recursion.", "provider": "loopback"}' \
  | nxuskit-cli branch fork --input - --models echo,echo-json-native --format json
```

**Common errors:**

- `Invalid fork input: missing field "prompt"` -- Fix: `prompt` is required.
- `Invalid fork input: missing field "models"` -- Fix: provide `models` in JSON or via `--models` flag.

---

### `branch compare`

Compare results from a previous `branch fork` invocation. Input is the JSON
output of `branch fork` (the `BranchForkResult` object).

**Input schema (`BranchForkResult`):**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `results` | array of `BranchModelResult` | **yes** | Results from `branch fork` |

**`BranchModelResult` fields:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `model` | string | **yes** | Model identifier |
| `content` | string | **yes** | Model's response text |
| `usage` | `{input_tokens, output_tokens, total_tokens}` | no | Token usage |
| `elapsed_ms` | f64 | **yes** | Response latency in milliseconds |

**Output includes:**

- `comparison`: per-model length and optional quality score
- `diffs`: structural differences (content_length, word_count, sentence_count, elapsed_ms, content_similarity for 2-model comparisons)

**Example (piping fork output to compare):**

```bash
echo '{
  "prompt": "Explain recursion.",
  "models": ["echo", "echo-json-native"],
  "provider": "loopback"
}' | nxuskit-cli branch fork --input - --format json \
   | jq '.result' \
   | nxuskit-cli branch compare --input - --format json
```

**Common errors:**

- `Invalid fork result input: missing field "results"` -- Fix: input must be a `BranchForkResult` object (the `result` field from a fork envelope, not the full envelope).

---

### `solver what-if`

Solver what-if analysis is a Pro capability. Public CE builds expose command discovery and entitlement errors only; detailed input/output contracts are published in Pro-labeled documentation.

### `clips session`

Persistent CLIPS sessions survive across multiple eval calls. Session count is
enforced per tier — exit code 4 when the limit is reached.

#### `clips session create`

Create a new CLIPS session, optionally pre-loading rules.

**Input schema (`ClipsSessionCreateInput`):**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `rules` | string | no | CLIPS rule definitions to load into the session |
| `label` | string | no | Human-readable label for the session |

**Example:**

```bash
echo '{
  "rules": "(defrule greet (person (name ?n)) => (printout t (str-cat \"Hello \" ?n) crlf))",
  "label": "greet-session"
}' | nxuskit-cli clips session create --input - --format json
```

**Output:**

```json
{
  "session_id": "sess_abc12345",
  "label": "greet-session",
  "created_at": "2026-04-13T10:00:00Z",
  "rule_count": 1
}
```

**Common errors:**

- `Entitlement check failed: clips_session_limit` (exit 4) -- Fix: destroy an existing session first, or upgrade edition.
- `Failed to load CLIPS rules: ...` (exit 1) -- Fix: check CLIPS syntax.

---

#### `clips session list`

List all active CLIPS sessions.

**No input required.** Pass `--format json` for machine-readable output.

```bash
nxuskit-cli clips session list --format json
```

**Output:**

```json
{
  "sessions": [
    {"session_id": "sess_abc12345", "label": "greet-session", "rule_count": 1, "created_at": "..."},
    {"session_id": "sess_def67890", "label": null, "rule_count": 5, "created_at": "..."}
  ],
  "count": 2
}
```

---

#### `clips session destroy`

Destroy a session and release its resources.

**CLI arguments:**

| Argument | Type | Required | Description |
|----------|------|----------|-------------|
| `--session-id` | string | **yes** | Session ID returned by `clips session create` |

```bash
nxuskit-cli clips session destroy --session-id sess_abc12345 --format json
```

**Output:**

```json
{"session_id": "sess_abc12345", "destroyed": true}
```

**Common errors:**

- `Unknown session ID: sess_abc12345` (exit 5) -- Fix: use `clips session list` to find valid IDs.

---

### `provider list`

List all registered providers with type, status, and capability metadata.

**No input required.**

```bash
nxuskit-cli provider list --format json
```

**Output shape:**

```json
{
  "providers": [
    {
      "name": "openai",
      "type": "llm",
      "status": "available",
      "capabilities": ["streaming", "vision", "tool_calling"],
      "auth_required": true
    },
    {
      "name": "loopback",
      "type": "llm",
      "status": "available",
      "capabilities": ["streaming"],
      "auth_required": false
    },
    {
      "name": "clips",
      "type": "rule_engine",
      "status": "available",
      "capabilities": [],
      "auth_required": false
    }
  ],
  "count": 3
}
```

---

### `provider info`

Show detailed information for a single provider. Accepts fuzzy name matching —
if the name is close but not exact, suggestions are printed to stderr and the
command exits with code 5.

**CLI arguments:**

| Argument | Type | Required | Description |
|----------|------|----------|-------------|
| `<name>` | string | **yes** | Provider name (positional argument) |

```bash
nxuskit-cli provider info openai --format json
```

**Output shape:**

```json
{
  "name": "openai",
  "type": "llm",
  "status": "available",
  "capabilities": ["streaming", "vision", "tool_calling"],
  "auth_required": true,
  "auth_env_var": "OPENAI_API_KEY",
  "default_model": "gpt-4o",
  "supported_formats": ["json", "yaml", "jsonl", "text"],
  "docs_url": "https://platform.openai.com/docs"
}
```

**Common errors:**

- `Unknown provider "opnai". Did you mean: openai?` (exit 5, stderr) -- Fix: use the exact provider name from `provider list`, or let the suggestion guide you.

---

### Error responses

All non-zero exits write a JSON `ErrorEnvelope` to **stderr**. Stdout may be
empty or contain partial output.

```json
{
  "code": "entitlement_denied",
  "message": "This command requires the pro edition",
  "details": {
    "feature": "solver",
    "current_tier": "oss",
    "required_tier": "pro"
  },
  "trace_id": "a1b2c3d4",
  "timestamp": "2026-04-13T10:00:00Z"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `code` | string | Machine-readable error code |
| `message` | string | Human-readable description |
| `details` | object | Optional structured context (tier info, field names, session IDs, etc.) |
| `trace_id` | string | 8-character hex trace ID |
| `timestamp` | string | ISO 8601 UTC timestamp |

**Exit code -> `code` mapping (Level 1 and Level 2):**

| Exit | `code` values |
|------|--------------|
| 0 | (success - no `code`) |
| 1 | `internal_error`, `provider_error`, `engine_error`, `internal` |
| 2 | `timeout`, `idle_timeout` |
| 3 | `auth_failure`, `auth_failed`, `token_expired`, `token_missing` |
| 4 | `entitlement_denied`, `session_limit_reached` |
| 5 | `validation`, `validation_error`, `parse_error`, `unknown_provider`, `unknown_session`, `zen_validate_error`, `zen_test_mismatch`, `zen_test_eval_error` |
| 130 | `cancelled` (SIGINT) |

The exit-code set itself is frozen (FR-001 / Article IV): Level 2 commands like
`zen validate` / `zen test` introduce new `code` strings within exit 5, not new
exit codes.
