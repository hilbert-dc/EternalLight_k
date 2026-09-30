import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { build } from "esbuild";
import electron from "electron";

test("workspace rebind continues through modeling and next-chapter planning across Runtime restart", { timeout: 60000 }, async t => {
  const directory = await mkdtemp(join(tmpdir(), "novel-rebind-runtime-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL("./fixtures/rebind-runtime-ui.tsx", import.meta.url))], outfile: join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic" });
    const { stdout } = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/rebind-runtime-main.mjs", import.meta.url)), directory], { timeout: 55000, env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } }).catch(error => {
      throw new Error(`${error.stdout ?? error.message}\n${(error.stderr ?? "").slice(-1000)}`);
    });
    const result = JSON.parse(stdout.trim());
    assert.equal(result.passed, true, result.error);
    t.assert.snapshot(result.blockedSnapshot); t.assert.snapshot(result.refreshSnapshot);
    t.assert.snapshot(result.previewSnapshot);
    t.assert.snapshot(result.reboundPreviewSnapshot);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
