# v2 Contract Fixture Set

This fixture set is the Gate D/E alpha conformance corpus for the v2
operation/capability/ref spine. Positive fixtures deserialize into
`nxuskit::contract::v2` DTOs and pass the reference-only boundary validator.
Negative fixtures are intentionally invalid public control-plane JSON and must
produce class-identifying boundary violations.

## Positive Fixtures

- `positive/operation-envelope.json`
- `positive/operation-accepted.json`
- `positive/operation-rejected-before-dispatch.json`
- `positive/operation-result.json`
- `positive/operation-event.json`
- `positive/capability-catalog-entry.json`

## Negative Fixtures

- `negative/bulk-rows.json`
- `negative/record-batch.json`
- `negative/inline-sql.json`
- `negative/file-body.json`
- `negative/blob-payload.json`
- `negative/persona-content.json`
- `negative/workflow-pack-content.json`
