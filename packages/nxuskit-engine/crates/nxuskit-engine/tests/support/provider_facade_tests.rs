use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, atomic::Ordering};

use serde_json::{Value, json};

#[path = "provider_fixture_support.rs"]
mod provider_fixture_support;

#[path = "provider_facade_harness.rs"]
mod provider_facade_harness;

use super::{ProviderBackendOutcome, ProviderContractMapper, ProviderFacade};
use crate::error::{ProviderContractResult, ProviderErrorCode, ProviderTransportFailureKind};
use crate::providers::ollama::OllamaContractMapper;
use crate::providers::ollama::OllamaProvider;
use crate::providers::openai::OpenAIContractMapper;
use crate::providers::openai::OpenAIProvider;
use crate::types::{
    ProviderCapabilities, ProviderConfig, ProviderRequest, ProviderResponse, ProviderTransportMode,
};
use provider_facade_harness::{
    OrderTrackingMapper, RecordingBackend, blocked_backend, failure, http_ok_json_response,
    http_ok_truncated_response, immediate_backend, spawn_loopback_server,
    spawn_loopback_server_capturing_body,
};
use provider_fixture_support::{
    AssertionLedger, fixture_document, fixture_records, record_by_id, sha256,
};

const DEFINITION_REVISION: &str = "45965759912ae23da1572a7692434e572ce26503";
const FACADE_SCHEMA: &str = "nxus-spec119-rust-provider-facade-fixtures/v1";
const EXPECTED_ASSERTIONS: usize = 32;
/// The exact count of literal request-wire records in the accepted
/// `wires.json`, used as an independent request-mapping authority (see
/// `run_wire_record`) rather than a second call to the production mapper.
const WIRE_EXPECTED_ASSERTIONS: usize = 33;
const COMBINED_EXPECTED_ASSERTIONS: usize = EXPECTED_ASSERTIONS + WIRE_EXPECTED_ASSERTIONS;

const REQUESTS: &str = include_str!("../fixtures/provider_parity/requests.json");
const WIRES: &str = include_str!("../fixtures/provider_parity/wires.json");
const RESPONSES: &str = include_str!("../fixtures/provider_parity/responses.json");
const ERRORS: &str = include_str!("../fixtures/provider_parity/errors.json");
const FACADE: &str = include_str!("../fixtures/provider_parity/facade.json");
const PROVIDER_SOURCE: &str = include_str!("../../src/provider_facade.rs");

const ACCEPTED_FIXTURE_HASHES: [(&str, &str); 4] = [
    (
        "requests.json",
        "2d5e0d4530b74fc9402dc6f7fd5a4db9af60bb61912136134ca88c80cab0e1c3",
    ),
    (
        "wires.json",
        "d421f1dd61ef61592a269b4aeed275dc315360797c4ccce0c9788a6b769d28fb",
    ),
    (
        "responses.json",
        "a7cf9a1e6fc6355a52580b4d2aee2112afe97823e04d65139db552effba637f2",
    ),
    (
        "errors.json",
        "47bb77117c435ab24f3eee631a422c131740f2e53197ada16cbed778d9071d04",
    ),
];

const EXPECTED_RECORD_IDS: [&str; EXPECTED_ASSERTIONS] = [
    "OLLAMA-SUCCESS",
    "OLLAMA-INVALID-PREFLIGHT",
    "OLLAMA-UNSUPPORTED-PREFLIGHT",
    "OLLAMA-RATE-WITH-RETRY",
    "OLLAMA-RATE-WITHOUT-RETRY",
    "OLLAMA-TIMEOUT",
    "OLLAMA-NETWORK",
    "OLLAMA-MALFORMED-RESPONSE",
    "OPENAI-SUCCESS",
    "OPENAI-INVALID-PREFLIGHT",
    "OPENAI-UNSUPPORTED-PREFLIGHT",
    "OPENAI-RATE-WITH-RETRY",
    "OPENAI-RATE-WITHOUT-RETRY",
    "OPENAI-TIMEOUT",
    "OPENAI-NETWORK",
    "OPENAI-MALFORMED-RESPONSE",
    "LIFECYCLE-CLOSE-FIRST",
    "LIFECYCLE-CLOSE-REPEAT",
    "LIFECYCLE-POST-CLOSE-REJECT",
    "LIFECYCLE-CAPABILITIES-STABLE",
    "LIFECYCLE-INFLIGHT-MAY-FINISH",
    "LIFECYCLE-POST-CLOSE-CONCURRENT-REJECT",
    "LIFECYCLE-DROP-NO-BACKGROUND-WORK",
    "LIFECYCLE-RETRY-METADATA-DOES-NOT-RETRY",
    "SECURITY-SECRET-ABSENT",
    "SECURITY-RAW-BODY-ABSENT",
    "PROVENANCE-OLLAMA-ENDPOINT-ORIGIN",
    "PROVENANCE-OPENAI-ENDPOINT-ORIGIN",
    "MUTATION-WRONG-MAPPER",
    "MUTATION-DOUBLE-BACKEND-CALL",
    "MUTATION-CLOSE-ORDERING",
    "MUTATION-SECRET-REDACTION",
];

const EXPECTED_WIRE_RECORD_IDS: [&str; WIRE_EXPECTED_ASSERTIONS] = [
    "OLL-FORMAT-JSON",
    "OLL-FORMAT-SCHEMA",
    "OLL-FORMAT-TEXT-OMIT",
    "OLL-FORMAT-WRAPPER-REJECT",
    "THINK-AUTO-MODEL",
    "THINK-ENABLED",
    "THINK-DISABLED",
    "THINK-INVALID",
    "THINK-OMIT",
    "TOP-K-POSITIVE",
    "TOP-K-NEGATIVE",
    "TOP-K-OMIT",
    "MIN-P-POSITIVE",
    "MIN-P-NEGATIVE",
    "MIN-P-OMIT",
    "SEED-POSITIVE",
    "SEED-NEGATIVE",
    "SEED-OMIT",
    "PRESENCE-PENALTY-POSITIVE",
    "PRESENCE-PENALTY-NEGATIVE",
    "PRESENCE-PENALTY-OMIT",
    "FREQUENCY-PENALTY-POSITIVE",
    "FREQUENCY-PENALTY-NEGATIVE",
    "FREQUENCY-PENALTY-OMIT",
    "REASONING-EFFORT-POSITIVE",
    "REASONING-EFFORT-NEGATIVE",
    "REASONING-EFFORT-OMIT",
    "LOGPROBS-POSITIVE",
    "LOGPROBS-NEGATIVE",
    "LOGPROBS-OMIT",
    "TOOLS-POSITIVE",
    "TOOLS-NEGATIVE",
    "TOOLS-OMIT",
];

fn facade_records() -> Vec<Value> {
    fixture_records(FACADE, "facade.json")
}

fn wire_records() -> Vec<Value> {
    fixture_records(WIRES, "wires.json")
}

/// Structural check on the accepted `wires.json` records used as the
/// independent request-wire authority: exact count, unique ids matching
/// `EXPECTED_WIRE_RECORD_IDS`, and required, non-empty `control_paths`.
fn assert_wire_fixture_structure() {
    let records = wire_records();
    assert_eq!(records.len(), WIRE_EXPECTED_ASSERTIONS);
    let mut ids = BTreeSet::new();
    for record in &records {
        let id = record["id"].as_str().expect("wire record id");
        assert!(ids.insert(id.to_owned()), "duplicate wire record id: {id}");
        assert!(
            record["anchor"]
                .as_str()
                .is_some_and(|anchor| matches!(anchor, "ollama_local" | "openai")),
            "{id}: unknown wire record anchor: {}",
            record["anchor"]
        );
        let control_paths: Vec<String> = record["control_paths"]
            .as_array()
            .unwrap_or_else(|| panic!("{id}: control_paths must be an array"))
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .unwrap_or_else(|| panic!("{id}: control_paths entries must be strings"))
                    .to_owned()
            })
            .collect();
        assert!(
            control_paths_are_valid(&control_paths),
            "{id}: control_paths must be nonempty, unique, strict lowercase dotted paths drawn \
             from the frozen accepted vocabulary (control_paths={control_paths:?})"
        );
        assert!(
            record["expected_wire_fragment"].is_object(),
            "{id}: expected_wire_fragment must be an object"
        );
    }
    assert_eq!(
        ids,
        EXPECTED_WIRE_RECORD_IDS
            .into_iter()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>()
    );
}

fn assert_fixture_structure() {
    let document = fixture_document(FACADE, "facade.json");
    let object = document
        .as_object()
        .expect("facade.json root must be an object");
    let keys: BTreeSet<&str> = object.keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        BTreeSet::from([
            "accepted_fixture_sha256",
            "definition_revision",
            "records",
            "schema_version",
        ])
    );
    assert_eq!(document["schema_version"], FACADE_SCHEMA);
    assert_eq!(document["definition_revision"], DEFINITION_REVISION);

    let accepted_bytes = [
        ("requests.json", REQUESTS.as_bytes()),
        ("wires.json", WIRES.as_bytes()),
        ("responses.json", RESPONSES.as_bytes()),
        ("errors.json", ERRORS.as_bytes()),
    ];
    for ((name, expected), (actual_name, bytes)) in
        ACCEPTED_FIXTURE_HASHES.iter().zip(accepted_bytes)
    {
        assert_eq!(*name, actual_name);
        assert_eq!(sha256(bytes), *expected, "{name}: fixture byte drift");
        assert_eq!(document["accepted_fixture_sha256"][*name], *expected);
    }

    let all_source_ids: BTreeSet<String> = [REQUESTS, WIRES, RESPONSES, ERRORS]
        .into_iter()
        .flat_map(|raw| fixture_records(raw, "accepted fixture"))
        .map(|record| record["id"].as_str().expect("source record id").to_owned())
        .collect();

    let records = facade_records();
    assert_eq!(records.len(), EXPECTED_ASSERTIONS);
    let mut ids = BTreeSet::new();
    let expected_keys = BTreeSet::from([
        "class",
        "expected",
        "id",
        "oracle_case_id",
        "provider_id",
        "request_case_id",
    ]);
    for record in &records {
        let record_object = record.as_object().expect("facade record must be an object");
        let record_keys: BTreeSet<&str> = record_object.keys().map(String::as_str).collect();
        assert_eq!(record_keys, expected_keys, "record key drift: {record:?}");
        let id = record["id"].as_str().expect("facade record id");
        assert!(
            ids.insert(id.to_owned()),
            "duplicate facade record id: {id}"
        );
        let class = record["class"].as_str().expect("facade record class");
        assert!(
            matches!(class, "provider" | "lifecycle" | "security" | "mutation"),
            "unknown facade record class: {class}"
        );
        assert!(
            matches!(
                record["provider_id"].as_str(),
                Some("ollama_local" | "openai")
            ),
            "unknown facade provider: {}",
            record["provider_id"]
        );
        for field in ["request_case_id", "oracle_case_id"] {
            let source_id = record[field].as_str().expect("source fixture id");
            assert!(
                all_source_ids.contains(source_id),
                "{id}: {field} references unknown accepted fixture id {source_id}"
            );
        }
        assert!(
            record["expected"]
                .as_object()
                .is_some_and(|value| !value.is_empty()),
            "{id}: expected object must be nonempty"
        );
    }
    assert_eq!(
        ids,
        EXPECTED_RECORD_IDS
            .into_iter()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>()
    );
}

fn config_for(provider_id: &str) -> ProviderConfig {
    let response_id = match provider_id {
        "ollama_local" => "RESPONSE-LITERAL-OLLAMA-LOCAL",
        "openai" => "RESPONSE-LITERAL-OPENAI",
        other => panic!("unsupported test provider: {other}"),
    };
    serde_json::from_value(record_by_id(RESPONSES, "responses.json", response_id)["config"].clone())
        .expect("response fixture config must deserialize")
}

fn request_by_id(id: &str) -> ProviderRequest {
    serde_json::from_value(record_by_id(REQUESTS, "requests.json", id)["request"].clone())
        .expect("request fixture must deserialize")
}

fn success_wire(provider_id: &str) -> Value {
    let response_id = match provider_id {
        "ollama_local" => "RESPONSE-LITERAL-OLLAMA-LOCAL",
        "openai" => "RESPONSE-LITERAL-OPENAI",
        other => panic!("unsupported test provider: {other}"),
    };
    record_by_id(RESPONSES, "responses.json", response_id)["raw_wire"].clone()
}

fn mapper_for(provider_id: &str) -> Arc<dyn ProviderContractMapper> {
    let config = config_for(provider_id);
    match provider_id {
        "ollama_local" => Arc::new(OllamaContractMapper::new(config).expect("Ollama mapper")),
        "openai" => Arc::new(OpenAIContractMapper::new(config).expect("OpenAI mapper")),
        other => panic!("unsupported test provider: {other}"),
    }
}

/// Builds a real mapper from a `wires.json` record's own literal `config`,
/// keyed on the record's `anchor` rather than a shared test config. This is
/// the independent-authority counterpart to `mapper_for`, which is reserved
/// for the pre-existing `facade.json` behavioral records.
fn wire_mapper_for(anchor: &str, config: ProviderConfig) -> Arc<dyn ProviderContractMapper> {
    match anchor {
        "ollama_local" => Arc::new(OllamaContractMapper::new(config).unwrap_or_else(|error| {
            panic!("wire config must construct an Ollama mapper: {error:?}")
        })),
        "openai" => Arc::new(OpenAIContractMapper::new(config).unwrap_or_else(|error| {
            panic!("wire config must construct an OpenAI mapper: {error:?}")
        })),
        other => panic!("unsupported wire anchor: {other}"),
    }
}

/// Looks up a dot-separated path (e.g. `"options.top_k"`) in a JSON object,
/// returning `None` if any segment is absent.
fn value_at_dotted_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

/// The exact frozen `wires.json` control-path vocabulary: only these dotted
/// object-key paths may govern a wire record's oracle comparison.
const ACCEPTED_CONTROL_PATHS: [&str; 11] = [
    "format",
    "frequency_penalty",
    "logprobs",
    "options.min_p",
    "options.seed",
    "options.top_k",
    "presence_penalty",
    "reasoning_effort",
    "think",
    "tools",
    "top_logprobs",
];

/// The single fail-closed source of truth for control-path validity, shared
/// by `assert_wire_fixture_structure` (parsed-fixture validation) and
/// `wire_fragment_matches` (per-comparison validation): nonempty, unique,
/// strict lowercase dotted-object-key grammar (`[a-z0-9_]+` segments joined
/// by exactly one `.`, no leading/trailing/repeated dot), and every entry a
/// member of `ACCEPTED_CONTROL_PATHS`.
fn control_paths_are_valid(control_paths: &[String]) -> bool {
    if control_paths.is_empty() {
        return false;
    }
    let mut seen = BTreeSet::new();
    control_paths.iter().all(|path| {
        let segments_are_valid = path.split('.').all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        });
        seen.insert(path.as_str())
            && segments_are_valid
            && ACCEPTED_CONTROL_PATHS.contains(&path.as_str())
    })
}

/// The independent `wires.json` request-wire oracle core: for each declared
/// `control_paths` entry, checks the actual wire's value at that path
/// equals the accepted `expected_wire_fragment`'s value at that path, or
/// that the path is absent from both when `expected_wire_fragment` omits
/// it. Never consults `mapper_for`, a mapper instance, or any other
/// production request builder — only the two `Value` trees it is given.
/// Returns `false` on the first mismatched control path rather than
/// panicking, so the mutation canary (`spec119_wire_oracle_mutation_is_killed`)
/// can exercise this exact function and observe a real rejection.
fn wire_fragment_matches(
    wire: &Value,
    expected_fragment: &Value,
    control_paths: &[String],
) -> bool {
    if !control_paths_are_valid(control_paths) {
        return false;
    }
    control_paths.iter().all(|path| {
        let expected = value_at_dotted_path(expected_fragment, path);
        let actual = value_at_dotted_path(wire, path);
        match expected {
            Some(expected_value) => actual == Some(expected_value),
            None => actual.is_none(),
        }
    })
}

fn assert_wire_fragment(
    id: &str,
    wire: &Value,
    expected_fragment: &Value,
    control_paths: &[String],
) {
    assert!(
        !control_paths.is_empty(),
        "{id}: control_paths must be nonempty"
    );
    assert!(
        wire_fragment_matches(wire, expected_fragment, control_paths),
        "{id}: wire drifted from the independent wires.json expected_wire_fragment at one or \
         more control_paths (wire={wire}, expected_fragment={expected_fragment}, \
         control_paths={control_paths:?})"
    );
}

/// Runs one `wires.json` record end-to-end through a real `ProviderFacade`:
/// the real `ProviderRequest` deserializer, the real provider mapper
/// (selected by the record's own `anchor`/`config`), and a `RecordingBackend`
/// that captures the exact wire the facade delivered to it. Checks the
/// outcome against the record's own `expected_error` and
/// `expected_wire_fragment` — the independent literal authority, never a
/// second call to the same production mapper being tested.
async fn run_wire_record(record: &Value) {
    let id = record["id"].as_str().expect("wire record id");
    let anchor = record["anchor"].as_str().expect("wire record anchor");
    let config: ProviderConfig = serde_json::from_value(record["config"].clone())
        .unwrap_or_else(|error| panic!("{id}: wire config fixture must deserialize: {error:?}"));
    assert_eq!(
        config.provider_id, anchor,
        "{id}: wire record anchor/config provider_id mismatch"
    );
    let expected_error = record["expected_error"].as_str();
    let expected_fragment = record["expected_wire_fragment"].clone();
    let control_paths: Vec<String> = record["control_paths"]
        .as_array()
        .unwrap_or_else(|| panic!("{id}: control_paths must be an array"))
        .iter()
        .map(|value| {
            value
                .as_str()
                .unwrap_or_else(|| panic!("{id}: control_paths entries must be strings"))
                .to_owned()
        })
        .collect();

    let request_result: Result<ProviderRequest, serde_json::Error> =
        serde_json::from_value(record["request"].clone());

    let request = match request_result {
        Err(deserialize_error) => {
            assert_eq!(
                expected_error,
                Some("invalid_request"),
                "{id}: request fixture failed to deserialize ({deserialize_error}) but \
                 expected_error was {expected_error:?}"
            );
            assert_eq!(
                expected_fragment,
                json!({}),
                "{id}: a deserialization-failure record must carry an empty expected_wire_fragment"
            );
            return;
        }
        Ok(request) => request,
    };

    let mapper = wire_mapper_for(anchor, config);
    let backend = immediate_backend(ProviderBackendOutcome::Response(success_wire(anchor)));
    let facade = ProviderFacade::new(
        mapper,
        Arc::new(backend.clone()),
        ProviderCapabilities::default(),
    );
    let result = facade.chat(&request).await;

    match expected_error {
        Some(expected_name) => {
            let error = result.expect_err(&format!("{id}: expected a validate/mapping failure"));
            assert_eq!(
                error.code,
                expected_code(expected_name),
                "{id}: wrong wire-mapping error code"
            );
            assert_eq!(
                expected_fragment,
                json!({}),
                "{id}: an expected_error record must carry an empty expected_wire_fragment"
            );
            assert_eq!(
                backend.calls.load(Ordering::SeqCst),
                0,
                "{id}: an expected request/mapping failure must never reach the backend"
            );
            assert!(
                backend.captured_wire_is_absent(),
                "{id}: an expected request/mapping failure must not capture a wire"
            );
        }
        None => {
            result.unwrap_or_else(|error| {
                panic!("{id}: expected a successful chat call, got an error instead: {error:?}")
            });
            assert_eq!(
                backend.calls.load(Ordering::SeqCst),
                1,
                "{id}: expected exactly one backend call"
            );
            let wire = backend.captured_wire();
            assert_wire_fragment(id, &wire, &expected_fragment, &control_paths);
        }
    }
}

fn facade_with_backend(
    provider_id: &str,
    backend: RecordingBackend,
) -> (ProviderFacade, RecordingBackend) {
    let mapper: Arc<dyn ProviderContractMapper> = Arc::new(OrderTrackingMapper {
        inner: mapper_for(provider_id),
        events: Arc::clone(&backend.events),
    });
    let facade = ProviderFacade::new(
        mapper,
        Arc::new(backend.clone()),
        ProviderCapabilities::default(),
    );
    (facade, backend)
}

fn expected_code(name: &str) -> ProviderErrorCode {
    match name {
        "authentication" => ProviderErrorCode::Authentication,
        "invalid_request" => ProviderErrorCode::InvalidRequest,
        "malformed_response" => ProviderErrorCode::MalformedResponse,
        "network" => ProviderErrorCode::Network,
        "rate_limited" => ProviderErrorCode::RateLimited,
        "timeout" => ProviderErrorCode::Timeout,
        "unavailable" => ProviderErrorCode::Unavailable,
        "unsupported_capability" => ProviderErrorCode::UnsupportedCapability,
        other => panic!("unexpected fixture error code: {other}"),
    }
}

fn outcome_for(record: &Value) -> ProviderBackendOutcome {
    let id = record["id"].as_str().expect("record id");
    if id.ends_with("SUCCESS")
        || id.contains("ENDPOINT-ORIGIN")
        || id == "MUTATION-DOUBLE-BACKEND-CALL"
        || id.starts_with("LIFECYCLE-") && !id.contains("RETRY-METADATA")
    {
        return ProviderBackendOutcome::Response(success_wire(
            record["provider_id"].as_str().expect("provider id"),
        ));
    }
    if id.contains("RATE-WITH-RETRY") || id.contains("RETRY-METADATA") {
        return failure(
            Some(429),
            Some("2"),
            None,
            ProviderTransportFailureKind::None,
        );
    }
    if id.contains("RATE-WITHOUT-RETRY") {
        return failure(Some(429), None, None, ProviderTransportFailureKind::None);
    }
    if id.ends_with("TIMEOUT") {
        return failure(None, None, None, ProviderTransportFailureKind::Timeout);
    }
    if id.ends_with("NETWORK") {
        return failure(None, None, None, ProviderTransportFailureKind::Network);
    }
    if id.contains("MALFORMED") {
        return ProviderBackendOutcome::Response(json!({"not": "a provider response"}));
    }
    ProviderBackendOutcome::Response(success_wire(
        record["provider_id"].as_str().expect("provider id"),
    ))
}

fn expected_backend_calls(record: &Value) -> usize {
    record["expected"]["backend_calls"].as_u64().unwrap_or(1) as usize
}

/// The exact mapper/backend invocation order a single accepted `chat` call
/// must produce for the given provider record, per `ProviderFacade::chat`:
/// one request mapping, then (if accepted) exactly one backend call, then
/// exactly one response/failure mapping.
fn expected_event_sequence(record: &Value) -> Vec<&'static str> {
    let id = record["id"].as_str().expect("record id");
    if id.contains("INVALID-PREFLIGHT") {
        Vec::new()
    } else if id.contains("UNSUPPORTED-PREFLIGHT") {
        vec!["request_to_wire"]
    } else if id.ends_with("SUCCESS") || id.contains("MALFORMED") {
        vec!["request_to_wire", "backend_execute", "response_from_wire"]
    } else {
        vec!["request_to_wire", "backend_execute", "error_from_failure"]
    }
}

/// Full canonical `ProviderResponse` fixture object for a success record's
/// `oracle_case_id`, normalized so that an absent `error` key (the wire
/// format `ProviderResponse` uses for `None`) compares equal to the
/// fixture's explicit `"error": null`.
fn expected_success_response(record: &Value) -> Value {
    let oracle_case_id = record["oracle_case_id"].as_str().expect("oracle case id");
    record_by_id(RESPONSES, "responses.json", oracle_case_id)["expected"].clone()
}

fn response_to_comparable_value(response: &ProviderResponse) -> Value {
    let mut value = serde_json::to_value(response).expect("ProviderResponse must serialize");
    if let Value::Object(map) = &mut value {
        map.entry("error".to_string()).or_insert(Value::Null);
    }
    value
}

async fn run_provider_record(record: &Value) {
    let provider_id = record["provider_id"].as_str().expect("provider id");
    let id = record["id"].as_str().expect("record id");
    let mut request = request_by_id(record["request_case_id"].as_str().expect("request id"));
    if id.contains("UNSUPPORTED-PREFLIGHT") {
        if provider_id == "ollama_local" {
            request.presence_penalty = Some(0.25);
        } else {
            request.top_k = Some(40);
        }
    }
    let backend = immediate_backend(outcome_for(record));
    let (facade, backend) = facade_with_backend(provider_id, backend);
    let result = facade.chat(&request).await;

    if record["expected"].get("result").and_then(Value::as_str) == Some("success") {
        let response = result.unwrap_or_else(|error| panic!("{id}: unexpected error: {error:?}"));
        assert!(response.error.is_none(), "{id}: success carried an error");
        assert_eq!(
            response_to_comparable_value(&response),
            expected_success_response(record),
            "{id}: canonical ProviderResponse drifted from the accepted responses.json expected object"
        );
    } else {
        let error = result.expect_err(&format!("{id}: expected failure"));
        let expected = record["expected"]["error_code"]
            .as_str()
            .expect("expected error code");
        assert_eq!(
            error.code,
            expected_code(expected),
            "{id}: wrong error code"
        );
        if record["expected"].get("retry_after_ms").is_some() {
            assert_eq!(
                error.retry_after_ms,
                record["expected"]["retry_after_ms"].as_u64(),
                "{id}: retry metadata drift"
            );
        }
    }
    let expected_calls = expected_backend_calls(record);
    assert_eq!(
        backend.calls.load(Ordering::SeqCst),
        expected_calls,
        "{id}: backend call count drift"
    );
    assert_eq!(
        backend.events_snapshot(),
        expected_event_sequence(record),
        "{id}: mapper/backend invocation order drifted from one request mapping, one backend call, one response/failure mapping"
    );
}

fn capabilities_snapshot(capabilities: &ProviderCapabilities) -> (bool, bool, bool, bool, bool) {
    (
        capabilities.supports_system_messages,
        capabilities.supports_streaming,
        capabilities.supports_vision,
        capabilities.supports_json_mode,
        capabilities.supports_json_schema,
    )
}

async fn run_lifecycle_record(record: &Value) {
    let id = record["id"].as_str().expect("record id");
    let request = request_by_id("REQ-VALID-COMPLETE");
    match id {
        "LIFECYCLE-CLOSE-FIRST" => {
            let (facade, backend) = facade_with_backend(
                "ollama_local",
                immediate_backend(ProviderBackendOutcome::Response(success_wire(
                    "ollama_local",
                ))),
            );
            facade.close().await.expect("first close");
            assert!(facade.is_closed());
            assert_eq!(backend.calls.load(Ordering::SeqCst), 0);
        }
        "LIFECYCLE-CLOSE-REPEAT" => {
            let (facade, backend) = facade_with_backend(
                "ollama_local",
                immediate_backend(ProviderBackendOutcome::Response(success_wire(
                    "ollama_local",
                ))),
            );
            facade.close().await.expect("first close");
            facade.close().await.expect("repeat close");
            assert!(facade.is_closed());
            assert_eq!(backend.calls.load(Ordering::SeqCst), 0);
        }
        "LIFECYCLE-POST-CLOSE-REJECT" => {
            let (facade, backend) = facade_with_backend(
                "ollama_local",
                immediate_backend(ProviderBackendOutcome::Response(success_wire(
                    "ollama_local",
                ))),
            );
            facade.close().await.expect("close");
            let error = facade.chat(&request).await.expect_err("post-close chat");
            assert_eq!(error.code, ProviderErrorCode::Unavailable);
            assert_eq!(backend.calls.load(Ordering::SeqCst), 0);
        }
        "LIFECYCLE-CAPABILITIES-STABLE" => {
            let (facade, _) = facade_with_backend(
                "ollama_local",
                immediate_backend(ProviderBackendOutcome::Response(success_wire(
                    "ollama_local",
                ))),
            );
            let before = capabilities_snapshot(&facade.capabilities());
            facade.close().await.expect("close");
            assert_eq!(before, capabilities_snapshot(&facade.capabilities()));
        }
        "LIFECYCLE-INFLIGHT-MAY-FINISH" | "LIFECYCLE-POST-CLOSE-CONCURRENT-REJECT" => {
            let backend = blocked_backend(ProviderBackendOutcome::Response(success_wire(
                "ollama_local",
            )));
            let entered = Arc::clone(&backend.entered);
            let release = Arc::clone(&backend.release);
            let (facade, backend) = facade_with_backend("ollama_local", backend);
            let facade = Arc::new(facade);
            let inflight_facade = Arc::clone(&facade);
            let inflight_request = request.clone();
            let inflight =
                tokio::spawn(async move { inflight_facade.chat(&inflight_request).await });
            entered.wait();
            facade.close().await.expect("close during inflight request");
            if id == "LIFECYCLE-POST-CLOSE-CONCURRENT-REJECT" {
                let error = facade
                    .chat(&request)
                    .await
                    .expect_err("new post-close request");
                assert_eq!(error.code, ProviderErrorCode::Unavailable);
                assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
            }
            release.wait();
            inflight
                .await
                .expect("inflight task join")
                .expect("inflight request may finish");
            assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
        }
        "LIFECYCLE-DROP-NO-BACKGROUND-WORK" => {
            let (facade, backend) = facade_with_backend(
                "ollama_local",
                immediate_backend(ProviderBackendOutcome::Response(success_wire(
                    "ollama_local",
                ))),
            );
            drop(facade);
            assert_eq!(backend.calls.load(Ordering::SeqCst), 0);
        }
        "LIFECYCLE-RETRY-METADATA-DOES-NOT-RETRY" => {
            let backend = immediate_backend(failure(
                Some(429),
                Some("2"),
                None,
                ProviderTransportFailureKind::None,
            ));
            let (facade, backend) = facade_with_backend("ollama_local", backend);
            let error = facade.chat(&request).await.expect_err("rate limit");
            assert_eq!(error.code, ProviderErrorCode::RateLimited);
            assert_eq!(error.retry_after_ms, Some(2000));
            assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
        }
        other => panic!("unknown lifecycle record: {other}"),
    }
}

async fn run_security_record(record: &Value) {
    let id = record["id"].as_str().expect("record id");
    let request = request_by_id("REQ-VALID-COMPLETE");
    match id {
        "SECURITY-SECRET-ABSENT" | "SECURITY-RAW-BODY-ABSENT" => {
            let forbidden = record["expected"]["forbidden"]
                .as_str()
                .expect("forbidden literal");
            let status = if id == "SECURITY-SECRET-ABSENT" {
                Some(401)
            } else {
                Some(200)
            };
            let backend = immediate_backend(failure(
                status,
                None,
                Some(forbidden),
                ProviderTransportFailureKind::None,
            ));
            let (facade, _) = facade_with_backend("openai", backend);
            let error = facade.chat(&request).await.expect_err("security failure");
            let rendered = serde_json::to_string(&error).expect("serialize provider error");
            assert!(
                !rendered.contains(forbidden),
                "{id}: sensitive input leaked"
            );
        }
        "PROVENANCE-OLLAMA-ENDPOINT-ORIGIN" | "PROVENANCE-OPENAI-ENDPOINT-ORIGIN" => {
            let provider_id = record["provider_id"].as_str().expect("provider id");
            let backend =
                immediate_backend(ProviderBackendOutcome::Response(success_wire(provider_id)));
            let (facade, _) = facade_with_backend(provider_id, backend);
            let response = facade.chat(&request).await.expect("provenance response");
            assert_eq!(
                response.descriptor.endpoint_origin.as_deref(),
                record["expected"]["endpoint_origin"].as_str()
            );
        }
        other => panic!("unknown security record: {other}"),
    }
}

async fn run_mutation_record(record: &Value) {
    let id = record["id"].as_str().expect("record id");
    let request = request_by_id("REQ-VALID-COMPLETE");
    match id {
        "MUTATION-WRONG-MAPPER" => {
            let backend =
                immediate_backend(ProviderBackendOutcome::Response(success_wire("openai")));
            let (facade, backend) = facade_with_backend("ollama_local", backend);
            let error = facade
                .chat(&request)
                .await
                .expect_err("wrong mapper mutation survived");
            assert_eq!(
                error.code,
                ProviderErrorCode::MalformedResponse,
                "wrong-mapper mutation must surface as a malformed response, not silently \
                 succeed or misclassify"
            );
            assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
        }
        "MUTATION-DOUBLE-BACKEND-CALL" => {
            let backend = immediate_backend(ProviderBackendOutcome::Response(success_wire(
                "ollama_local",
            )));
            let (facade, backend) = facade_with_backend("ollama_local", backend);
            facade.chat(&request).await.expect("single call success");
            assert_eq!(
                backend.calls.load(Ordering::SeqCst),
                1,
                "double-call mutation survived"
            );
        }
        "MUTATION-CLOSE-ORDERING" => {
            let backend = immediate_backend(ProviderBackendOutcome::Response(success_wire(
                "ollama_local",
            )));
            let (facade, backend) = facade_with_backend("ollama_local", backend);
            facade.close().await.expect("close");
            let error = facade.chat(&request).await.expect_err("closed facade");
            assert_eq!(error.code, ProviderErrorCode::Unavailable);
            assert_eq!(
                backend.calls.load(Ordering::SeqCst),
                0,
                "close-order mutation survived"
            );
        }
        "MUTATION-SECRET-REDACTION" => {
            let secret = record["expected"]["forbidden"]
                .as_str()
                .expect("secret literal");
            let backend = immediate_backend(failure(
                Some(200),
                None,
                Some(secret),
                ProviderTransportFailureKind::None,
            ));
            let (facade, _) = facade_with_backend("openai", backend);
            let error = facade.chat(&request).await.expect_err("malformed response");
            assert!(
                !error.message.contains(secret),
                "secret-redaction mutation survived"
            );
        }
        other => panic!("unknown mutation record: {other}"),
    }
}

async fn run_all_records() {
    let mut ledger = AssertionLedger::new();
    for record in facade_records() {
        match record["class"].as_str().expect("record class") {
            "provider" => run_provider_record(&record).await,
            "lifecycle" => run_lifecycle_record(&record).await,
            "security" => run_security_record(&record).await,
            "mutation" => run_mutation_record(&record).await,
            other => panic!("unknown record class: {other}"),
        }
        ledger.record(record["id"].as_str().expect("record id"));
    }
    assert_eq!(ledger.ids.len(), EXPECTED_ASSERTIONS);
}

/// Drives every accepted `wires.json` record through the independent
/// request-wire oracle (`run_wire_record`). No marker is printed here: the
/// combined `SPEC119_RUST_FACADE_ASSERTIONS` marker is only emitted once
/// both this and `run_all_records` have completed, so a partially-executed
/// suite cannot be mistaken for a passing one.
async fn run_all_wire_records() {
    let mut ledger = AssertionLedger::new();
    for record in wire_records() {
        run_wire_record(&record).await;
        ledger.record(record["id"].as_str().expect("wire record id"));
    }
    assert_eq!(ledger.ids.len(), WIRE_EXPECTED_ASSERTIONS);
}

#[test]
fn spec119_facade_fixture_structure_is_exact() {
    assert_fixture_structure();
}

#[test]
fn spec119_facade_visibility_exports_and_anchor_signatures_are_exact() {
    assert!(
        PROVIDER_SOURCE.contains("pub(crate) enum ProviderBackendOutcome"),
        "backend outcome must remain crate-private"
    );
    assert!(
        PROVIDER_SOURCE.contains("pub(crate) trait ProviderExecutionBackend"),
        "backend trait must remain crate-private"
    );
    let facade_impl = PROVIDER_SOURCE
        .split_once("impl ProviderFacade {")
        .expect("ProviderFacade implementation")
        .1;
    let constructor_line = facade_impl
        .lines()
        .find(|line| line.contains("fn new("))
        .expect("ProviderFacade constructor")
        .trim();
    assert_eq!(
        constructor_line, "pub(crate) fn new(",
        "facade constructor must remain crate-private"
    );

    let _: fn(OllamaProvider) -> ProviderContractResult<crate::ProviderFacade> =
        OllamaProvider::into_contract_facade;
    let _: fn(OpenAIProvider) -> ProviderContractResult<crate::ProviderFacade> =
        OpenAIProvider::into_contract_facade;

    fn accepts_root_export(_: &crate::ProviderFacade) {}
    fn accepts_prelude_export(_: &crate::prelude::ProviderFacade) {}
    let _ = (accepts_root_export, accepts_prelude_export);
}

#[test]
fn spec119_facade_anchor_construction_is_offline_and_redacted() {
    let ollama = OllamaProvider::builder()
        .base_url("http://127.0.0.1:11434")
        .model("qwen3:latest")
        .build()
        .expect("offline Ollama construction");
    let ollama_facade = ollama
        .into_contract_facade()
        .expect("Ollama anchor conversion");
    assert!(!ollama_facade.is_closed());
    assert!(ollama_facade.capabilities().supports_streaming);

    let openai = OpenAIProvider::builder()
        .api_key("sk-fixture-secret-must-not-leak")
        .base_url("https://api.openai.example/v1")
        .model("gpt-4o-mini")
        .build()
        .expect("offline OpenAI construction");
    let openai_facade = openai
        .into_contract_facade()
        .expect("OpenAI anchor conversion");
    assert!(!openai_facade.is_closed());
    assert!(openai_facade.capabilities().supports_streaming);
    assert!(!format!("{openai_facade:?}").contains("sk-fixture-secret-must-not-leak"));
}

/// Drives one `wires.json` record's literal `config`/`request` through the
/// real `OllamaProvider`/`OpenAIProvider` anchor over a capturing loopback
/// server, and returns the exact JSON body the anchor's own `reqwest`
/// client sent — never a value computed by calling a mapper directly.
async fn wire_record_http_body(record: &Value) -> Value {
    let id = record["id"].as_str().expect("wire record id");
    let anchor = record["anchor"].as_str().expect("wire record anchor");
    let config: ProviderConfig = serde_json::from_value(record["config"].clone())
        .unwrap_or_else(|error| panic!("{id}: wire config fixture must deserialize: {error:?}"));
    let request: ProviderRequest = serde_json::from_value(record["request"].clone())
        .unwrap_or_else(|error| panic!("{id}: wire request fixture must deserialize: {error:?}"));
    let response = http_ok_json_response(&success_wire(anchor));
    let (addr, body_rx) = spawn_loopback_server_capturing_body(response).await;

    let facade = match anchor {
        "ollama_local" => OllamaProvider::builder()
            .base_url(format!("http://{addr}"))
            .model(config.model.clone())
            .build()
            .unwrap_or_else(|error| panic!("{id}: offline Ollama construction: {error:?}"))
            .into_contract_facade()
            .unwrap_or_else(|error| panic!("{id}: Ollama anchor conversion: {error:?}")),
        "openai" => OpenAIProvider::builder()
            .api_key("sk-fixture-secret-must-not-leak")
            .base_url(format!("http://{addr}"))
            .model(config.model.clone())
            .build()
            .unwrap_or_else(|error| panic!("{id}: offline OpenAI construction: {error:?}"))
            .into_contract_facade()
            .unwrap_or_else(|error| panic!("{id}: OpenAI anchor conversion: {error:?}")),
        other => panic!("{id}: unsupported wire anchor: {other}"),
    };

    facade
        .chat(&request)
        .await
        .unwrap_or_else(|error| panic!("{id}: real anchor chat call must succeed: {error:?}"));

    let body_bytes = body_rx.await.unwrap_or_else(|error| {
        panic!("{id}: loopback server must capture a request body: {error:?}")
    });
    serde_json::from_slice(&body_bytes)
        .unwrap_or_else(|error| panic!("{id}: captured request body must be JSON: {error}"))
}

#[tokio::test]
async fn spec119_facade_anchors_bind_their_own_mapper_not_the_other_providers() {
    let request = request_by_id("REQ-VALID-COMPLETE");

    // The Ollama anchor pointed at a loopback server returning OpenAI's
    // wire shape must fail via its own OllamaContractMapper — proving the
    // real `OllamaProvider::into_contract_facade` binding, not a swapped
    // mapper.
    let ollama_wrong_addr =
        spawn_loopback_server(http_ok_json_response(&success_wire("openai"))).await;
    let ollama_wrong_facade = OllamaProvider::builder()
        .base_url(format!("http://{ollama_wrong_addr}"))
        .model("qwen3:latest")
        .build()
        .expect("offline Ollama construction")
        .into_contract_facade()
        .expect("Ollama anchor conversion");
    let error = ollama_wrong_facade
        .chat(&request)
        .await
        .expect_err("Ollama anchor must reject an OpenAI-shaped wire body");
    assert_eq!(
        error.code,
        ProviderErrorCode::MalformedResponse,
        "Ollama anchor must classify an OpenAI-shaped wire as malformed via its own mapper"
    );

    // The OpenAI anchor pointed at a loopback server returning Ollama's
    // wire shape must likewise fail via its own OpenAIContractMapper —
    // proving the real `OpenAIProvider::into_contract_facade` binding.
    let openai_wrong_addr =
        spawn_loopback_server(http_ok_json_response(&success_wire("ollama_local"))).await;
    let openai_wrong_facade = OpenAIProvider::builder()
        .api_key("sk-fixture-secret-must-not-leak")
        .base_url(format!("http://{openai_wrong_addr}"))
        .model("gpt-4o-mini")
        .build()
        .expect("offline OpenAI construction")
        .into_contract_facade()
        .expect("OpenAI anchor conversion");
    let error = openai_wrong_facade
        .chat(&request)
        .await
        .expect_err("OpenAI anchor must reject an Ollama-shaped wire body");
    assert_eq!(
        error.code,
        ProviderErrorCode::MalformedResponse,
        "OpenAI anchor must classify an Ollama-shaped wire as malformed via its own mapper"
    );

    // Positive controls: each anchor correctly accepts its own wire shape
    // through the same loopback harness, proving the mismatches above are
    // driven by mapper/wire-shape binding rather than a broken harness.
    let ollama_ok_addr =
        spawn_loopback_server(http_ok_json_response(&success_wire("ollama_local"))).await;
    let ollama_response = OllamaProvider::builder()
        .base_url(format!("http://{ollama_ok_addr}"))
        .model("qwen3:latest")
        .build()
        .expect("offline Ollama construction")
        .into_contract_facade()
        .expect("Ollama anchor conversion")
        .chat(&request)
        .await
        .expect("Ollama anchor must accept its own wire shape");
    assert_eq!(ollama_response.descriptor.provider_id, "ollama_local");

    let openai_ok_addr =
        spawn_loopback_server(http_ok_json_response(&success_wire("openai"))).await;
    let openai_response = OpenAIProvider::builder()
        .api_key("sk-fixture-secret-must-not-leak")
        .base_url(format!("http://{openai_ok_addr}"))
        .model("gpt-4o-mini")
        .build()
        .expect("offline OpenAI construction")
        .into_contract_facade()
        .expect("OpenAI anchor conversion")
        .chat(&request)
        .await
        .expect("OpenAI anchor must accept its own wire shape");
    assert_eq!(openai_response.descriptor.provider_id, "openai");
}

#[tokio::test]
async fn spec119_facade_ollama_anchor_body_read_failure_after_headers_is_network_not_malformed() {
    let addr = spawn_loopback_server(http_ok_truncated_response()).await;
    let facade = OllamaProvider::builder()
        .base_url(format!("http://{addr}"))
        .model("qwen3:latest")
        .build()
        .expect("offline Ollama construction")
        .into_contract_facade()
        .expect("Ollama anchor conversion");
    let request = request_by_id("REQ-VALID-COMPLETE");
    let error = facade
        .chat(&request)
        .await
        .expect_err("a truncated post-header body must not be treated as a successful response");
    assert_eq!(
        error.code,
        ProviderErrorCode::Network,
        "a post-header body read failure (connection closed before the advertised \
         Content-Length was satisfied, no deliberate timeout induced) must classify as network \
         through the real OllamaProvider anchor, not {:?}",
        error.code
    );
}

#[tokio::test]
async fn spec119_facade_openai_anchor_body_read_failure_after_headers_is_network_not_malformed() {
    let addr = spawn_loopback_server(http_ok_truncated_response()).await;
    let facade = OpenAIProvider::builder()
        .api_key("sk-fixture-secret-must-not-leak")
        .base_url(format!("http://{addr}"))
        .model("gpt-4o-mini")
        .build()
        .expect("offline OpenAI construction")
        .into_contract_facade()
        .expect("OpenAI anchor conversion");
    let request = request_by_id("REQ-VALID-COMPLETE");
    let error = facade
        .chat(&request)
        .await
        .expect_err("a truncated post-header body must not be treated as a successful response");
    assert_eq!(
        error.code,
        ProviderErrorCode::Network,
        "a post-header body read failure (connection closed before the advertised \
         Content-Length was satisfied, no deliberate timeout induced) must classify as network \
         through the real OpenAIProvider anchor, not {:?}",
        error.code
    );
}

/// Full-HTTP-body loopback proof for a real Ollama anchor: the captured
/// request body from `OllamaProvider::into_contract_facade`'s own
/// `reqwest::Client`, framed by the anchor's own `Content-Length` header,
/// must satisfy the independent `wires.json` `expected_wire_fragment` for
/// both a present-value record (`TOP-K-POSITIVE`) and a required-omission
/// record (`OLL-FORMAT-TEXT-OMIT`).
#[tokio::test]
async fn spec119_facade_ollama_anchor_http_body_matches_independent_wire_fixture() {
    for id in ["TOP-K-POSITIVE", "OLL-FORMAT-TEXT-OMIT"] {
        let record = record_by_id(WIRES, "wires.json", id);
        assert_eq!(
            record["expected_error"],
            Value::Null,
            "{id}: fixture selection error"
        );
        let control_paths: Vec<String> = record["control_paths"]
            .as_array()
            .expect("control_paths must be an array")
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .expect("control_paths entries must be strings")
                    .to_owned()
            })
            .collect();
        let body = wire_record_http_body(&record).await;
        assert!(
            wire_fragment_matches(&body, &record["expected_wire_fragment"], &control_paths),
            "{id}: real Ollama anchor HTTP request body drifted from the independent \
             wires.json expected_wire_fragment (body={body})"
        );
    }
}

/// Full-HTTP-body loopback proof for a real OpenAI anchor: the captured
/// request body from `OpenAIProvider::into_contract_facade`'s own
/// `reqwest::Client`, framed by the anchor's own `Content-Length` header,
/// must satisfy the independent `wires.json` `expected_wire_fragment` for
/// both a present-value record (`PRESENCE-PENALTY-POSITIVE`) and a
/// required-omission record (`PRESENCE-PENALTY-OMIT`).
#[tokio::test]
async fn spec119_facade_openai_anchor_http_body_matches_independent_wire_fixture() {
    for id in ["PRESENCE-PENALTY-POSITIVE", "PRESENCE-PENALTY-OMIT"] {
        let record = record_by_id(WIRES, "wires.json", id);
        assert_eq!(
            record["expected_error"],
            Value::Null,
            "{id}: fixture selection error"
        );
        let control_paths: Vec<String> = record["control_paths"]
            .as_array()
            .expect("control_paths must be an array")
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .expect("control_paths entries must be strings")
                    .to_owned()
            })
            .collect();
        let body = wire_record_http_body(&record).await;
        assert!(
            wire_fragment_matches(&body, &record["expected_wire_fragment"], &control_paths),
            "{id}: real OpenAI anchor HTTP request body drifted from the independent \
             wires.json expected_wire_fragment (body={body})"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn spec119_facade_behavior_asserts_65_records() {
    assert_fixture_structure();
    run_all_records().await;
    assert_wire_fixture_structure();
    run_all_wire_records().await;
    println!("SPEC119_RUST_FACADE_ASSERTIONS={COMBINED_EXPECTED_ASSERTIONS}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn spec119_facade_lifecycle_concurrency_is_fail_closed() {
    for id in [
        "LIFECYCLE-INFLIGHT-MAY-FINISH",
        "LIFECYCLE-POST-CLOSE-CONCURRENT-REJECT",
    ] {
        run_lifecycle_record(&record_by_id(FACADE, "facade.json", id)).await;
    }
}

#[tokio::test]
async fn spec119_facade_mutation_wrong_mapper_is_killed() {
    run_mutation_record(&record_by_id(
        FACADE,
        "facade.json",
        "MUTATION-WRONG-MAPPER",
    ))
    .await;
}

#[tokio::test]
async fn spec119_facade_mutation_double_backend_call_is_killed() {
    run_mutation_record(&record_by_id(
        FACADE,
        "facade.json",
        "MUTATION-DOUBLE-BACKEND-CALL",
    ))
    .await;
}

#[tokio::test]
async fn spec119_facade_mutation_close_ordering_is_killed() {
    run_mutation_record(&record_by_id(
        FACADE,
        "facade.json",
        "MUTATION-CLOSE-ORDERING",
    ))
    .await;
}

#[tokio::test]
async fn spec119_facade_mutation_secret_redaction_is_killed() {
    run_mutation_record(&record_by_id(
        FACADE,
        "facade.json",
        "MUTATION-SECRET-REDACTION",
    ))
    .await;
}

/// Realistic wrong-wire mutation canary for the independent wires.json
/// oracle: builds the real wire for `TOP-K-POSITIVE` via the real Ollama
/// mapper, then perturbs the one governed field (`options.top_k`) the
/// record's own `control_paths` declares, and proves `wire_fragment_matches`
/// — the exact function `run_wire_record` uses to accept records — rejects
/// it. A positive control on the unmutated wire proves the harness itself
/// is not simply always returning false.
#[test]
fn spec119_wire_oracle_mutation_is_killed() {
    let record = record_by_id(WIRES, "wires.json", "TOP-K-POSITIVE");
    let anchor = record["anchor"].as_str().expect("wire record anchor");
    let config: ProviderConfig = serde_json::from_value(record["config"].clone())
        .expect("wire config fixture must deserialize");
    let request: ProviderRequest =
        serde_json::from_value(record["request"].clone()).expect("wire request must deserialize");
    let expected_fragment = record["expected_wire_fragment"].clone();
    let control_paths: Vec<String> = record["control_paths"]
        .as_array()
        .expect("control_paths must be an array")
        .iter()
        .map(|value| {
            value
                .as_str()
                .expect("control_paths entries must be strings")
                .to_owned()
        })
        .collect();

    let real_wire = wire_mapper_for(anchor, config)
        .request_to_wire(&request)
        .expect("TOP-K-POSITIVE must map to a wire");
    assert!(
        wire_fragment_matches(&real_wire, &expected_fragment, &control_paths),
        "positive control: the unmutated real wire must match the independent fixture"
    );

    let mut mutated_wire = real_wire.clone();
    let governed_value = mutated_wire
        .get_mut("options")
        .and_then(|options| options.get_mut("top_k"))
        .expect("real wire must carry options.top_k for a governed-field mutation");
    assert_eq!(
        *governed_value,
        json!(37),
        "fixture literal drift; update mutation canary"
    );
    *governed_value = json!(99);

    assert!(
        !wire_fragment_matches(&mutated_wire, &expected_fragment, &control_paths),
        "a wire with an altered governed field (options.top_k) must be rejected by the \
         independent wires.json oracle, not silently accepted"
    );
}

#[test]
fn spec119_wire_oracle_rejects_duplicate_control_paths() {
    let wire = json!({"options": {"top_k": 37}});
    let expected = wire.clone();
    let paths = vec!["options.top_k".to_owned(), "options.top_k".to_owned()];

    assert!(
        !wire_fragment_matches(&wire, &expected, &paths),
        "duplicate control paths must fail closed instead of double-counting one governed field"
    );
}

#[test]
fn spec119_wire_oracle_rejects_empty_control_path() {
    let wire = json!({"options": {"top_k": 37}});
    let expected = wire.clone();

    assert!(
        !wire_fragment_matches(&wire, &expected, &[String::new()]),
        "an empty control path must fail closed even when absent from both JSON trees"
    );
}

#[test]
fn spec119_wire_oracle_rejects_malformed_control_paths() {
    let wire = json!({"options": {"top_k": 37}});
    let expected = wire.clone();

    for path in [".options.top_k", "options..top_k", "options.top_k."] {
        assert!(
            !wire_fragment_matches(&wire, &expected, &[path.to_owned()]),
            "malformed dotted path {path:?} must fail closed"
        );
    }
}

#[test]
fn spec119_wire_oracle_rejects_unknown_control_path() {
    let wire = json!({"options": {"top_k": 37}});
    let expected = wire.clone();

    assert!(
        !wire_fragment_matches(
            &wire,
            &expected,
            &["options.unrecognized_control".to_owned()],
        ),
        "a well-formed but unknown path must fail closed even when absent from both JSON trees"
    );
}

#[allow(dead_code)]
fn frozen_transport_mode_anchor() -> ProviderTransportMode {
    ProviderTransportMode::Auto
}

#[allow(dead_code)]
fn frozen_fixture_hash_map() -> BTreeMap<&'static str, &'static str> {
    ACCEPTED_FIXTURE_HASHES.into_iter().collect()
}
