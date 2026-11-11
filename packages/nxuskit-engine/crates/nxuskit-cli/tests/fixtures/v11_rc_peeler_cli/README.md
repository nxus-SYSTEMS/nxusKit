# v1.1.0 RC Peeler CLI Fixture Inventory

This directory holds the public-safe fixtures used by feature 106
(v1.1.0 RC Peeler CLI update). Every fixture is checked against the
public-boundary deny-list contract.

## Inventory

| File | Story | Purpose |
|---|---|---|
| `v11-llm-only.json` | US2 | Minimal v1.1 `PipelineDefinition` with a single loopback LLM stage. |
| `v11-fanout.json` | US3 | v1.1 stage with `metadata.fan_out` carrying two loopback pairs. |
| `v11-tool-external.json` | US3 | `tool.emit_artifact` external-only stage. |
| `v11-substrait-only.json` | US3 | Substrait plan reference → `declaration_only`. |
| `v11-arrow-missing.json` | US3 | Arrow runtime reference → `missing_plugin`. |
| `v11-zenoh-ref.json` | US3 | Zenoh transport reference → `deferred`. |
| `v11-clips-orchestration.json` | US3 | CLIPS orchestration request → `unsupported_stage_type`. |
| `v11-clips-rule-eval.json` | US3 | CLIPS `rule_eval.generic` rules-provider stage (executes). |
| `v11-orchestration.json` | US6 | Declaration-only `OrchestrationDefinition`. |
| `examples-manifest.json` | US8 | `ExamplesManifest` with public, private, credential, and PHI entries to exercise redaction. |

## Public-boundary rules (applied to every fixture)

- No private repository identifiers or internal persona identifiers.
- No engine-internal routing/control types, internal-channel topology,
  plugin paths, library filenames, or Zenoh route identifiers.
- No PHI examples (SSN, MRN, ICD/CPT codes, persona name leaks).
- No `shell://` execution payloads beyond external-action requests.

These fixtures are read-only by tests; they are NOT consumed at
runtime by the CLI binary unless a test invokes the binary against
them explicitly.
