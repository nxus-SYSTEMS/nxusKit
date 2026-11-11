//! Gate E preview schema and fixture validation for the v2 contract spine.

use std::fs;
use std::path::{Path, PathBuf};

use clap::Subcommand;
use nxuskit::contract::v2::{
    BoundaryViolation, CONTRACT_VERSION, CapabilityCatalogEntry, OperationAccepted,
    OperationEnvelope, OperationEvent, OperationResult, ReferenceOnlyBoundary,
    validate_reference_only_value,
};
use serde::Serialize;
use serde_json::{Value, json};

use crate::cli_error::CliError;

#[derive(Subcommand)]
pub enum ContractV2Action {
    /// Emit the preview v2 schema manifest used by fixture validation.
    Schema {
        #[command(subcommand)]
        action: ContractV2SchemaAction,
    },
    /// Validate a fixture file or directory against the v2 DTOs.
    Validate {
        /// JSON fixture file or directory containing fixture JSON files.
        path: PathBuf,
        /// Expected fixture posture.
        #[arg(long, value_enum, default_value_t = FixturePostureArg::Positive)]
        posture: FixturePostureArg,
    },
    /// Run P0 projection-safety lint over a fixture file or directory.
    Lint {
        /// JSON fixture file or directory containing fixture JSON files.
        path: PathBuf,
        /// Expected fixture posture.
        #[arg(long, value_enum, default_value_t = FixturePostureArg::Positive)]
        posture: FixturePostureArg,
    },
    /// Compare an expected schema manifest against the current v2 schema manifest.
    Diff {
        /// Expected schema manifest JSON file.
        expected: PathBuf,
    },
}

#[derive(Subcommand)]
pub enum ContractV2SchemaAction {
    /// Emit the deterministic v2 schema manifest as JSON.
    Emit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum FixturePostureArg {
    Positive,
    Negative,
}

pub fn run_contract_v2_command(action: ContractV2Action) -> Result<(), CliError> {
    match action {
        ContractV2Action::Schema { action } => match action {
            ContractV2SchemaAction::Emit => write_json(&schema_manifest()),
        },
        ContractV2Action::Validate { path, posture } => {
            let report = validate_path(&path, posture)?;
            write_report_or_fail(report)
        }
        ContractV2Action::Lint { path, posture } => {
            let report = lint_path(&path, posture)?;
            write_report_or_fail(report)
        }
        ContractV2Action::Diff { expected } => {
            let report = diff_schema_manifest(&expected)?;
            write_report_or_fail(report)
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct ContractV2Report {
    command: &'static str,
    contract_version: &'static str,
    status: ReportStatus,
    checked: usize,
    diagnostics: Vec<ContractV2Diagnostic>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ReportStatus {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Serialize)]
struct ContractV2Diagnostic {
    path: String,
    status: DiagnosticStatus,
    code: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    artifact_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    field_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    key: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum DiagnosticStatus {
    Pass,
    Fail,
}

fn validate_path(path: &Path, posture: FixturePostureArg) -> Result<ContractV2Report, CliError> {
    let files = collect_json_files(path)?;
    let mut diagnostics = Vec::with_capacity(files.len());
    for file in files {
        diagnostics.push(validate_fixture_file(&file, posture)?);
    }
    Ok(report("validate", diagnostics))
}

fn lint_path(path: &Path, posture: FixturePostureArg) -> Result<ContractV2Report, CliError> {
    let files = collect_json_files(path)?;
    let mut diagnostics = Vec::with_capacity(files.len());
    for file in files {
        diagnostics.push(lint_fixture_file(&file, posture)?);
    }
    Ok(report("lint", diagnostics))
}

fn diff_schema_manifest(expected: &Path) -> Result<ContractV2Report, CliError> {
    let expected_raw = fs::read_to_string(expected).map_err(|e| CliError::FileNotFound {
        path: format!("{} ({e})", expected.display()),
    })?;
    let expected_value: Value =
        serde_json::from_str(&expected_raw).map_err(|e| CliError::ParseError {
            message: format!("failed to parse {}: {e}", expected.display()),
        })?;
    let current = schema_manifest();
    let diagnostic = if expected_value == current {
        pass_diag(expected, "schema_manifest", "schema manifests match")
    } else {
        ContractV2Diagnostic {
            path: expected.display().to_string(),
            status: DiagnosticStatus::Fail,
            code: "schema_manifest_diff",
            message: "expected schema manifest differs from current v2 preview manifest"
                .to_string(),
            artifact_type: Some("schema_manifest"),
            field_path: None,
            key: None,
        }
    };
    Ok(report("diff", vec![diagnostic]))
}

fn validate_fixture_file(
    path: &Path,
    posture: FixturePostureArg,
) -> Result<ContractV2Diagnostic, CliError> {
    let value = read_json(path)?;
    match posture {
        FixturePostureArg::Positive => match deserialize_known_fixture(&value) {
            Ok(artifact_type) => Ok(pass_diag(path, artifact_type, "fixture conforms")),
            Err(message) => Ok(fail_diag(
                path,
                "dto_validation_failed",
                message,
                Some("unknown"),
                None,
            )),
        },
        FixturePostureArg::Negative => match validate_reference_only_value(&value) {
            Ok(()) => Ok(fail_diag(
                path,
                "negative_fixture_not_rejected",
                "negative fixture did not trigger a boundary violation".to_string(),
                Some("boundary_probe"),
                None,
            )),
            Err(violation) => Ok(boundary_diag(path, violation, DiagnosticStatus::Pass)),
        },
    }
}

fn lint_fixture_file(
    path: &Path,
    posture: FixturePostureArg,
) -> Result<ContractV2Diagnostic, CliError> {
    let value = read_json(path)?;
    match validate_reference_only_value(&value).and_then(|()| validate_public_terms(&value)) {
        Ok(()) if posture == FixturePostureArg::Positive => {
            Ok(pass_diag(path, "projection_safety", "no P0 hazards found"))
        }
        Ok(()) => Ok(fail_diag(
            path,
            "negative_fixture_not_rejected",
            "negative fixture did not trigger a P0 projection-safety hazard".to_string(),
            Some("projection_safety"),
            None,
        )),
        Err(violation) if posture == FixturePostureArg::Negative => {
            Ok(boundary_diag(path, violation, DiagnosticStatus::Pass))
        }
        Err(violation) => Ok(boundary_diag(path, violation, DiagnosticStatus::Fail)),
    }
}

fn deserialize_known_fixture(value: &Value) -> Result<&'static str, String> {
    if let Ok(envelope) = serde_json::from_value::<OperationEnvelope>(value.clone()) {
        envelope
            .validate_reference_only()
            .map_err(|e| e.to_string())?;
        return Ok("operation_envelope");
    }
    if let Ok(accepted) = serde_json::from_value::<OperationAccepted>(value.clone()) {
        accepted
            .validate_reference_only()
            .map_err(|e| e.to_string())?;
        return Ok("operation_accepted");
    }
    if let Ok(result) = serde_json::from_value::<OperationResult>(value.clone()) {
        result
            .validate_reference_only()
            .map_err(|e| e.to_string())?;
        return Ok("operation_result");
    }
    if let Ok(event) = serde_json::from_value::<OperationEvent>(value.clone()) {
        event.validate_reference_only().map_err(|e| e.to_string())?;
        return Ok("operation_event");
    }
    if let Ok(entry) = serde_json::from_value::<CapabilityCatalogEntry>(value.clone()) {
        entry.validate_reference_only().map_err(|e| e.to_string())?;
        return Ok("capability_catalog_entry");
    }
    Err("fixture did not deserialize into any v2 preview DTO".to_string())
}

fn validate_public_terms(value: &Value) -> Result<(), BoundaryViolation> {
    validate_term_at(value, "$")
}

fn validate_term_at(value: &Value, path: &str) -> Result<(), BoundaryViolation> {
    match value {
        Value::Object(map) => {
            for (key, nested) in map {
                let normalized = key.to_ascii_lowercase();
                if is_private_boundary_term(&normalized) {
                    return Err(BoundaryViolation {
                        path: path.to_string(),
                        key: key.to_string(),
                        kind: nxuskit::contract::v2::BoundaryViolationKind::ProtectedContentKey,
                    });
                }
                validate_term_at(nested, &format!("{path}.{key}"))?;
            }
        }
        Value::Array(items) => {
            for (index, nested) in items.iter().enumerate() {
                validate_term_at(nested, &format!("{path}[{index}]"))?;
            }
        }
        Value::String(text) => {
            let normalized = text.to_ascii_lowercase();
            if is_private_boundary_term(&normalized) {
                return Err(BoundaryViolation {
                    path: path.to_string(),
                    key: "value".to_string(),
                    kind: nxuskit::contract::v2::BoundaryViolationKind::ProtectedContentKey,
                });
            }
        }
        _ => {}
    }
    Ok(())
}

fn is_private_boundary_term(normalized: &str) -> bool {
    normalized.contains("providerchannel")
        || normalized.contains("provider_channel")
        || normalized.contains("toolcatalog")
        || normalized.contains("tool_catalog")
        || normalized.contains("private_route")
        || normalized.contains("protected_route")
        || normalized.contains("pro_private_route")
}

fn collect_json_files(path: &Path) -> Result<Vec<PathBuf>, CliError> {
    if !path.exists() {
        return Err(CliError::FileNotFound {
            path: path.display().to_string(),
        });
    }
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }

    let mut files = Vec::new();
    collect_json_files_inner(path, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_json_files_inner(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), CliError> {
    for entry in fs::read_dir(path).map_err(|e| CliError::ParseError {
        message: format!("failed to read {}: {e}", path.display()),
    })? {
        let entry = entry.map_err(|e| CliError::ParseError {
            message: format!("failed to read directory entry in {}: {e}", path.display()),
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_json_files_inner(&path, files)?;
        } else if path.extension().is_some_and(|ext| ext == "json") {
            files.push(path);
        }
    }
    Ok(())
}

fn read_json(path: &Path) -> Result<Value, CliError> {
    let raw = fs::read_to_string(path).map_err(|e| CliError::FileNotFound {
        path: format!("{} ({e})", path.display()),
    })?;
    serde_json::from_str(&raw).map_err(|e| CliError::ParseError {
        message: format!("failed to parse {}: {e}", path.display()),
    })
}

fn report(command: &'static str, diagnostics: Vec<ContractV2Diagnostic>) -> ContractV2Report {
    let status = if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.status == DiagnosticStatus::Fail)
    {
        ReportStatus::Fail
    } else {
        ReportStatus::Pass
    };
    ContractV2Report {
        command,
        contract_version: CONTRACT_VERSION,
        status,
        checked: diagnostics.len(),
        diagnostics,
    }
}

fn pass_diag(path: &Path, artifact_type: &'static str, message: &str) -> ContractV2Diagnostic {
    ContractV2Diagnostic {
        path: path.display().to_string(),
        status: DiagnosticStatus::Pass,
        code: "ok",
        message: message.to_string(),
        artifact_type: Some(artifact_type),
        field_path: None,
        key: None,
    }
}

fn fail_diag(
    path: &Path,
    code: &'static str,
    message: String,
    artifact_type: Option<&'static str>,
    field_path: Option<String>,
) -> ContractV2Diagnostic {
    ContractV2Diagnostic {
        path: path.display().to_string(),
        status: DiagnosticStatus::Fail,
        code,
        message,
        artifact_type,
        field_path,
        key: None,
    }
}

fn boundary_diag(
    path: &Path,
    violation: BoundaryViolation,
    status: DiagnosticStatus,
) -> ContractV2Diagnostic {
    ContractV2Diagnostic {
        path: path.display().to_string(),
        status,
        code: "reference_only_boundary_violation",
        message: format!("{:?} at {}", violation.kind, violation.path),
        artifact_type: Some("projection_safety"),
        field_path: Some(violation.path),
        key: Some(violation.key),
    }
}

fn write_report_or_fail(report: ContractV2Report) -> Result<(), CliError> {
    if report.status == ReportStatus::Pass {
        write_json(&report)
    } else {
        let details = serde_json::to_value(&report).map_err(|e| CliError::ParseError {
            message: format!("failed to serialize report: {e}"),
        })?;
        Err(CliError::CommandValidation {
            code: "contract_v2_validation_failed",
            message: format!(
                "contract v2 {} failed with {} diagnostic(s)",
                report.command,
                report.diagnostics.len()
            ),
            details: Some(details),
        })
    }
}

fn write_json<T: Serialize>(value: &T) -> Result<(), CliError> {
    let text = serde_json::to_string_pretty(value).map_err(|e| CliError::ParseError {
        message: format!("failed to serialize JSON: {e}"),
    })?;
    println!("{text}");
    Ok(())
}

fn schema_manifest() -> Value {
    json!({
        "manifest_kind": "nxuskit.contract.v2.schema_manifest",
        "contract_version": CONTRACT_VERSION,
        "classification": "preview",
        "vocabulary": {
            "product": "Capability Catalog",
            "sdk_implementation": "SDK capability catalog",
            "reserved": "CapabilityGraph",
            "canonical_envelope": "OperationEnvelope"
        },
        "dto_schemas": [
            {"name": "OperationEnvelope", "classification": "preview"},
            {"name": "OperationAccepted", "classification": "preview"},
            {"name": "OperationResult", "classification": "preview"},
            {"name": "OperationEvent", "classification": "preview"},
            {"name": "ExecutionPolicy", "classification": "preview"},
            {"name": "ProjectionMetadata", "classification": "preview"},
            {"name": "CapabilityCatalogEntry", "classification": "preview"}
        ],
        "reference_schemas": [
            "ArtifactRef",
            "BatchRef",
            "FactSetRef",
            "PlanRef",
            "PacketRef",
            "SchemaRef",
            "CapabilityRef",
            "RouteRef"
        ],
        "boundary_lint": {
            "posture": "reference_only_control_plane",
            "p0_hazards": [
                "rows",
                "record_batch",
                "inline_sql",
                "file_body",
                "blob_payload",
                "payload",
                "body",
                "persona",
                "workflow_pack",
                concat!("Provider", "Channel"),
                concat!("Tool", "Catalog"),
                "private_route"
            ]
        }
    })
}
