import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import { runInNewContext } from "node:vm";
import test from "node:test";
import { build } from "esbuild";
import electron from "electron";

test("startup gate waits for online, retries once, and preserves a mounted unsaved editor", async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "novel-startup-gate-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL("./fixtures/startup-gate-ui.tsx", import.meta.url))], outfile: path.join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic" });
    await writeFile(path.join(directory, "index.html"), '<html><head><meta charset="utf-8"><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
    let output;
    try { output = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/creative-workspace-ui-main.cjs", import.meta.url)), path.join(directory, "index.html")], { timeout: 35000, env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } }); }
    catch (error) { throw new Error(`${error.stdout ?? ""}\n${error.stderr ?? ""}`, { cause: error }); }
    const result = JSON.parse(output.stdout.trim());
    assert.equal(result.passed, true, `${result.error ?? ""}\n${result.dom ?? ""}`);
    assert.deepEqual(result, { passed: true, retries: 1, closes: 1, editorCloses: 1, earlyReads: 0, preservedEditor: true });
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test("production preload exposes a payload-free startup retry on its dedicated IPC channel", async () => {
  const exposed = {}, calls = [];
  const status = { state: "starting", mode: "local", error: null };
  runInNewContext(await readFile(new URL("../preload.cjs", import.meta.url), "utf8"), {
    require: name => {
      assert.equal(name, "electron");
      return { contextBridge: { exposeInMainWorld: (name, api) => { exposed[name] = api; } }, ipcRenderer: { invoke: async (...args) => { calls.push(args); return status; } } };
    },
    process: { platform: "test" },
  });
  assert.equal(await exposed.novelCodex.runtime.retryStartup("ignored-untrusted-payload"), status);
  assert.deepEqual(calls, [["runtime:retry-startup"]]);
});
