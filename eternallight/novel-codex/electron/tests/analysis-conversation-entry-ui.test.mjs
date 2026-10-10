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

test("planning discussion preserves the author draft and explicit multi-role review", async t => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "novel-analysis-entry-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL("./fixtures/analysis-conversation-entry-ui.tsx", import.meta.url))], outfile: path.join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic" });
    await writeFile(path.join(directory, "index.html"), '<html><head><meta charset="utf-8"><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
    const { stdout } = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/creative-workspace-ui-main.cjs", import.meta.url)), path.join(directory, "index.html")], { timeout: 35000, killSignal: "SIGKILL", env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } });
    const result = JSON.parse(stdout.trim()); assert.equal(result.passed, true, `${result.error}\n${result.dom ?? ""}`);
    assert.equal(result.starts, 1); assert.equal(result.reads, 0); assert.equal(result.prompts, 3); t.assert.snapshot(result.snapshots);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
