# CE Boundary Seeded-Leak Fixtures

Tier: CE boundary test fixtures.

This directory is for deliberately seeded leak fixtures used by the v1.1.x
A-Team CE/Pro hardening tests. Fixtures here are test inputs only. They are
not public docs and must not be shipped as CE runtime artifacts.

## Leak Classes

Boundary tests must cover these seeded classes:

- `internal-symbol`: internal SDK symbols such as ProviderChannel,
  ToolCatalog, RequestEnvelope, ResultEnvelope, CapabilityTruthSnapshot, or
  internal-channel posture exposed through CE files
- `pro-path`: private Pro/internal source paths or module names included in CE
  exports or bundle staging
- `pro-doc`: Pro behavioral details included in CE public docs
- `pro-dep`: Pro-only dependency entries included in CE manifests
- `non-oss-edition-accepted`: CE build or metadata accepting non-OSS edition
  values such as Pro or internal editions

## Expected Test Behavior

Each seeded leak should fail with a class-identifying message. The same helper
must pass on a compliant CE source export and CE bundle staging directory.

## Guardrails

Tests must not depend on live credentials, release publication, or artifact
upload. They should operate on local temporary staging directories produced by
test helpers.
