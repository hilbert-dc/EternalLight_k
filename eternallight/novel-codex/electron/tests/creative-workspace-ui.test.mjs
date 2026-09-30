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

for (const [name, fixture] of [
  ["actual workspace preserves scene identity, author intent, and save boundaries", "creative-workspace-ui.tsx"],
  ["task list serializes reads, preserves failures, and isolates project lifetimes", "work-task-list-ui.tsx"],
]) test(name, async t => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "novel-codex-ui-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL(`./fixtures/${fixture}`, import.meta.url))], outfile: path.join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic" });
    await writeFile(path.join(directory, "index.html"), '<html><head><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
    const { stdout } = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/creative-workspace-ui-main.cjs", import.meta.url)), path.join(directory, "index.html")], { timeout: 35000, killSignal: "SIGKILL", env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } });
    const result = JSON.parse(stdout.trim());
    assert.equal(result.passed, true, result.error);
    if (fixture === "creative-workspace-ui.tsx") t.assert.snapshot(result.retrySnapshot);
    if (fixture === "work-task-list-ui.tsx") t.assert.snapshot(result.deliverySnapshot);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
