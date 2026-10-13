import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { runInNewContext } from "node:vm";
import { registerCommitmentHandlers } from "../commitment-handlers.mjs";
import { mainHandlers, rendererEvent } from "./main-handler-harness.mjs";

test("commitment report bridge preserves exact revision selection and rejects invalid identities", async () => {
  const handlers = new Map();
  const requests = [];
  registerCommitmentHandlers({ handle: (channel, handler) => handlers.set(channel, handler) }, async path => {
    requests.push(path);
    return null;
  });
  const read = handlers.get("runtime:get-commitment-extraction");
  const base = "/api/projects/project/chapters/chapter-one/commitment-extractions";
  assert.equal(await read({}, "project", "chapter-one"), null);
  assert.equal(await read({}, "project", "chapter-one", null), null);
  assert.equal(await read({}, "project", "chapter-one", "报告&原文=一"), null);
  assert.deepEqual(requests, [base, base, `${base}?reportRevisionId=${encodeURIComponent("报告&原文=一")}`]);
  // A missing exact result must remain missing; no subsequent current read is made.
  assert.equal(await read({}, "project", "chapter-one", "missing-report"), null);
  assert.equal(requests.at(-1), `${base}?reportRevisionId=missing-report`);
  for (const revision of ["", " ", "../other", "report?other", "report#other", "report\nother", "x".repeat(161), 1, {}]) {
    assert.throws(() => read({}, "project", "chapter-one", revision), /标识无效/);
  }
  assert.throws(() => read({}, "../project", "chapter-one", "report-one"), /标识无效/);
  assert.throws(() => read({}, "project", "../chapter", "report-one"), /标识无效/);
  assert.equal(requests.length, 4);
});

test("real preload forwards the fixed report revision and explicit current selection", async () => {
  let api;
  const calls = [];
  runInNewContext(await readFile(new URL("../preload.cjs", import.meta.url), "utf8"), {
    process: { platform: "test" },
    require: name => {
      assert.equal(name, "electron");
      return {
        contextBridge: { exposeInMainWorld: (key, value) => { if (key === "novelCodex") api = value; } },
        ipcRenderer: { invoke: async (...args) => { calls.push(args); return null; } },
      };
    },
  });
  await api.runtime.getCommitmentExtraction("project", "chapter-one", "report-one");
  await api.runtime.getCommitmentExtraction("project", "chapter-one");
  await api.runtime.getCommitmentExtraction("project", "chapter-one", null);
  assert.deepEqual(calls, [
    ["runtime:get-commitment-extraction", "project", "chapter-one", "report-one"],
    ["runtime:get-commitment-extraction", "project", "chapter-one", null],
    ["runtime:get-commitment-extraction", "project", "chapter-one", null],
  ]);
});

test("production author bridge forwards the exact report query and preserves its current flags", async () => {
  const requests = [];
  const report = { revisionId: "report-one", reportCurrent: false, sourceCurrent: true, payload: { candidates: [{ summary: "原报告内容" }] } };
  const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;
  const { handlers, adapter } = await mainHandlers(async (path, body, options, url) => {
    requests.push({ path, query: Object.fromEntries(url.searchParams), body, method: options?.method ?? "GET" });
    return report;
  }, command);
  try {
    const result = await handlers.get("runtime:get-commitment-extraction")(rendererEvent(), "project", "chapter-one", "report-one");
    assert.deepEqual(result, report);
    assert.deepEqual(requests, [{ path: "/api/projects/project/chapters/chapter-one/commitment-extractions",
      query: { reportRevisionId: "report-one" }, body: null, method: "GET" }]);
  } finally { adapter.close(); }
});
