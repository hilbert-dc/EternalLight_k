import assert from "node:assert/strict";
import test from "node:test";
import { registerWorkCatalog } from "../work-catalog.mjs";

test("catalog IPC preserves exact volume source and idempotency while rejecting malformed writes", async () => {
  const handlers = new Map();
  const requests = [];
  registerWorkCatalog({ handle: (name, handler) => handlers.set(name, handler) }, async (...args) => { requests.push(args); return { catalog: { revision: 1, volumes: [], placements: [] } }; });
  const input = { expectedRevision: 0, idempotencyKey: "create-volume-1", operation: { type: "createVolume", title: "第一卷", source: { outlineRevisionId: "outline-1", nodeId: "volume-client-key" } } };
  await handlers.get("runtime:update-work-catalog")(null, "project-1", input);
  assert.deepEqual(requests, [["/api/projects/project-1/catalog", { method: "POST", body: JSON.stringify(input) }]]);
  requests.length = 0;
  for (const operation of [ { type: "createVolume", title: "" }, { type: "createVolume", title: "卷", unexpected: "不可透传" }, { type: "placeChapter", chapterId: "chapter-1", volumeId: "" }, { type: "linkOutline", volumeId: "volume-1", source: { outlineRevisionId: "outline-1" } }, { type: "linkOutline", volumeId: "volume-1", source: { ...input.operation.source, extra: "不可透传" } }, { type: "accept" } ]) {
    assert.throws(() => handlers.get("runtime:update-work-catalog")(null, "project-1", { ...input, operation }));
  }
  assert.deepEqual(requests, []);
  const placement = { ...input, expectedRevision: 1, operation: { type: "placeChapter", chapterId: "chapter-1", volumeId: null } };
  await handlers.get("runtime:update-work-catalog")(null, "project-1", placement);
  assert.deepEqual(requests, [["/api/projects/project-1/catalog", { method: "POST", body: JSON.stringify(placement) }]]);
  requests.length = 0;
  const linking = { ...input, expectedRevision: 2, idempotencyKey: "link-once", operation: { type: "linkOutline", volumeId: "volume/1", source: { outlineRevisionId: "outline-new", nodeId: "exact-client-key" } } };
  await handlers.get("runtime:update-work-catalog")(null, "project/1", linking);
  assert.deepEqual(requests, [["/api/projects/project%2F1/catalog", { method: "POST", body: JSON.stringify(linking) }]]);
});
