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

test("review, generation and chapter creation preserve atomic choices and scoped receipts", async () => {
  const directory = await mkdtemp(join(tmpdir(), "novel-frontend-boundaries-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL("./fixtures/frontend-boundaries-ui.tsx", import.meta.url))], outfile: join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic" });
    await writeFile(join(directory, "index.html"), '<html><body><div id="root"></div><script src="test.js"></script></body></html>');
    const { stdout } = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/creative-workspace-ui-main.cjs", import.meta.url)), join(directory, "index.html")], { timeout: 30000, env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } });
    const result = JSON.parse(stdout.trim());
    assert.equal(result.passed, true, result.error);
    assert.equal(result.writes, 1);
    assert.equal(result.restoredChoices, true);
    assert.equal(result.isolatedLateFailure, true);
    assert.equal(result.creationWrites, 2);
    assert.equal(result.creationReadRecovery, true);
    assert.equal(result.creationScopeIsolation, true);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
