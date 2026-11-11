//! Fan-out normalization helper for the v1.1 contract-native runner.
//!
//! `metadata.fan_out` is RC input sugar: a stage carries
//! `{ "mode": "parallel", "provider_model_pairs": [...] }` in metadata
//! and the runner expands it into per-pair sibling operations before
//! dispatch (FR-016). The normalized representation is canonical for
//! event emission and result construction.

use serde::{Deserialize, Serialize};

/// One pair inside a fan-out spec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderModelPair {
    pub provider_ref: String,
    pub model_ref: String,
}

/// Parsed `metadata.fan_out` block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FanOutSpec {
    pub mode: String,
    pub provider_model_pairs: Vec<ProviderModelPair>,
}

/// One normalized fan-out operation. Stable operation id is
/// `<stage_id>::op-<pair_index>` per FR-019.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FanOutOperation {
    pub operation_id: String,
    pub pair_index: u32,
    pub provider_ref: String,
    pub model_ref: String,
}

/// Try to parse a `metadata.fan_out` block from a stage's metadata map.
/// Returns `None` if no fan-out metadata is present.
pub fn parse_fan_out_metadata(
    metadata: &std::collections::BTreeMap<String, serde_json::Value>,
) -> Option<FanOutSpec> {
    let raw = metadata.get("fan_out")?;
    serde_json::from_value(raw.clone()).ok()
}

/// Normalize a [`FanOutSpec`] into a stable list of [`FanOutOperation`]s.
///
/// `stage_id` must be the parent stage's name; the operation id
/// generator follows FR-019 (`<stage_id>::op-<pair_index>`).
pub fn normalize_fan_out(stage_id: &str, spec: &FanOutSpec) -> Vec<FanOutOperation> {
    spec.provider_model_pairs
        .iter()
        .enumerate()
        .map(|(idx, pair)| FanOutOperation {
            operation_id: format!("{stage_id}::op-{idx}"),
            pair_index: idx as u32,
            provider_ref: pair.provider_ref.clone(),
            model_ref: pair.model_ref.clone(),
        })
        .collect()
}

/// Default in-flight concurrency for a fan-out group when the caller
/// does not provide an explicit cap (R-1).
pub const DEFAULT_FAN_OUT_CONCURRENCY: u32 = 4;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;

    #[test]
    fn parse_fan_out_metadata_returns_none_for_absent_block() {
        let metadata: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        assert!(parse_fan_out_metadata(&metadata).is_none());
    }

    #[test]
    fn parse_fan_out_metadata_returns_spec_for_documented_shape() {
        let mut metadata: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        metadata.insert(
            "fan_out".into(),
            json!({
                "mode": "parallel",
                "provider_model_pairs": [
                    {"provider_ref": "loopback", "model_ref": "echo"},
                    {"provider_ref": "loopback", "model_ref": "echo-json-native"}
                ]
            }),
        );
        let spec = parse_fan_out_metadata(&metadata).expect("parsed");
        assert_eq!(spec.mode, "parallel");
        assert_eq!(spec.provider_model_pairs.len(), 2);
    }

    #[test]
    fn normalize_fan_out_produces_stable_operation_ids() {
        let spec = FanOutSpec {
            mode: "parallel".into(),
            provider_model_pairs: vec![
                ProviderModelPair {
                    provider_ref: "loopback".into(),
                    model_ref: "echo".into(),
                },
                ProviderModelPair {
                    provider_ref: "loopback".into(),
                    model_ref: "echo-json-native".into(),
                },
            ],
        };
        let ops = normalize_fan_out("compare_summaries", &spec);
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0].operation_id, "compare_summaries::op-0");
        assert_eq!(ops[0].pair_index, 0);
        assert_eq!(ops[0].model_ref, "echo");
        assert_eq!(ops[1].operation_id, "compare_summaries::op-1");
        assert_eq!(ops[1].pair_index, 1);
        assert_eq!(ops[1].model_ref, "echo-json-native");
    }

    #[test]
    fn default_fan_out_concurrency_is_four() {
        assert_eq!(DEFAULT_FAN_OUT_CONCURRENCY, 4);
    }
}
