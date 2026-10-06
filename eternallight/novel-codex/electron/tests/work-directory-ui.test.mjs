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

test("work directory preserves authoritative volume identities, mutations and editor state", async t => {
  const directory = await mkdtemp(join(tmpdir(), "novel-work-directory-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL("./fixtures/work-directory-ui.tsx", import.meta.url))], outfile: join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic" });
    await writeFile(join(directory, "index.html"), '<html lang="zh-CN"><head><meta charset="utf-8"><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
    let output;
    try { output = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/work-directory-ui-main.cjs", import.meta.url)), join(directory, "index.html")], { timeout: 30000, env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } }); }
    catch (error) { throw new Error(`${error.stdout ?? ""}\n${error.stderr ?? ""}`, { cause: error }); }
    const result = JSON.parse(output.stdout.trim());
    assert.equal(result.passed, true, result.error);
    assert.equal(result.comparisons.length, 4);
    for (const comparison of result.comparisons) assert.deepEqual(comparison.actual, comparison.expected);
    assert.equal(result.placements.length, 3);
    assert.equal(result.creates.length, 4);
    t.assert.snapshot(result.snapshots);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test("catalog reads and writes isolate owners and reconcile uncertain receipts without replay", async t => {
  const directory = await mkdtemp(join(tmpdir(), "novel-work-directory-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL("./fixtures/work-catalog-ownership-ui.tsx", import.meta.url))], outfile: join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic" });
    await writeFile(join(directory, "index.html"), '<html lang="zh-CN"><head><meta charset="utf-8"><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
    let output;
    try { output = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/creative-workspace-ui-main.cjs", import.meta.url)), join(directory, "index.html")], { timeout: 30000, env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } }); }
    catch (error) { throw new Error(`${error.stdout ?? ""}\n${error.stderr ?? ""}`, { cause: error }); }
    const result = JSON.parse(output.stdout.trim());
    assert.equal(result.passed, true, result.error);
    assert.equal(result.snapshot.writes, 3);
    t.assert.snapshot(result.snapshot);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test("workspace creates a source-bound volume chapter and recovers its receipt without repeating writes", async t => {
  const directory = await mkdtemp(join(tmpdir(), "novel-work-directory-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL("./fixtures/work-directory-workspace-ui.tsx", import.meta.url))], outfile: join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic" });
    await writeFile(join(directory, "index.html"), '<html lang="zh-CN"><head><meta charset="utf-8"><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
    let output;
    try { output = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/creative-workspace-ui-main.cjs", import.meta.url)), join(directory, "index.html")], { timeout: 30000, env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } }); }
    catch (error) { throw new Error(`${error.stdout ?? ""}\n${error.stderr ?? ""}`, { cause: error }); }
    const result = JSON.parse(output.stdout.trim());
    assert.equal(result.passed, true, result.error);
    assert.equal(result.models, 0);
    assert.equal(result.writes, 2);
    t.assert.snapshot(result.snapshots);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
