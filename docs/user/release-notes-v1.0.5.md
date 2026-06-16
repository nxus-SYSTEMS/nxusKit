# nxusKit SDK v1.0.5 Release Notes Candidate

Status: release-candidate source text only. Do not publish, tag, upload, or
attach assets from this file without a manager/operator release go-gate.

## Summary

nxusKit SDK v1.0.5 is a public-trust and release-process hardening patch for
the stabilized v1.0 SDK line. It preserves the v1.0.0 API and C ABI contracts,
keeps the v1.0.1 Pro CLI packaging fix, and carries forward the v1.0.4 Windows
release-build remediation.

## What Changed

- Aligns SDK lockstep version metadata across Rust, Go, Python, release
  verification guards, and shipped bundle documentation to `1.0.5`.
- Keeps `nxuskit-py==1.0.5` as the public package-index distribution while
  preserving `import nxuskit`.
- Promotes the Python package metadata to `Development Status :: 5 -
  Production/Stable` for the stabilized v1 SDK posture.
- Points PyPI project metadata to nxusKit documentation, SDK downloads,
  examples, changelog, and issue tracker instead of only the repository root.
- Clarifies that Python package-index wheels are pure-Python `py3-none-any`
  artifacts and do not include native `libnxuskit` binaries, SDK bundles, or
  Pro command modules.
- Keeps native/FFI Python features tied to an installed matching SDK bundle via
  `NXUSKIT_SDK_DIR`, `NXUSKIT_LIB_DIR`, legacy `NXUSKIT_LIB_PATH`, or the
  standard SDK install location.
- Documents that public SDK release pages include both Community/OSS and Pro
  binary packages, while public source/tag archives remain CE-safe and Pro
  capabilities remain runtime entitlement-gated.
- Adds release preflight checks before expensive platform builds so package
  metadata, public posture, and release workflow assumptions fail early.
- Preserves the Windows release-build fix for robust Visual Studio discovery
  through `vswhere`, with explicit `cl` and `dumpbin` diagnostics.

## Compatibility

- No API or C ABI signature changes from v1.0.0 are introduced.
- CLIPS and Bayesian inference remain Community-capable where supported by the
  installed SDK bundle.
- Solver and ZEN remain Pro-only and require a Pro SDK build plus Pro
  entitlement.

## Publication Gates

- TestPyPI/PyPI upload, SDK release tags, GitHub release assets, public mirrors,
  docs deploys, and website deploys remain manager/operator-gated.
- The preferred production PyPI publication source remains the public
  `nxus-SYSTEMS/nxusKit` repository/mirror.
- Publish package-index artifacts and verify clean install/smoke before moving
  the `sdk-v1.0.5` tag and publishing SDK release assets.
