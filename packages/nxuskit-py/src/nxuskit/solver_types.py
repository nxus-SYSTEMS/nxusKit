"""Public CE solver placeholder types.

Pro solver domain types are not shipped in public CE source or release bundles.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Any


class VariableType(str, Enum):
    INTEGER = "integer"
    REAL = "real"
    BOOLEAN = "boolean"


class ConstraintType(str, Enum):
    UNAVAILABLE = "unavailable"


class ObjectiveDirection(str, Enum):
    MINIMIZE = "minimize"
    MAXIMIZE = "maximize"


class MultiObjectiveMode(str, Enum):
    WEIGHTED = "weighted"
    LEXICOGRAPHIC = "lexicographic"


class SolveStatus(str, Enum):
    UNKNOWN = "unknown"


class SessionStatus(str, Enum):
    UNAVAILABLE = "unavailable"


SolverValue = Any


@dataclass
class DomainDef:
    pass


@dataclass
class VariableDef:
    name: str = ""
    var_type: VariableType = VariableType.INTEGER


@dataclass
class ConstraintDef:
    constraint_type: ConstraintType = ConstraintType.UNAVAILABLE
    variables: list[str] = field(default_factory=list)


@dataclass
class ObjectiveDef:
    name: str = ""
    direction: ObjectiveDirection = ObjectiveDirection.MINIMIZE


@dataclass
class SolverConfig:
    pass


@dataclass
class SolverStats:
    pass


@dataclass
class SolverExplanation:
    pass


@dataclass
class SolveResult:
    status: SolveStatus = SolveStatus.UNKNOWN


@dataclass
class SolverCapabilities:
    available: bool = False
