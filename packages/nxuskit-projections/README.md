# nxusKit v2 Projection Conformance Alpha

This directory contains Gate H alpha projection artifacts for the v2 operation,
reference, and Capability Catalog contract family.

The generated Python and TypeScript validators are preview conformance artifacts.
They are derived from the Rust DTO fixture corpus in
`packages/nxuskit/tests/fixtures/contract_v2` and from the S03 CLI schema/lint
surface. They are not public SDK finalization, protected execution proof, or
runtime entitlement authority.

## Artifacts

- `python/nxuskit_projection_alpha/v2_contracts.py` contains Pydantic v2 models
  and reference-only boundary checks for the selected Gate H alpha contract set.
- `typescript/src/v2_contracts.zod.ts` contains TypeScript/Zod schemas and the
  same reference-only boundary checks.
- `provenance/v2_projection_alpha.json` records fixture hashes, source inputs,
  generated paths, and gate locks for deterministic regeneration.

## Regeneration

Run the generator from the repository root:

```bash
python3 tools/projection/generate_v2_projection_alpha.py
```

Then run the Python and TypeScript conformance tests in the package-specific
directories before consuming the artifacts downstream.
