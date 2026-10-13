import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { build } from "esbuild";
import electron from "electron";

test("actual App keeps conversation titles author-facing across naming, restore and project scopes", async t => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "novel-title-ui-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL("./fixtures/conversation-title-ui.tsx", import.meta.url))],
      outfile: path.join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic" });
    await writeFile(path.join(directory, "late-styles.css"), await readFile(new URL("../../src/styles.css", import.meta.url)));
    await writeFile(path.join(directory, "index.html"), '<html><head><link rel="stylesheet" href="test.css"><link rel="stylesheet" href="late-styles.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
    const { stdout } = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/creative-workspace-ui-main.cjs", import.meta.url)), path.join(directory, "index.html")],
      { timeout: 35000, killSignal: "SIGKILL", env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } });
    const result = JSON.parse(stdout.trim()); assert.equal(result.passed, true, result.error); t.assert.snapshot(result.snapshot);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
