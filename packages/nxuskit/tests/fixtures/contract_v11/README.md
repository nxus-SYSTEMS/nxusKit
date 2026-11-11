# Golden Fixture Set — `nxuskit::contract::v1_1`

This directory mirrors the authoritative golden fixture index defined in
the v1.1 contract planning artifacts.

Each fixture in that index has (or will have) a corresponding JSON file under
this directory, used by the `contract_v11_*` integration tests. The pairing is
asserted mechanically by `tests/contract_v11_fixture_inventory.rs`.

## Inventory anchor (verbatim from the spec index)

The following fixture names MUST exist as files (with the listed extension) by
the time the v1.1.0 contract feature is complete. Phase 2 lands the
`boundary_positive`, `boundary_negative_*`, and a minimum subset of
`pipeline_*`, `readiness_*`, `envelope_*`, and `packet_*` fixtures.

Pipeline:

- `pipeline_llm_only.json`
- `pipeline_llm_plus_clips_rule_eval.json`
- `pipeline_llm_plus_tool.json`
- `pipeline_declaration_solver.json`
- `pipeline_declaration_bn.json`
- `pipeline_declaration_zen.json`
- `pipeline_declaration_data_store_search.json`
- `pipeline_dag_cycle.json`
- `pipeline_duplicate_stage_name.json`
- `pipeline_tool_missing_semantics.json`

Readiness:

- `readiness_all_ready.json`
- `readiness_auth_missing.json`
- `readiness_auth_invalid.json`
- `readiness_unreachable.json`
- `readiness_timeout.json`
- `readiness_tier_restricted.json`
- `readiness_internal_only.json`

Envelope:

- `envelope_success.json`
- `envelope_validation_failed.json`
- `envelope_capability_unavailable.json`
- `envelope_auth_failed.json`
- `envelope_unreachable.json`
- `envelope_timeout.json`
- `envelope_cancelled.json`
- `envelope_internal_only_denied.json`
- `envelope_partial_result.json`
- `envelope_with_legacy_aliases.json`

Regulated workflow packet:

- `packet_validates.json`
- `packet_validation_fails.json`
- `packet_redaction_round_trip.json`

Boundary scan (positive + one negative per banned class):

- `boundary_positive.json`
- `boundary_negative/medical_coding_persona_schema.json`
- `boundary_negative/denial_appeals_product_claim.md`
- `boundary_negative/phi_like_fixture_content.json`
- `boundary_negative/internal_repo_name.json`
- `boundary_negative/pro_only_topology_indicator.json`
- `boundary_negative/internal_provider_channel_detail.json`
- `boundary_negative/unauthorized_decision_claim.json`

## Phase 2 minimum subset

Phase 2 (T016) lands:

- `boundary_positive.json` — minimal exemplar.
- `boundary_negative/internal_repo_name.json` — one seeded negative-control to
  prove the inventory test machinery.
- `pipeline_llm_only.json` — minimal valid pipeline payload exercising the
  pure DTO primitives shipped in Phase 2.

The remaining fixtures land with their owning user-story phase (US1–US5).
