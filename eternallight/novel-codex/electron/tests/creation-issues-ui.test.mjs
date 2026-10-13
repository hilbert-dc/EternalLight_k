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

test("production Work and report UI share stable issues, reveal evidence and isolate paged reads", { timeout: 40000 }, async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "novel-creation-issues-ui-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL("./fixtures/creation-issues-ui.tsx", import.meta.url))], outfile: path.join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic" });
    await writeFile(path.join(directory, "index.html"), '<html lang="zh-CN"><head><meta charset="utf-8"><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
    let output;
    try { output = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/creative-workspace-ui-main.cjs", import.meta.url)), path.join(directory, "index.html")],
      { timeout: 35000, env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } }); }
    catch (error) { throw new Error(`${error.stdout ?? ""}\n${error.stderr ?? ""}`, { cause: error }); }
    const result = JSON.parse(output.stdout.trim());
    assert.equal(result.passed, true, `${result.error}\n${JSON.stringify(result.calls)}\n${result.dom ?? ""}`);
    assert.equal(result.calls.writes, 0);
    assert.deepEqual(result.cases, ["issue tool real page presentation", "metadata expansion reads actual views", "one issue across three reports", "independent evidence and Native dispute", "same text separate identities", "Native identity validation", "current scene criterion identities", "saved pending and stale remain unresolved", "explicit resolution and persisted identity", "bounded views and numeric keyset", "paged issue reference singleflight", "paged observations and provenance", "foreign observation page retry", "empty nonterminal page rejected", "changed project ignores late page", "unmounted reference stops page chain", "malformed summary retry", "unknown report reference rejected", "dark narrow reading"]);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
