//! Integration tests module

#[cfg(feature = "clips")]
pub mod clips_provider_test;

// 033-programmatic-rule-loading contract tests (Phase 2a - TDD gate)
pub mod clips_cache_eviction_test;
pub mod clips_export_test;
pub mod clips_module_definition_test;
pub mod clips_policy_cache_test;
pub mod clips_rule_definition_test;
pub mod clips_rule_program_test;

pub mod model_info_test;
pub mod parameter_adapter_test;
pub mod peeler_streaming_issue_test;
pub mod streaming_token_usage_test;
pub mod timeout_config_test;
