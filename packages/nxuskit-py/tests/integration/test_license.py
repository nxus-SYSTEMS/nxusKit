"""License integration tests for the nxusKit SDK.

These tests require the native library (libnxuskit.dylib/.so) to be built
and accessible. They are skipped in CI unless the library is available.

To run locally:
    cargo build -p nxuskit-core --features full
    cp target/debug/libnxuskit_core.dylib target/debug/libnxuskit.dylib
    NXUSKIT_LIB_PATH=target/debug/libnxuskit.dylib pytest tests/integration/test_license.py -v
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

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


# ── T048: Deployment token via environment ─────────────────────────────


def test_deployment_token_via_env_var():
    """Test deployment token resolution via NXUSKIT_LICENSE_TOKEN env var."""
    pytest.skip("requires ES256-signed deployment token fixtures")
    # TODO: When ES256 test fixtures are available for Python:
    # 1. Set NXUSKIT_LICENSE_TOKEN env var with ES256 deployment token
    # 2. Call license_resolve()
    # 3. Verify result.valid == True
    # 4. Verify result.product_id == "nxuskit"


# ── T051: License resolve precedence ───────────────────────────────────


def test_license_resolve_precedence():
    """Test env var > file > API param resolution with ES256 tokens."""
    pytest.skip("requires ES256-signed token fixtures")
    # TODO: Test resolution chain with ES256 tokens


def test_v2_deployment_token_info_exposes_only_safe_additive_metadata(tmp_path, monkeypatch):
    """The Python boundary retains safe v2 facts and never a compact token."""
    import jwt
    from cryptography.hazmat.primitives import serialization
    from cryptography.hazmat.primitives.asymmetric import ec

    from nxuskit.license import license_validate

    fixture_path = (
        Path(__file__).resolve().parents[4]
        / "tests"
        / "fixtures"
        / "licensing"
        / "deployment-claims-v2.json"
    )
    claims = json.loads(fixture_path.read_text())
    private_key = ec.derive_private_key(
        int.from_bytes(bytes(range(1, 33)), byteorder="big"), ec.SECP256R1()
    )
    private_pem = private_key.private_bytes(
        serialization.Encoding.PEM,
        serialization.PrivateFormat.PKCS8,
        serialization.NoEncryption(),
    )
    key_path = tmp_path / "deployment-v2-public.pem"
    key_path.write_bytes(
        private_key.public_key().public_bytes(
            serialization.Encoding.PEM,
            serialization.PublicFormat.SubjectPublicKeyInfo,
        )
    )
    token = jwt.encode(claims, private_pem, algorithm="ES256", headers={"kid": "es256-v1"})

    monkeypatch.setenv("NXUS_SIGNING_KEY_PATH", str(key_path))
    info = license_validate(token)

    assert info.sdk_release_line == 2
    assert info.grant_revision == 3
    assert info.authorization_scope == "client_entitlement"
    assert info.signing_key_id == "es256-v1"
    assert info.token_fingerprint and info.token_fingerprint.startswith("sha256:")
    assert token not in repr(info)


def test_v2_deployment_token_cross_line_rejection_is_stable_and_safe(tmp_path, monkeypatch):
    """A v3 token rejects through the Python boundary without echoing input."""
    import jwt
    from cryptography.hazmat.primitives import serialization
    from cryptography.hazmat.primitives.asymmetric import ec

    from nxuskit.license import license_validate

    fixture_path = (
        Path(__file__).resolve().parents[4]
        / "tests"
        / "fixtures"
        / "licensing"
        / "deployment-claims-v2.json"
    )
    claims = json.loads(fixture_path.read_text())
    claims["sdk_release_line"] = 3
    private_key = ec.derive_private_key(
        int.from_bytes(bytes(range(1, 33)), byteorder="big"), ec.SECP256R1()
    )
    private_pem = private_key.private_bytes(
        serialization.Encoding.PEM,
        serialization.PrivateFormat.PKCS8,
        serialization.NoEncryption(),
    )
    key_path = tmp_path / "deployment-v3-public.pem"
    key_path.write_bytes(
        private_key.public_key().public_bytes(
            serialization.Encoding.PEM,
            serialization.PublicFormat.SubjectPublicKeyInfo,
        )
    )
    token = jwt.encode(claims, private_pem, algorithm="ES256", headers={"kid": "es256-v1"})

    monkeypatch.setenv("NXUS_SIGNING_KEY_PATH", str(key_path))
    info = license_validate(token)

    assert not info.valid
    assert info.result == "incompatible_release_line"
    assert token not in (info.error or "")
