//! Public CE ZEN wrapper stub.

use crate::NxuskitError;

pub fn zen_evaluate(_model: &str, _input: &str) -> Result<serde_json::Value, NxuskitError> {
    Err(NxuskitError::FeatureUnavailable {
        feature: "zen".to_string(),
        message: "ZEN evaluation is a Pro capability and is not shipped in public CE".to_string(),
    })
}
