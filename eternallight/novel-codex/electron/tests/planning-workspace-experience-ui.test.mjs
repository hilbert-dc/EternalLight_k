import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { build } from "esbuild";
import electron from "electron";

test("planning pages isolate node editing and preserve saved-source decisions across themes", async t => {
  const directory = await mkdtemp(join(tmpdir(), "novel-planning-experience-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL("./fixtures/planning-workspace-experience-ui.tsx", import.meta.url))], outfile: join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic" });
    await writeFile(join(directory, "index.html"), '<html lang="zh-CN"><head><meta charset="utf-8"><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
    let output;
    try { output = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/creative-workspace-ui-main.cjs", import.meta.url)), join(directory, "index.html")], { timeout: 30000, env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } }); }
    catch (error) { throw new Error(`${error.stdout ?? ""}\n${error.stderr ?? ""}`, { cause: error }); }
    const result = JSON.parse(output.stdout.trim());
    assert.equal(result.passed, true, result.error);
    assert.deepEqual({ writes: result.writes, reviews: result.reviewCalls, accepts: result.acceptCalls, modelCalls: result.modelCalls }, { writes: 4, reviews: 2, accepts: 1, modelCalls: 0 });
    assert.deepEqual(result.versionRequests, ["brief", "outline", "plans", "plans"]);
    t.assert.snapshot(result.snapshots);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
