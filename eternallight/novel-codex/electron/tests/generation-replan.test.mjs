import assert from "node:assert/strict";
import test from "node:test";
import { registerGenerationReplanHandlers } from "../generation-replan-handlers.mjs";
import { mainHandlers, rendererEvent } from "./main-handler-harness.mjs";
import { fileURLToPath } from "node:url";

test("replanning submits the exact author preview without fetching newer authority", async () => {
  const handlers = new Map(); const calls = []; const reconciliations = [];
  registerGenerationReplanHandlers({ handle: (key, fn) => handlers.set(key, fn) }, async (...args) => { calls.push(args); return { status: "running" }; }, async (...args) => { reconciliations.push(args); });
  const id = `generation-${"a".repeat(32)}`;
  const path = `/api/projects/project/generation-runs/${id}/replan`;
  const request = { expectedVersion: 7, expectedProjectRevision: 2, expectedSourceDigest: "b".repeat(64), restartFrom: "outline", instruction: "调整藏信情节" };
  await handlers.get("runtime:preview-generation-replan")({}, "project", id);
  await handlers.get("runtime:replan-generation")({}, "project", id, request);
  assert.deepEqual(calls, [[path], [path, { method: "POST", body: JSON.stringify(request) }]]);
  assert.deepEqual(reconciliations, [["project", id], ["project", id]]);
  for (const invalid of [{ ...request, expectedVersion: -1 }, { ...request, restartFrom: "draft" }, { ...request, instruction: " " }, { ...request, extra: true }]) {
    await assert.rejects(handlers.get("runtime:replan-generation")({}, "project", id, invalid), /参数无效/);
  }
  assert.equal(calls.length, 2);
});

test("production main registers the author preview and replan endpoints", async () => {
  const calls = [];
  const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;
  const { handlers, adapter } = await mainHandlers(async (path, body, options) => {
    calls.push({ path, body, method: options?.method ?? "GET" }); return { status: "running" };
  }, command);
  const id = `generation-${"a".repeat(32)}`;
  const request = { expectedVersion: 7, expectedProjectRevision: 2, expectedSourceDigest: "b".repeat(64), restartFrom: "outline", instruction: "调整藏信情节" };
  try {
    await handlers.get("runtime:preview-generation-replan")(rendererEvent(), "project", id);
    await handlers.get("runtime:replan-generation")(rendererEvent(), "project", id, request);
    const path = `/api/projects/project/generation-runs/${id}/replan`;
    const runPath = `/api/projects/project/generation-runs/${id}`;
    assert.deepEqual(calls, [{ path: runPath, body: null, method: "GET" }, { path, body: null, method: "GET" }, { path: runPath, body: null, method: "GET" }, { path, body: request, method: "POST" }]);
  } finally { adapter.close(); }
});
