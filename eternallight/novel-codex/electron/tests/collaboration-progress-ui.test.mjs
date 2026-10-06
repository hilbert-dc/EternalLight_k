import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { build } from "esbuild";
import electron from "electron";
test("collaboration rows preserve identity, actual child status, results and narrow layout", async () => {
  const directory = await mkdtemp(join(tmpdir(), "novel-collaboration-ui-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL("./fixtures/collaboration-progress-ui.tsx", import.meta.url))], outfile: join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic" });
    await writeFile(join(directory, "index.html"), '<html><head><meta charset="utf-8"><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
    const { stdout } = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/creative-workspace-ui-main.cjs", import.meta.url)), join(directory, "index.html")], { timeout: 35000, env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } });
    const result = JSON.parse(stdout.trim()); assert.equal(result.passed, true, result.error);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
