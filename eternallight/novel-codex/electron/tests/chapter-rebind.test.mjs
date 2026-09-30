import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { readFile } from "node:fs/promises";
import { runInNewContext } from "node:vm";
import { registerChapterRebindHandlers } from "../chapter-rebind-handlers.mjs";
import { mainHandlers, rendererEvent } from "./main-handler-harness.mjs";

const request = { chapterId: "chapter-1", previousPlanRevisionId: "old-plan", previousChapterPlanId: "plan-1", sourceRevisionId: "draft-1", planRevisionId: "new-plan", chapterPlanId: "plan-1", expectedRevision: 2, idempotencyKey: "confirmation-1" };
test("production preload forwards the exact rebind request over the typed IPC channel", async () => {
  const calls = []; let api;
  const electron = { contextBridge: { exposeInMainWorld: (_name, value) => { api = value; } }, ipcRenderer: { invoke: async (...args) => { calls.push(args); return { binding: { resultRevision: 3 } }; } } };
  runInNewContext(await readFile(new URL("../preload.cjs", import.meta.url), "utf8"), { process: { platform: "test" }, require: name => { assert.equal(name, "electron"); return electron; } });
  assert.deepEqual(await api.runtime.rebindChapterPlan("project", request), { binding: { resultRevision: 3 } });
  assert.deepEqual(calls, [["runtime:rebind-chapter-plan", "project", request]]);
});
test("chapter rebind IPC freezes the full author confirmation and rejects invalid input", async () => {
  const handlers = new Map(); const calls = [];
  registerChapterRebindHandlers({ handle: (key, fn) => handlers.set(key, fn) }, async (...args) => { calls.push(args); return { success: true }; });
  const submit = handlers.get("runtime:rebind-chapter-plan");
  await submit({}, "作品/港城", request);
  assert.deepEqual(calls, [[`/api/projects/${encodeURIComponent("作品/港城")}/chapters/rebind-plan`, { method: "POST", body: JSON.stringify(request) }]]);
  for (const invalid of [{ ...request, extra: true }, { ...request, sourceRevisionId: "" }, { ...request, previousPlanRevisionId: "a".repeat(161) }, { ...request, chapterPlanId: "bad\nentry" }, { ...request, expectedRevision: -1 }, { ...request, expectedRevision: Number.MAX_SAFE_INTEGER + 1 }, { ...request, idempotencyKey: null }]) assert.throws(() => submit({}, "project", invalid), /参数无效/);
  assert.throws(() => submit({}, "", request), /参数无效/);
  assert.equal(calls.length, 1);
});

test("production main routes rebind only through the existing author transport", async () => {
  const calls = [];
  const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;
  const { handlers, adapter } = await mainHandlers(async (path, body, options) => { calls.push({ path, body, method: options?.method }); return { binding: { resultRevision: 3 } }; }, command);
  try {
    await handlers.get("runtime:rebind-chapter-plan")(rendererEvent(), "project", request);
    assert.deepEqual(calls, [{ path: "/api/projects/project/chapters/rebind-plan", body: request, method: "POST" }]);
  } finally { adapter.close(); }
});
