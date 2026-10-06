import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import test from "node:test";
import { build } from "esbuild";
import electron from "electron";

test("actual App preserves author drafts by project and creates once until a receipt arrives", async t => {
  const directory = await mkdtemp(join(tmpdir(), "novel-author-input-ui-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  await build({ entryPoints: [fileURLToPath(new URL("./fixtures/app-author-input-ui.tsx", import.meta.url))], outfile: join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic" });
  await writeFile(join(directory, "index.html"), '<html><head><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
  const { stdout } = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/creative-workspace-ui-main.cjs", import.meta.url)), join(directory, "index.html")], { timeout: 30_000, env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } }).catch(error => { throw new Error(`${error.stdout ?? ""}\n${error.stderr ?? ""}`); });
  const result = JSON.parse(stdout.trim());
  assert.equal(result.passed, true, result.error);
  assert.deepEqual({ modelCalls: result.modelCalls, createCalls: result.createCalls }, { modelCalls: 1, createCalls: 2 });
  assert.equal(result.geometry.length, 5);
  t.assert.snapshot({ sizeNotice: result.sizeNotice, capacityNotice: result.capacityNotice, retainedTitle: result.retainedTitle, geometry: result.geometry });
});
