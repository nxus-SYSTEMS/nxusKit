"""Tests for the constraint solver types and wrapper.

Tests that require the native library are skipped when it's not available.
Pure-Python type tests run unconditionally.
"""

from __future__ import annotations

import json

from nxuskit.solver_types import (
    ConstraintDef,
    ConstraintType,
    DomainDef,
    MultiObjectiveMode,
    ObjectiveDef,
    ObjectiveDirection,
    SessionStatus,
    SolverCapabilities,
    SolverConfig,
    SolveResult,
    SolverExplanation,
    SolveStatus,
    VariableDef,
    VariableType,
)

# ── T084: Solver types dataclass creation and JSON round-trip ─


class TestVariableDef:
    def test_create_integer(self):
        v = VariableDef(
            name="x",
            var_type=VariableType.INTEGER,
            domain=DomainDef(min=0, max=10),
        )
        assert v.name == "x"
        assert v.var_type == VariableType.INTEGER
        assert v.domain is not None
        assert v.domain.min == 0
        assert v.domain.max == 10

    def test_json_round_trip(self):
        v = VariableDef(
            name="y",
            var_type=VariableType.REAL,
            domain=DomainDef(min=-1.5, max=1.5),
            label="some var",
        )
        d = v.to_dict()
        v2 = VariableDef.from_dict(d)
        assert v2.name == "y"
        assert v2.var_type == VariableType.REAL
        assert v2.domain is not None
        assert v2.domain.min == -1.5
        assert v2.label == "some var"

    def test_no_domain(self):
        v = VariableDef(name="b", var_type=VariableType.BOOLEAN)
        d = v.to_dict()
        assert "domain" not in d
        v2 = VariableDef.from_dict(d)
        assert v2.domain is None

    def test_domain_values(self):
        dom = DomainDef(values=[1.0, 2.0, 3.0])
        d = dom.to_dict()
        assert d["values"] == [1.0, 2.0, 3.0]
        dom2 = DomainDef.from_dict(d)
        assert dom2.values == [1.0, 2.0, 3.0]


class TestConstraintDef:
    def test_create_le(self):
        c = ConstraintDef(
            constraint_type=ConstraintType.LE,
            variables=["x"],
            parameters={"right": 5},
        )
        assert c.constraint_type == ConstraintType.LE
        assert c.variables == ["x"]

    def test_json_round_trip(self):
        c = ConstraintDef(
            name="c1",
            constraint_type=ConstraintType.GE,
            variables=["x"],
            parameters={"right": 10},
            weight=0.5,
            label="minimum x",
        )
        d = c.to_dict()
        c2 = ConstraintDef.from_dict(d)
        assert c2.name == "c1"
        assert c2.weight == 0.5
        assert c2.label == "minimum x"

    def test_all_constraint_types(self):
        for ct in ConstraintType:
            c = ConstraintDef(constraint_type=ct, variables=["x"])
            d = c.to_dict()
            c2 = ConstraintDef.from_dict(d)
            assert c2.constraint_type == ct


class TestObjectiveDef:
    def test_create_minimize(self):
        o = ObjectiveDef(
            name="min_x",
            direction=ObjectiveDirection.MINIMIZE,
            expression="x",
            weight=1.0,
        )
        assert o.direction == ObjectiveDirection.MINIMIZE
        assert o.weight == 1.0

    def test_json_round_trip(self):
        o = ObjectiveDef(
            name="max_y",
            direction=ObjectiveDirection.MAXIMIZE,
            variable="y",
            priority=2,
        )
        d = o.to_dict()
        o2 = ObjectiveDef.from_dict(d)
        assert o2.name == "max_y"
        assert o2.direction == ObjectiveDirection.MAXIMIZE
        assert o2.priority == 2


class TestSolverConfig:
    def test_defaults(self):
        cfg = SolverConfig()
        d = cfg.to_dict()
        assert d == {}

    def test_full_config(self):
        cfg = SolverConfig(
            timeout_ms=5000,
            random_seed=42,
            max_conflicts=1000,
            multi_objective_mode=MultiObjectiveMode.WEIGHTED,
            produce_explanation=True,
        )
        d = cfg.to_dict()
        assert d["timeout_ms"] == 5000
        assert d["multi_objective_mode"] == "weighted"
        assert d["produce_explanation"] is True

        cfg2 = SolverConfig.from_dict(d)
        assert cfg2.timeout_ms == 5000
        assert cfg2.multi_objective_mode == MultiObjectiveMode.WEIGHTED

    def test_to_json(self):
        cfg = SolverConfig(timeout_ms=3000)
        j = cfg.to_json()
        parsed = json.loads(j)
        assert parsed["timeout_ms"] == 3000


class TestSolveResult:
    def test_from_dict_sat(self):
        data = {
            "status": "sat",
            "assignments": {"x": {"type": "integer", "value": 5}},
            "stats": {"solve_time_ms": 10, "num_variables": 1, "num_constraints": 1},
        }
        r = SolveResult.from_dict(data)
        assert r.status == SolveStatus.SAT
        assert "x" in r.assignments
        assert r.assignments["x"].value == 5
        assert r.stats.solve_time_ms == 10

    def test_from_dict_unsat(self):
        data = {
            "status": "unsat",
            "unsat_core": ["c1", "c2"],
            "stats": {},
        }
        r = SolveResult.from_dict(data)
        assert r.status == SolveStatus.UNSAT
        assert r.unsat_core == ["c1", "c2"]

    def test_from_dict_with_explanation(self):
        data = {
            "status": "unsat",
            "stats": {},
            "explanation": {
                "unsat_core_labels": ["label1", "label2"],
                "slack_values": {"c1": 0.5},
            },
        }
        r = SolveResult.from_dict(data)
        assert r.explanation is not None
        assert r.explanation.unsat_core_labels == ["label1", "label2"]
        assert r.explanation.slack_values["c1"] == 0.5

    def test_from_dict_with_multi_objective(self):
        data = {
            "status": "optimal",
            "objective_values": {"min_x": 0.0, "min_y": 0.0},
            "stats": {},
        }
        r = SolveResult.from_dict(data)
        assert r.status == SolveStatus.OPTIMAL
        assert r.objective_values == {"min_x": 0.0, "min_y": 0.0}

    def test_from_dict_with_violations(self):
        data = {
            "status": "sat",
            "violated_soft_constraints": ["soft_upper"],
            "stats": {},
        }
        r = SolveResult.from_dict(data)
        assert r.violated_soft_constraints == ["soft_upper"]


class TestSolverCapabilities:
    def test_from_dict(self):
        data = {
            "backend": "z3",
            "supports_incremental": True,
            "supports_unsat_core": True,
            "supports_multi_objective": True,
            "supports_push_pop": True,
            "supports_assumptions": True,
            "supports_soft_constraints": True,
            "supports_explanation": True,
        }
        caps = SolverCapabilities.from_dict(data)
        assert caps.backend == "z3"
        assert caps.supports_multi_objective is True
        assert caps.supports_explanation is True


class TestSolverExplanation:
    def test_from_dict(self):
        data = {
            "unsat_core_labels": ["a", "b"],
            "binding_constraints": ["c1"],
            "slack_values": {"c2": 1.5},
        }
        expl = SolverExplanation.from_dict(data)
        assert expl.unsat_core_labels == ["a", "b"]
        assert expl.binding_constraints == ["c1"]
        assert expl.slack_values["c2"] == 1.5

    def test_empty(self):
        expl = SolverExplanation.from_dict({})
        assert expl.unsat_core_labels == []
        assert expl.slack_values == {}


class TestSessionStatus:
    def test_from_dict(self):
        data = {
            "num_variables": 3,
            "num_constraints": 2,
            "has_objective": True,
            "scope_depth": 1,
            "last_status": "sat",
        }
        s = SessionStatus.from_dict(data)
        assert s.num_variables == 3
        assert s.has_objective is True
        assert s.last_status == SolveStatus.SAT

    def test_no_last_status(self):
        s = SessionStatus.from_dict({"num_variables": 0, "num_constraints": 0})
        assert s.last_status is None


class TestEnumValues:
    def test_variable_types(self):
        assert VariableType.INTEGER.value == "integer"
        assert VariableType.REAL.value == "real"
        assert VariableType.BOOLEAN.value == "boolean"

    def test_solve_statuses(self):
        assert SolveStatus.SAT.value == "sat"
        assert SolveStatus.UNSAT.value == "unsat"
        assert SolveStatus.OPTIMAL.value == "optimal"
        assert SolveStatus.UNKNOWN.value == "unknown"
        assert SolveStatus.TIMEOUT.value == "timeout"

    def test_objective_directions(self):
        assert ObjectiveDirection.MINIMIZE.value == "minimize"
        assert ObjectiveDirection.MAXIMIZE.value == "maximize"

    def test_multi_objective_modes(self):
        assert MultiObjectiveMode.WEIGHTED.value == "weighted"
        assert MultiObjectiveMode.LEXICOGRAPHIC.value == "lexicographic"


# ── T085: SolverSession lifecycle (mock-based — requires native lib) ──


class TestSolverSessionImport:
    """T088: Import solver types without native library (lazy loading)."""

    def test_import_solver_types(self):
        """Solver types are importable without the native library."""
        from nxuskit.solver_types import (  # noqa: F401
            ConstraintDef,
            ObjectiveDef,
            SolverConfig,
            SolveResult,
            VariableDef,
        )

    def test_import_solver_session_class(self):
        """SolverSession class is importable (but instantiation needs native lib)."""
        from nxuskit.solver import SolverSession  # noqa: F401

    def test_import_solver_error(self):
        """SolverError is importable."""
        from nxuskit.solver import SolverError  # noqa: F401

    def test_create_types_without_native_lib(self):
        """Can create type instances without native library."""
        v = VariableDef(name="x", var_type=VariableType.INTEGER)
        c = ConstraintDef(
            constraint_type=ConstraintType.LE,
            variables=["x"],
            parameters={"right": 5},
        )
        o = ObjectiveDef(
            name="min_x",
            direction=ObjectiveDirection.MINIMIZE,
            expression="x",
        )
        cfg = SolverConfig(timeout_ms=1000)

        # Serialize to JSON and back
        v_json = json.dumps(v.to_dict())
        c_json = json.dumps(c.to_dict())
        o_json = json.dumps(o.to_dict())
        cfg_json = cfg.to_json()

        assert json.loads(v_json)["name"] == "x"
        assert json.loads(c_json)["constraint_type"] == "le"
        assert json.loads(o_json)["direction"] == "minimize"
        assert json.loads(cfg_json)["timeout_ms"] == 1000
