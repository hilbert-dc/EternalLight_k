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

test("actual App reads history without restoring archived writers or replacing the current execution", async t => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "novel-history-view-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL("./fixtures/thread-history-view-ui.tsx", import.meta.url))], outfile: path.join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic" });
    await writeFile(path.join(directory, "late-styles.css"), await readFile(new URL("../../src/styles.css", import.meta.url)));
    await writeFile(path.join(directory, "index.html"), '<html><head><meta charset="utf-8"><link rel="stylesheet" href="test.css"><link rel="stylesheet" href="late-styles.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
    let result;
    try { result = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/creative-workspace-ui-main.cjs", import.meta.url)), path.join(directory, "index.html")], { timeout: 35000, env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } }); }
    catch (error) { throw Error(`${error.stdout ?? ""}\n${error.stderr ?? ""}`, { cause: error }); }
    const parsed = JSON.parse(result.stdout.trim()); assert.equal(parsed.passed, true, parsed.error); t.assert.snapshot(parsed.snapshot);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
