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

test("author replan UI preserves preview authority and survives rejected submissions", async t => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "novel-replan-ui-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL("./fixtures/generation-replan-ui.tsx", import.meta.url))], outfile: path.join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic" });
    await writeFile(path.join(directory, "index.html"), '<html><head><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
    const { stdout } = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/creative-workspace-ui-main.cjs", import.meta.url)), path.join(directory, "index.html")], { timeout: 25000, env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } });
    const result = JSON.parse(stdout.trim()); assert.equal(result.passed, true, result.error);
    t.assert.snapshot(result.snapshot);
    t.assert.snapshot(result.chapterSnapshot);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
