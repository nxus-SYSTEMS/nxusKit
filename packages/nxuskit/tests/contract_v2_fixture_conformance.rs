use nxuskit::contract::v2::*;

fn fixture(name: &str) -> serde_json::Value {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/contract_v2");
    let raw = std::fs::read_to_string(root.join(name)).expect(name);
    serde_json::from_str(&raw).expect(name)
}

#[test]
fn positive_operation_envelope_fixture_conforms() {
    let value = fixture("positive/operation-envelope.json");
    let envelope: OperationEnvelope = serde_json::from_value(value).unwrap();

    envelope.validate_reference_only().unwrap();
    assert_eq!(envelope.contract_version, CONTRACT_VERSION);
    assert_eq!(envelope.classification, SurfaceClassification::Preview);
    assert_eq!(envelope.input_refs.len(), 5);
    assert_eq!(envelope.plan_refs.len(), 1);
    assert!(envelope.route_ref.is_some());

    let mut seen = Vec::new();
    for reference in &envelope.input_refs {
        seen.push(match reference {
            OperationRef::Artifact(_) => "artifact",
            OperationRef::Batch(_) => "batch",
            OperationRef::FactSet(_) => "fact_set",
            OperationRef::Packet(_) => "packet",
            OperationRef::Schema(_) => "schema",
        });
    }
    seen.sort_unstable();
    assert_eq!(seen, ["artifact", "batch", "fact_set", "packet", "schema"]);
}

#[test]
fn positive_accepted_result_and_event_fixtures_conform() {
    let accepted: OperationAccepted =
        serde_json::from_value(fixture("positive/operation-accepted.json")).unwrap();
    let rejected: OperationAccepted =
        serde_json::from_value(fixture("positive/operation-rejected-before-dispatch.json"))
            .unwrap();
    let result: OperationResult =
        serde_json::from_value(fixture("positive/operation-result.json")).unwrap();
    let event: OperationEvent =
        serde_json::from_value(fixture("positive/operation-event.json")).unwrap();

    accepted.validate_reference_only().unwrap();
    rejected.validate_reference_only().unwrap();
    result.validate_reference_only().unwrap();
    event.validate_reference_only().unwrap();

    assert_eq!(accepted.status, AcceptanceStatus::Accepted);
    assert_eq!(rejected.status, AcceptanceStatus::RejectedBeforeDispatch);
    assert_eq!(result.status, OperationStatus::Succeeded);
    assert_eq!(event.event_kind, OperationEventKind::ResultAvailable);
    assert_eq!(event.status, Some(OperationStatus::Succeeded));
}

#[test]
fn positive_capability_catalog_fixture_conforms() {
    let entry: CapabilityCatalogEntry =
        serde_json::from_value(fixture("positive/capability-catalog-entry.json")).unwrap();

    entry.validate_reference_only().unwrap();
    assert_eq!(
        entry.vocabulary,
        CapabilityCatalogVocabulary::CapabilityCatalog
    );
    assert_eq!(entry.route_kind, RuntimeRouteKind::LocalAdapter);
    assert_eq!(entry.classification, SurfaceClassification::Preview);
    assert!(
        entry
            .allowed_input_ref_kinds
            .contains(&AllowedRefKind::Batch)
    );
    assert!(
        entry
            .allowed_output_ref_kinds
            .contains(&AllowedRefKind::Packet)
    );
}

#[test]
fn negative_boundary_fixtures_report_expected_classes() {
    for (file_name, kind, key) in [
        (
            "negative/bulk-rows.json",
            BoundaryViolationKind::BulkDataKey,
            "rows",
        ),
        (
            "negative/record-batch.json",
            BoundaryViolationKind::BulkDataKey,
            "record_batch",
        ),
        (
            "negative/inline-sql.json",
            BoundaryViolationKind::BulkDataKey,
            "inline_sql",
        ),
        (
            "negative/file-body.json",
            BoundaryViolationKind::InlineBodyKey,
            "file_body",
        ),
        (
            "negative/blob-payload.json",
            BoundaryViolationKind::InlineBodyKey,
            "blob_payload",
        ),
        (
            "negative/persona-content.json",
            BoundaryViolationKind::ProtectedContentKey,
            "private_persona_ref",
        ),
        (
            "negative/workflow-pack-content.json",
            BoundaryViolationKind::ProtectedContentKey,
            "workflow_pack_content_ref",
        ),
    ] {
        let value = fixture(file_name);
        let violation = validate_reference_only_value(&value).unwrap_err();
        assert_eq!(violation.kind, kind, "{file_name}");
        assert_eq!(violation.key, key, "{file_name}");
    }
}
