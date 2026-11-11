//! Public CE contract surface for nxusKit SDK.
//!
//! Versioned public Data Transfer Object (DTO) and validation contracts that
//! downstream consumers (e.g. Peeler-style pipeline products) can adopt
//! directly. Each contract version lives in its own submodule. The current
//! contract version is `v2` preview; `v1_1` remains available for prior
//! contract-alpha consumers until migration policy says otherwise.
//!
//! Engine-internal dispatch, catalog, capability-truth, and product-specific
//! pipeline types are NOT re-exported here. The contract layer talks to engine
//! internals through a narrow read-only adapter only.

pub mod v1_1;
pub mod v2;
