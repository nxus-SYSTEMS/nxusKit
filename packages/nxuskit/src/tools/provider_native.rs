//! Provider-native tool-call support helpers.
//!
//! tier: ce-impl

use super::CeToolPosture;

pub(crate) fn provider_native_posture(supports_tool_calling: bool) -> CeToolPosture {
    if supports_tool_calling {
        CeToolPosture::Native
    } else {
        CeToolPosture::Unavailable
    }
}
