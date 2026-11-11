use nxuskit::contract::v2::*;
use serde_json::json;
use std::collections::BTreeMap;

fn schema_ref() -> SchemaRef {
    SchemaRef {
        schema_id: SchemaId::from("schema.operation.input.v2"),
        schema_kind: SchemaKind::JsonSchema,
        schema_version: CONTRACT_VERSION.to_owned(),
        uri: Some("nxuskit://schemas/v2/operation-envelope.schema.json".to_owned()),
        fingerprint: Some("sha256:fixture-schema".to_owned()),
        classification: SurfaceClassification::Preview,
        metadata: BTreeMap::new(),
    }
}

fn artifact_ref(schema: SchemaRef) -> ArtifactRef {
    ArtifactRef {
        artifact_id: ArtifactId::from("artifact.result.001"),
        artifact_kind: ArtifactKind::Report,
        schema_ref: Some(schema),
        fingerprint: Some("sha256:artifact".to_owned()),
        locality: ReferenceLocality::Remote,
        trust: RefTrustPosture::Verified,
        classification: SurfaceClassification::Preview,
        metadata: BTreeMap::new(),
    }
}

fn batch_ref(schema: SchemaRef) -> BatchRef {
    BatchRef {
        batch_id: BatchId::from("batch.input.001"),
        schema_ref: schema,
        row_count_hint: Some(20),
        byte_size_hint: Some(4096),
        locality: ReferenceLocality::Remote,
        trust: RefTrustPosture::Verified,
        classification: SurfaceClassification::Preview,
        metadata: BTreeMap::new(),
    }
}

fn packet_ref(schema: SchemaRef) -> PacketRef {
    PacketRef {
        packet_id: PacketId::from("packet.result.001"),
        schema_ref: Some(schema),
        artifact_ref: None,
        locality: ReferenceLocality::Remote,
        trust: RefTrustPosture::Verified,
        classification: SurfaceClassification::Preview,
        metadata: BTreeMap::new(),
    }
}

fn operation_envelope() -> OperationEnvelope {
    let schema = schema_ref();
    let artifact = artifact_ref(schema.clone());
    let batch = batch_ref(schema.clone());
    let packet = packet_ref(schema.clone());
    let fact_set = FactSetRef {
        fact_set_id: FactSetId::from("facts.input.001"),
        schema_refs: vec![schema.clone()],
        input_refs: Vec::new(),
        locality: ReferenceLocality::Remote,
        trust: RefTrustPosture::Verified,
        classification: SurfaceClassification::Preview,
        metadata: BTreeMap::new(),
    };
    let plan = PlanRef {
        plan_id: PlanId::from("plan.analysis.001"),
        plan_kind: PlanKind::PortablePlan,
        compatibility_posture: PlanCompatibilityPosture::DeclarationOnly,
        schema_ref: Some(schema.clone()),
        locality: ReferenceLocality::Packaged,
        trust: RefTrustPosture::Trusted,
        classification: SurfaceClassification::Preview,
        metadata: BTreeMap::new(),
    };
    let actor = ActorContext {
        actor_id: ActorId::from("actor.integration-test"),
        tenant_id: Some(TenantId::from("tenant.test")),
        subject_kind: Some("service_account".to_owned()),
        metadata: BTreeMap::new(),
    };

    OperationEnvelope {
        contract_version: CONTRACT_VERSION.to_owned(),
        operation_id: OperationId::from("op.001"),
        correlation_id: CorrelationId::from("corr.001"),
        causality_id: Some(CausalityId::from("cause.001")),
        actor: actor.clone(),
        capability_ref: CapabilityRef {
            capability_id: CapabilityId::from("capability.contract.roundtrip"),
            capability_version: Some(CONTRACT_VERSION.to_owned()),
            classification: SurfaceClassification::Preview,
            metadata: BTreeMap::new(),
        },
        route_ref: Some(RouteRef {
            route_id: RouteId::from("route.local.preview"),
            route_label: Some("local preview route".to_owned()),
            locality: ReferenceLocality::Local,
            trust: RefTrustPosture::Trusted,
            classification: SurfaceClassification::Internal,
            metadata: BTreeMap::new(),
        }),
        input_refs: vec![
            OperationRef::Artifact(artifact.clone()),
            OperationRef::Batch(batch),
            OperationRef::FactSet(fact_set),
            OperationRef::Packet(packet),
            OperationRef::Schema(schema.clone()),
        ],
        plan_refs: vec![plan],
        expected_output_schema_refs: vec![schema.clone()],
        side_effect: SideEffectClass::ReadOnly,
        determinism: DeterminismClass::Deterministic,
        execution_policy: ExecutionPolicy {
            timeout_ms: Some(30_000),
            cancellable: true,
            cancellation_token: Some("cancel.op.001".to_owned()),
            retry: RetryPolicy {
                max_attempts: 2,
                retry_after_ms: Some(1000),
                backoff_hint: Some("linear".to_owned()),
            },
            idempotency_key: Some("idem.op.001".to_owned()),
            dedupe_key: Some("dedupe.op.001".to_owned()),
            causality_id: Some(CausalityId::from("cause.001")),
            budget: BudgetPolicy {
                max_runtime_ms: Some(60_000),
                max_output_refs: Some(10),
                max_cost_units: Some(100),
                budget_tag: Some("contract-test".to_owned()),
            },
        },
        projection: ProjectionMetadata {
            posture: ProjectionPosture::ProjectionReady,
            source_schema_refs: vec![schema.clone()],
            projected_schema_refs: Vec::new(),
            generated_artifact_refs: vec![artifact],
            classification: SurfaceClassification::Preview,
            metadata: BTreeMap::new(),
        },
        audit: AuditMetadata {
            submitted_at: Some("2026-06-05T00:00:00Z".to_owned()),
            actor,
            reason: Some("contract spine test".to_owned()),
            audit_ref: None,
            classification: SurfaceClassification::Preview,
            metadata: BTreeMap::new(),
        },
        classification: SurfaceClassification::Preview,
        metadata: BTreeMap::new(),
    }
}

#[test]
fn operation_envelope_round_trips_and_covers_required_refs() {
    let envelope = operation_envelope();
    envelope.validate_reference_only().unwrap();

    let json = serde_json::to_string(&envelope).unwrap();
    let decoded: OperationEnvelope = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded.contract_version, CONTRACT_VERSION);
    assert_eq!(decoded.input_refs.len(), 5);
    assert_eq!(decoded.plan_refs.len(), 1);
    assert_eq!(decoded.classification, SurfaceClassification::Preview);
    assert!(matches!(
        decoded.input_refs[0],
        OperationRef::Artifact(ArtifactRef { .. })
    ));
}

#[test]
fn operation_envelope_rejects_bulk_data_metadata() {
    let mut envelope = operation_envelope();
    envelope
        .metadata
        .insert("rows".to_owned(), json!([{"a": 1}]));

    let violation = envelope.validate_reference_only().unwrap_err();
    assert_eq!(violation.kind, BoundaryViolationKind::BulkDataKey);
    assert_eq!(violation.key, "rows");
}

#[test]
fn accepted_and_result_statuses_are_distinct() {
    let accepted = OperationAccepted {
        contract_version: CONTRACT_VERSION.to_owned(),
        operation_id: OperationId::from("op.001"),
        correlation_id: CorrelationId::from("corr.001"),
        status: AcceptanceStatus::Accepted,
        accepted_at: None,
        validation_errors: Vec::new(),
        event_stream_ref: None,
        result_ref: None,
        classification: SurfaceClassification::Preview,
        metadata: BTreeMap::new(),
    };
    let rejected = OperationAccepted {
        status: AcceptanceStatus::RejectedBeforeDispatch,
        ..accepted.clone()
    };
    let validation_failed = OperationAccepted {
        status: AcceptanceStatus::ValidationFailed,
        validation_errors: vec![OperationErrorSummary {
            error_code: "validation.missing_input".to_owned(),
            message: "required input ref missing".to_owned(),
            category: Some("validation".to_owned()),
            retryable: false,
            metadata: BTreeMap::new(),
        }],
        ..accepted.clone()
    };
    let result = OperationResult {
        contract_version: CONTRACT_VERSION.to_owned(),
        operation_id: OperationId::from("op.001"),
        correlation_id: CorrelationId::from("corr.001"),
        status: OperationStatus::BudgetExceeded,
        output_refs: Vec::new(),
        result_ref: None,
        error: None,
        completed_at: None,
        classification: SurfaceClassification::Preview,
        metadata: BTreeMap::new(),
    };

    assert_eq!(
        serde_json::to_value(&accepted).unwrap()["status"],
        "accepted"
    );
    assert_eq!(
        serde_json::to_value(&rejected).unwrap()["status"],
        "rejected_before_dispatch"
    );
    assert_eq!(
        serde_json::to_value(&validation_failed).unwrap()["status"],
        "validation_failed"
    );
    assert_eq!(
        serde_json::to_value(&result).unwrap()["status"],
        "budget_exceeded"
    );
}

#[test]
fn capability_catalog_entry_uses_product_vocabulary() {
    let schema = schema_ref();
    let entry = CapabilityCatalogEntry {
        contract_version: CONTRACT_VERSION.to_owned(),
        capability_ref: CapabilityRef {
            capability_id: CapabilityId::from("capability.contract.roundtrip"),
            capability_version: Some(CONTRACT_VERSION.to_owned()),
            classification: SurfaceClassification::Preview,
            metadata: BTreeMap::new(),
        },
        title: "Contract round-trip".to_owned(),
        summary: "Reference-only v2 contract test capability".to_owned(),
        vocabulary: CapabilityCatalogVocabulary::CapabilityCatalog,
        input_schema_refs: vec![schema.clone()],
        output_schema_refs: vec![schema],
        route_refs: Vec::new(),
        route_kind: RuntimeRouteKind::LocalAdapter,
        locality: CapabilityLocality::Local,
        determinism: DeterminismClass::Deterministic,
        side_effect: SideEffectClass::ReadOnly,
        cancellable: true,
        cost: CostResourceClass::Free,
        tier: SurfaceTier::Community,
        entitlement: EntitlementPosture::CommunityAllowed,
        actor_scope: Some("tenant".to_owned()),
        tenant_scoped: true,
        trust: TrustPosture::PublicContract,
        readiness: ReadinessPosture::Preview,
        health: CapabilityHealth::Healthy,
        allowed_input_ref_kinds: vec![AllowedRefKind::Artifact, AllowedRefKind::Batch],
        allowed_output_ref_kinds: vec![AllowedRefKind::Packet],
        projection: ProjectionMetadata {
            posture: ProjectionPosture::ProjectionReady,
            source_schema_refs: Vec::new(),
            projected_schema_refs: Vec::new(),
            generated_artifact_refs: Vec::new(),
            classification: SurfaceClassification::Preview,
            metadata: BTreeMap::new(),
        },
        audit: None,
        classification: SurfaceClassification::Preview,
        metadata: BTreeMap::new(),
    };

    entry.validate_reference_only().unwrap();
    let value = serde_json::to_value(&entry).unwrap();
    assert_eq!(value["vocabulary"], "capability_catalog");
    assert_eq!(value["route_kind"], "local_adapter");
    assert_eq!(value["tier"], "community");
}

#[test]
fn schema_artifacts_exist_and_carry_contract_markers() {
    let schema_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("schemas/v2");
    for file_name in [
        "operation-envelope.schema.json",
        "operation-accepted.schema.json",
        "operation-result.schema.json",
        "operation-event.schema.json",
        "capability-catalog-entry.schema.json",
        "refs.schema.json",
    ] {
        let raw = std::fs::read_to_string(schema_dir.join(file_name)).unwrap();
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value["x-nxuskit-contract-version"], CONTRACT_VERSION);
        assert!(value["x-nxuskit-classification"].is_string());
    }
}
