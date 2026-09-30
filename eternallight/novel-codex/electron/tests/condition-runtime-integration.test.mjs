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

test("production preload and condition IPC reach the native worker, reopen results and revoke changed sources", { timeout: 60000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), "novel-condition-runtime-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL("./fixtures/condition-runtime-ui.tsx", import.meta.url))], outfile: join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic" });
    const { stdout } = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/condition-runtime-main.mjs", import.meta.url)), directory], { timeout: 55000, env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } }).catch(error => {
      throw new Error(`${error.message}\n${error.stdout ?? ""}\n${error.stderr ?? ""}`);
    });
    const result = JSON.parse(stdout.trim());
    assert.equal(result.passed, true, result.error);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
