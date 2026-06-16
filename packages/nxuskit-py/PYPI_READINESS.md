# nxuskit-py Python PyPI Readiness

**Status:** v1.0.4 release-candidate readiness note. This file documents the
package shape, local checks, and operator setup needed for package-index
publication. It does not itself approve TestPyPI/PyPI upload, SDK release tags,
or GitHub release assets.

## Package Shape

- Public distribution name: `nxuskit-py`
- Python import name: `nxuskit`
- Version: `1.0.4`
- Wheel target: pure-Python `py3-none-any`
- Native libraries: not included in PyPI wheels or sdists
- Trove classifier: `Development Status :: 4 - Beta`, reflecting first-time
  PyPI publication readiness rather than a change to the v1.0.x SDK API status

The Python package supports pure-Python provider APIs directly. Native/FFI
features require an installed nxusKit SDK bundle discovered through
`NXUSKIT_SDK_DIR`, `NXUSKIT_LIB_DIR`, legacy `NXUSKIT_LIB_PATH`, or the standard
`~/.nxuskit/sdk/current` install.

CLIPS and Bayesian inference are Community-capable where supported by the
installed SDK. Solver and ZEN are Pro-only and require a Pro SDK build plus Pro
entitlement.

## Local Readiness Commands

Run from `packages/nxuskit-py` with Python 3.11+:

```bash
python -m pip install --upgrade build twine
python -m build
python -m twine check dist/*
python -m venv .venv-pypi-check
. .venv-pypi-check/bin/activate
python -m pip install dist/nxuskit_py-1.0.4-py3-none-any.whl
python -c "import nxuskit; print(nxuskit.__version__)"
python -c "from nxuskit.mock import MockProvider; print(''.join(c.delta for c in MockProvider().chat_stream([])))"
```

No-SDK FFI error smoke:

```bash
NXUSKIT_LIB_DIR=/nonexistent python -c "import nxuskit._ffi"
```

Expected result: a helpful `ConfigError` mentioning `NXUSKIT_LIB_DIR`.

Native/FFI success smoke, when a local SDK v1.0.4 bundle is available:

```bash
NXUSKIT_SDK_DIR="$HOME/.nxuskit/sdk/current" python -c \
  "from nxuskit._ffi import lib, ffi; print(ffi.string(lib.nxuskit_version()).decode())"
```

Expected result: `1.0.4`.

Archive inspection:

```bash
python - <<'PY'
from pathlib import Path
import tarfile
import zipfile

dist = Path("dist")
for wheel in dist.glob("*.whl"):
    with zipfile.ZipFile(wheel) as zf:
        names = zf.namelist()
        assert not any(name.endswith((".so", ".dylib", ".dll")) for name in names)
for sdist in dist.glob("*.tar.gz"):
    with tarfile.open(sdist) as tf:
        names = tf.getnames()
        assert not any(name.endswith((".so", ".dylib", ".dll")) for name in names)
print("no native libraries in Python distribution artifacts")
PY
```

## Publication Gate

Before upload, an operator must configure package ownership and the selected
Trusted Publisher binding. Do not work around missing setup with ad hoc tokens
or secrets in chat; stop and request the exact operator action.

## Recommended TestPyPI / PyPI Setup

The controlled sequence should be:

1. Source branch is green and carries `nxuskit-py==1.0.4` metadata.
2. TestPyPI upload/install/smoke runs from the exact approved release branch
   commit.
3. Production PyPI upload/install/smoke runs only after a manager/operator
   go-gate.
4. Move/push `sdk-v1.0.4` and trigger aggregate SDK release only after
   production PyPI install/smoke verification is green, so SDK release text does
   not advertise an unavailable package-index install path.

Preferred production publication path for v1.0.4 is the public
`nxus-SYSTEMS/nxusKit` repository, not the internal repository. The public
repository should contain the exact public-safe `packages/nxuskit-py` source,
the manual PyPI workflow, and the release branch/tag that PyPI Trusted Publisher
is configured to trust.

Required PyPI Trusted Publisher fields for the preferred public path:

- Project: `nxuskit-py`
- Publisher: GitHub
- Owner: `nxus-SYSTEMS`
- Repository: `nxusKit`
- Workflow: `publish-nxuskit-py-pypi.yml`
- Environment: `pypi`
- Source branch for this cycle: `release/sdk-v1.0.4`

If an internal-repo fallback is approved, use the internal manual workflows only
with their fail-closed branch/SHA/version guards. Internal-repo workflows keep
attestations disabled so package artifacts are not publicly bound to private
source provenance. Public-repo publication may enable attestations after the
operator confirms that public provenance is desired for the cycle.
