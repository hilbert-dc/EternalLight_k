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

test("actual App pages remain reachable across themes and viewport sizes without model or write calls", async t => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "novel-viewport-ui-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL("./fixtures/workspace-viewport-ui.tsx", import.meta.url))], outfile: path.join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic" });
    await writeFile(path.join(directory, "index.html"), '<!doctype html><html><head><meta charset="UTF-8"><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
    let output;
    try {
      output = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/workspace-viewport-ui-main.cjs", import.meta.url)), path.join(directory, "index.html")], { timeout: 45000, env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } });
    } catch (error) { throw new Error(`${error.stdout ?? ""}\n${error.stderr ?? ""}`, { cause: error }); }
    const result = JSON.parse(output.stdout.trim());
    assert.equal(result.passed, true, result.error);
    assert.equal(result.checks.length, 6);
    assert.ok(result.checks.every(check => check.pages.length === 9 && check.modelCalls === 0 && check.writeCalls === 0));
    assert.ok(result.checks.every(check => check.conversation.localDiscussionSends === 2 && check.conversation.localStopCalls === 2 && check.conversation.currentAndHistoryOriginalCopy && check.conversation.providerRequests === 0));
    t.assert.snapshot(result.checks);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
