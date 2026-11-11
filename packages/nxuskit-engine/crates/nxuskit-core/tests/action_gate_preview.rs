use nxuskit_core::action_gate_preview::{PreviewDecision, PreviewStatus, evaluate_preview_json};
use serde_json::{Value, json};

const FIXTURES: &[&str] = &[
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/action_gate_preview/allow.json"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/action_gate_preview/deny.json"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/action_gate_preview/review-required.json"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/action_gate_preview/review-missing-handoff.json"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/action_gate_preview/missing-evidence.json"
    )),
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/action_gate_preview/forbidden-authority.json"
    )),
];

#[test]
fn shared_fixtures_match_the_safe_preview_projection() {
    for fixture_json in FIXTURES {
        let fixture: Value =
            serde_json::from_str(fixture_json).expect("fixture must be valid JSON");
        let request = fixture["request"].to_string();
        let actual = serde_json::to_value(evaluate_preview_json(&request))
            .expect("preview result must serialize");
        assert_eq!(
            normalize(actual),
            normalize(fixture["expected_result"].clone()),
            "fixture request must have the committed safe result projection"
        );
    }
}

#[test]
fn malformed_unknown_and_oversized_requests_fail_closed() {
    assert_invalid_request("{");
    assert_eq!(
        evaluate_preview_json(r#"{"schema_version":"unknown"}"#).status,
        PreviewStatus::InvalidRequest
    );
    let mut unknown_field = allow_fixture_request();
    unknown_field["unknown"] = json!(true);
    assert_invalid_request(&unknown_field.to_string());

    let mut unknown_enum = allow_fixture_request();
    unknown_enum["action"]["kind"] = json!("other_action");
    assert_invalid_request(&unknown_enum.to_string());

    let mut duplicate_reference = allow_fixture_request();
    duplicate_reference["evidence"]["evidence_refs"] = json!([
        "evidence.synthetic.allow.action",
        "evidence.synthetic.allow.action"
    ]);
    assert_invalid_request(&duplicate_reference.to_string());

    let mut invalid_reference = allow_fixture_request();
    invalid_reference["actor_context"]["actor_ref"] = json!("Synthetic.Actor.invalid");
    assert_invalid_request(&invalid_reference.to_string());

    assert_eq!(
        evaluate_preview_json(&"x".repeat(65_537)).decision,
        PreviewDecision::Deny
    );
}

fn allow_fixture_request() -> Value {
    serde_json::from_str::<Value>(FIXTURES[0]).expect("allow fixture must be valid JSON")["request"]
        .clone()
}

fn assert_invalid_request(request_json: &str) {
    let result = evaluate_preview_json(request_json);
    assert_eq!(result.status, PreviewStatus::InvalidRequest);
    assert_eq!(result.decision, PreviewDecision::Deny);
    assert!(!result.derived_from_typed_facts);
}

fn normalize(value: Value) -> Value {
    match value {
        Value::Array(values) => {
            let mut normalized = values.into_iter().map(normalize).collect::<Vec<_>>();
            normalized.sort_by_key(Value::to_string);
            Value::Array(normalized)
        }
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, normalize(value)))
                .collect(),
        ),
        value => value,
    }
}
