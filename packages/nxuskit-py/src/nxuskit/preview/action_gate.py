"""Typed CFFI binding for the private, local-only Action Gate Python Preview."""

from __future__ import annotations

import json
from dataclasses import dataclass
from enum import StrEnum
from typing import Any, Mapping

from nxuskit._ffi import EXPECTED_VERSION, ffi, lib

REQUEST_SCHEMA_VERSION = "nxuskit-action-gate-preview/request/v1"
RESULT_SCHEMA_VERSION = "nxuskit-action-gate-preview/result/v1"
CAPABILITY_SCHEMA_VERSION = "nxuskit-action-gate-preview/v1"
POLICY_FAMILY = "d1_support_playbook"
MAX_REQUEST_BYTES = 65_536
MAX_REFERENCE_LENGTH = 128
MAX_REFERENCES = 16


class ActionGateError(RuntimeError):
    """A safe Action Gate preview validation or native-boundary error."""


class ActionKind(StrEnum):
    """Synthetic action families accepted by the frozen preview contract."""

    SUPPORT_REPLY = "support_reply"
    ACCOUNT_EXPORT = "account_export"
    SERVICE_CREDIT = "service_credit"


class ConsequenceKind(StrEnum):
    """Synthetic consequence families accepted by the frozen preview contract."""

    LOW_SUPPORT_REPLY = "low_support_reply"
    ACCOUNT_EXPORT = "account_export"
    SERVICE_CREDIT = "service_credit"


class PreviewDecision(StrEnum):
    """Safe Action Gate preview decisions."""

    ALLOW = "allow"
    DENY = "deny"
    REVIEW_REQUIRED = "review_required"


class PreviewStatus(StrEnum):
    """Safe Action Gate preview evaluation statuses."""

    DECISION_DERIVED = "decision_derived"
    MISSING_EVIDENCE = "missing_evidence"
    REVIEW_HANDOFF_MISSING = "review_handoff_missing"
    FORBIDDEN_AUTHORITY = "forbidden_authority"
    INVALID_REQUEST = "invalid_request"


@dataclass(frozen=True, slots=True)
class ActionGateAction:
    kind: ActionKind
    action_ref: str
    public_projection_requested: bool
    runtime_provider_claim_requested: bool
    unbounded_file_or_network_access_requested: bool

    @classmethod
    def from_mapping(cls, value: Mapping[str, Any]) -> ActionGateAction:
        _require_keys(
            value,
            {
                "kind",
                "action_ref",
                "public_projection_requested",
                "runtime_provider_claim_requested",
                "unbounded_file_or_network_access_requested",
            },
            "action",
        )
        return cls(
            kind=_enum(ActionKind, value["kind"], "action kind"),
            action_ref=_reference(value["action_ref"], "action reference"),
            public_projection_requested=_boolean(
                value["public_projection_requested"], "public projection flag"
            ),
            runtime_provider_claim_requested=_boolean(
                value["runtime_provider_claim_requested"], "runtime provider flag"
            ),
            unbounded_file_or_network_access_requested=_boolean(
                value["unbounded_file_or_network_access_requested"], "file/network flag"
            ),
        )

    def to_mapping(self) -> dict[str, object]:
        _validate_action(self)
        return {
            "kind": self.kind.value,
            "action_ref": self.action_ref,
            "public_projection_requested": self.public_projection_requested,
            "runtime_provider_claim_requested": self.runtime_provider_claim_requested,
            "unbounded_file_or_network_access_requested": (
                self.unbounded_file_or_network_access_requested
            ),
        }


@dataclass(frozen=True, slots=True)
class ActionGateActorContext:
    actor_ref: str
    context_ref: str
    local_synthetic_context: bool
    contains_secret_material: bool
    contains_customer_or_production_data: bool

    @classmethod
    def from_mapping(cls, value: Mapping[str, Any]) -> ActionGateActorContext:
        _require_keys(
            value,
            {
                "actor_ref",
                "context_ref",
                "local_synthetic_context",
                "contains_secret_material",
                "contains_customer_or_production_data",
            },
            "actor context",
        )
        return cls(
            actor_ref=_reference(value["actor_ref"], "actor reference"),
            context_ref=_reference(value["context_ref"], "context reference"),
            local_synthetic_context=_boolean(
                value["local_synthetic_context"], "local synthetic context flag"
            ),
            contains_secret_material=_boolean(
                value["contains_secret_material"], "secret material flag"
            ),
            contains_customer_or_production_data=_boolean(
                value["contains_customer_or_production_data"], "customer data flag"
            ),
        )

    def to_mapping(self) -> dict[str, object]:
        _validate_actor_context(self)
        return {
            "actor_ref": self.actor_ref,
            "context_ref": self.context_ref,
            "local_synthetic_context": self.local_synthetic_context,
            "contains_secret_material": self.contains_secret_material,
            "contains_customer_or_production_data": self.contains_customer_or_production_data,
        }


@dataclass(frozen=True, slots=True)
class ActionGateConsequence:
    kind: ConsequenceKind
    consequence_ref: str
    exports_sensitive_data: bool
    material_effect: bool
    requires_review: bool

    @classmethod
    def from_mapping(cls, value: Mapping[str, Any]) -> ActionGateConsequence:
        _require_keys(
            value,
            {
                "kind",
                "consequence_ref",
                "exports_sensitive_data",
                "material_effect",
                "requires_review",
            },
            "consequence",
        )
        return cls(
            kind=_enum(ConsequenceKind, value["kind"], "consequence kind"),
            consequence_ref=_reference(value["consequence_ref"], "consequence reference"),
            exports_sensitive_data=_boolean(
                value["exports_sensitive_data"], "sensitive export flag"
            ),
            material_effect=_boolean(value["material_effect"], "material effect flag"),
            requires_review=_boolean(value["requires_review"], "review flag"),
        )

    def to_mapping(self) -> dict[str, object]:
        _validate_consequence(self)
        return {
            "kind": self.kind.value,
            "consequence_ref": self.consequence_ref,
            "exports_sensitive_data": self.exports_sensitive_data,
            "material_effect": self.material_effect,
            "requires_review": self.requires_review,
        }


@dataclass(frozen=True, slots=True)
class ActionGatePolicy:
    policy_ref: str
    low_consequence_action_allowed: bool
    sensitive_export_allowed: bool
    material_effect_requires_review: bool
    public_projection_allowed: bool

    @classmethod
    def from_mapping(cls, value: Mapping[str, Any]) -> ActionGatePolicy:
        _require_keys(
            value,
            {
                "policy_ref",
                "low_consequence_action_allowed",
                "sensitive_export_allowed",
                "material_effect_requires_review",
                "public_projection_allowed",
            },
            "policy",
        )
        return cls(
            policy_ref=_reference(value["policy_ref"], "policy reference"),
            low_consequence_action_allowed=_boolean(
                value["low_consequence_action_allowed"], "low consequence policy flag"
            ),
            sensitive_export_allowed=_boolean(
                value["sensitive_export_allowed"], "sensitive export policy flag"
            ),
            material_effect_requires_review=_boolean(
                value["material_effect_requires_review"], "material review policy flag"
            ),
            public_projection_allowed=_boolean(
                value["public_projection_allowed"], "public projection policy flag"
            ),
        )

    def to_mapping(self) -> dict[str, object]:
        _validate_policy(self)
        return {
            "policy_ref": self.policy_ref,
            "low_consequence_action_allowed": self.low_consequence_action_allowed,
            "sensitive_export_allowed": self.sensitive_export_allowed,
            "material_effect_requires_review": self.material_effect_requires_review,
            "public_projection_allowed": self.public_projection_allowed,
        }


@dataclass(frozen=True, slots=True)
class ActionGateEvidence:
    evidence_refs: tuple[str, ...]
    required_evidence_refs: tuple[str, ...]
    receipt_ready_refs: tuple[str, ...]
    gate_ready_refs: tuple[str, ...]

    @classmethod
    def from_mapping(cls, value: Mapping[str, Any]) -> ActionGateEvidence:
        _require_keys(
            value,
            {"evidence_refs", "required_evidence_refs", "receipt_ready_refs", "gate_ready_refs"},
            "evidence",
        )
        return cls(
            evidence_refs=_reference_list(value["evidence_refs"], "evidence references"),
            required_evidence_refs=_reference_list(
                value["required_evidence_refs"], "required evidence references"
            ),
            receipt_ready_refs=_reference_list(value["receipt_ready_refs"], "receipt references"),
            gate_ready_refs=_reference_list(value["gate_ready_refs"], "gate references"),
        )

    def to_mapping(self) -> dict[str, object]:
        _validate_evidence(self)
        return {
            "evidence_refs": list(self.evidence_refs),
            "required_evidence_refs": list(self.required_evidence_refs),
            "receipt_ready_refs": list(self.receipt_ready_refs),
            "gate_ready_refs": list(self.gate_ready_refs),
        }


@dataclass(frozen=True, slots=True)
class ActionGateRequest:
    schema_version: str
    request_ref: str
    case_ref: str
    action: ActionGateAction
    actor_context: ActionGateActorContext
    consequence: ActionGateConsequence
    policy: ActionGatePolicy
    evidence: ActionGateEvidence
    review_handoff_ref: str | None = None

    @classmethod
    def from_mapping(cls, value: Mapping[str, Any]) -> ActionGateRequest:
        _require_keys(
            value,
            {
                "schema_version",
                "request_ref",
                "case_ref",
                "action",
                "actor_context",
                "consequence",
                "policy",
                "evidence",
            },
            "request",
            optional={"review_handoff_ref"},
        )
        return cls(
            schema_version=_string(value["schema_version"], "schema version"),
            request_ref=_reference(value["request_ref"], "request reference"),
            case_ref=_reference(value["case_ref"], "case reference"),
            action=ActionGateAction.from_mapping(_mapping(value["action"], "action")),
            actor_context=ActionGateActorContext.from_mapping(
                _mapping(value["actor_context"], "actor context")
            ),
            consequence=ActionGateConsequence.from_mapping(
                _mapping(value["consequence"], "consequence")
            ),
            policy=ActionGatePolicy.from_mapping(_mapping(value["policy"], "policy")),
            evidence=ActionGateEvidence.from_mapping(_mapping(value["evidence"], "evidence")),
            review_handoff_ref=(
                None
                if value.get("review_handoff_ref") is None
                else _reference(value["review_handoff_ref"], "review handoff reference")
            ),
        )

    def to_mapping(self) -> dict[str, object]:
        _validate_request(self)
        payload: dict[str, object] = {
            "schema_version": self.schema_version,
            "request_ref": self.request_ref,
            "case_ref": self.case_ref,
            "action": self.action.to_mapping(),
            "actor_context": self.actor_context.to_mapping(),
            "consequence": self.consequence.to_mapping(),
            "policy": self.policy.to_mapping(),
            "evidence": self.evidence.to_mapping(),
        }
        if self.review_handoff_ref is not None:
            payload["review_handoff_ref"] = self.review_handoff_ref
        return payload

    def to_json(self) -> str:
        serialized = json.dumps(self.to_mapping(), sort_keys=True, separators=(",", ":"))
        if len(serialized.encode("utf-8")) > MAX_REQUEST_BYTES:
            raise ActionGateError("Action Gate preview request exceeds the size limit")
        return serialized


@dataclass(frozen=True, slots=True)
class ActionGateResult:
    schema_version: str
    preview: bool
    case_ref: str
    policy_family: str
    decision: PreviewDecision
    status: PreviewStatus
    derived_from_typed_facts: bool
    decision_labels: tuple[str, ...]
    denial_labels: tuple[str, ...]
    blocker_labels: tuple[str, ...]
    handoff_labels: tuple[str, ...]
    evidence_ready_refs: tuple[str, ...]
    receipt_ready_refs: tuple[str, ...]
    gate_ready_refs: tuple[str, ...]
    review_handoff_ref: str | None
    closed_gate_labels: tuple[str, ...]
    action_execution_authorized: bool
    public_projection_authorized: bool
    provider_runtime_authorized: bool
    customer_or_production_data_authorized: bool
    release_or_package_authorized: bool
    downstream_publication_authorized: bool

    @classmethod
    def from_mapping(cls, value: Mapping[str, Any]) -> ActionGateResult:
        keys = {
            "schema_version",
            "preview",
            "case_ref",
            "policy_family",
            "decision",
            "status",
            "derived_from_typed_facts",
            "decision_labels",
            "denial_labels",
            "blocker_labels",
            "handoff_labels",
            "evidence_ready_refs",
            "receipt_ready_refs",
            "gate_ready_refs",
            "review_handoff_ref",
            "closed_gate_labels",
            "action_execution_authorized",
            "public_projection_authorized",
            "provider_runtime_authorized",
            "customer_or_production_data_authorized",
            "release_or_package_authorized",
            "downstream_publication_authorized",
        }
        _require_keys(value, keys, "result")
        result = cls(
            schema_version=_string(value["schema_version"], "result schema version"),
            preview=_boolean(value["preview"], "preview flag"),
            case_ref=_reference(value["case_ref"], "result case reference"),
            policy_family=_string(value["policy_family"], "policy family"),
            decision=_enum(PreviewDecision, value["decision"], "result decision"),
            status=_enum(PreviewStatus, value["status"], "result status"),
            derived_from_typed_facts=_boolean(value["derived_from_typed_facts"], "derived flag"),
            decision_labels=_string_list(value["decision_labels"], "decision labels"),
            denial_labels=_string_list(value["denial_labels"], "denial labels"),
            blocker_labels=_string_list(value["blocker_labels"], "blocker labels"),
            handoff_labels=_string_list(value["handoff_labels"], "handoff labels"),
            evidence_ready_refs=_reference_list(value["evidence_ready_refs"], "ready evidence"),
            receipt_ready_refs=_reference_list(value["receipt_ready_refs"], "ready receipts"),
            gate_ready_refs=_reference_list(value["gate_ready_refs"], "ready gates"),
            review_handoff_ref=(
                None
                if value["review_handoff_ref"] is None
                else _reference(value["review_handoff_ref"], "result review handoff reference")
            ),
            closed_gate_labels=_string_list(value["closed_gate_labels"], "closed gate labels"),
            action_execution_authorized=_boolean(
                value["action_execution_authorized"], "action execution authorization"
            ),
            public_projection_authorized=_boolean(
                value["public_projection_authorized"], "public projection authorization"
            ),
            provider_runtime_authorized=_boolean(
                value["provider_runtime_authorized"], "provider runtime authorization"
            ),
            customer_or_production_data_authorized=_boolean(
                value["customer_or_production_data_authorized"], "customer data authorization"
            ),
            release_or_package_authorized=_boolean(
                value["release_or_package_authorized"], "release authorization"
            ),
            downstream_publication_authorized=_boolean(
                value["downstream_publication_authorized"], "publication authorization"
            ),
        )
        _validate_result(result)
        return result

    def to_mapping(self) -> dict[str, object]:
        return {
            "schema_version": self.schema_version,
            "preview": self.preview,
            "case_ref": self.case_ref,
            "policy_family": self.policy_family,
            "decision": self.decision.value,
            "status": self.status.value,
            "derived_from_typed_facts": self.derived_from_typed_facts,
            "decision_labels": list(self.decision_labels),
            "denial_labels": list(self.denial_labels),
            "blocker_labels": list(self.blocker_labels),
            "handoff_labels": list(self.handoff_labels),
            "evidence_ready_refs": list(self.evidence_ready_refs),
            "receipt_ready_refs": list(self.receipt_ready_refs),
            "gate_ready_refs": list(self.gate_ready_refs),
            "review_handoff_ref": self.review_handoff_ref,
            "closed_gate_labels": list(self.closed_gate_labels),
            "action_execution_authorized": self.action_execution_authorized,
            "public_projection_authorized": self.public_projection_authorized,
            "provider_runtime_authorized": self.provider_runtime_authorized,
            "customer_or_production_data_authorized": self.customer_or_production_data_authorized,
            "release_or_package_authorized": self.release_or_package_authorized,
            "downstream_publication_authorized": self.downstream_publication_authorized,
        }


class ActionGate:
    """Private, local-only evaluator backed exclusively by the native C ABI."""

    @staticmethod
    def capability() -> dict[str, object]:
        version_ptr = lib.nxuskit_version()
        if version_ptr == ffi.NULL:
            raise ActionGateError("native Action Gate preview version is unavailable")
        try:
            native_sdk_version = ffi.string(version_ptr).decode("utf-8")
        except (UnicodeDecodeError, TypeError):
            raise ActionGateError("native Action Gate preview version is invalid") from None
        if native_sdk_version != EXPECTED_VERSION:
            raise ActionGateError(
                "native Action Gate preview version does not match the Python package"
            )
        return {
            "namespace": "nxuskit.preview.action_gate",
            "schema_version": CAPABILITY_SCHEMA_VERSION,
            "policy_family": POLICY_FAMILY,
            "preview": True,
            "native_sdk_version": native_sdk_version,
            "production_authorization": False,
            "action_execution_authorized": False,
            "public_projection_authorized": False,
            "provider_runtime_authorized": False,
            "customer_or_production_data_authorized": False,
            "release_or_package_authorized": False,
            "downstream_publication_authorized": False,
        }

    @staticmethod
    def evaluate(request: ActionGateRequest) -> ActionGateResult:
        if not isinstance(request, ActionGateRequest):
            raise ActionGateError("Action Gate preview requires a typed request")
        encoded = request.to_json().encode("utf-8")
        ptr = lib.nxuskit_action_gate_preview_evaluate_json(encoded)
        if ptr == ffi.NULL:
            raise ActionGateError("native Action Gate preview failed")
        try:
            result_json = ffi.string(ptr).decode("utf-8")
        except UnicodeDecodeError:
            raise ActionGateError("native Action Gate preview returned invalid UTF-8") from None
        finally:
            lib.nxuskit_free_string(ptr)
        try:
            parsed = json.loads(result_json)
            return ActionGateResult.from_mapping(_mapping(parsed, "native result"))
        except (ActionGateError, TypeError, ValueError, json.JSONDecodeError):
            raise ActionGateError("native Action Gate preview returned an invalid result") from None


def _validate_request(request: ActionGateRequest) -> None:
    if request.schema_version != REQUEST_SCHEMA_VERSION:
        raise ActionGateError("unsupported Action Gate preview request schema version")
    _reference(request.request_ref, "request reference")
    _reference(request.case_ref, "case reference")
    request.action.to_mapping()
    request.actor_context.to_mapping()
    request.consequence.to_mapping()
    request.policy.to_mapping()
    request.evidence.to_mapping()
    if request.review_handoff_ref is not None:
        _reference(request.review_handoff_ref, "review handoff reference")


def _validate_action(action: ActionGateAction) -> None:
    if not isinstance(action.kind, ActionKind):
        raise ActionGateError("invalid action kind")
    _reference(action.action_ref, "action reference")
    _boolean(action.public_projection_requested, "public projection flag")
    _boolean(action.runtime_provider_claim_requested, "runtime provider flag")
    _boolean(action.unbounded_file_or_network_access_requested, "file/network flag")


def _validate_actor_context(actor_context: ActionGateActorContext) -> None:
    _reference(actor_context.actor_ref, "actor reference")
    _reference(actor_context.context_ref, "context reference")
    if not actor_context.actor_ref.startswith("synthetic.actor."):
        raise ActionGateError("actor reference must be synthetic")
    if not actor_context.context_ref.startswith("synthetic.context."):
        raise ActionGateError("context reference must be synthetic")
    if actor_context.local_synthetic_context is not True:
        raise ActionGateError("Action Gate preview requires a local synthetic context")
    if actor_context.contains_secret_material is not False:
        raise ActionGateError("Action Gate preview does not accept secret material")
    if actor_context.contains_customer_or_production_data is not False:
        raise ActionGateError("Action Gate preview does not accept customer or production data")


def _validate_consequence(consequence: ActionGateConsequence) -> None:
    if not isinstance(consequence.kind, ConsequenceKind):
        raise ActionGateError("invalid consequence kind")
    _reference(consequence.consequence_ref, "consequence reference")
    _boolean(consequence.exports_sensitive_data, "sensitive export flag")
    _boolean(consequence.material_effect, "material effect flag")
    _boolean(consequence.requires_review, "review flag")


def _validate_policy(policy: ActionGatePolicy) -> None:
    _reference(policy.policy_ref, "policy reference")
    _boolean(policy.low_consequence_action_allowed, "low consequence policy flag")
    _boolean(policy.sensitive_export_allowed, "sensitive export policy flag")
    _boolean(policy.material_effect_requires_review, "material review policy flag")
    _boolean(policy.public_projection_allowed, "public projection policy flag")


def _validate_evidence(evidence: ActionGateEvidence) -> None:
    _reference_list(evidence.evidence_refs, "evidence references")
    _reference_list(evidence.required_evidence_refs, "required evidence references")
    _reference_list(evidence.receipt_ready_refs, "receipt references")
    _reference_list(evidence.gate_ready_refs, "gate references")


def _validate_result(result: ActionGateResult) -> None:
    if result.schema_version != RESULT_SCHEMA_VERSION:
        raise ActionGateError("native Action Gate preview returned an unsupported result schema")
    if result.preview is not True or result.policy_family != POLICY_FAMILY:
        raise ActionGateError("native Action Gate preview returned an invalid posture")
    _reference(result.case_ref, "result case reference")
    for values, label in (
        (result.decision_labels, "decision labels"),
        (result.denial_labels, "denial labels"),
        (result.blocker_labels, "blocker labels"),
        (result.handoff_labels, "handoff labels"),
        (result.closed_gate_labels, "closed gate labels"),
    ):
        _sorted_string_list(values, label)
    for values, label in (
        (result.evidence_ready_refs, "ready evidence"),
        (result.receipt_ready_refs, "ready receipts"),
        (result.gate_ready_refs, "ready gates"),
    ):
        _sorted_reference_list(values, label)
    if result.review_handoff_ref is not None:
        _reference(result.review_handoff_ref, "result review handoff reference")
    if any(
        (
            result.action_execution_authorized,
            result.public_projection_authorized,
            result.provider_runtime_authorized,
            result.customer_or_production_data_authorized,
            result.release_or_package_authorized,
            result.downstream_publication_authorized,
        )
    ):
        raise ActionGateError("native Action Gate preview returned an unauthorized posture")


def _mapping(value: object, label: str) -> Mapping[str, Any]:
    if not isinstance(value, Mapping):
        raise ActionGateError(f"invalid {label}")
    return value


def _require_keys(
    value: Mapping[str, Any], required: set[str], label: str, optional: set[str] | None = None
) -> None:
    optional = optional or set()
    if set(value) != required | optional and not (required <= set(value) <= required | optional):
        raise ActionGateError(f"invalid {label} fields")


def _enum(enum_type: type[StrEnum], value: object, label: str) -> StrEnum:
    if not isinstance(value, str):
        raise ActionGateError(f"invalid {label}")
    try:
        return enum_type(value)
    except ValueError:
        raise ActionGateError(f"invalid {label}") from None


def _string(value: object, label: str) -> str:
    if not isinstance(value, str):
        raise ActionGateError(f"invalid {label}")
    return value


def _boolean(value: object, label: str) -> bool:
    if type(value) is not bool:
        raise ActionGateError(f"invalid {label}")
    return value


def _reference(value: object, label: str) -> str:
    reference = _string(value, label)
    if not (
        1 <= len(reference) <= MAX_REFERENCE_LENGTH
        and reference.isascii()
        and reference[0].isalnum()
        and reference[0].isascii()
        and all(
            character.isascii()
            and (character.islower() or character.isdigit() or character in "._:-")
            for character in reference
        )
    ):
        raise ActionGateError(f"invalid {label}")
    return reference


def _reference_list(value: object, label: str) -> tuple[str, ...]:
    if not isinstance(value, (list, tuple)):
        raise ActionGateError(f"invalid {label}")
    references = tuple(_reference(item, label) for item in value)
    if len(references) > MAX_REFERENCES or len(set(references)) != len(references):
        raise ActionGateError(f"invalid {label}")
    return references


def _string_list(value: object, label: str) -> tuple[str, ...]:
    if not isinstance(value, (list, tuple)):
        raise ActionGateError(f"invalid {label}")
    values = tuple(_string(item, label) for item in value)
    _sorted_string_list(values, label)
    return values


def _sorted_string_list(values: tuple[str, ...], label: str) -> None:
    if len(set(values)) != len(values) or tuple(sorted(values)) != values:
        raise ActionGateError(f"invalid {label}")


def _sorted_reference_list(values: tuple[str, ...], label: str) -> None:
    _reference_list(values, label)
    if tuple(sorted(values)) != values:
        raise ActionGateError(f"invalid {label}")


__all__ = [
    "ActionGate",
    "ActionGateAction",
    "ActionGateActorContext",
    "ActionGateConsequence",
    "ActionGateError",
    "ActionGateEvidence",
    "ActionGatePolicy",
    "ActionGateRequest",
    "ActionGateResult",
    "ActionKind",
    "ConsequenceKind",
    "PreviewDecision",
    "PreviewStatus",
]
