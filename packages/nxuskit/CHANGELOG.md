# Changelog

All notable changes to nxuskit will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.9.3] - 2026-04-29

### Added

- **First-class unary chat logprobs**: `ChatRequest::with_logprobs(bool)` and
  `ChatRequest::with_top_logprobs(u8)` builders, plus typed
  `ChatResponse.logprobs` access through `LogprobsData`, `TokenLogprob`, and
  `TopLogprob`.
- **Production activation documentation**: Rust SDK README now points to the
  v0.9.3 production activation flow and offline-first entitlement behavior.

### Changed

- **SDK lockstep versioning**: Rust wrapper metadata is aligned with the
  published SDK `0.9.3` C ABI and release bundles.
- **Backward compatibility coverage**: v0.9.2 chat request fixtures remain
  byte-identical when logprobs fields are omitted.

## [0.7.7] - 2026-02-21

### Added

- **Solver Tier 2 Session API**: Stateful Z3 solver sessions via C ABI
  - **`solver_types` module**: Typed domain types — `VariableDef`, `ConstraintDef`
    (20 variants), `ObjectiveDef`, `SolveResult`, `SolverStats`, `SolverConfig`,
    `SolverCapabilities`, `SessionStatus`, `SolverValue` (Integer/Real/Boolean),
    `SolveStatus` (Sat/Unsat/Optimal/Unknown/Timeout). Full serde round-trip.
  - **`solver` module**: `SolverSession` RAII wrapper with dual-dispatch
    (static-link + dynamic-link). Methods: `create`, `add_variables`,
    `add_constraints`, `set_objective`, `retract`, `push`, `pop`, `solve`,
    `reset`, `variables`, `constraints`, `status`, `capabilities`,
    `num_variables`, `num_constraints`. `Send` but not `Sync`.
  - **`mock_solver` module**: `MockSolverProvider` for deterministic contract
    testing — configurable responses, operation recording, capability flags.
  - 17 FFI function declarations in `ffi.rs` for solver session C ABI.

- **Bayesian Network wrapper** (`bn` module): RAII wrapper for BN C ABI
  - 14 FFI function declarations for BN session lifecycle
  - Dual-dispatch (static-link + dynamic-link) matching solver pattern

## [0.7.6] - 2026-02-19

### Changed

- **Version aligned to SDK lockstep**: nxuskit version now matches the
  workspace/SDK version (0.7.6) per Constitution Article XIII. All SDK
  archive components (nxuskit-core, nxuskit) share the same version.
  Previous independent versioning (0.1.0 → 0.2.0 → 0.3.0) is superseded.
  The version compatibility check in `version.rs` ensures the wrapper
  version matches the loaded SDK binary.

### Previous Releases (independent versioning)

The releases below used an independent version track before lockstep
versioning was adopted.

## [0.3.0] - 2026-02-19

### Added

- **MockProvider** for test isolation without the `libnxuskit` SDK binary.
  Implements `AsyncProvider` with configurable single or sequential responses,
  request recording for assertions, and a builder API (`MockProviderBuilder`).
- **Builder pattern** for `ChatRequest`: `ChatRequest::new("model")` with
  chainable `.with_message()`, `.with_temperature()`, `.with_max_tokens()`,
  `.with_top_p()`, `.with_stop()`, `.with_thinking_mode()`, and
  `.with_provider_options()` methods.
- **Factory methods** for `Message`: `Message::user()`, `Message::system()`,
  `Message::assistant()` for concise message construction.
- **License key field** on `ProviderConfig`: optional `license_key` field
  serialized to JSON when present, omitted when `None`. Passed through to the
  C ABI SDK for tiered feature access.

## [0.2.0] - 2026-02-18

### Added

- `AsyncProvider` trait with `chat()`, `chat_stream()`, and `list_models()`.
- `NxuskitProvider` implementation with dynamic library loading.
- Streaming support via `StreamReceiver` iterator.
- Synchronous wrappers: `chat()`, `chat_stream()`, `list_models()`.
- Concurrent async requests with `Arc<NxuskitProvider>`.
- Model discovery for local providers (Ollama, LM Studio).
- Typed errors: `NxuskitError` with `Configuration`, `Provider`, `Internal`,
  `LibraryNotFound`, `VersionMismatch` variants.

## [0.1.0] - 2026-02-17

### Added

- Initial release with `ProviderConfig`, `ChatRequest`, `ChatResponse`,
  `Message`, and core type definitions.
- Serde-based JSON serialization for FFI boundary crossing.
- `ThinkingMode` enum for extended thinking support.
