# nxusKit SDK v1.0.4 Release Notes Candidate

Status: release-candidate source text only. Do not publish, tag, upload, or
attach assets from this file without a manager/operator release go-gate.

## Summary

nxusKit SDK v1.0.4 is a public-trust and Python packaging alignment patch for
the stabilized v1.0 SDK line. It preserves the v1.0.0 API and C ABI contracts
and keeps the v1.0.1 Pro CLI packaging fix. It supersedes the abandoned
v1.0.3 SDK release attempt; public latest remains v1.0.2 until v1.0.4 release
assets are published and verified.

## What Changed

- Aligns SDK lockstep version metadata across Rust, Go, Python, release
  verification guards, and shipped bundle documentation to `1.0.4`.
- Prepares the Python distribution source for TestPyPI/PyPI validation as
  `nxuskit-py==1.0.4` while preserving `import nxuskit`.
- Clarifies that Python package-index wheels are pure-Python `py3-none-any`
  artifacts and do not include native `libnxuskit` binaries.
- Keeps package-local Python distribution `LICENSE` and `NOTICE` metadata in
  the wheel/sdist and updates release-target license links for v1.0.4.
- Keeps native/FFI Python features tied to an installed matching SDK bundle via
  `NXUSKIT_SDK_DIR`, `NXUSKIT_LIB_DIR`, legacy `NXUSKIT_LIB_PATH`, or the
  standard SDK install location.
- Carries forward the Windows release-build fix for robust Visual Studio
  discovery through `vswhere`, with explicit `cl` and `dumpbin` diagnostics.

## Compatibility

- No API or C ABI signature changes from v1.0.0 are introduced.
- CLIPS and Bayesian inference remain Community-capable where supported by the
  installed SDK bundle.
- Solver and ZEN remain Pro-only and require a Pro SDK build plus Pro
  entitlement.

## Publication Gates

- TestPyPI and PyPI uploads are not approved by this source file.
- Production PyPI upload and install/smoke verification must complete before
  moving the `sdk-v1.0.4` release tag and publishing SDK release assets.
- SDK release tags, GitHub release assets, public mirrors, docs deploys, and
  website deploys remain manager/operator-gated.
