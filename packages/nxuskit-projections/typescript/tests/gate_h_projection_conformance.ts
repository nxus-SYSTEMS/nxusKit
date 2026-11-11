import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { readdir } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  classifyContractV2,
  collectCoreRefKinds,
  findBoundaryViolation,
  parseContractV2,
} from "../src/v2_contracts.zod";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../../../..");
const fixtureRoot = join(root, "packages/nxuskit/tests/fixtures/contract_v2");

function loadJson(path: string): unknown {
  return JSON.parse(readFileSync(path, "utf8"));
}

const positiveDir = join(fixtureRoot, "positive");
const negativeDir = join(fixtureRoot, "negative");

const positiveFiles = (await readdir(positiveDir)).filter((name) => name.endsWith(".json")).sort();
const seen = new Set<string>();
for (const name of positiveFiles) {
  const value = loadJson(join(positiveDir, name));
  const parsed = parseContractV2(value) as Record<string, unknown>;
  seen.add(classifyContractV2(value));
  assert.equal(parsed.contract_version, "2.0-alpha", name);
}
assert.deepEqual(
  [...seen].sort(),
  [
    "CapabilityCatalogEntry",
    "OperationAccepted",
    "OperationEnvelope",
    "OperationEvent",
    "OperationResult",
  ],
);

const operationRefs = collectCoreRefKinds(loadJson(join(positiveDir, "operation-envelope.json")));
const capabilityRefs = collectCoreRefKinds(
  loadJson(join(positiveDir, "capability-catalog-entry.json")),
);
assert.deepEqual(
  [...new Set([...operationRefs, ...capabilityRefs])].sort(),
  [
    "ArtifactRef",
    "BatchRef",
    "CapabilityRef",
    "FactSetRef",
    "PacketRef",
    "PlanRef",
    "RouteRef",
    "SchemaRef",
  ],
);

const expectedKeys: Record<string, string> = {
  "blob-payload.json": "blob_payload",
  "bulk-rows.json": "rows",
  "file-body.json": "file_body",
  "inline-sql.json": "inline_sql",
  "persona-content.json": "private_persona_ref",
  "record-batch.json": "record_batch",
  "workflow-pack-content.json": "workflow_pack_content_ref",
};

const negativeFiles = (await readdir(negativeDir)).filter((name) => name.endsWith(".json")).sort();
for (const name of negativeFiles) {
  const violation = findBoundaryViolation(loadJson(join(negativeDir, name)));
  assert.ok(violation, name);
  assert.equal(violation.key, expectedKeys[name], name);
}

console.log(
  JSON.stringify({
    status: "pass",
    positive: positiveFiles.length,
    negative: negativeFiles.length,
    models: [...seen].sort(),
  }),
);
