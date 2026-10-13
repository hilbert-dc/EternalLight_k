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

test("App routes a generic conversation suggestion into one source-bound editor without auto-confirming or reopening", async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "novel-app-inline-commitment-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL("./fixtures/app-inline-commitment-ui.tsx", import.meta.url))], outfile: path.join(directory, "test.js"),
      bundle: true, platform: "browser", jsx: "automatic" });
    await writeFile(path.join(directory, "index.html"), '<html><head><meta charset="utf-8"><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
    let output;
    try { output = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/creative-workspace-ui-main.cjs", import.meta.url)), path.join(directory, "index.html")],
      { timeout: 35000, killSignal: "SIGKILL", env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } }); }
    catch (error) { throw Error(`${error.stdout ?? ""}\n${error.stderr ?? ""}`, { cause: error }); }
    const result = JSON.parse(output.stdout.trim());
    assert.equal(result.passed, true, `${result.error}\n${result.dom}\n${JSON.stringify(result.writes)}`);
    assert.deepEqual(result.writes, ["save"]);
    assert.deepEqual(result.visits, ["characters", "chapter-seven:editor", "characters", "chapter-one:no-reopen", "characters"]);
    assert.deepEqual(result.isolatedNavigations, ["switch", "create"]); assert.equal(result.createdThreads, 1);
    assert.equal(result.confirmationState, "candidate"); assert.equal(result.confirmationCommitId, null);
    assert.equal(result.modelCalls, 0); assert.equal(result.controlCalls, 0); assert.ok(result.reportReads > 0);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
