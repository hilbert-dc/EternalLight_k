import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { build } from "esbuild";
import electron from "electron";

test("actual App pauses explicit persisted Work offline and isolates stale or foreign control requests", { timeout: 40000 }, async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "novel-autonomous-controls-ui-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL("./fixtures/autonomous-controls-ui.tsx", import.meta.url))], outfile: path.join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic" });
    await writeFile(path.join(directory, "index.html"), '<html><head><meta charset="utf-8"><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
    let output;
    try { output = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/creative-workspace-ui-main.cjs", import.meta.url)), path.join(directory, "index.html")],
      { timeout: 35000, env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } }); }
    catch (error) { throw new Error(`${error.stdout ?? ""}\n${error.stderr ?? ""}`, { cause: error }); }
    const result = JSON.parse(output.stdout.trim());
    assert.equal(result.passed, true, `${result.error}\n${JSON.stringify(result.calls)}\n${result.dom ?? ""}`);
    assert.equal(result.calls.newRoot, 0);
    assert.equal(result.calls.models, 0);
    assert.deepEqual(result.cases, ["returned reading scope", "model pause receipt", "independent acceptance states", "restart offline choices", "explicit original Work", "persisted paused receipt", "progress does not invalidate pause", "open menu survives progress", "open menu invalidates changed control", "scheduled pause and immediate override", "control revision changed", "status changed", "foreign receipt", "Native failure refresh", "invalid pause receipt", "successful pause refresh failure", "live thread race", "late project read", "late pause receipt"]);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
