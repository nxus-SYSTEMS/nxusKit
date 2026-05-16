"""Tests for the Bayesian Network Python wrapper.

Tests are split into:
  - Unit tests: Test error handling and missing library behavior (no native lib needed)
  - Integration tests: Require libnxuskit at runtime (marked with @pytest.mark.nxuskit)

Run integration tests: pytest tests/test_bn.py -v -m nxuskit
Run unit tests only: pytest tests/test_bn.py -v -m "not nxuskit"
"""

from __future__ import annotations

import json
import os
from pathlib import Path

import pytest


def fixture_path(name: str) -> str:
    """Resolve a BN fixture file path."""
    base = (
        Path(__file__).parent.parent.parent.parent
        / "nxuskit-engine"
        / "crates"
        / "nxuskit-engine"
        / "tests"
        / "fixtures"
        / "bn"
        / name
    )
    return str(base.resolve())


# Custom marker for tests that require native library
nxuskit = pytest.mark.skipif(
    not os.environ.get("NXUSKIT_LIB_PATH") and not os.environ.get("NXUSKIT_AVAILABLE"),
    reason="Requires libnxuskit native library",
)


# ── Unit Tests (no native library needed) ────────────────────────


class TestBnLibraryErrors:
    def test_get_lib_returns_handle_or_raises(self):
        """_get_lib() returns a valid handle or raises BnLibraryNotFoundError."""
        from nxuskit._bn_ffi import BnLibraryNotFoundError

        try:
            from nxuskit._bn_ffi import _get_lib

            # If we get here, the import succeeded (dev machine with lib).
            # Verify _get_lib returns a non-None handle.
            lib = _get_lib()
            assert lib is not None, "_get_lib() returned None on dev machine"
        except BnLibraryNotFoundError:
            # On CI without native library: this is the expected error.
            pass
        except Exception as e:
            # Any other exception from _ffi.py (e.g., ConfigError) during
            # import is also acceptable — it means the library isn't available.
            # But we assert it's the right kind of error, not a random crash.
            assert "nxuskit" in str(e).lower() or "library" in str(e).lower(), (
                f"Unexpected error (not library-related): {e}"
            )


class TestBnDataclasses:
    def test_continuous_marginal_dataclass(self):
        from nxuskit.bn import ContinuousMarginal

        m = ContinuousMarginal(mean=1.0, variance=0.5, ci_lower=-0.5, ci_upper=2.5)
        assert m.mean == 1.0
        assert m.variance == 0.5
        assert m.ci_lower == -0.5
        assert m.ci_upper == 2.5

    def test_stream_chunk_dataclass(self):
        from nxuskit.bn import BnStreamChunk

        chunk = BnStreamChunk(
            chunk_json='{"test": true}', iteration=100, total=1000, is_final=False
        )
        assert chunk.iteration == 100
        assert not chunk.is_final


# ── Integration Tests (require native library) ──────────────────


@nxuskit
class TestBnNetworkLifecycle:
    def test_create_empty(self):
        from nxuskit.bn import BnNetwork

        with BnNetwork.create() as net:
            assert net.num_variables == 0

    def test_load_file(self):
        from nxuskit.bn import BnNetwork

        path = fixture_path("asia.bif")
        with BnNetwork.load(path) as net:
            assert net.num_variables == 8

    def test_load_nonexistent(self):
        from nxuskit.bn import BnError, BnNetwork

        with pytest.raises(BnError):
            BnNetwork.load("nonexistent.bif")

    def test_variables(self):
        from nxuskit.bn import BnNetwork

        path = fixture_path("asia.bif")
        with BnNetwork.load(path) as net:
            vars = net.variables
            assert len(vars) == 8
            assert "Smoking" in vars

    def test_variable_states(self):
        from nxuskit.bn import BnNetwork

        path = fixture_path("asia.bif")
        with BnNetwork.load(path) as net:
            states = net.variable_states("Smoking")
            assert len(states) == 2
            assert "yes" in states
            assert "no" in states


@nxuskit
class TestBnSaveFile:
    def test_save_roundtrip(self, tmp_path):
        from nxuskit.bn import BnNetwork

        path = fixture_path("asia.bif")
        with BnNetwork.load(path) as net:
            out = str(tmp_path / "asia_export.bif")
            net.save_file(out)
            assert os.path.getsize(out) > 0

            with BnNetwork.load(out) as reloaded:
                assert reloaded.num_variables == 8


@nxuskit
class TestBnGaussianVariables:
    def test_add_gaussian(self):
        from nxuskit.bn import BnNetwork

        with BnNetwork.create() as net:
            net.add_gaussian_variable("X", 0.0, 1.0)

    def test_set_gaussian_weight(self):
        from nxuskit.bn import BnNetwork

        with BnNetwork.create() as net:
            net.add_gaussian_variable("X", 0.0, 1.0)
            net.add_gaussian_variable("Y", 0.0, 1.0)
            net.set_gaussian_weight("Y", "X", 0.5)


@nxuskit
class TestBnEvidence:
    def test_set_discrete(self):
        from nxuskit.bn import BnEvidence, BnNetwork

        path = fixture_path("asia.bif")
        with BnNetwork.load(path) as net:
            ev = BnEvidence()
            ev.set_discrete(net, "Smoking", "yes")
            ev.close()

    def test_set_continuous(self):
        from nxuskit.bn import BnEvidence, BnNetwork

        with BnNetwork.create() as net:
            net.add_gaussian_variable("X", 0.0, 1.0)
            ev = BnEvidence()
            ev.set_continuous(net, "X", 2.5)
            ev.close()

    def test_retract(self):
        from nxuskit.bn import BnEvidence, BnNetwork

        path = fixture_path("asia.bif")
        with BnNetwork.load(path) as net:
            ev = BnEvidence()
            ev.set_discrete(net, "Smoking", "yes")
            ev.retract("Smoking")
            ev.close()

    def test_clear(self):
        from nxuskit.bn import BnEvidence, BnNetwork

        path = fixture_path("asia.bif")
        with BnNetwork.load(path) as net:
            ev = BnEvidence()
            ev.set_discrete(net, "Smoking", "yes")
            ev.clear()
            ev.close()

    def test_context_manager(self):
        from nxuskit.bn import BnEvidence, BnNetwork

        path = fixture_path("asia.bif")
        with BnNetwork.load(path) as net:
            with BnEvidence() as ev:
                ev.set_discrete(net, "Smoking", "yes")


@nxuskit
class TestBnInference:
    def test_infer_ve(self):
        from nxuskit.bn import BnEvidence, BnNetwork

        path = fixture_path("asia.bif")
        with BnNetwork.load(path) as net:
            with BnEvidence() as ev:
                result = net.infer(ev, "ve")
                assert result.num_variables == 8
                result.close()

    def test_infer_ve_with_evidence(self):
        from nxuskit.bn import BnEvidence, BnNetwork

        path = fixture_path("asia.bif")
        with BnNetwork.load(path) as net:
            with BnEvidence() as ev:
                ev.set_discrete(net, "Smoking", "yes")
                with net.infer(ev, "ve") as result:
                    dist = result.marginal("Bronchitis")
                    assert dist["present"] > 0.5

    def test_infer_jt(self):
        from nxuskit.bn import BnEvidence, BnNetwork

        path = fixture_path("asia.bif")
        with BnNetwork.load(path) as net:
            with BnEvidence() as ev:
                with net.infer(ev, "jt") as result:
                    assert result.num_variables == 8

    def test_infer_gibbs(self):
        from nxuskit.bn import BnEvidence, BnNetwork

        path = fixture_path("asia.bif")
        with BnNetwork.load(path) as net:
            with BnEvidence() as ev:
                with net.infer(ev, "gibbs", num_samples=5000, burn_in=500, seed=42) as result:
                    assert result.num_variables == 8

    def test_infer_lbp(self):
        from nxuskit.bn import BnEvidence, BnNetwork

        path = fixture_path("asia.bif")
        with BnNetwork.load(path) as net:
            with BnEvidence() as ev:
                with net.infer(ev, "lbp") as result:
                    assert result.num_variables == 8


@nxuskit
class TestBnInferWithConfig:
    def test_lbp_config(self):
        from nxuskit.bn import BnEvidence, BnNetwork

        path = fixture_path("asia.bif")
        with BnNetwork.load(path) as net:
            with BnEvidence() as ev:
                config = {"max_iterations": 200, "damping": 0.3}
                with net.infer_with_config(ev, "lbp", config) as result:
                    assert result.num_variables == 8

    def test_gibbs_config(self):
        from nxuskit.bn import BnEvidence, BnNetwork

        path = fixture_path("asia.bif")
        with BnNetwork.load(path) as net:
            with BnEvidence() as ev:
                with net.infer_with_config(
                    ev, "gibbs", {"num_samples": 5000, "burn_in": 500, "seed": 42}
                ) as result:
                    assert result.num_variables == 8

    def test_nuts_config(self):
        from nxuskit.bn import BnEvidence, BnNetwork

        with BnNetwork.create() as net:
            net.add_gaussian_variable("X", 0.0, 1.0)
            net.add_gaussian_variable("Y", 0.0, 1.0)
            net.set_gaussian_weight("Y", "X", 0.8)

            with BnEvidence() as ev:
                config = {"num_samples": 500, "num_tune": 200, "seed": 42}
                with net.infer_with_config(ev, "nuts", config) as result:
                    j = json.loads(result.to_json())
                    assert "continuous_marginals" in j


@nxuskit
class TestBnResultAccess:
    def test_to_json(self):
        from nxuskit.bn import BnEvidence, BnNetwork

        path = fixture_path("asia.bif")
        with BnNetwork.load(path) as net:
            with BnEvidence() as ev:
                with net.infer(ev, "ve") as result:
                    j = json.loads(result.to_json())
                    assert "marginals" in j
                    assert j["algorithm"] == "ve"

    def test_marginal(self):
        from nxuskit.bn import BnEvidence, BnNetwork

        path = fixture_path("asia.bif")
        with BnNetwork.load(path) as net:
            with BnEvidence() as ev:
                with net.infer(ev, "ve") as result:
                    dist = result.marginal("Smoking")
                    assert len(dist) == 2
                    assert abs(dist["yes"] - 0.5) < 1e-6
                    assert abs(dist["no"] - 0.5) < 1e-6

    def test_variable_names(self):
        from nxuskit.bn import BnEvidence, BnNetwork

        path = fixture_path("asia.bif")
        with BnNetwork.load(path) as net:
            with BnEvidence() as ev:
                with net.infer(ev, "ve") as result:
                    names = result.variable_names()
                    assert len(names) == 8

    def test_iterator(self):
        from nxuskit.bn import BnEvidence, BnNetwork

        path = fixture_path("asia.bif")
        with BnNetwork.load(path) as net:
            with BnEvidence() as ev:
                with net.infer(ev, "ve") as result:
                    names = list(result)
                    assert len(names) == 8

    def test_marginals_dict(self):
        from nxuskit.bn import BnEvidence, BnNetwork

        path = fixture_path("asia.bif")
        with BnNetwork.load(path) as net:
            with BnEvidence() as ev:
                with net.infer(ev, "ve") as result:
                    m = result.marginals_dict
                    assert "Smoking" in m
                    assert len(m) == 8


@nxuskit
class TestBnContinuousMarginal:
    def test_mean_variance(self):
        from nxuskit.bn import BnEvidence, BnNetwork

        with BnNetwork.create() as net:
            net.add_gaussian_variable("X", 5.0, 2.0)
            with BnEvidence() as ev:
                config = {"num_samples": 1000, "num_tune": 500, "seed": 42}
                with net.infer_with_config(ev, "nuts", config) as result:
                    mean = result.mean("X")
                    assert abs(mean - 5.0) < 2.0
                    var = result.variance("X")
                    assert var > 0

    def test_continuous_marginal(self):
        from nxuskit.bn import BnEvidence, BnNetwork

        with BnNetwork.create() as net:
            net.add_gaussian_variable("X", 0.0, 1.0)
            with BnEvidence() as ev:
                config = {"num_samples": 500, "num_tune": 200, "seed": 42}
                with net.infer_with_config(ev, "nuts", config) as result:
                    m = result.continuous_marginal("X")
                    assert m.variance > 0
                    assert m.ci_lower < m.mean
                    assert m.ci_upper > m.mean


@nxuskit
class TestBnCrossValidation:
    def test_ve_jt_agreement(self):
        from nxuskit.bn import BnEvidence, BnNetwork

        path = fixture_path("asia.bif")
        with BnNetwork.load(path) as net:
            with BnEvidence() as ev:
                ev.set_discrete(net, "Smoking", "yes")
                with net.infer(ev, "ve") as result_ve:
                    with net.infer(ev, "jt") as result_jt:
                        ve_dist = result_ve.marginal("Bronchitis")
                        jt_dist = result_jt.marginal("Bronchitis")
                        for state in ve_dist:
                            assert abs(ve_dist[state] - jt_dist[state]) < 1e-6

    def test_lbp_ve_approximate(self):
        from nxuskit.bn import BnEvidence, BnNetwork

        path = fixture_path("asia.bif")
        with BnNetwork.load(path) as net:
            with BnEvidence() as ev:
                ev.set_discrete(net, "Smoking", "yes")
                with net.infer(ev, "ve") as result_ve:
                    with net.infer(ev, "lbp") as result_lbp:
                        ve_dist = result_ve.marginal("Bronchitis")
                        lbp_dist = result_lbp.marginal("Bronchitis")
                        for state in ve_dist:
                            assert abs(ve_dist[state] - lbp_dist[state]) < 0.05


@nxuskit
class TestBnAlarmNetwork:
    def test_alarm_37_nodes(self):
        from nxuskit.bn import BnEvidence, BnNetwork

        path = fixture_path("alarm.bif")
        with BnNetwork.load(path) as net:
            assert net.num_variables == 37
            with BnEvidence() as ev:
                with net.infer(ev, "ve") as result:
                    assert result.num_variables == 37


@nxuskit
class TestBnStreaming:
    def test_gibbs_stream(self):
        from nxuskit.bn import BnEvidence, BnNetwork

        path = fixture_path("asia.bif")
        with BnNetwork.load(path) as net:
            with BnEvidence() as ev:
                chunks = list(
                    net.infer_stream(ev, num_samples=5000, burn_in=500, seed=42, chunk_size=1000)
                )
                assert len(chunks) > 0
                assert chunks[-1].is_final


@nxuskit
class TestBnAsync:
    @pytest.mark.asyncio
    async def test_infer_async(self):
        from nxuskit.bn import BnEvidence, BnNetwork

        path = fixture_path("asia.bif")
        with BnNetwork.load(path) as net:
            with BnEvidence() as ev:
                result = await net.infer_async(ev, "ve")
                assert result.num_variables == 8
                result.close()


@nxuskit
class TestBnResourceCleanup:
    def test_double_close(self):
        from nxuskit.bn import BnEvidence, BnNetwork

        path = fixture_path("asia.bif")
        net = BnNetwork.load(path)
        ev = BnEvidence()
        ev.set_discrete(net, "Smoking", "yes")
        result = net.infer(ev, "ve")
        # Close everything twice — should not panic
        result.close()
        result.close()
        ev.close()
        ev.close()
        net.close()
        net.close()
