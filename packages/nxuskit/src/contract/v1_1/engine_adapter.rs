//! Narrow read-only chokepoint between the public CE contract DTOs and the
//! engine-internal Sprint-0 surfaces (research.md §6).
//!
//! US1 ships this module as the *named* chokepoint — file-content boundary
//! tests in `tests/contract_v11_public_boundary.rs` will check that NO
//! other file under `src/contract/v1_1/` imports `nxuskit_engine`. The
//! actual FFI bridges to `auth_providers` / `auth_status_all` / live
//! `CapabilityView` data are Integration follow-up work; US1 only needs
//! the pure constructors on [`crate::contract::v1_1::ReadinessSnapshot`]
//! (`from_pure`) and the schema-only validation on [`PipelineDefinition`].
//!
//! TODO(Integration): provide
//!   `query_provider_readiness(&[ProviderId]) -> ReadinessSnapshot`
//!   that wraps the existing public `auth_providers` /
//!   `auth_status_all` surfaces. The body MUST stay narrow:
//!   one call site per upstream surface, projected into the public
//!   DTO. No internal-only fields may pass through.
//!
//! The Phase-2 boundary test allows zero exceptions; when the FFI bridge
//! lands, this file is the single allowlist entry — see
//! `tests/contract_v11_public_boundary.rs` `phase_2_allowlist`.

/// Sentinel for the narrow adapter. Carries no state today.
///
/// The presence of this type pins the chokepoint so reviewers can grep
/// for one symbol when auditing the engine-internal boundary.
#[derive(Debug, Default, Clone, Copy)]
#[allow(dead_code)] // Integration work attaches FFI bridges here.
pub struct EngineAdapter;
