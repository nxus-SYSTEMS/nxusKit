//! Parameter adaptation for graceful degradation
//!
//! This module provides parameter adaptation logic that allows providers to
//! gracefully handle parameters they don't support. Instead of failing, the
//! adapter modifies the request to fit provider capabilities and returns
//! warnings about what was changed.

use crate::types::{
    ChatRequest, Message, MessageContent, ParameterWarning, ProviderCapabilities, ResponseFormat,
    Role, WarningSeverity,
};

/// Adapted request with warnings
#[derive(Debug, Clone)]
pub struct AdaptedRequest {
    /// The adapted chat request
    pub request: ChatRequest,
    /// Warnings about adaptations made
    pub warnings: Vec<ParameterWarning>,
}

/// Parameter adapter for graceful degradation
#[derive(Debug)]
pub struct ParameterAdapter;

impl ParameterAdapter {
    /// Adapt a request to provider capabilities
    ///
    /// Takes a ChatRequest and provider capabilities, and returns an adapted
    /// request that fits the provider's constraints along with warnings about
    /// any changes made.
    ///
    /// # Arguments
    ///
    /// * `request` - The original chat request
    /// * `capabilities` - The provider's capabilities
    ///
    /// # Returns
    ///
    /// Returns an `AdaptedRequest` containing the modified request and warnings.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use nxuskit_engine::parameter_adapter::ParameterAdapter;
    /// use nxuskit_engine::types::{ChatRequest, Message, ProviderCapabilities};
    ///
    /// let request = ChatRequest::new("gpt-4")
    ///     .with_message(Message::user("Hello"))
    ///     .with_temperature(0.7);
    ///
    /// let capabilities = ProviderCapabilities::default();
    /// let adapted = ParameterAdapter::adapt(&request, &capabilities);
    ///
    /// println!("Warnings: {:?}", adapted.warnings);
    /// ```
    pub fn adapt(request: &ChatRequest, capabilities: &ProviderCapabilities) -> AdaptedRequest {
        let mut adapted_request = request.clone();
        let mut warnings = Vec::new();

        // GPT-5.4 reasoning-compat warn-and-drop. Runs before the generic
        // adapters so the dropped fields don't generate redundant
        // capability-mismatch warnings downstream.
        let reasoning_effort = request.reasoning_effort.as_deref();
        adapt_gpt54_reasoning_compat(&mut adapted_request, reasoning_effort, &mut warnings);

        // Adapt stop sequences
        Self::adapt_stop_sequences(&mut adapted_request, capabilities, &mut warnings);

        // Adapt penalties
        Self::adapt_penalties(&mut adapted_request, capabilities, &mut warnings);

        // Adapt seed
        Self::adapt_seed(&mut adapted_request, capabilities, &mut warnings);

        // Adapt logprobs
        Self::adapt_logprobs(&mut adapted_request, capabilities, &mut warnings);

        // Adapt response format (JSON mode)
        Self::adapt_response_format(&mut adapted_request, capabilities, &mut warnings);

        AdaptedRequest {
            request: adapted_request,
            warnings,
        }
    }

    /// Adapt stop sequences to provider limits
    fn adapt_stop_sequences(
        request: &mut ChatRequest,
        capabilities: &ProviderCapabilities,
        warnings: &mut Vec<ParameterWarning>,
    ) {
        if let Some(ref stops) = request.stop
            && let Some(max_stop) = capabilities.max_stop_sequences
            && stops.len() > max_stop
        {
            // Truncate to provider limit
            let original_count = stops.len();
            request.stop = Some(stops[..max_stop].to_vec());

            warnings.push(ParameterWarning {
                parameter: "stop".to_string(),
                message: format!(
                    "Truncated from {} to {} stop sequences (provider limit)",
                    original_count, max_stop
                ),
                severity: WarningSeverity::Warning,
            });
        }
    }

    /// Adapt penalty parameters
    fn adapt_penalties(
        request: &mut ChatRequest,
        capabilities: &ProviderCapabilities,
        warnings: &mut Vec<ParameterWarning>,
    ) {
        // Handle presence_penalty
        if request.presence_penalty.is_some() {
            if !capabilities.supports_presence_penalty {
                warnings.push(ParameterWarning {
                    parameter: "presence_penalty".to_string(),
                    message: "Not supported by provider, parameter ignored".to_string(),
                    severity: WarningSeverity::Info,
                });
                request.presence_penalty = None;
            } else if let Some(penalty) = request.presence_penalty
                && let Some((min, max)) = capabilities.penalty_range
                && (penalty < min || penalty > max)
            {
                // Validate range if provider specifies one
                warnings.push(ParameterWarning {
                    parameter: "presence_penalty".to_string(),
                    message: format!(
                        "Value {} outside provider range [{}, {}], may be clamped",
                        penalty, min, max
                    ),
                    severity: WarningSeverity::Warning,
                });
            }
        }

        // Handle frequency_penalty
        if request.frequency_penalty.is_some() {
            if !capabilities.supports_frequency_penalty {
                warnings.push(ParameterWarning {
                    parameter: "frequency_penalty".to_string(),
                    message: "Not supported by provider, parameter ignored".to_string(),
                    severity: WarningSeverity::Info,
                });
                request.frequency_penalty = None;
            } else if let Some(penalty) = request.frequency_penalty
                && let Some((min, max)) = capabilities.penalty_range
                && (penalty < min || penalty > max)
            {
                // Validate range if provider specifies one
                warnings.push(ParameterWarning {
                    parameter: "frequency_penalty".to_string(),
                    message: format!(
                        "Value {} outside provider range [{}, {}], may be clamped",
                        penalty, min, max
                    ),
                    severity: WarningSeverity::Warning,
                });
            }
        }
    }

    /// Adapt seed parameter
    fn adapt_seed(
        request: &mut ChatRequest,
        capabilities: &ProviderCapabilities,
        warnings: &mut Vec<ParameterWarning>,
    ) {
        if request.seed.is_some() && !capabilities.supports_seed {
            warnings.push(ParameterWarning {
                parameter: "seed".to_string(),
                message: "Deterministic generation not supported, seed ignored".to_string(),
                severity: WarningSeverity::Info,
            });
            request.seed = None;
        }
    }

    /// Adapt logprobs parameters
    fn adapt_logprobs(
        request: &mut ChatRequest,
        capabilities: &ProviderCapabilities,
        warnings: &mut Vec<ParameterWarning>,
    ) {
        if request.logprobs.is_some() && !capabilities.supports_logprobs {
            warnings.push(ParameterWarning {
                parameter: "logprobs".to_string(),
                message: "Log probabilities not supported by provider".to_string(),
                severity: WarningSeverity::Info,
            });
            request.logprobs = None;
            request.top_logprobs = None;
        } else if let Some(top_n) = request.top_logprobs
            && let Some(max_logprobs) = capabilities.max_logprobs
            && top_n > max_logprobs
        {
            warnings.push(ParameterWarning {
                parameter: "top_logprobs".to_string(),
                message: format!(
                    "Requested {} alternatives, provider maximum is {}",
                    top_n, max_logprobs
                ),
                severity: WarningSeverity::Warning,
            });
            request.top_logprobs = Some(max_logprobs);
        }
    }

    /// Adapt response format (JSON mode)
    fn adapt_response_format(
        request: &mut ChatRequest,
        capabilities: &ProviderCapabilities,
        warnings: &mut Vec<ParameterWarning>,
    ) {
        if let Some(ref format) = request.response_format {
            match format {
                ResponseFormat::Text => {
                    // Text format is always supported
                }
                ResponseFormat::Json => {
                    if !capabilities.supports_json_mode {
                        // Provider doesn't support native JSON mode
                        // Add system message to request JSON output
                        Self::add_json_system_message(request);

                        warnings.push(ParameterWarning {
                            parameter: "response_format".to_string(),
                            message: "Native JSON mode not supported, using prompt-based approach"
                                .to_string(),
                            severity: WarningSeverity::Info,
                        });
                    }
                }
                ResponseFormat::JsonSchema { .. } => {
                    if !capabilities.supports_json_schema {
                        if capabilities.supports_json_mode {
                            // Downgrade to basic JSON mode
                            request.response_format = Some(ResponseFormat::Json);
                            warnings.push(ParameterWarning {
                                parameter: "response_format".to_string(),
                                message:
                                    "JSON schema validation not supported, using basic JSON mode"
                                        .to_string(),
                                severity: WarningSeverity::Warning,
                            });
                        } else {
                            // Fall back to prompt-based
                            Self::add_json_system_message(request);
                            warnings.push(ParameterWarning {
                                parameter: "response_format".to_string(),
                                message:
                                    "JSON schema not supported, using prompt-based JSON request"
                                        .to_string(),
                                severity: WarningSeverity::Warning,
                            });
                        }
                    }
                }
            }
        }
    }

    /// Add a system message requesting JSON output
    fn add_json_system_message(request: &mut ChatRequest) {
        // Check if there's already a system message
        let has_system_message = request
            .messages
            .iter()
            .any(|m| matches!(m.role, Role::System));

        if !has_system_message {
            // Add new system message at the beginning
            let json_instruction = Message::system(
                "Respond only with valid JSON. Do not include any text outside the JSON structure.",
            );
            request.messages.insert(0, json_instruction);
        } else if let Some(first_msg) = request.messages.first_mut()
            && matches!(first_msg.role, Role::System)
        {
            // Append to existing system message
            match &mut first_msg.content {
                MessageContent::Text(text) => {
                    text.push_str(
                        "\n\nIMPORTANT: Respond only with valid JSON. \
                         Do not include any text outside the JSON structure.",
                    );
                }
                MessageContent::Parts(_) => {
                    // Can't easily append to parts, create new system message
                    let json_instruction = Message::system(
                        "IMPORTANT: Respond only with valid JSON. \
                         Do not include any text outside the JSON structure.",
                    );
                    request.messages.insert(1, json_instruction);
                }
            }
        }
    }
}

/// Returns `true` if `model` is a GPT-5.4 family model (case-insensitive).
fn is_gpt54_family(model: &str) -> bool {
    let lower = model.to_ascii_lowercase();
    lower.starts_with("gpt-5.4")
}

/// GPT-5.4 reasoning-compat warn-and-drop rule.
///
/// When the request targets a GPT-5.4 family model AND `reasoning_effort` is
/// `Some(value)` where `value != "none"`, the following parameters are
/// incompatible with the model's reasoning mode and MUST be dropped with a
/// warning:
/// - `temperature`
/// - `top_p`
/// - `logprobs` (and `top_logprobs`)
///
/// When `reasoning_effort` is `None` or `Some("none")`, no fields are dropped
/// and no warnings are emitted.
///
/// Warnings use the same `WarningSeverity::Warning` channel as other adapter
/// warnings; consumers can inspect `AdaptedRequest::warnings`.
pub fn adapt_gpt54_reasoning_compat(
    request: &mut ChatRequest,
    reasoning_effort: Option<&str>,
    warnings: &mut Vec<ParameterWarning>,
) {
    if !is_gpt54_family(&request.model) {
        return;
    }
    let effort = match reasoning_effort {
        Some(e) => e,
        None => return,
    };
    if effort.eq_ignore_ascii_case("none") {
        return;
    }

    if request.temperature.is_some() {
        warnings.push(ParameterWarning {
            parameter: "temperature".to_string(),
            message: format!(
                "GPT-5.4 with reasoning.effort='{}' does not accept temperature; dropped",
                effort
            ),
            severity: WarningSeverity::Warning,
        });
        request.temperature = None;
    }
    if request.top_p.is_some() {
        warnings.push(ParameterWarning {
            parameter: "top_p".to_string(),
            message: format!(
                "GPT-5.4 with reasoning.effort='{}' does not accept top_p; dropped",
                effort
            ),
            severity: WarningSeverity::Warning,
        });
        request.top_p = None;
    }
    if request.logprobs.is_some() || request.top_logprobs.is_some() {
        warnings.push(ParameterWarning {
            parameter: "logprobs".to_string(),
            message: format!(
                "GPT-5.4 with reasoning.effort='{}' does not accept logprobs; dropped",
                effort
            ),
            severity: WarningSeverity::Warning,
        });
        request.logprobs = None;
        request.top_logprobs = None;
    }
}

#[cfg(test)]
mod gpt54_reasoning_compat_tests {
    use super::*;

    fn req_with_all_params(model: &str) -> ChatRequest {
        let mut r = ChatRequest::new(model).with_message(Message::user("hi"));
        r.temperature = Some(0.7);
        r.top_p = Some(0.9);
        r.logprobs = Some(true);
        r.top_logprobs = Some(5);
        r
    }

    #[test]
    fn gpt54_with_reasoning_medium_drops_all_three_params_and_warns() {
        let mut r = req_with_all_params("gpt-5.4");
        let mut warnings = Vec::new();
        adapt_gpt54_reasoning_compat(&mut r, Some("medium"), &mut warnings);

        assert!(r.temperature.is_none(), "temperature must be dropped");
        assert!(r.top_p.is_none(), "top_p must be dropped");
        assert!(r.logprobs.is_none(), "logprobs must be dropped");
        assert!(r.top_logprobs.is_none(), "top_logprobs must be dropped");
        let params: Vec<&str> = warnings.iter().map(|w| w.parameter.as_str()).collect();
        assert!(params.contains(&"temperature"));
        assert!(params.contains(&"top_p"));
        assert!(params.contains(&"logprobs"));
        for w in &warnings {
            assert_eq!(w.severity, WarningSeverity::Warning);
        }
    }

    #[test]
    fn gpt54_with_reasoning_none_keeps_params_and_emits_no_warning() {
        let mut r = req_with_all_params("gpt-5.4");
        let mut warnings = Vec::new();
        adapt_gpt54_reasoning_compat(&mut r, Some("none"), &mut warnings);

        assert_eq!(r.temperature, Some(0.7));
        assert_eq!(r.top_p, Some(0.9));
        assert_eq!(r.logprobs, Some(true));
        assert_eq!(r.top_logprobs, Some(5));
        assert!(warnings.is_empty(), "no warning when effort is 'none'");
    }

    #[test]
    fn gpt54_without_effort_signal_keeps_params() {
        // No reasoning effort signaled (None) → rule is a no-op even on gpt-5.4.
        let mut r = req_with_all_params("gpt-5.4");
        let mut warnings = Vec::new();
        adapt_gpt54_reasoning_compat(&mut r, None, &mut warnings);
        assert_eq!(r.temperature, Some(0.7));
        assert!(warnings.is_empty());
    }

    #[test]
    fn non_gpt54_model_is_unaffected_by_rule() {
        let mut r = req_with_all_params("gpt-4o");
        let mut warnings = Vec::new();
        adapt_gpt54_reasoning_compat(&mut r, Some("high"), &mut warnings);
        assert_eq!(r.temperature, Some(0.7));
        assert_eq!(r.top_p, Some(0.9));
        assert!(warnings.is_empty());
    }

    #[test]
    fn gpt54_variant_models_match_prefix() {
        let mut r = req_with_all_params("gpt-5.4-mini");
        let mut warnings = Vec::new();
        adapt_gpt54_reasoning_compat(&mut r, Some("low"), &mut warnings);
        assert!(r.temperature.is_none());
        assert!(!warnings.is_empty());
    }

    #[test]
    fn parameter_adapter_pipeline_uses_request_reasoning_effort_carrier() {
        let request = req_with_all_params("gpt-5.4").with_reasoning_effort("medium");
        let capabilities = ProviderCapabilities {
            supports_logprobs: true,
            max_logprobs: Some(20),
            ..Default::default()
        };

        let adapted = ParameterAdapter::adapt(&request, &capabilities);

        assert!(adapted.request.temperature.is_none());
        assert!(adapted.request.top_p.is_none());
        assert!(adapted.request.logprobs.is_none());
        assert!(adapted.request.top_logprobs.is_none());
        assert!(
            adapted
                .warnings
                .iter()
                .any(|w| w.parameter == "logprobs" && w.message.contains("reasoning.effort"))
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapt_no_changes_needed() {
        let request = ChatRequest::new("test-model")
            .with_message(Message::user("Hello"))
            .with_temperature(0.7);

        let capabilities = ProviderCapabilities {
            supports_system_messages: true,
            supports_streaming: true,
            supports_vision: false,
            max_stop_sequences: Some(4),
            supports_presence_penalty: true,
            supports_frequency_penalty: true,
            supports_seed: true,
            supports_logprobs: true,

            supports_streaming_logprobs: false,
            supports_json_mode: true,
            supports_json_schema: false,
            penalty_range: Some((-2.0, 2.0)),
            max_logprobs: Some(20),
        };

        let adapted = ParameterAdapter::adapt(&request, &capabilities);

        assert_eq!(adapted.warnings.len(), 0);
        assert_eq!(adapted.request.messages.len(), 1);
    }

    #[test]
    fn test_stop_sequence_truncation() {
        let mut request = ChatRequest::new("test-model").with_message(Message::user("Hello"));
        request.stop = Some(vec![
            "END".to_string(),
            "STOP".to_string(),
            "DONE".to_string(),
            "QUIT".to_string(),
            "EXIT".to_string(),
            "FINISH".to_string(),
        ]);

        let capabilities = ProviderCapabilities {
            max_stop_sequences: Some(4),
            ..Default::default()
        };

        let adapted = ParameterAdapter::adapt(&request, &capabilities);

        assert_eq!(adapted.request.stop.as_ref().unwrap().len(), 4);
        assert_eq!(adapted.warnings.len(), 1);
        assert_eq!(adapted.warnings[0].parameter, "stop");
        assert_eq!(adapted.warnings[0].severity, WarningSeverity::Warning);
    }

    #[test]
    fn test_unsupported_penalty() {
        let mut request = ChatRequest::new("test-model").with_message(Message::user("Hello"));
        request.presence_penalty = Some(0.5);
        request.frequency_penalty = Some(0.3);

        let capabilities = ProviderCapabilities {
            supports_presence_penalty: false,
            supports_frequency_penalty: false,
            ..Default::default()
        };

        let adapted = ParameterAdapter::adapt(&request, &capabilities);

        assert!(adapted.request.presence_penalty.is_none());
        assert!(adapted.request.frequency_penalty.is_none());
        assert_eq!(adapted.warnings.len(), 2);
        assert!(
            adapted
                .warnings
                .iter()
                .any(|w| w.parameter == "presence_penalty")
        );
        assert!(
            adapted
                .warnings
                .iter()
                .any(|w| w.parameter == "frequency_penalty")
        );
    }

    #[test]
    fn test_json_mode_fallback() {
        let mut request =
            ChatRequest::new("test-model").with_message(Message::user("Generate JSON"));
        request.response_format = Some(ResponseFormat::Json);

        let capabilities = ProviderCapabilities {
            supports_json_mode: false,
            supports_system_messages: true,
            ..Default::default()
        };

        let adapted = ParameterAdapter::adapt(&request, &capabilities);

        // Should have added a system message
        assert_eq!(adapted.request.messages.len(), 2);
        assert!(matches!(adapted.request.messages[0].role, Role::System));
        assert_eq!(adapted.warnings.len(), 1);
        assert_eq!(adapted.warnings[0].parameter, "response_format");
    }

    #[test]
    fn test_seed_not_supported() {
        let mut request = ChatRequest::new("test-model").with_message(Message::user("Hello"));
        request.seed = Some(12345);

        let capabilities = ProviderCapabilities {
            supports_seed: false,
            ..Default::default()
        };

        let adapted = ParameterAdapter::adapt(&request, &capabilities);

        assert!(adapted.request.seed.is_none());
        assert_eq!(adapted.warnings.len(), 1);
        assert_eq!(adapted.warnings[0].parameter, "seed");
        assert_eq!(adapted.warnings[0].severity, WarningSeverity::Info);
    }

    #[test]
    fn test_logprobs_limit() {
        let mut request = ChatRequest::new("test-model").with_message(Message::user("Hello"));
        request.logprobs = Some(true);
        request.top_logprobs = Some(50);

        let capabilities = ProviderCapabilities {
            supports_logprobs: true,

            supports_streaming_logprobs: false,
            max_logprobs: Some(20),
            ..Default::default()
        };

        let adapted = ParameterAdapter::adapt(&request, &capabilities);

        assert_eq!(adapted.request.top_logprobs, Some(20));
        assert_eq!(adapted.warnings.len(), 1);
        assert_eq!(adapted.warnings[0].parameter, "top_logprobs");
    }
}
