"""Pythonic Bayesian Network wrapper over the nxusKit C ABI.

Provides BnNetwork, BnEvidence, and BnResult classes with context manager
support, numpy integration for marginals, and async inference.

Example::

    from nxuskit.bn import BnNetwork, BnEvidence

    with BnNetwork.load("asia.bif") as net:
        ev = BnEvidence()
        ev.set_discrete(net, "Smoking", "yes")
        result = net.infer(ev, "ve")
        print(result.marginal("Bronchitis"))
"""

from __future__ import annotations

import asyncio
import json
import math
from dataclasses import dataclass
from typing import Any, Iterator

from ._bn_ffi import BnLibraryNotFoundError, bn_ffi, last_error, read_and_free_string

__all__ = [
    "BnNetwork",
    "BnEvidence",
    "BnResult",
    "ContinuousMarginal",
    "BnStreamChunk",
    "BnError",
    "BnLibraryNotFoundError",
]


class BnError(RuntimeError):
    """Base error for Bayesian Network operations."""


def _get_lib():
    from ._bn_ffi import _get_lib as _gl

    return _gl()


def _check_ok(ok: bool, fallback: str) -> None:
    """Raise BnError if a C ABI call returned false."""
    if not ok:
        err = last_error()
        raise BnError(err if err else fallback)


# ── ContinuousMarginal ───────────────────────────────────────


@dataclass
class ContinuousMarginal:
    """Posterior summary for a continuous variable."""

    mean: float
    variance: float
    ci_lower: float
    ci_upper: float


@dataclass
class BnStreamChunk:
    """A progressive inference result from streaming."""

    chunk_json: str
    iteration: int
    total: int
    is_final: bool


# ── BnNetwork ────────────────────────────────────────────────


class BnNetwork:
    """Bayesian Network handle with RAII cleanup."""

    def __init__(self, handle):
        self._handle = handle

    @classmethod
    def create(cls) -> BnNetwork:
        """Create an empty network."""
        lib = _get_lib()
        h = lib.nxuskit_bn_net_create()
        if h == bn_ffi.NULL:
            raise BnError(last_error() or "failed to create BN")
        return cls(h)

    @classmethod
    def load(cls, path: str) -> BnNetwork:
        """Load a BIF file."""
        lib = _get_lib()
        c_path = bn_ffi.new("char[]", path.encode("utf-8"))
        h = lib.nxuskit_bn_net_load_file(c_path)
        if h == bn_ffi.NULL:
            raise BnError(last_error() or f"failed to load: {path}")
        return cls(h)

    def close(self) -> None:
        """Destroy the underlying C handle."""
        if self._handle is not None and self._handle != bn_ffi.NULL:
            _get_lib().nxuskit_bn_net_destroy(self._handle)
            self._handle = None

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()

    def __del__(self):
        self.close()

    @property
    def num_variables(self) -> int:
        return int(_get_lib().nxuskit_bn_net_num_variables(self._handle))

    @property
    def variables(self) -> list[str]:
        ptr = _get_lib().nxuskit_bn_net_variables(self._handle)
        return json.loads(read_and_free_string(ptr))

    def variable_states(self, variable: str) -> list[str]:
        c_var = bn_ffi.new("char[]", variable.encode("utf-8"))
        ptr = _get_lib().nxuskit_bn_net_variable_states(self._handle, c_var)
        return json.loads(read_and_free_string(ptr))

    def save_file(self, path: str) -> None:
        c_path = bn_ffi.new("char[]", path.encode("utf-8"))
        _check_ok(_get_lib().nxuskit_bn_net_save_file(self._handle, c_path), "save failed")

    def add_gaussian_variable(self, name: str, mean_base: float, variance: float) -> None:
        c_name = bn_ffi.new("char[]", name.encode("utf-8"))
        _check_ok(
            _get_lib().nxuskit_bn_net_add_gaussian_variable(
                self._handle, c_name, mean_base, variance
            ),
            f"failed to add Gaussian variable {name}",
        )

    def set_gaussian_weight(self, variable: str, parent: str, weight: float) -> None:
        c_var = bn_ffi.new("char[]", variable.encode("utf-8"))
        c_parent = bn_ffi.new("char[]", parent.encode("utf-8"))
        _check_ok(
            _get_lib().nxuskit_bn_net_set_gaussian_weight(self._handle, c_var, c_parent, weight),
            f"failed to set weight {parent}->{variable}",
        )

    def infer(
        self,
        evidence: BnEvidence,
        algorithm: str,
        *,
        num_samples: int = 0,
        burn_in: int = 0,
        seed: int = 0,
    ) -> BnResult:
        """Run inference with the given algorithm."""
        c_algo = bn_ffi.new("char[]", algorithm.encode("utf-8"))
        ptr = _get_lib().nxuskit_bn_infer(
            self._handle, evidence._handle, c_algo, num_samples, burn_in, seed
        )
        if ptr == bn_ffi.NULL:
            raise BnError(last_error() or "inference failed")
        return BnResult(ptr)

    def infer_with_config(
        self,
        evidence: BnEvidence,
        algorithm: str,
        config: dict[str, Any] | str,
    ) -> BnResult:
        """Run inference with algorithm-specific JSON configuration."""
        c_algo = bn_ffi.new("char[]", algorithm.encode("utf-8"))
        config_str = json.dumps(config) if isinstance(config, dict) else config
        c_config = bn_ffi.new("char[]", config_str.encode("utf-8"))
        ptr = _get_lib().nxuskit_bn_infer_with_config(
            self._handle, evidence._handle, c_algo, c_config
        )
        if ptr == bn_ffi.NULL:
            raise BnError(last_error() or "inference with config failed")
        return BnResult(ptr)

    def learn_mle(self, csv_path: str, pseudocount: float = 1.0) -> None:
        """Learn CPT parameters from CSV data using Maximum Likelihood Estimation.

        Args:
            csv_path: Path to CSV file with column headers matching variable names.
            pseudocount: Laplace smoothing (0.0 = no smoothing, 1.0 = default).
        """
        c_path = bn_ffi.new("char[]", csv_path.encode("utf-8"))
        _check_ok(
            _get_lib().nxuskit_bn_learn_mle(self._handle, c_path, pseudocount),
            f"MLE learning failed for {csv_path}",
        )

    def log_likelihood(self, csv_path: str) -> float:
        """Compute log-likelihood of CSV data given the current network CPTs.

        Args:
            csv_path: Path to CSV file with column headers matching variable names.

        Returns:
            Log-likelihood value (more negative = worse fit).
        """
        c_path = bn_ffi.new("char[]", csv_path.encode("utf-8"))
        val = float(_get_lib().nxuskit_bn_log_likelihood(self._handle, c_path))
        if math.isinf(val) and val < 0:
            raise BnError(last_error() or f"log-likelihood failed for {csv_path}")
        return val

    def search_structure(
        self,
        csv_path: str,
        algorithm: str = "hill_climb",
        scoring: str = "bic",
        *,
        max_parents: int = 3,
        max_steps: int = 1000,
        ess: float = 1.0,
        ordering: list[str] | None = None,
    ) -> dict:
        """Run structure learning to discover network edges from data.

        Args:
            csv_path: Path to CSV file with observational data.
            algorithm: "hill_climb" or "k2".
            scoring: Scoring function — "bic", "aic", "bdeu", or "k2".
            max_parents: Maximum parents per node.
            max_steps: Maximum search iterations.
            ess: Equivalent sample size for BDeu scoring.
            ordering: Variable ordering for K2 algorithm (required for "k2").

        Returns:
            Dict with "edges" (list of [parent, child] pairs), "score",
            "iterations", and "algorithm".
        """
        c_path = bn_ffi.new("char[]", csv_path.encode("utf-8"))
        c_algo = bn_ffi.new("char[]", algorithm.encode("utf-8"))
        c_scoring = bn_ffi.new("char[]", scoring.encode("utf-8"))
        if ordering is not None:
            ordering_json = json.dumps(ordering)
            c_ordering = bn_ffi.new("char[]", ordering_json.encode("utf-8"))
        else:
            c_ordering = bn_ffi.NULL
        ptr = _get_lib().nxuskit_bn_search_structure(
            self._handle,
            c_path,
            c_algo,
            c_scoring,
            max_parents,
            max_steps,
            ess,
            c_ordering,
        )
        return json.loads(read_and_free_string(ptr))

    async def infer_async(
        self,
        evidence: BnEvidence,
        algorithm: str,
        *,
        num_samples: int = 0,
        burn_in: int = 0,
        seed: int = 0,
    ) -> BnResult:
        """Run inference on a thread pool to avoid blocking the event loop."""
        loop = asyncio.get_running_loop()

        def _run():
            return self.infer(
                evidence, algorithm, num_samples=num_samples, burn_in=burn_in, seed=seed
            )

        return await loop.run_in_executor(None, _run)

    def infer_stream(
        self,
        evidence: BnEvidence,
        num_samples: int = 10000,
        burn_in: int = 1000,
        seed: int = 0,
        chunk_size: int = 0,
    ) -> Iterator[BnStreamChunk]:
        """Stream Gibbs inference results as a generator."""
        import queue
        import threading

        q: queue.Queue[BnStreamChunk | None | Exception] = queue.Queue(maxsize=32)

        @bn_ffi.callback("_Bool(const char *, uint32_t, uint32_t, _Bool, void *)")
        def on_chunk(chunk_json, iteration, total, is_final, user_data):
            try:
                chunk = BnStreamChunk(
                    chunk_json=bn_ffi.string(chunk_json).decode("utf-8"),
                    iteration=int(iteration),
                    total=int(total),
                    is_final=bool(is_final),
                )
                q.put(chunk)
            except Exception as e:
                q.put(e)
            return True

        def run_stream():
            try:
                ok = _get_lib().nxuskit_bn_infer_stream(
                    self._handle,
                    evidence._handle,
                    num_samples,
                    burn_in,
                    seed,
                    chunk_size,
                    on_chunk,
                    bn_ffi.NULL,
                )
                if not ok:
                    q.put(BnError(last_error() or "streaming failed"))
            except Exception as e:
                q.put(e)
            finally:
                q.put(None)

        thread = threading.Thread(target=run_stream, daemon=True)
        thread.start()

        while True:
            item = q.get()
            if item is None:
                break
            if isinstance(item, Exception):
                raise item
            yield item


# ── BnEvidence ───────────────────────────────────────────────


class BnEvidence:
    """Evidence (observations) for Bayesian Network inference."""

    def __init__(self, handle=None):
        if handle is None:
            lib = _get_lib()
            handle = lib.nxuskit_bn_ev_create()
            if handle == bn_ffi.NULL:
                raise BnError(last_error() or "failed to create evidence")
        self._handle = handle

    def close(self) -> None:
        if self._handle is not None and self._handle != bn_ffi.NULL:
            _get_lib().nxuskit_bn_ev_destroy(self._handle)
            self._handle = None

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()

    def __del__(self):
        self.close()

    def set_discrete(self, network: BnNetwork, variable: str, state: str) -> None:
        c_var = bn_ffi.new("char[]", variable.encode("utf-8"))
        c_state = bn_ffi.new("char[]", state.encode("utf-8"))
        _check_ok(
            _get_lib().nxuskit_bn_ev_set_discrete(self._handle, network._handle, c_var, c_state),
            f"failed to set evidence {variable}={state}",
        )

    def set_continuous(self, network: BnNetwork, variable: str, value: float) -> None:
        c_var = bn_ffi.new("char[]", variable.encode("utf-8"))
        _check_ok(
            _get_lib().nxuskit_bn_ev_set_continuous(self._handle, network._handle, c_var, value),
            f"failed to set continuous evidence for {variable}",
        )

    def retract(self, variable: str) -> None:
        c_var = bn_ffi.new("char[]", variable.encode("utf-8"))
        _check_ok(
            _get_lib().nxuskit_bn_ev_retract(self._handle, c_var),
            f"failed to retract evidence for {variable}",
        )

    def clear(self) -> None:
        _check_ok(_get_lib().nxuskit_bn_ev_clear(self._handle), "failed to clear evidence")


# ── BnResult ─────────────────────────────────────────────────


class BnResult:
    """Inference result with posterior distributions."""

    def __init__(self, handle):
        self._handle = handle

    def close(self) -> None:
        if self._handle is not None and self._handle != bn_ffi.NULL:
            _get_lib().nxuskit_bn_result_destroy(self._handle)
            self._handle = None

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()

    def __del__(self):
        self.close()

    def to_json(self) -> str:
        """Get full result as JSON string."""
        ptr = _get_lib().nxuskit_bn_result_json(self._handle)
        return read_and_free_string(ptr)

    def marginal(self, variable: str) -> dict[str, float]:
        """Get posterior distribution for a discrete variable."""
        c_var = bn_ffi.new("char[]", variable.encode("utf-8"))
        ptr = _get_lib().nxuskit_bn_result_query(self._handle, c_var)
        return json.loads(read_and_free_string(ptr))

    @property
    def num_variables(self) -> int:
        return int(_get_lib().nxuskit_bn_result_num_variables(self._handle))

    def variable_names(self) -> list[str]:
        """Collect all variable names."""
        lib = _get_lib()
        lib.nxuskit_bn_result_reset(self._handle)
        names = []
        while True:
            ptr = lib.nxuskit_bn_result_next(self._handle)
            if ptr == bn_ffi.NULL:
                break
            name = bn_ffi.string(ptr).decode("utf-8")
            lib.nxuskit_free_string(ptr)
            names.append(name)
        return names

    def __iter__(self) -> Iterator[str]:
        """Iterate over variable names."""
        return iter(self.variable_names())

    def mean(self, variable: str) -> float:
        """Get posterior mean for a continuous variable."""
        c_var = bn_ffi.new("char[]", variable.encode("utf-8"))
        val = float(_get_lib().nxuskit_bn_result_mean(self._handle, c_var))
        if math.isnan(val):
            raise BnError(last_error() or f"mean not available for {variable}")
        return val

    def variance(self, variable: str) -> float:
        """Get posterior variance for a continuous variable."""
        c_var = bn_ffi.new("char[]", variable.encode("utf-8"))
        val = float(_get_lib().nxuskit_bn_result_variance(self._handle, c_var))
        if math.isnan(val):
            raise BnError(last_error() or f"variance not available for {variable}")
        return val

    def continuous_marginal(self, variable: str) -> ContinuousMarginal:
        """Get full continuous marginal (mean, variance, CI)."""
        c_var = bn_ffi.new("char[]", variable.encode("utf-8"))
        ptr = _get_lib().nxuskit_bn_result_continuous_marginal(self._handle, c_var)
        data = json.loads(read_and_free_string(ptr))
        return ContinuousMarginal(
            mean=data["mean"],
            variance=data["variance"],
            ci_lower=data["ci_lower"],
            ci_upper=data["ci_upper"],
        )

    @property
    def marginals_dict(self) -> dict[str, dict[str, float]]:
        """Get all discrete marginals as a nested dict."""
        data = json.loads(self.to_json())
        return data.get("marginals", {})
