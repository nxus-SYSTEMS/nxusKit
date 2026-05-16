"""License integration tests for nxusKit SDK (055-licensing-client-integration).

These tests require the native library (libnxuskit.dylib/.so) to be built
and accessible. They are skipped in CI unless the library is available.

To run locally:
    cargo build -p nxuskit-core --features full
    cp target/debug/libnxuskit_core.dylib target/debug/libnxuskit.dylib
    NXUSKIT_LIB_PATH=target/debug/libnxuskit.dylib pytest tests/integration/test_license.py -v
"""

from __future__ import annotations

import os
import sys

import pytest

# Skip all tests if native library is not available
_lib_available = False
try:
    sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "..", "src"))
    if os.environ.get("NXUSKIT_LIB_PATH"):
        from nxuskit._ffi import lib  # noqa: F401

        _lib_available = True
except Exception:
    pass

pytestmark = pytest.mark.skipif(
    not _lib_available,
    reason="requires libnxuskit native library (set NXUSKIT_LIB_PATH)",
)


# ── T048: Deployment token via env var (055-licensing-client-integration) ──


def test_deployment_token_via_env_var():
    """Test deployment token resolution via NXUSKIT_LICENSE_TOKEN env var."""
    pytest.skip("requires ES256-signed deployment token fixtures")
    # TODO: When ES256 test fixtures are available for Python:
    # 1. Set NXUSKIT_LICENSE_TOKEN env var with ES256 deployment token
    # 2. Call license_resolve()
    # 3. Verify result.valid == True
    # 4. Verify result.product_id == "nxuskit"


# ── T051: License resolve precedence (055-licensing-client-integration) ──


def test_license_resolve_precedence():
    """Test env var > file > API param resolution with ES256 tokens."""
    pytest.skip("requires ES256-signed token fixtures")
    # TODO: Test resolution chain with ES256 tokens
