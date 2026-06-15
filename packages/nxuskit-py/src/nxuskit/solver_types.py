"""Solver types for the nxusKit constraint solver.

These dataclasses mirror the Rust solver types and are used for JSON
serialization to/from the C ABI.  They are available without the native
library so that application code can reference them freely.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from enum import Enum
from typing import Any

# ── Enums ────────────────────────────────────────────────────


class VariableType(str, Enum):
    """Type of a solver variable."""

    INTEGER = "integer"
    REAL = "real"
    BOOLEAN = "boolean"


class ConstraintType(str, Enum):
    """Constraint operation type."""

    EQ = "eq"
    NEQ = "neq"
    LT = "lt"
    GT = "gt"
    LE = "le"
    GE = "ge"
    ADD = "add"
    SUB = "sub"
    MUL = "mul"
    DIV = "div"
    AND = "and"
    OR = "or"
    NOT = "not"
    IMPLIES = "implies"
    IFF = "iff"
    ALL_DIFFERENT = "all_different"
    AT_MOST = "at_most"
    AT_LEAST = "at_least"
    EXACTLY = "exactly"
    IN_RANGE = "in_range"


class ObjectiveDirection(str, Enum):
    """Optimization direction."""

    MINIMIZE = "minimize"
    MAXIMIZE = "maximize"


class MultiObjectiveMode(str, Enum):
    """Multi-objective solving strategy."""

    WEIGHTED = "weighted"
    LEXICOGRAPHIC = "lexicographic"


class SolveStatus(str, Enum):
    """Solver outcome status."""

    SAT = "sat"
    UNSAT = "unsat"
    OPTIMAL = "optimal"
    UNKNOWN = "unknown"
    TIMEOUT = "timeout"


# ── Dataclasses ──────────────────────────────────────────────


@dataclass
class DomainDef:
    """Optional bounds or allowed values for a variable."""

    min: float | None = None
    max: float | None = None
    values: list[float] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        """Serialize to a dict for JSON encoding."""
        d: dict[str, Any] = {}
        if self.min is not None:
            d["min"] = self.min
        if self.max is not None:
            d["max"] = self.max
        if self.values:
            d["values"] = self.values
        return d

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> DomainDef:
        """Construct a DomainDef from a dict."""
        return cls(
            min=data.get("min"),
            max=data.get("max"),
            values=data.get("values", []),
        )


@dataclass
class VariableDef:
    """Solver variable definition."""

    name: str
    var_type: VariableType
    domain: DomainDef | None = None
    label: str = ""

    def to_dict(self) -> dict[str, Any]:
        """Serialize to a dict for JSON encoding."""
        d: dict[str, Any] = {"name": self.name, "var_type": self.var_type.value}
        if self.domain is not None:
            d["domain"] = self.domain.to_dict()
        if self.label:
            d["label"] = self.label
        return d

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> VariableDef:
        """Construct a VariableDef from a dict."""
        domain = DomainDef.from_dict(data["domain"]) if "domain" in data else None
        return cls(
            name=data["name"],
            var_type=VariableType(data["var_type"]),
            domain=domain,
            label=data.get("label", ""),
        )


@dataclass
class ConstraintDef:
    """Constraint definition."""

    constraint_type: ConstraintType
    variables: list[str]
    parameters: Any = None
    name: str = ""
    weight: float | None = None
    label: str = ""

    def to_dict(self) -> dict[str, Any]:
        """Serialize to a dict for JSON encoding."""
        d: dict[str, Any] = {
            "constraint_type": self.constraint_type.value,
            "variables": self.variables,
        }
        if self.parameters is not None:
            d["parameters"] = self.parameters
        if self.name:
            d["name"] = self.name
        if self.weight is not None:
            d["weight"] = self.weight
        if self.label:
            d["label"] = self.label
        return d

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> ConstraintDef:
        """Construct a ConstraintDef from a dict."""
        return cls(
            constraint_type=ConstraintType(data["constraint_type"]),
            variables=data["variables"],
            parameters=data.get("parameters"),
            name=data.get("name", ""),
            weight=data.get("weight"),
            label=data.get("label", ""),
        )


@dataclass
class ObjectiveDef:
    """Optimization objective definition."""

    name: str
    direction: ObjectiveDirection
    expression: str = ""
    variable: str = ""
    weight: float | None = None
    label: str = ""
    priority: int | None = None

    def to_dict(self) -> dict[str, Any]:
        """Serialize to a dict for JSON encoding."""
        d: dict[str, Any] = {
            "name": self.name,
            "direction": self.direction.value,
        }
        if self.expression:
            d["expression"] = self.expression
        if self.variable:
            d["variable"] = self.variable
        if self.weight is not None:
            d["weight"] = self.weight
        if self.label:
            d["label"] = self.label
        if self.priority is not None:
            d["priority"] = self.priority
        return d

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> ObjectiveDef:
        """Construct an ObjectiveDef from a dict."""
        return cls(
            name=data["name"],
            direction=ObjectiveDirection(data["direction"]),
            expression=data.get("expression", ""),
            variable=data.get("variable", ""),
            weight=data.get("weight"),
            label=data.get("label", ""),
            priority=data.get("priority"),
        )


@dataclass
class SolverConfig:
    """Per-session or per-solve configuration."""

    timeout_ms: int | None = None
    random_seed: int | None = None
    max_conflicts: int | None = None
    multi_objective_mode: MultiObjectiveMode | None = None
    produce_explanation: bool | None = None

    def to_dict(self) -> dict[str, Any]:
        """Serialize to a dict for JSON encoding."""
        d: dict[str, Any] = {}
        if self.timeout_ms is not None:
            d["timeout_ms"] = self.timeout_ms
        if self.random_seed is not None:
            d["random_seed"] = self.random_seed
        if self.max_conflicts is not None:
            d["max_conflicts"] = self.max_conflicts
        if self.multi_objective_mode is not None:
            d["multi_objective_mode"] = self.multi_objective_mode.value
        if self.produce_explanation is not None:
            d["produce_explanation"] = self.produce_explanation
        return d

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> SolverConfig:
        """Construct a SolverConfig from a dict."""
        mode = data.get("multi_objective_mode")
        return cls(
            timeout_ms=data.get("timeout_ms"),
            random_seed=data.get("random_seed"),
            max_conflicts=data.get("max_conflicts"),
            multi_objective_mode=MultiObjectiveMode(mode) if mode else None,
            produce_explanation=data.get("produce_explanation"),
        )

    def to_json(self) -> str:
        """Serialize to a JSON string."""
        return json.dumps(self.to_dict())


@dataclass
class SolverValue:
    """Tagged union for variable assignments."""

    type: str
    value: Any

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> SolverValue:
        """Construct a SolverValue from a dict."""
        return cls(type=data["type"], value=data["value"])


@dataclass
class SolverStats:
    """Solver performance statistics."""

    solve_time_ms: int = 0
    num_variables: int = 0
    num_constraints: int = 0
    num_conflicts: int | None = None
    num_decisions: int | None = None

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> SolverStats:
        """Construct a SolverStats from a dict."""
        return cls(
            solve_time_ms=data.get("solve_time_ms", 0),
            num_variables=data.get("num_variables", 0),
            num_constraints=data.get("num_constraints", 0),
            num_conflicts=data.get("num_conflicts"),
            num_decisions=data.get("num_decisions"),
        )


@dataclass
class SolverExplanation:
    """Explainability artifacts from a solve."""

    conflict_labels: list[str] = field(default_factory=list)
    binding_constraints: list[str] = field(default_factory=list)
    slack_values: dict[str, float] = field(default_factory=dict)

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> SolverExplanation:
        """Construct a SolverExplanation from a dict."""
        return cls(
            conflict_labels=data.get("conflict_labels", []),
            binding_constraints=data.get("binding_constraints", []),
            slack_values=data.get("slack_values", {}),
        )


@dataclass
class SolveResult:
    """Outcome of a solve operation."""

    status: SolveStatus
    assignments: dict[str, SolverValue] = field(default_factory=dict)
    objective_value: float | None = None
    stats: SolverStats = field(default_factory=SolverStats)
    conflict_labels: list[str] = field(default_factory=list)
    objective_values: dict[str, float] = field(default_factory=dict)
    violated_soft_constraints: list[str] = field(default_factory=list)
    explanation: SolverExplanation | None = None

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> SolveResult:
        """Construct a SolveResult from a dict."""
        assignments = {}
        for k, v in data.get("assignments", {}).items():
            assignments[k] = SolverValue.from_dict(v)
        expl = data.get("explanation")
        return cls(
            status=SolveStatus(data["status"]),
            assignments=assignments,
            objective_value=data.get("objective_value"),
            stats=SolverStats.from_dict(data.get("stats", {})),
            conflict_labels=data.get("conflict_labels", []),
            objective_values=data.get("objective_values", {}),
            violated_soft_constraints=data.get("violated_soft_constraints", []),
            explanation=SolverExplanation.from_dict(expl) if expl else None,
        )


@dataclass
class SolverCapabilities:
    """Solver backend capabilities."""

    backend: str = ""
    supports_incremental: bool = False
    supports_conflict_explanations: bool = False
    supports_multi_objective: bool = False
    supports_push_pop: bool = False
    supports_assumptions: bool = False
    supports_soft_constraints: bool = False
    supports_explanation: bool = False

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> SolverCapabilities:
        """Construct a SolverCapabilities from a dict."""
        return cls(
            backend=data.get("backend", ""),
            supports_incremental=data.get("supports_incremental", False),
            supports_conflict_explanations=data.get("supports_conflict_explanations", False),
            supports_multi_objective=data.get("supports_multi_objective", False),
            supports_push_pop=data.get("supports_push_pop", False),
            supports_assumptions=data.get("supports_assumptions", False),
            supports_soft_constraints=data.get("supports_soft_constraints", False),
            supports_explanation=data.get("supports_explanation", False),
        )


@dataclass
class SessionStatus:
    """Snapshot of the solver session state."""

    num_variables: int = 0
    num_constraints: int = 0
    has_objective: bool = False
    scope_depth: int = 0
    last_status: SolveStatus | None = None

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> SessionStatus:
        """Construct a SessionStatus from a dict."""
        ls = data.get("last_status")
        return cls(
            num_variables=data.get("num_variables", 0),
            num_constraints=data.get("num_constraints", 0),
            has_objective=data.get("has_objective", False),
            scope_depth=data.get("scope_depth", 0),
            last_status=SolveStatus(ls) if ls else None,
        )
