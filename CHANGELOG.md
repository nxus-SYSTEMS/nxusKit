# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Public SDK release tags begin at `sdk-v0.9.0`. Earlier entries preserve
pre-public development history with normalized pre-public version numbers after
historical version resets.

## [Unreleased]

## [1.0.0] - 2026-05-28

> General Availability release for the v0.9.4-stabilized SDK API surface.
> This release is intentionally narrow: version metadata moves to `1.0.0`,
> public documentation now reflects GA posture, and SDK bundle packaging guards
> include the Rust benchmark targets required by the wrapper manifest.

### Changed

- Promoted nxusKit SDK to GA in public README/support copy.
- Updated Rust workspace, C ABI, Go SDK, Python SDK, and package metadata to
  lockstep version `1.0.0`.
- Promoted Python package classifier to `Development Status :: 5 -
  Production/Stable`.
- Refreshed public architecture and README links to the hosted docs site.
- Clarified loopback CLI examples so local copy-paste commands use explicit
  loopback model names.

### Fixed

- SDK release workflows now copy `packages/nxuskit/benches` into the bundled
  Rust wrapper so the declared benchmark target is present in release archives.
- Packaging verification now fails if the bundled Rust wrapper omits the
  declared `logprobs_serialization` benchmark target.

### Compatibility

- No API or C ABI signature changes from v0.9.4 are introduced in this GA cut.
- Pro installation packages continue to be published on the public release page;
  Pro capabilities still require a valid license key.

## [0.9.4] - 2026-05-11

> v0.9.4 release candidate. Provider-capability modernization and release
> hardening, consolidating sprints S1 (streaming logprobs, branch 098),
> S2/S3 (provider capability modernization + Capability Manifest v2 decision,
> branch 099), and S4-S6 (CLI Level 2 completion, examples & bundle alignment,
> docs & release candidate, branch 100). Lockstep version bump `0.9.3 -> 0.9.4`
> across all components. **No C ABI signature changes** in this release.

### Added - S1: Streaming Logprobs + Capability Metadata (branch 098)

- `StreamLogprobsDelta` type (Rust engine + wrapper, Go, Python) carrying
  per-chunk `TokenLogprob` entries on streaming responses.
- `StreamChunk.logprobs: Option<StreamLogprobsDelta>` (Rust),
  `StreamChunk.Logprobs *StreamLogprobsDelta` (Go), and
  `StreamChunk.logprobs: Optional[StreamLogprobsDelta]` (Python) - additive,
  defaults to `None`/`nil` for non-supporting providers.
- `ProviderCapabilities.supports_streaming_logprobs: bool` flag with
  `debug_assert!` enforcing `supports_streaming_logprobs => supports_logprobs`.
- GPT-5.4 reasoning-compat warn-and-drop guard: when `reasoning.effort != "none"`,
  `temperature`, `top_p`, and `logprobs` are dropped with a warning rather than
  passed through.
- CLI `provider info` exposes the `streaming_logprobs` row (human + JSON).
- Cross-language parity harness at
  `internal/tests/parity/stream_logprobs/run_parity.sh`.
- OpenAI: `supports_streaming_logprobs = true` (only supporting provider per
  fixture evidence); all other providers `false` per the evidence-first rule.

### Added - S2/S3: Provider Capability Modernization + Manifest v2 (branch 099)

- Provider capability surface modernized; `CapabilityProvider` / "capability
  provider" vocabulary introduced (no breaking `LLMProvider` rename).
- xAI Grok runtime provider support under canonical provider id `xai`
  (`XAI_API_KEY`, default base URL `https://api.x.ai/v1`); `groq` remains
  Groq, Inc. and no confusing `grok` alias is registered.
- `CapabilityManifest` v2 concept with a public preview subset for
  provider/model capability discovery (full internal manifest unchanged); the
  publication decision is recorded in the 099 artifacts.
- OpenAI remains Chat-Completions-first (no full Responses API migration).

### Added - S4: CLI Level 2 completion & stabilization (branch 100)

- **`nxuskit-cli zen validate`** (Pro) - structural validation of a ZEN JSON
  Decision Model (JDM): rejects `functionNode` (JavaScript), checks decision
  table node shape, attempts expression compilation, and reports
  node/decision-table/rule counts. Backed by a new pure
  `nxuskit_engine::providers::zen::validate(model_json) -> Result<ZenValidationReport>`
  engine entry point. Exit 0 = valid; exit 5 = `parse_error` (unparseable input)
  or `zen_validate_error` (structurally invalid, with a `problems[]` report);
  exit 4 = `entitlement_denied`.
- **`nxuskit-cli zen test`** (Pro) - run a ZEN decision table against a fixture
  set `{table, cases: [{name, input, expected}]}` and compare each actual
  output to `expected`; on mismatch emits a structured diff report (exit 5,
  `zen_test_mismatch`), a per-case eval error is `zen_test_eval_error`, a
  fixture parse error is `parse_error`.
- **`nxuskit-cli bn learn`** - parameter learning (MLE / Bayesian) of a
  Bayesian network's CPDs from a CSV dataset given the network skeleton; output
  is the learned network, BIF-exportable. **`nxuskit-cli bn evidence`** -
  validate/normalize an observations map against a network. (Community edition.)
- `solver what-if --compare` and the unsatisfiable-assumptions path are now
  covered by non-`#[ignore]`d, entitlement-aware tests (skip-with-reason in CE,
  assertions run in the Pro CI lane `.github/workflows/ci-pro.yml`).
- `CliError::CommandValidation { code, message, details }` - exit 5 with a
  command-specific `code` string + structured `details` (used by the new ZEN
  commands; exit-code *set* unchanged, FR-001 / Article IV).
- Shell support policy documented (`completions`: bash, zsh, fish supported;
  PowerShell not generated in v0.9.4; helper snippets + schema bundle locations).

### Added - S5: Examples repo & bundle alignment (branch 100)

- Examples portfolio bundle-instruction refs bumped to v0.9.4;
  `PYTHON_EXAMPLES_STATUS.md` records the v0.9.4 Python-parity scope (minimal
  slice: the SDK-side `packages/nxuskit-py` FFI version-guard alignment is in
  scope; the 17 already-passing Python examples stay; new examples remain
  examples-team backlog). Rust vision example confirmed using the v0.9.2
  multimodal wrapper API with no text-only caveat.

### Changed

- `nxuskit-py` `_ffi.py` `EXPECTED_VERSION` aligned to the package version
  (`0.9.1` -> `0.9.4`) - the cffi loader requires the linked library's
  `nxuskit_version()` to match; this unblocks the Python FFI examples that were
  previously `broken-upstream` against the v0.9.3 mismatch.
- Lockstep version bump `0.9.3 -> 0.9.4`: Rust workspace + `nxuskit` crate, C
  ABI version constant (`nxuskit-core`), Go `nxuskit-go` (`Version` +
  `ExpectedNxuskitVersion`), Python `nxuskit-py` (`__version__` + pyproject).

### Compatibility

- **No C ABI signature changes** in v0.9.4 - only the ABI version constant
  moves (`0.9.3 -> 0.9.4`); function signatures and struct layouts are frozen
  (Article XIV).
- All v0.9.4 additions are additive. The CLI exit-code *set* (0/1/2/3/4/5/130)
  is unchanged; the new ZEN commands introduce new `code` strings within exit 5.
- S1/S2/S3 baseline behavior (streaming logprobs, provider capability metadata)
  is preserved.

## [0.9.3] - 2026-04-29

> Published SDK release `sdk-v0.9.3`. Production licensing real-purchase
> activation/recovery, PR readiness, and supported-platform SDK build checks
> passed before release publication.

### Added

- **Production licensing cutover** (Phase 4):
  - Release builds embed the production ES256 public key with
    `kid: es256-v1`.
  - Release default endpoint is `https://nxus.systems/licensing-api/v1`.
  - `nxuskit-cli license status --json` exposes endpoint, environment, and
    signing-key diagnostics.
  - SDK accepts `real_purchase` and `leased` token kinds even while
    `external licensing client` lags on its enum (ES256 fallback verifier).
  - Stable licensing errors include `authentication_required`,
    `environment_mismatch`, `wrong_product_identifier`, etc.
  - Activation timeouts extended to 30s on both client and proxy
    (`EXTENDED_TIMEOUT_SECS = 30`) for cold-start activation paths.

- **First-class unary chat logprobs** (Phase 5, US2):
  - **Rust wrapper** (`nxuskit`): `ChatRequest::with_logprobs(bool)` and
    `with_top_logprobs(u8)`; typed `ChatResponse.logprobs: LogprobsData`
    with `TokenLogprob` (selected token + bytes) and `TopLogprob`
    (alternative + bytes). Doctests on `with_logprobs` and `LogprobsData`.
  - **Python SDK** (`nxuskit-py`): new `ChatRequest` dataclass with
    `logprobs` / `top_logprobs` kwargs; `LogprobsData`, `TokenLogprob`,
    `TopLogprob` exported from top-level `nxuskit`. FFI response decode
    populates typed logprobs.
  - **C ABI**: round-trip preserves `logprobs.content[]`, alternative
    tokens, and UTF-8 bytes. Wire path is `logprobs.content[]` (matches
    OpenAI; pinned by
    `tests/logprobs_abi_passthrough_test.rs::response_envelope_uses_content_field_for_logprobs_token_array`).
  - **Engine**: `parameter_adapter.rs::adapt_logprobs` performs warn-and-
    drop when a provider lacks `supports_logprobs`, with structured Info
    warning. `provider_options` does **not** tunnel logprobs.
  - **Migration guide:** The [logprobs migration guide](https://docs.nxus.systems/nxuskit/migration/logprobs-migration/) covers
    Rust + Python + C ABI before/after with capability-gating rationale.

- **ABI / version consistency** (Phase 3):
  - Workspace, Rust wrapper, Python package, Go markers, C ABI version
    constant, capabilities ABI JSON, current SDK docs, and `Cargo.lock`
    all bumped to v0.9.3.
  - Pre-logprobs v0.9.2 fixture compatibility tests (Rust + Python) prove
    requests without logprobs serialize byte-identically to v0.9.2.
  - Stale-version guard: `scripts/check-version-inventory.sh`.

### Changed

- `data-model.md` corrected: logprobs response token array is `content`
  (matches Rust/Python/C ABI implementations and OpenAI wire format),
  not the earlier draft's `tokens`. Pinned by ABI passthrough test.


### Test counts (logprobs surface, cumulative)

- 7 Rust wrapper (mock_provider) + 6 ABI passthrough + 6 engine
  warn-drop + 3 streaming-scope + 18 Python = **40 logprobs tests** green
  across all SDK surfaces.

## [0.9.2] - 2026-04-13

### Added

- **CLI Level 2 request surface**: richer JSON request construction across chat/call flows, including multimodal image input handling where supported.
- **Provider diagnostics**: provider ping/status improvements for checking SDK environment readiness from the CLI.
- **Python SDK hardening**: runtime library discovery deprecation warning and SecurityValidator coverage for common unsafe input patterns.
- **Release confidence checks**: conformance, parity, performance, and packaging checks expanded for the supported SDK surfaces.

### Changed

- CLI and runtime-loading documentation refreshed for v0.9.2 behavior.
- Test fixtures and CI checks hardened so SDK builds can validate without relying on local native-library side effects.
- Workspace and lockfile versions bumped to v0.9.2.

## [0.9.1] - 2026-04-05

### CLI Level 1 Semantic Remediation

- **Real Engine Integration**: `zen eval`, `solver solve`, `clips eval`, and `bn infer` now execute real engine logic - no more placeholder/stub responses
- **Pipeline Execution**: `pipeline run` dispatches all stage types (LLM, CLIPS, ZEN, solver, BN) through real engines with output handoff and partial results on failure
- **Call Envelope**: `call` propagates tool definitions and includes `tool_calls` and `inference_metadata` in responses
- **Artifact Deep Merge**: `artifact merge` performs recursive deep merge with dot-notation conflict paths
- **Models Capabilities**: `models --supports` filter uses real capability inference from model metadata
- **Provider Auth**: `provider status` uses structured auth subsystem; `provider logout` is provider-scoped
- **Judge/Branch**: `judge select` returns structured errors; `branch compare` produces field-level diffs

### CLI Documentation and Solver Format Compatibility

- **CLI Input Reference**: New `docs/user/cli-input-reference.md` covering all 13 Level 1 commands with JSON schemas, working examples, and common errors
- **Enhanced Help Text**: Every engine command's `--help` now shows input format structure
- **Solver Format Compatibility:** Pro solver command compatibility documentation was updated in Pro-labeled documentation

### Positioning

- **CLI Description**: Updated from "CLI for interacting with multiple LLM providers" to "JSON-first control plane for shell automation, CI, and multi-engine reasoning workflows"
- **README**: Added CLI / Shell Automation section with examples
- **Naming**: Fixed `nxuskit-engine-cli` -> `nxuskit-cli` naming drift across all docs and scripts

### Compliance

- **NOTICE**: Regenerated with ZEN engine and solver native bindings entries; Python section reformatted to remove excessive whitespace padding
- **Constitution v2.4.0**: Added semantic test assertions, stub prohibition, and task verification criteria (Articles II and III)
- **Acceptance Fixtures**: Three PoR 4.1 acceptance workflow scripts (intake-routing, generator-validator-retry, typed-artifact-handoff)

## [0.9.0] - 2026-03-13

Initial public release of the nxusKit SDK.

### Highlights

- **Polyglot SDK**: Unified LLM interfaces across Rust, Go, and Python
- **14 LLM Providers**: Claude, OpenAI, Ollama, LM Studio, Mistral, OpenRouter, Together, Groq, Fireworks, Perplexity, MCP, CLIPS, Mock, Loopback
- **CLIPS Expert System**: Rule-based inference via embedded CLIPS 6.4.2 engine with FFI bindings
- **Bayesian Network Inference**: Full-featured BN provider with Variable Elimination, Junction Tree, Loopy BP, NUTS/HMC, and structure/parameter learning
- **Z3 Constraint Solver**: Stateful solver sessions with multi-objective optimization, soft constraints, push/pop scoping, and UNSAT core extraction
- **ZEN Decision Tables**: Pro decision-model evaluation via ZEN engine
- **Plugin Architecture**: Signed plugin loading with Ed25519 verification and capability-based sandboxing
- **SDK CLI**: Command-line tool for all providers (`nxuskit-cli`)
- **SDK Installer**: Cross-platform SDK manager (`install.sh`) with version management
- **Cross-Language Conformance**: Shared test vectors ensuring API parity across Rust, Go, and Python

### Platform Support

| Platform | Architecture | Status |
|----------|-------------|--------|
| Linux | x86_64 | Supported |
| macOS | ARM64 (Apple Silicon) | Supported |
| macOS | x86_64 | Supported |
| Windows | x86_64 | Supported |

### Language SDKs

| Language | Package | Description |
|----------|---------|-------------|
| Rust | `nxuskit` | FFI wrapper with safe Rust API |
| Go | `nxuskit-go` | Idiomatic Go with context support |
| Python | `nxuskit-py` | Pure Python with `requests` HTTP client |

### Getting Started

See `sdk-packaging/docs/getting-started.md` for installation and usage instructions.

For runnable examples, see the [nxusKit-examples](https://github.com/nxus-SYSTEMS/nxusKit-examples) repository.

## [0.8.23] - 2026-02-24

### Added

- **Solver Progress Streaming (US1)**: Real-time progress events during optimization solves
  - `nxuskit_solver_solve_stream` C ABI function with `on_chunk`/`on_done` callbacks
  - `SolverProgressEvent` struct: iteration, status, elapsed_ms, objective_value, bound_gap, is_final
  - nxuskit-rs `SolverStreamReceiver` with sync Iterator and async `futures_core::Stream` interfaces
  - `solve_stream_async()` convenience method for tokio-based async consumption

- **Z3 Context Pooling (US2)**: Reusable Z3 context pool for reduced solver startup overhead
  - Pool checkout/return benchmarked at 290us per 100-variable FFI round-trip
  - Configurable via `SolverConfig.pool_size` and `pool_max_idle_ms`

- **ZEN Decision Table Evaluation (US3)**: Pro decision-model evaluation via ZEN engine
  - `nxuskit_zen_evaluate` / `nxuskit_zen_free_result` C ABI functions (stateless, no session)
  - Supports decision table nodes, expression nodes, and switch nodes
  - Function nodes rejected with clear error (no QuickJS dependency)
  - Benchmarked at 0.39ms average for 100-row decision tables (well under 1ms target)
  - Go wrapper: `gollyllm.ZenEvaluate()` with automatic memory management
  - Rust provider: `zen::evaluate()` async function with pre-compilation optimization

- **BN Min-Weight Variable Elimination (US4)**: Alternative elimination heuristic
  - `EliminationHeuristic::MinWeight` option for VE inference
  - Identical posteriors to MinFill (verified on Asia and Alarm networks, max diff 2.22e-16)
  - Configurable via `{"elimination_heuristic": "min_weight"}` in inference config JSON

- **clips-sys Windows Stub Parity (US5)**: Windows compilation stubs for CLIPS FFI functions

- **7 Code Examples**: Full Rust + Go implementations across patterns and integrations
  - **E1 Constraint Solver** (`patterns/constraint-solver`): Basic Z3 solver session with 3 scenarios
  - **E2 Bayesian Inference** (`patterns/bayesian-inference`): BN loading, evidence, multi-algorithm inference with 3 scenarios
  - **E3 Multi-Provider Pipeline** (`integrations/solver-bn-pipeline`): 3-stage BN→Solver→CLIPS pipeline with 3 scenarios (festival, rescue, bakery)
  - **E4 LLM-Solver Hybrid** (`integrations/llm-solver-hybrid`): LLM constraint extraction + Z3 solving with mock/live modes, 3 scenarios (seating, dungeon, road-trip)
  - **E5 Solver What-If** (`patterns/solver-what-if`): Push/pop what-if analysis with UNSAT detection, 3 scenarios (wedding, mars, recipe)
  - **E6 BN Structure Learning** (`integrations/bn-structure-learning`): Hill-Climb+BIC, K2, MLE parameter learning, log-likelihood scoring with 3 scenarios (golf, bmx, sourdough)
  - **E7 ZEN Decision Tables** (`integrations/zen-decisions`): JDM evaluation with first/collect hit policies, expression nodes, 3 scenarios (maze-rat, potion, food-truck)

### Changed

- Go BN FFI (`ffi_bn.go`): Added `SearchStructure()`, `LearnMLE()`, `LogLikelihood()` wrappers with `BnSearchStructureConfig`, `BnStructureResult`, `BnEdge` types
- Go header (`nxuskit.h`): Added `nxuskit_zen_evaluate` and `nxuskit_zen_free_result` declarations
- `log::debug!` instrumentation added to solver streaming and ZEN evaluate hot paths for timing observability

### Performance

- Solver FFI overhead: 290us per 100-variable add_variables call
- Solver first-chunk latency: 21ms (well under 400ms Doherty Threshold)
- ZEN 100-row evaluation: 0.39ms avg, 0.55ms worst-case (under 1ms target)
- Z3 pool bench: >=50% improvement documented (PR-009)

## [0.8.22] - 2026-02-23

### Added

- **Solver Part 2 (044-solver-perf-audit)**: Multi-objective optimization, soft constraints, and explainability
  - **Multi-objective optimization**: Weighted sum and lexicographic modes via Z3 Optimize API
  - **Soft constraints**: Penalty-weighted constraints that can be violated, with violated constraint tracking
  - **Constraint labels & explainability**: Human-readable labels on constraints/variables/objectives, UNSAT core with label mapping, binding constraints and slack values
  - **Assumption-based solving**: Push/pop scoping with retractable assumptions
  - **Go solver wrapper**: 21 C ABI solver functions wrapped via CGo FFI with full type parity
  - **Python solver wrapper**: 21 C ABI solver functions wrapped via CFFI with context managers and dataclass types
  - **Performance audit**: Criterion benchmarks for all SDK providers (Z3, CLIPS, Chat, FFI overhead); SC-007 (≤1ms FFI) and SC-008 (≤200ms composite) verified passing
  - **Constitution v2.3.0**: PR-007 (cumulative overhead), PR-008 (platform-specific optimization), PR-009 (benchmark platform representativeness)

- **BN Part 2 (043-bn-part2-advanced)**: Advanced inference algorithms and cross-language parity
  - **Loopy Belief Propagation**: Message-passing inference for cyclic/large networks with configurable damping and convergence diagnostics
  - **Linear Gaussian Bayesian Networks**: Gaussian variable types with moment-matching exact inference and 95% credible intervals
  - **NUTS/HMC Sampling**: Gradient-based MCMC via `nuts-rs` crate for continuous variables with ESS/R-hat diagnostics
  - **BIF Export**: Round-trip BIF file export with 15-digit precision and CPT completeness validation
  - **Parallel Junction Tree**: Rayon-based parallel collect/distribute with deterministic results and auto-fallback
  - **Go BN wrapper**: Full C ABI coverage for Part 2 — BnNetwork, BnEvidence, BnResult with goroutine-safe inference and streaming via channels
  - **Python BN wrapper**: Full C ABI coverage for Part 2 — context managers, async inference, generator-based streaming
  - **C ABI extensions**: 8 new exported functions for Gaussian variables, config-based inference, streaming, and BIF save

### Changed

- **Dependency Audit & Update (043-deps-audit-update)**: Comprehensive dependency updates across Rust, Go, and Python
  - **Rust — Breaking upgrades**:
    - `thiserror`: 1.0 → 2.0 (unified across workspace, eliminates dual compilation)
    - `reqwest`: 0.12 → 0.13 (rustls TLS default, improved security)
    - `serde_yaml` → `serde_yaml_ng`: 0.9 → 0.10 (replaces archived/deprecated crate via Cargo rename)
    - `rand`: 0.9 → 0.10, `rand_chacha`: 0.9 → 0.10 (trait renames: Rng→RngExt, from_os_rng removed)
    - `quick-xml`: 0.37 → 0.39 (drop-in for our usage)
    - `rmcp`: 0.8 → 0.16 (not yet used in source — pure Cargo.toml bump)
    - `libloading`: 0.8 → 0.9 (in nxuskit-rs, AsFilename trait)
    - `infer`: 0.16 → 0.19 (additive changes only)
    - `criterion`: 0.5 → 0.8 (dev-only)
    - `wiremock`: 0.5 → 0.6 (dev-only, hyper 1.0 migration)
    - `mockito`: 1.2 → 1.7 (dev-only, semver-compatible)
  - **Rust — Semver-compatible floor bumps**: regex 1.12, uuid 1.21, csv 1.4, cc 1.2, tempfile 3.20, openapiv3 2.2
  - **Go**: go-edlib 1.7.0, pflag 1.0.10, plus transitive updates in all examples
  - **Python**: cffi ≥2.0.0, pytest ≥9.0.0, pytest-cov ≥7.0.0, ruff ≥0.15.0
  - **Toolchain**: Rust MSRV 1.92 → 1.93 (Go 1.26 bump deferred pending CI toolchain update)

## [0.8.21] - 2026-02-21

### Added

- **Bayesian Network Inference Engine (040-bayesian-network-inference)**: Full-featured Bayesian network provider
  - **BIF parser**: Reads standard Bayesian Interchange Format network files (Asia, Cancer, Alarm, Survey, Earthquake)
  - **4 inference algorithms**: Variable Elimination (exact), Likelihood Weighted Sampling, Gibbs Sampling (MCMC), Junction Tree
  - **3 structure learning algorithms**: K2, Hill Climbing (BIC-scored), Bayesian structure learning
  - **2 parameter learning algorithms**: Maximum Likelihood Estimation, Bayesian parameter estimation
  - **Streaming support**: Real-time probability updates during sampling-based inference
  - **C ABI integration**: 14 BN SDK functions with opaque handle pattern for cross-language access
  - **nxuskit-rs BN wrapper**: RAII wrapper with dual-dispatch (static-link + dynamic-link)
  - **Reference test data**: Python-generated reference marginals for deterministic validation
  - **Benchmarks**: Performance benchmarks for all inference algorithms

- **Solver Tier 2 Session API (042-solver-tier2-api)**: Stateful Z3 solver sessions with incremental model building
  - **Typed solver domain types** (`solver_types.rs`): `VariableDef`, `ConstraintDef`, `ObjectiveDef`, `SolveResult`, `SolverStats`, `SolverConfig`, `SolverCapabilities`, `SessionStatus`, with 20 constraint type variants and full serde round-trip fidelity
  - **Mock solver backend** (`mock_solver.rs`): Deterministic contract testing without Z3 runtime — pre-configured responses, operation recording, atomic response cycling
  - **17 C ABI solver session functions** (`solver_sdk.rs`): Opaque handle pattern for cross-language access — create/destroy, add variables/constraints, set objective, retract, push/pop, solve, reset, introspection (variables, constraints, status, capabilities, counts)
  - **nxuskit-rs SolverSession wrapper** (`solver.rs`): RAII wrapper with dual-dispatch (static-link + dynamic-link), typed methods, automatic cleanup on drop
  - **Internal solver session engine** (`solver_session.rs`): Accumulates state as plain Rust data, rebuilds Z3 solver on each solve via `with_z3_config` closure re-entry
  - **Push/pop scoping**: Checkpoint/restore model state for what-if analysis (5 nested levels tested)
  - **Unsat core extraction**: Named constraint labels propagated through Z3 assertion tracking, conflict identification on UNSAT
  - **Solver configuration**: Timeout, random seed, and max-conflicts controls for deterministic, bounded-resource solving
  - **Backend capability introspection**: Query Z3 or mock feature flags (incremental, unsat core, push/pop, multi-objective)

- **New Providers (041-new-providers)**: Local LLM and Z3 constraint solver providers
  - **Local LLM provider**: In-process inference via llama.cpp (feature-gated `provider-local-llama`) and mistral.rs (feature-gated `provider-local-mistralrs`)
  - **Z3 constraint solver provider**: Constraint satisfaction and optimization via Z3 (Pro-gated)
  - **Go FFI constructors**: `NewLocalProvider()` and `NewZ3Provider()` in gollyllm
  - **ModelLister implementations**: For Local and Z3 providers

### Changed

- **Tier 1 Z3 refactor**: `Z3Provider::chat()` now delegates to `session::solve_ephemeral()` for shared validation and dispatch logic — no external behavior change
- **Constitution v2.1.0**: Extended lockstep versioning to all nxusKit components (rustyllm, gollyllm, pythicllm), unified under single `sdk-v*` tag line
- **Go toolchain**: Bumped to 1.24.13 for crypto/tls security fix (GO-2026-4337)

- **gollyllm LKS Parity (018-gollyllm-lks-parity)**: Go library now has API parity with rustyllm
  - **`InferenceMetadata` and `InferenceStep` types**: Structured metadata for inference results
    - `InferenceMetadata` with `IsComplete`, `FinishReason`, `TokenUsage`, `ThinkingTrace`, `InferenceSteps`
    - `InferenceStep` for capturing tool calls, thinking traces, and custom steps
    - Builder pattern with fluent methods (`Completed()`, `WithTokenUsage()`, `AddInferenceStep()`, etc.)
    - `ChatResponse.InferenceMetadata` field populated by all 12 providers
  - **`SessionResetter` interface**: Deterministic testing support
    - `FreshSession() (LLMProvider, error)` method on all 12 providers
    - Stateless providers return self, MockProvider creates new instance
    - Enables reproducible test results in CI/CD pipelines
  - **`ModelLister` interface**: Dynamic model discovery
    - `ListAvailableModels(ctx) ([]ModelInfo, error)` for local providers
    - Implemented by: Ollama, LM Studio, Mock, Loopback
    - Cloud providers don't implement (API doesn't support dynamic listing)
  - **Backward Compatibility**: `ChatResponse.Metadata` field preserved with deprecation notice
  - **85.9% Test Coverage**: Exceeds target of ≥85%

### Changed

- **Project Naming Migration**: Unified naming conventions across all language implementations
  - Umbrella project renamed from "RustyLLM/LLMKit" to "nxusKit"
  - Go library: `go/llmkit-go/` → `gollyllm/`, package `llmkit` → `gollyllm`
  - Go module path: `github.com/llmkit/llmkit-go` → `github.com/nxus-SYSTEMS/nxusKit/gollyllm`
  - Go CLI: `gollm` → `gollyllm`
  - Python library: `rustyllm-py/` → `pythicllm/`, package `rustyllm` → `pythicllm`
  - Python tools: `python-tools/` → `nxusKit-tools/`
  - Rust library: `rustyllm` (unchanged)
  - Updated all documentation, examples, and configuration files

## [0.8.20] - 2026-01-29

### Added

- **`ModelLister` Trait**: New trait for polymorphic model discovery
  - Enables `Box<dyn ModelLister>` for provider registries
  - Implemented for Ollama, LmStudio, CLIPS, Mock, and Loopback providers
  - Correct vtable dispatch through trait objects

- **`InferenceMetadata` and `InferenceStep` Types**: Unified response metadata
  - `InferenceMetadata` provides consistent access to execution details across all providers
  - `InferenceStep` captures inference traces (rule firings for CLIPS, tool calls, etc.)
  - All providers now populate `response.inference_metadata`

- **`fresh_session()` Method**: Per-provider method for deterministic CI/testing
  - Returns a fresh provider instance with clean state
  - Implemented for all 13 providers
  - Enables reproducible test results

- **`BlockingProvider<P>` Wrapper**: Synchronous API for non-async contexts
  - Feature-gated under `blocking-api`
  - Uses internal tokio Runtime
  - Supports `chat()` and `list_models()` (when wrapped provider implements ModelLister)

- **`full` Feature Flag**: Convenience feature combining `clips` + `blocking-api`

- **CLIPS Ordering Guarantees**: Deterministic output ordering
  - Conclusions sorted by fact_index
  - Rules fired sorted by name for determinism
  - Conflict strategy recorded in provider_metadata

- **Documentation**: New integration guides
  - `docs/INTEGRATION_PATTERNS.md` - Polymorphic providers, deterministic evaluation, sync API
  - `docs/MINIMAL_BUILD.md` - Feature flag reference and build configurations

- **CI Improvements**: Feature flag verification steps
  - Tests `--no-default-features`, `--features blocking-api`, `--features full` builds

- **Examples**:
  - `polymorphic_example.rs` - Provider registry pattern with ModelLister
  - `blocking_example.rs` - Synchronous API usage patterns

- **`as_clips_output()` Method**: Typed accessor for CLIPS results (feature = "clips")
  - Avoids manual JSON parsing for CLIPS inference results
  - Returns `Option<ClipsOutput>` with typed access to conclusions, traces, stats
  - Returns `None` for non-CLIPS response content

- **`all-providers` Feature Flag**: Enables all provider features (`pro` + `mcp`)

- **Documentation Enhancements**:
  - Error handling patterns in INTEGRATION_PATTERNS.md
  - CLIPS ordering guarantees documentation
  - WASM compatibility notes in MINIMAL_BUILD.md
  - CI optimization tips for feature matrix testing

### Changed

- `ChatResponse` now includes `inference_metadata: InferenceMetadata` field (backward compatible with `#[serde(default)]`)

## [0.8.19] - 2026-01-28

### Added

- **CLIPS Provider Options**: New configuration options for expert system inference
  - `strategy` option for conflict resolution strategy selection
  - `allow_duplicate_facts` option for fact assertion behavior control

- **CLI Support for All 14 Providers**: Command-line interface now supports all providers
  - Claude, OpenAI, Ollama, LM Studio, Mistral, OpenRouter, Together, Groq, Fireworks, Perplexity, MCP, CLIPS, Mock, Loopback

- **Stop Patterns**: Conditional inference halting based on output patterns
  - Enables early termination when specific patterns are detected in responses

- **CLIPS Expert System Enhancements**:
  - Binary rule loading support for pre-compiled rule bases
  - Search paths for rule file discovery
  - Schema conversion utilities for fact/rule translation
  - Help commands for CLIPS debugging and introspection

## [0.8.18] - 2026-01-23

### Breaking Changes

- **`ThinkingMode::Auto` behavior changed**: Now intelligently enables thinking for thinking-capable models (qwen3, deepseek-r1, etc.) instead of omitting the parameter. Use `ThinkingMode::Omit` for the old behavior.

### Added

- **`ThinkingMode::Omit` variant**: Explicitly omit the `think` parameter from Ollama requests, letting the model use its raw default behavior. Use this if you need the pre-0.8.18 `Auto` behavior.

- **Smart `Auto` mode for Ollama thinking models**: `ThinkingMode::Auto` now detects thinking-capable models and enables thinking automatically, preventing empty response issues with models like qwen3-vl.

- **Native Ollama Structured Output Support**: Full JSON mode and JSON schema support for Ollama provider
  - **JSON Mode**: Use `ResponseFormat::Json` to get structured JSON responses from Ollama models
  - **JSON Schema Mode**: Use `ResponseFormat::JsonSchema { schema }` for schema-validated responses (Ollama 0.5.0+)
  - Native API integration - sends `format: "json"` or `format: { schema }` directly to Ollama
  - No more prompt-based fallback needed for JSON mode with Ollama
  - Updated `supports_json_schema: true` in Ollama provider capabilities

- **Helper methods on `ThinkingMode`**:
  - `is_auto()` - Check if mode requires smart automatic behavior
  - `is_omit()` - Check if mode explicitly omits the thinking parameter

### Fixed

- **Ollama JSON Mode Gap**: Previously, Ollama declared `supports_json_mode: true` but didn't actually send the `format` field to the API. Now properly implemented with native support.

- **Empty responses with qwen3-vl models**: Models like qwen3-vl would return empty content when the `think` parameter was omitted. The new smart `Auto` behavior prevents this by enabling thinking for known thinking models.

## [0.8.17] - 2026-01-20

### Added

- **`ThinkingMode` Enum**: Provider-agnostic control over chain-of-thought reasoning
  - `ThinkingMode::Auto` - Use model's default behavior (recommended)
  - `ThinkingMode::Enabled` - Force thinking mode on
  - `ThinkingMode::Disabled` - Force thinking mode off for faster responses
  - New `ChatRequest.with_thinking_mode()` builder method
  - Automatically translated to provider-specific parameters (e.g., `think` for Ollama)

- **Ollama `think` Parameter Support**: Direct API integration for thinking control
  - Added `think: Option<bool>` to internal `OllamaRequest` struct
  - Maps from `ChatRequest.thinking_mode` automatically

- **Thinking Model Detection**: Auto-detection with warnings
  - `OllamaProvider::is_thinking_model()` detects qwen3*, deepseek-r1/v3, :thinking variants
  - Debug warnings when `max_tokens < 200` for thinking models (token budget may be exhausted)

- **StreamChunk Helper Methods**: Easier access to combined content
  - `all_content()` - Returns `(Option<&str>, Option<&str>)` tuple of (thinking, content)
  - `combined_text(separator)` - Combines thinking + content into single string

### Fixed

- **Empty Response Bug**: Resolved issue where thinking models returned empty responses
  - Root cause: `max_tokens` setting too low, causing thinking tokens to exhaust budget
  - Solution: Added `ThinkingMode` control and detection warnings

## [0.8.16] - 2026-01-20

### Added

- **Ollama Thinking Mode Support**: Reliable streaming for models with chain-of-thought reasoning (e.g., Qwen3)
  - **Bug Fix**: Streaming no longer returns empty responses when using thinking-enabled models
    - Previously, Qwen3 models would intermittently return empty responses because thinking chunks were dropped
    - Stream now correctly stays active during thinking phase and delivers all content
  - **Important**: Thinking tokens count toward `max_tokens` limit - avoid setting low `max_tokens` values with thinking models or you may get empty responses
  - New `StreamChunk.thinking` field exposes model reasoning content (when available)
  - New helper methods on `StreamChunk`:
    - `thinking(String)` - Create a thinking-only chunk
    - `with_thinking(String, String)` - Create a chunk with both content and thinking
    - `has_thinking()` - Check if chunk contains thinking content
    - `has_content()` - Check if chunk has any content (delta or thinking)
  - Token counting now includes thinking tokens in completion count
  - `StreamingTokenAccumulator` updated with `add_thinking_chunk()` method
  - Backward compatible: existing code works unchanged, `thinking` field is `Option<String>`
  - Affected models: `qwen3:*`, `qwen3-vl:*`, and any future models using Ollama's thinking field
  - See the Ollama provider documentation for usage details

### Fixed

- Custom `Debug` implementation for `TokenEstimator` to avoid derive issues with `tiktoken_rs::CoreBPE`

## [0.8.15] - 2026-01-19

### Added

- **Streaming Token Usage Tracking**: Real-time token consumption monitoring across all 13 LLM providers
  - Every streaming chunk now includes token usage information (both actual and estimated)
  - Dual accuracy: 100% actual counts from providers + 95-99% estimated counts via client-side tokenization
  - New `TokenEstimator` for client-side token counting with tiktoken-rs support (95-99% accuracy)
  - New `StreamingTokenAccumulator` for real-time token aggregation during streaming
  - New `TokenUsage` structure with dual count support (actual + estimated)
  - New `stream_with_usage()` convenience method on `LLMProvider` trait
  - Feature-gated `stream-token-estimation` enables high-accuracy token counting (~50KB binary size)
  - Provider Support:
    - **Tier 1 (100% Actual)**: Claude, OpenAI, Ollama
    - **Tier 2 (95-99% Estimated)**: Groq, Mistral, Fireworks, Together, OpenRouter, Perplexity, LM Studio
    - **Tier 3 (70-90% Heuristic)**: MCP
    - **Tier 4 (100% Test)**: Mock, Loopback providers
  - All 13 providers updated with streaming token tracking implementation
  - 6 new integration tests covering token tracking across providers
  - See: [docs/PROVIDER_TOKEN_ACCURACY.md](docs/PROVIDER_TOKEN_ACCURACY.md)

- **Comprehensive Documentation**
  - Enhanced README.md with new "Streaming Token Usage Tracking" section
  - New PROVIDER_TOKEN_ACCURACY.md with detailed provider breakdown and cost tracking guidance
  - Updated quickstart.md with 5 practical examples and migration guide
  - New DOCUMENTATION_GUIDE.md for navigation and reader journeys
  - 1,110+ lines of documentation, 30+ code examples

### Changed

- TokenUsage structure now includes both actual and estimated counts for dual accuracy
- All streaming chunks now include token usage information (previously only some providers)
- Updated exports in lib.rs to include TokenEstimator and EstimationMethod

### Fixed

- Fixed unit tests to use new TokenUsage structure (estimated_only, best_available methods)
- Addressed clippy warning about unnecessary clone of Copy type in lmstudio.rs

## [0.8.14] - 2025-12-08

### Added

- **Retry-After Header Parsing**: All providers now parse the `Retry-After` header from HTTP 429 responses
  - When rate limited, `LlmError::RateLimit { retry_after }` now contains the duration
  - Enables intelligent retry logic: clients can wait exactly as long as needed
  - Supports delay-seconds format (e.g., "30" for 30 seconds)
  - Added `parse_retry_after()` helper function in `providers/mod.rs`
  - All 9 providers updated: Claude, OpenAI, Ollama, Groq, Together, Fireworks, Mistral, Perplexity, OpenRouter
  - Implements Retry-After header parsing per HTTP/1.1 specification

## [0.8.13] - 2025-12-08

### Fixed

- **Critical Bug Fix**: Streaming requests now use `total_timeout` instead of `connection_timeout`
  - The `chat_stream()` methods were incorrectly using `.timeout(self.connection_timeout)` on the request builder
  - This overrode the client's `total_timeout` (default 600s) with `connection_timeout` (default 10s)
  - Caused streaming to fail after ~10 seconds even with longer timeout configurations
  - Affects: `ClaudeProvider`, `OpenAIProvider`, `OllamaProvider`
  - Added regression tests to prevent this issue from recurring

- **All providers now use centralized `build_http_client()` helper**
  - Previously, only Claude, OpenAI, and Ollama used the helper
  - Now all providers use consistent timeout configuration with `read_timeout` for streaming
  - Fixed providers: `FireworksProvider`, `GroqProvider`, `PerplexityProvider`, `TogetherProvider`, `LmStudioProvider`, `MistralProvider`, `OpenRouterProvider`
  - `MistralProvider` and `OpenRouterProvider` were using `Client::new()` which ignored ALL timeouts
  - Other providers were missing `read_timeout` which is critical for streaming

## [0.8.12] - 2025-12-08

### Fixed

- **Critical Bug Fix**: Timeout configurations are now properly applied to HTTP clients
  - Previously, `connection_timeout`, `stream_read_timeout`, and `total_timeout` values were stored but never applied to the underlying `reqwest::Client`
  - This caused requests to use reqwest's default timeouts instead of user-configured values
  - Streaming requests would fail prematurely (~10s) regardless of timeout configuration
  - Affects: `ClaudeProvider`, `OpenAIProvider`, `OllamaProvider`
  - Root cause: timeout was set on the request builder instead of the HTTP client builder

### Added

- **`build_http_client()` helper function**: Centralized HTTP client creation with proper timeout application
  - Ensures all providers use consistent timeout configuration
  - Prevents this class of bug in future provider implementations
  - See: [docs/PROVIDER_IMPLEMENTATION.md](rustyllm/docs/PROVIDER_IMPLEMENTATION.md)

- **Timeout configuration regression tests**: Comprehensive test suite to catch timeout misconfigurations
  - Tests for all three timeout types across all providers
  - `verify_provider_respects_timeout()` helper for testing new providers
  - Behavioral tests with mock servers and configurable delays

### Changed

- **Upgraded `reqwest` from 0.11 to 0.12**: Enables `read_timeout` support for streaming responses
  - `read_timeout` applies per-chunk during response body reading
  - Critical for LLM streaming where there are pauses between tokens

## [0.8.11] - 2025-12-07

### Added

- **Loopback Provider**: Test/development provider that echoes back user messages
  - Useful for testing without API calls
  - Configurable response delays for timeout testing

## [0.8.10] - 2025-11-25

### Breaking Changes

- **LLMProvider Trait**: Added `get_capabilities()` method to the trait
  - All custom provider implementations must now implement this method
  - Returns `ProviderCapabilities` struct describing what the provider supports

- **ChatResponse**: Added new fields
  - `warnings: Vec<ParameterWarning>` - Parameter adaptation warnings
  - `logprobs: Option<LogprobsData>` - Token probability data (when requested)

### Added

- **New Parameters in ChatRequest**:
  - `stop: Option<Vec<String>>` - Stop sequences for generation
  - `presence_penalty: Option<f32>` - Repetition penalty (-2.0 to 2.0)
  - `frequency_penalty: Option<f32>` - Frequency penalty (-2.0 to 2.0)
  - `seed: Option<u64>` - Deterministic generation seed
  - `logprobs: Option<bool>` - Enable token probabilities
  - `top_logprobs: Option<u8>` - Number of top alternatives (0-20)
  - `response_format: Option<ResponseFormat>` - JSON mode control
  - `provider_options: Option<ProviderOptions>` - Provider-specific parameters

- **Parameter Adapter**: Graceful degradation system
  - Automatically adapts parameters to provider capabilities
  - Truncates stop sequences to provider limits with warnings
  - Ignores unsupported parameters with info-level warnings
  - Falls back to prompt-based JSON mode when native not supported

- **Provider Capabilities System**:
  - `ProviderCapabilities` struct for querying provider support
  - Runtime capability detection for all parameters
  - Enables write-once code that adapts to any provider

- **New Providers** (6 total):
  - `MistralProvider` - Mistral AI API
  - `OpenRouterProvider` - OpenRouter unified API (100+ models)
  - `TogetherProvider` - Together AI (open source models)
  - `GroqProvider` - Groq (ultra-fast inference)
  - `FireworksProvider` - Fireworks AI (optimized inference)
  - `PerplexityProvider` - Perplexity AI (search-augmented)

- **MCP Auto-Discovery**:
  - Automatic server discovery from `~/.config/mcp/servers.json`
  - `RUSTYLLM_MCP_CONFIG` environment variable override
  - `McpProvider::discover_servers()` API
  - `McpProvider::builder().discover_server("name")` pattern

- **Supporting Types**:
  - `ResponseFormat` enum (Text, Json, JsonSchema)
  - `ProviderOptions` enum with `OllamaOptions` variant
  - `ParameterWarning` and `WarningSeverity` for warnings
  - `LogprobsData`, `TokenLogprob`, `TopLogprob` for token probabilities

### Changed

- All existing providers updated to implement `get_capabilities()`
- All providers now integrate with ParameterAdapter for graceful degradation
- MockProvider enhanced with full capability support for testing

### Fixed

- Test race conditions in env_detector and MCP tests (mutex serialization)
- Test assertions for provider metadata flexibility

## [0.8.9] - 2025-11-25

### Changed

- Updated plans for new versioning with open source beta target (pre-v1.x.x)
- Cleanup and tightened various compiler warnings

## [0.8.8] - 2025-11-25

### Changed

- Version bump for release preparation

## [0.8.7] - 2025-11-25

### Changed

- Release preparation updates

## [0.8.6] - 2025-11-25

### Changed

- Minor fixes and updates

## [0.8.5] - 2025-11-25

### Changed

- Release updates

## [0.8.4] - 2025-11-24

### Changed

- Post-reset stabilization

## [0.8.3] - 2025-11-24

### Added

- Initial Tier-1 provider implementations (Mistral, OpenRouter, Together, Groq, Fireworks, Perplexity)
- MCP auto-discovery groundwork

## [0.8.2] - 2025-11-24

### Added

- LiteLLM-style convenience API with automatic provider detection
- Graceful parameter degradation foundation

## [0.8.1] - 2025-11-24

### Added

- Provider expansion preparation
- API refinements

## [0.8.0] - 2025-11-24

### Changed

- **Version Reset**: Reset version numbering from the earlier 2.x line into the pre-public development line
  - Pre-1.0 versioning (0.x.y) signals API is not yet stable per semantic versioning
  - Allows breaking changes in minor versions during development
  - Functionality from the earlier 2.x line carried forward unchanged
  - Historical 2.x changelog preserved in the internal archive
  - Previous 2.x release artifacts archived outside the public SDK release line

### Notes

This was a version numbering reset only - no code functionality changed. Features, fixes, and improvements from the earlier 2.x line carried forward into the pre-public development line. The reset better reflected the library's development stage and followed Rust ecosystem conventions for pre-release crates.

The older 2.x history is intentionally kept out of the public SDK changelog.
