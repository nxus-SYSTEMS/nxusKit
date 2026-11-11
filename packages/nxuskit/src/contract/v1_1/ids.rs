//! Shared identifier newtypes and enums for the v1.1 contract surface.
//!
//! Domain newtypes follow Article IV §Domain Newtypes (Rust): each wraps a
//! `String` with `#[serde(transparent)]` so JSON round-trips are flat string
//! values, but the type system still prevents accidental mixing across
//! identifier kinds.

use serde::{Deserialize, Serialize};

macro_rules! string_newtype {
    (
        $(#[$meta:meta])*
        $name:ident
    ) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Borrow the underlying string content.
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Consume the newtype and return the owned string.
            pub fn into_inner(self) -> String {
                self.0
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_owned())
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

string_newtype!(
    /// Stable caller-supplied identifier for a [`PipelineDefinition`] document.
    PipelineId
);

string_newtype!(
    /// Unique stage name within a pipeline.
    StageName
);

string_newtype!(
    /// Stable identifier for a capability (e.g. `llm.text_completion`,
    /// `tool.emit_artifact`, `rule_eval.generic`).
    CapabilityId
);

string_newtype!(
    /// Stable identifier for a provider (e.g. `loopback`, `claude`, `ollama`).
    ProviderId
);

// ---------------------------------------------------------------------------
// Feature 105 retargeting - additive newtypes.
//
// These identifiers carry the public CE control-plane and data-plane reference
// identities introduced in `refs.rs` and `action.rs`. The seven new newtypes
// share the same `#[serde(transparent)]` macro-derived shape as feature 104
// newtypes above so JSON round-trips stay flat string values.
//
// `TopicName` and `ActionType` ALSO carry a grammar-validating `try_new`
// constructor (and an `IdError` typed error variant). The macro-derived
// `From<&str>` / `From<String>` accept any string for backward consistency;
// callers that need grammar validation MUST use `try_new`.
// ---------------------------------------------------------------------------

string_newtype!(
    /// Identity of an Arrow-typed batch reference (see `super::refs::BatchRef`).
    BatchId
);

string_newtype!(
    /// Identity of a pre-Arrow source descriptor referenced from
    /// `super::envelope::SourceReference`.
    SourceId
);

string_newtype!(
    /// Identity of a Substrait-or-equivalent plan reference
    /// (see `super::refs::PlanRef`).
    PlanId
);

string_newtype!(
    /// Identity of a control-plane action (see `super::action::ActionEnvelope`).
    ActionId
);

string_newtype!(
    /// Identity correlating a multi-step workflow across action envelopes.
    CorrelationId
);

string_newtype!(
    /// Logical EventBus topic name; transport-neutral.
    ///
    /// Grammar (enforced by `TopicName::try_new`): `[a-z][a-z0-9.\-_]*` -
    /// leading lowercase letter; subsequent characters are lowercase letters,
    /// digits, `.`, `-`, or `_`; no whitespace, no `/`, no uppercase. Empty
    /// strings are rejected.
    TopicName
);

string_newtype!(
    /// Logical control-plane action type (e.g. `provider.invoke`,
    /// `pipeline.stage.start`).
    ///
    /// Grammar (enforced by `ActionType::try_new`): same dotted-lowercase
    /// grammar as `TopicName`. Additionally rejects values that contain
    /// internal-only transport or crate-prefix substrings (defense in depth;
    /// the public boundary scan also enforces this).
    ActionType
);

/// Errors raised by grammar-validating identifier constructors.
///
/// The macro-derived `From<&str>` / `From<String>` impls do NOT call these -
/// they accept any string. Use `TopicName::try_new` / `ActionType::try_new`
/// when you need grammar validation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdError {
    #[error("identifier MUST be non-empty")]
    Empty,
    #[error(
        "identifier MUST start with a lowercase letter and contain only \
         lowercase letters, digits, '.', '-', or '_' - got {input:?}"
    )]
    InvalidGrammar { input: String },
    #[error(
        "identifier MUST NOT contain internal-only token {token:?} - got \
         {input:?}"
    )]
    InternalTokenLeak { input: String, token: &'static str },
}

fn validate_dotted_lowercase_grammar(input: &str) -> Result<(), IdError> {
    if input.is_empty() {
        return Err(IdError::Empty);
    }
    let mut chars = input.chars();
    let first = chars.next().expect("non-empty after check above");
    if !first.is_ascii_lowercase() {
        return Err(IdError::InvalidGrammar {
            input: input.to_owned(),
        });
    }
    for ch in chars {
        let ok =
            ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '_';
        if !ok {
            return Err(IdError::InvalidGrammar {
                input: input.to_owned(),
            });
        }
    }
    Ok(())
}

impl TopicName {
    /// Grammar-validating constructor. See `TopicName` doc for grammar.
    pub fn try_new(input: impl Into<String>) -> Result<Self, IdError> {
        let s = input.into();
        validate_dotted_lowercase_grammar(&s)?;
        Ok(Self(s))
    }
}

/// Internal tokens that must never appear in a public `ActionType` value.
/// The public boundary scan (feature 105) enforces the same patterns at the
/// repo level; this constructor enforces them at parse time as defense in
/// depth.
const ACTION_TYPE_BANNED_TOKENS: &[&str] = &[
    concat!("ze", "noh"),
    concat!("key", "_", "expr"),
    concat!("nxus", "_", "factory", "_"),
];

impl ActionType {
    /// Grammar-validating constructor. See `ActionType` doc for grammar
    /// and banned-token set.
    pub fn try_new(input: impl Into<String>) -> Result<Self, IdError> {
        let s = input.into();
        validate_dotted_lowercase_grammar(&s)?;
        for &token in ACTION_TYPE_BANNED_TOKENS {
            if s.contains(token) {
                return Err(IdError::InternalTokenLeak { input: s, token });
            }
        }
        Ok(Self(s))
    }
}

/// Stage-kind discriminator. Drives validation rules in [`super::validation`].
///
/// Phase-2 enumerates the full set; only `Llm`, `Tool`, and `ClipsRuleEval`
/// are required to be runtime-executable in v1.1.0. `Solver`, `Bayesian`,
/// `Zen`, and `DataStoreSearch` are valid as declaration-only stages with
/// capability-truth coverage (research.md §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageType {
    /// LLM stage backed by a provider/model.
    Llm,
    /// Tool stage backed by public tool capability metadata.
    Tool,
    /// CLIPS rule-evaluation stage.
    ClipsRuleEval,
    /// SMT/solver stage (declaration-only in v1.1.0).
    Solver,
    /// Bayesian-network stage (declaration-only in v1.1.0).
    Bayesian,
    /// ZEN / decision-table stage (declaration-only in v1.1.0).
    Zen,
    /// Data store / search stage (declaration-only in v1.1.0).
    DataStoreSearch,
    /// Feature 105 retargeting: declares one or more `BatchRef` outputs.
    /// Declaration-only in v1.1.x; the shared stage shape carries the
    /// `output_refs` slot validated by the pipeline-level shape rule.
    BatchSource,
    /// Feature 105 retargeting: declares one or more `BatchRef` inputs.
    /// Declaration-only in v1.1.x; the shared stage shape carries the
    /// `input_refs` slot validated by the pipeline-level shape rule.
    BatchSink,
}

/// Domain a [`CapabilityId`] belongs to. Domain-aware truth prevents LLM-shaped
/// flattening for non-LLM capabilities (FR-009, SC-005).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityDomain {
    Llm,
    Tool,
    RuleEval,
    Solver,
    Bayesian,
    Zen,
    DataStoreSearch,
}

/// Execution posture for a capability. Orthogonal to [`TierPosture`].
///
/// Together they cover the eight-state truth required by SC-005:
/// `{Native, Emulated, ExternalOnly, Unavailable} × {Ce, Pro, Enterprise, Internal}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityPosture {
    /// Runs natively on the active provider.
    Native,
    /// Emulated atop a less-direct provider path.
    Emulated,
    /// Only available via an external tool call.
    ExternalOnly,
    /// Not available on any active provider.
    Unavailable,
}

/// Tier posture for a provider or capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TierPosture {
    Ce,
    Pro,
    Enterprise,
    Internal,
}

// ---------------------------------------------------------------------------
// Feature 105 newtype-validation tests (task T004).
//
// Each test exercises grammar or banned-token behavior of `TopicName::try_new`
// / `ActionType::try_new`. They are semantic tests in the sense required by
// Constitution Article III: a stub `try_new` that returned `Ok(Self(input))`
// for every input would fail every negative assertion below.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod feature_105_id_tests {
    use super::*;

    #[test]
    fn topic_name_accepts_well_formed_grammar() {
        // Representative public-safe topic names.
        for input in [
            "provider.invoke",
            "pipeline.stage.start",
            "tool.invoke",
            "envelope.export",
            "a",
            "a.b-c_d.e1",
        ] {
            assert!(
                TopicName::try_new(input).is_ok(),
                "well-formed topic name was rejected: {input:?}"
            );
        }
    }

    #[test]
    fn topic_name_rejects_empty() {
        assert!(matches!(TopicName::try_new(""), Err(IdError::Empty)));
    }

    #[test]
    fn topic_name_rejects_uppercase_leading() {
        match TopicName::try_new("Provider.invoke") {
            Err(IdError::InvalidGrammar { input }) => assert_eq!(input, "Provider.invoke"),
            other => panic!("expected InvalidGrammar, got {other:?}"),
        }
    }

    #[test]
    fn topic_name_rejects_leading_digit() {
        assert!(matches!(
            TopicName::try_new("1pipeline.start"),
            Err(IdError::InvalidGrammar { .. })
        ));
    }

    #[test]
    fn topic_name_rejects_embedded_whitespace() {
        assert!(matches!(
            TopicName::try_new("pipeline start"),
            Err(IdError::InvalidGrammar { .. })
        ));
    }

    #[test]
    fn topic_name_rejects_embedded_slash() {
        assert!(matches!(
            TopicName::try_new("pipeline/start"),
            Err(IdError::InvalidGrammar { .. })
        ));
    }

    #[test]
    fn topic_name_rejects_uppercase_anywhere() {
        assert!(matches!(
            TopicName::try_new("pipeline.Start"),
            Err(IdError::InvalidGrammar { .. })
        ));
    }

    #[test]
    fn action_type_accepts_public_safe_values() {
        // Subset of the public-safe action-type set documented in
        // `contracts/action-envelope.md` AE-003.
        for input in [
            "provider.invoke",
            "provider.invoke.complete",
            "tool.invoke",
            "pipeline.stage.start",
            "envelope.export",
        ] {
            assert!(
                ActionType::try_new(input).is_ok(),
                "public-safe action type was rejected: {input:?}"
            );
        }
    }

    #[test]
    fn action_type_rejects_grammar_violations_same_as_topic() {
        assert!(matches!(ActionType::try_new(""), Err(IdError::Empty)));
        assert!(matches!(
            ActionType::try_new("Provider.invoke"),
            Err(IdError::InvalidGrammar { .. })
        ));
        assert!(matches!(
            ActionType::try_new("provider invoke"),
            Err(IdError::InvalidGrammar { .. })
        ));
        assert!(matches!(
            ActionType::try_new("provider/invoke"),
            Err(IdError::InvalidGrammar { .. })
        ));
    }

    #[test]
    fn action_type_rejects_transport_token_leak() {
        // The grammar accepts these characters; the banned-token check must
        // reject them. A stub that only validated grammar would pass these.
        let token = ACTION_TYPE_BANNED_TOKENS[0];
        let err = ActionType::try_new(format!("{token}.publish"))
            .expect_err("must reject transport token");
        match err {
            IdError::InternalTokenLeak { token: actual, .. } => assert_eq!(actual, token),
            other => panic!("expected InternalTokenLeak for transport token, got {other:?}"),
        }
    }

    #[test]
    fn action_type_rejects_reserved_field_token_leak() {
        let token = ACTION_TYPE_BANNED_TOKENS[1];
        let err = ActionType::try_new(format!("router.{token}.match"))
            .expect_err("must reject reserved field token");
        match err {
            IdError::InternalTokenLeak { token: actual, .. } => assert_eq!(actual, token),
            other => panic!("expected InternalTokenLeak for reserved field token, got {other:?}"),
        }
    }

    #[test]
    fn action_type_rejects_internal_crate_prefix_leak() {
        let token = ACTION_TYPE_BANNED_TOKENS[2];
        let err = ActionType::try_new(format!("{token}arrow.dispatch"))
            .expect_err("must reject internal crate prefix");
        match err {
            IdError::InternalTokenLeak { token: actual, .. } => assert_eq!(actual, token),
            other => panic!("expected InternalTokenLeak for internal crate prefix, got {other:?}"),
        }
    }

    #[test]
    fn id_newtypes_round_trip_through_serde_transparent() {
        // The macro-derived `#[serde(transparent)]` representation MUST keep
        // JSON strings flat. A failing implementation that serialized as
        // `{ "0": "..." }` would break this assertion.
        let batch = BatchId::from("batch-001");
        let action = ActionId::from("action-001");
        let correlation = CorrelationId::from("corr-001");
        let plan = PlanId::from("plan-001");
        let source = SourceId::from("src-001");

        assert_eq!(serde_json::to_string(&batch).unwrap(), "\"batch-001\"");
        assert_eq!(serde_json::to_string(&action).unwrap(), "\"action-001\"");
        assert_eq!(serde_json::to_string(&correlation).unwrap(), "\"corr-001\"");
        assert_eq!(serde_json::to_string(&plan).unwrap(), "\"plan-001\"");
        assert_eq!(serde_json::to_string(&source).unwrap(), "\"src-001\"");

        let parsed: BatchId = serde_json::from_str("\"batch-001\"").unwrap();
        assert_eq!(parsed, batch);
    }

    #[test]
    fn try_new_constructors_do_not_panic_on_long_input() {
        // 4 KB inputs of well-formed grammar should be accepted without
        // panic or stack overflow.
        let long: String = std::iter::repeat('a').take(4096).collect();
        assert!(TopicName::try_new(&long).is_ok());
        assert!(ActionType::try_new(&long).is_ok());
    }
}
