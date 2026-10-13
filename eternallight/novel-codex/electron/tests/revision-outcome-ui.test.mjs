import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import { runInNewContext } from "node:vm";
import { test } from "node:test";
import { build } from "esbuild";
import electron from "electron";
import { registerRevisionOutcomeHandlers } from "../revision-outcome-handlers.mjs";

test("revision outcome read bridge validates identities through the Author transport", async () => {
  const handlers = new Map(), requests = [];
  registerRevisionOutcomeHandlers({ handle: (channel, handler) => handlers.set(channel, handler) }, async (...args) => {
    requests.push(args); return null;
  });
  assert.deepEqual([...handlers.keys()], ["runtime:get-revision-outcome"]);
  const read = handlers.get("runtime:get-revision-outcome");
  const writer = `task-${"a".repeat(32)}`;
  assert.equal(await read({}, "作品/a?b#c", writer), null);
  assert.deepEqual(requests, [[`/api/projects/${encodeURIComponent("作品/a?b#c")}/revision-outcomes/${writer}`]]);
  for (const project of [null, {}, "", " ", "x".repeat(161), "a\n", "a\r", "a\0", "a\t", "a\x7f", ".", ".."]) {
    assert.throws(() => read({}, project, writer), /标识无效/);
  }
  for (const task of [null, {}, "", "../task", writer + "?x=1", writer + "/", "task-" + "a".repeat(31), "action-" + "a".repeat(32)]) {
    assert.throws(() => read({}, "project", task), /标识无效/);
  }
  assert.equal(requests.length, 1);
  let api;
  const calls = [];
  runInNewContext(await readFile(new URL("../preload.cjs", import.meta.url), "utf8"), {
    process: { platform: "darwin" },
    require: () => ({ contextBridge: { exposeInMainWorld: (_name, value) => { api = value; } },
      ipcRenderer: { invoke: async (...args) => { calls.push(args); return null; } } }),
  });
  assert.equal(await api.runtime.getRevisionOutcome("project", writer), null);
  assert.deepEqual(calls, [["runtime:get-revision-outcome", "project", writer]]);
});

test("production revision UI shows criteria before approval and isolates lazy candidate judgments", async t => {
  const directory = await mkdtemp(join(tmpdir(), "novel-revision-outcome-ui-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL("./fixtures/revision-outcome-ui.tsx", import.meta.url))],
      outfile: join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic" });
    await writeFile(join(directory, "index.html"), '<html lang="zh-CN"><head><meta charset="utf-8"><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
    let output;
    try {
      output = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/creative-workspace-ui-main.cjs", import.meta.url)), join(directory, "index.html")],
        { timeout: 30000, env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } });
    } catch (error) {
      throw new Error(`Electron revision outcome UI failed:\n${error.stdout ?? ""}\n${error.stderr ?? ""}`, { cause: error });
    }
    const result = JSON.parse(output.stdout.trim());
    assert.equal(result.passed, true, result.error);
    assert.deepEqual(result.unexpectedCalls, []);
    assert.ok(result.reads > 0);
    t.assert.snapshot(result.snapshots);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
