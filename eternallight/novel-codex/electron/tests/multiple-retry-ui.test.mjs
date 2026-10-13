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

test("actual App chooses an existing conversation Work and rejects stale or foreign recovery choices", { timeout: 40000 }, async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "novel-multiple-retry-ui-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL("./fixtures/multiple-retry-ui.tsx", import.meta.url))], outfile: path.join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic" });
    await writeFile(path.join(directory, "index.html"), '<html><head><meta charset="utf-8"><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
    let output;
    try { output = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/creative-workspace-ui-main.cjs", import.meta.url)), path.join(directory, "index.html")],
      { timeout: 35000, env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } }); }
    catch (error) { throw new Error(`${error.stdout ?? ""}\n${error.stderr ?? ""}`, { cause: error }); }
    const result = JSON.parse(output.stdout.trim());
    assert.equal(result.passed, true, `${result.error}\n${JSON.stringify(result.calls)}\n${result.dom ?? ""}`);
    assert.equal(result.calls.newRoot, 0);
    assert.equal(result.calls.models, 0);
    assert.deepEqual(result.cases, ["restart choices", "explicit original Work", "version changed", "mode changed", "Native blocked", "live thread ref", "late thread read", "late project read", "recover saved result", "single retry", "menu identity"]);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
