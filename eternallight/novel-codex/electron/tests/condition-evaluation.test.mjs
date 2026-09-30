import assert from "node:assert/strict";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { readFile } from "node:fs/promises";
import { runInNewContext } from "node:vm";
import { mainHandlers, rendererEvent } from "./main-handler-harness.mjs";

test("condition evaluation bridge routes only typed author operations", async () => {
  const requests = [];
  const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;
  const { handlers, adapter } = await mainHandlers(async (path, body, options, url) => {
    requests.push({ path, body, method: options?.method ?? "GET", query: Object.fromEntries(url.searchParams) });
    return { ok: true };
  }, command);
  const event = rendererEvent();
  const job = `condition-${"a".repeat(32)}`;
  const project = "作品/a?b";
  const target = { snapshotId: null, sourceBatchId: "modeling-event", assertionIndex: 2 };
  try {
    await handlers.get("runtime:start-condition-evaluation")(event, project, target);
    await handlers.get("runtime:get-condition-evaluation")(event, project, job);
    await handlers.get("runtime:control-condition-evaluation")(event, project, job, "pause", 7);
    const cursor = '{"position":"a&b?c=1"}';
    await handlers.get("runtime:get-condition-explanations")(event, project, job, cursor);
    const base = `/api/projects/${encodeURIComponent(project)}/condition-evaluations`;
    assert.deepEqual(requests, [
      { path: base, body: target, method: "POST", query: {} },
      { path: `${base}/${job}`, body: null, method: "GET", query: {} },
      { path: `${base}/${job}/control`, body: { command: "pause", expectedVersion: 7 }, method: "POST", query: {} },
      { path: `${base}/${job}/explanations`, body: null, method: "GET", query: { limit: "16", cursor } },
    ]);
    for (const invalid of [null, {}, { ...target, bindingDigest: "forged" }, { ...target, assertionIndex: 100 }, { ...target, assertionIndex: 1.5 }, { ...target, snapshotId: undefined }]) {
      assert.throws(() => handlers.get("runtime:start-condition-evaluation")(event, project, invalid), /目标无效/);
    }
    for (const id of [undefined, "../other", job + "?"]) {
      assert.throws(() => handlers.get("runtime:get-condition-evaluation")(event, project, id), /编号无效/);
    }
    for (const [control, version] of [["complete", 1], ["pause", -1], ["resume", Number.MAX_SAFE_INTEGER + 1]]) {
      assert.throws(() => handlers.get("runtime:control-condition-evaluation")(event, project, job, control, version), /参数无效/);
    }
    assert.throws(() => handlers.get("runtime:get-condition-explanations")(event, project, job, "x".repeat(2049)), /游标无效/);
    assert.equal(requests.length, 4);
  } finally { adapter.close(); }
});

test("preload exposes explicit condition operations without generic transport", async () => {
  let api;
  const calls = [];
  runInNewContext(await readFile(new URL("../preload.cjs", import.meta.url), "utf8"), {
    process: { platform: "darwin" },
    require: () => ({ contextBridge: { exposeInMainWorld: (_name, value) => { api = value; } }, ipcRenderer: { invoke: async (...args) => { calls.push(args); return "reply"; } } }),
  });
  const target = { snapshotId: null, sourceBatchId: "batch", assertionIndex: 0 };
  assert.equal(await api.runtime.startConditionEvaluation("project", target), "reply");
  await api.runtime.getConditionEvaluation("project", "job");
  await api.runtime.controlConditionEvaluation("project", "job", "pause", 2);
  await api.runtime.getConditionExplanations("project", "job", "cursor");
  assert.deepEqual(calls, [
    ["runtime:start-condition-evaluation", "project", target],
    ["runtime:get-condition-evaluation", "project", "job"],
    ["runtime:control-condition-evaluation", "project", "job", "pause", 2],
    ["runtime:get-condition-explanations", "project", "job", "cursor"],
  ]);
});
