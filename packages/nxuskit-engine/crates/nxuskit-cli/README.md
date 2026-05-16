# nxuskit-cli

JSON-first control plane for shell automation, CI, and multi-engine reasoning workflows. Provides machine-facing access to LLM providers, CLIPS rule evaluation, ZEN decision tables, Z3 constraint solving, Bayesian network inference, multi-stage pipelines, and typed artifact composition.

## Building

```bash
# From workspace root
cargo build -p nxuskit-cli --release

# Binary location
target/release/nxuskit-cli
```

## Commands

### Interactive Commands

| Command | Description |
|---------|-------------|
| `chat` | Send a message to an LLM provider (human-facing) |
| `models` | List available models from a provider |
| `capabilities` | Query model capabilities (vision, streaming, function calling) |
| `schema to-json` | Convert CLIPS deftemplate to JSON Schema |
| `schema to-clips` | Convert JSON Schema to CLIPS deftemplate |
| `pipeline validate` | Validate a pipeline definition |
| `pipeline convert` | Convert between JSON and YAML formats |

### Level 1 Commands (Machine-Facing)

All Level 1 commands produce structured JSON/YAML/JSONL output with trace envelopes, stable exit codes, and deterministic entitlement errors.

| Command | Description | Exit Codes |
|---------|-------------|------------|
| `call` | JSON-first LLM invocation with streaming | 0, 1, 2, 130 |
| `clips eval` | CLIPS rule evaluation | 0, 1, 2 |
| `zen eval` | ZEN decision table evaluation (Pro) | 0, 1, 2, 3, 4 |
| `solver solve` | Z3 constraint solving (Pro) | 0, 1, 2, 3, 4 |
| `solver what-if` | What-if analysis with optional comparison (Pro) | 0, 1, 2, 3, 4 |
| `bn infer` | Bayesian network inference | 0, 1, 2 |
| `pipeline run` | Multi-stage pipeline execution | 0, 1, 2 |
| `packet validate` | JSON Schema packet validation | 0, 1, 2, 5 |
| `artifact merge` | Deep-merge JSON artifacts | 0, 1, 2 |
| `artifact summarize` | Artifact field summary | 0, 1, 2 |
| `tool-loop run` | Tool-augmented LLM iteration | 0, 1, 2, 3, 4 |
| `judge select` | LLM-based candidate selection | 0, 1, 2 |
| `branch fork` | Multi-model concurrent invocation | 0, 1, 2 |
| `branch compare` | Compare fork results | 0, 1, 2 |
| `provider list` | List available providers with metadata | 0, 1 |
| `provider info <name>` | Detailed info for a single provider | 0, 1, 5 |
| `clips session create` | Create a persistent CLIPS session | 0, 1, 4 |
| `clips session list` | List active CLIPS sessions | 0, 1 |
| `clips session destroy` | Destroy a CLIPS session by ID | 0, 1, 5 |

### Level 2 Commands

#### `provider list`

List all registered providers with their type, status, and capabilities.

```bash
nxuskit-cli provider list --format json
```

#### `provider info <name>`

Show detailed information for a specific provider. Accepts fuzzy matching — if
`<name>` is close but not exact, suggestions are printed to stderr with exit
code 5.

```bash
nxuskit-cli provider info openai --format json
# Fuzzy example: "opnai" → stderr: Did you mean: openai?
```

#### `clips session create`

Create a persistent CLIPS session that survives across multiple eval calls.
Session count is enforced per tier (exit code 4 when the limit is reached).

```bash
echo '{"rules": "(defrule greet (person (name ?n)) => (printout t ?n crlf))"}' \
  | nxuskit-cli clips session create --input - --format json
# Output: {"session_id": "sess_abc123", ...}
```

#### `clips session list`

List all active CLIPS sessions for the current process.

```bash
nxuskit-cli clips session list --format json
```

#### `clips session destroy`

Destroy a session and release its resources. Returns exit code 5 if the session
ID is not found.

```bash
nxuskit-cli clips session destroy --session-id sess_abc123 --format json
```

#### `solver what-if`

Run a what-if scenario solve. With `--compare`, solves a base problem and an
assumed variant, then outputs a diff of the two results. Pro-gated.

```bash
# Single what-if
echo '{"variables": [...], "constraints": [...], "assumptions": [...]}' \
  | nxuskit-cli solver what-if --input - --format json

# Compare base vs. assumed (Pro)
nxuskit-cli solver what-if --input base.json --compare assumed.json --format json
```

The `--compare` output includes a `diff` field with changed variable values and
objective delta between the base and assumed solutions.

### Shell Completions

Generate shell completion scripts for `bash`, `zsh`, or `fish`:

```bash
# bash
nxuskit-cli completions bash >> ~/.bashrc

# zsh (add to ~/.zshrc, or drop into /usr/local/share/zsh/site-functions/)
nxuskit-cli completions zsh > ~/.zfunc/_nxuskit-cli
echo 'fpath=(~/.zfunc $fpath)' >> ~/.zshrc
echo 'autoload -Uz compinit && compinit' >> ~/.zshrc

# fish
nxuskit-cli completions fish > ~/.config/fish/completions/nxuskit-cli.fish
```

Reload your shell after installing to activate completions.

### Common Flags

| Flag | Description |
|------|-------------|
| `--input`, `-i` | Input file or `-` for stdin |
| `--output`, `-o` | Output file or `-` for stdout |
| `--format`, `-f` | Output format: `json` (default), `yaml`, `jsonl`, `text` |
| `--quiet`, `-q` | Suppress non-essential output |
| `--accept-eula` | Accept the EULA non-interactively (required in CI/CD) |

### Exit Codes

All errors are written to **stderr** as a JSON `ErrorEnvelope` (see below). The
process exit code signals the error category.

| Code | Meaning | Trigger |
|------|---------|---------|
| 0 | Success | Command completed normally |
| 1 | Internal error | Unexpected engine failure, file not found, parse error |
| 2 | Timeout / idle | Request or idle timeout exceeded |
| 3 | Auth failure | Token missing, expired, or invalid |
| 4 | Entitlement denied | Feature requires a higher edition (e.g., Pro) |
| 5 | Validation error | Bad input, unknown provider name, unknown session ID |
| 130 | Interrupted | SIGINT received (Ctrl-C during streaming) |

### ErrorEnvelope

All non-zero exits write a JSON `ErrorEnvelope` to **stderr**:

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
| `code` | string | Machine-readable error code (e.g., `entitlement_denied`, `auth_failure`, `validation_error`) |
| `message` | string | Human-readable description |
| `details` | object | Optional structured context (feature name, tier info, constraint violations, etc.) |
| `trace_id` | string | 8-character hex trace ID, correlates with server-side logs |
| `timestamp` | string | ISO 8601 UTC timestamp of the error |

### Example

```bash
# JSON-first LLM call
echo '{"prompt":"hello","provider":"loopback","model":"echo"}' | nxuskit-cli call --input - --format json

# CLIPS rule evaluation
nxuskit-cli clips eval --input rules.json --format json

# List providers
nxuskit-cli provider list --format json

# Multi-stage pipeline
nxuskit-cli pipeline run --input pipeline.yaml --format jsonl

# Packet validation
nxuskit-cli packet validate --input data.json --schema schema.json

# What-if comparison (Pro)
nxuskit-cli solver what-if --input base.json --compare assumed.json --format json
```

Run `nxuskit-cli --help` for full usage, or `nxuskit-cli <command> --help` for command-specific help.

## Documentation

See [GETTING_STARTED.md](../../../../GETTING_STARTED.md#cli-tool) for detailed usage with examples and environment variable reference.
