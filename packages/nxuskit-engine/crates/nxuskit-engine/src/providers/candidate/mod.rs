//! Design-only candidate provider capability records for feature 099.
//!
//! These modules intentionally do not implement `LLMProvider` and are not
//! re-exported from `crate::providers`. They exist to keep provider-intake
//! evidence and promote/defer decisions close to the adapter namespace without
//! registering runnable direct providers in v0.9.4.

use crate::capabilities::CapabilityEvidence;

pub(crate) mod cohere;
pub(crate) mod deepseek;
pub(crate) mod gemini;

pub(super) const REVIEWED_ON: &str = "2026-05-09";

pub(super) fn evidence(source_url: &str, note: impl Into<String>) -> CapabilityEvidence {
    CapabilityEvidence {
        source_url: Some(source_url.into()),
        source_reviewed_on: REVIEWED_ON.into(),
        adapter_test: None,
        fixture_path: None,
        live_test: None,
        notes: vec![note.into()],
    }
}
