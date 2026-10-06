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

test("actual App reconciles Electron cancellation rejection and authoritative stop races", async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "novel-app-stop-race-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL("./fixtures/app-stop-race-ui.tsx", import.meta.url))], outfile: path.join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic" });
    await writeFile(path.join(directory, "index.html"), '<html><head><meta charset="utf-8"><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
    const { stdout } = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/creative-workspace-ui-main.cjs", import.meta.url)), path.join(directory, "index.html")], { timeout: 35000, env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } });
    const result = JSON.parse(stdout.trim());
    assert.equal(result.passed, true, `${result.error}\n${JSON.stringify(result.calls)}\n${result.dom ?? ""}`);
    assert.deepEqual(result.calls, { model: 8, interrupt: 7 });
    assert.deepEqual(result.snapshots.map(item => item.case), ["reject-before-receipt", "receipt-before-reject", "late-reject", "native-before-receipt", "false-receipt-retry"]);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test("App retains execution results and restores isolated thread history with read-only recovery", async t => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "novel-app-recovery-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL("./fixtures/app-recovery-ui.tsx", import.meta.url))], outfile: path.join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic",
      plugins: [{ name: "app-callback-controls", setup(builder) { builder.onResolve({ filter: /^\.\/CreativeWorkspaceView$/ }, () => ({ path: fileURLToPath(new URL("./fixtures/app-execution-controls.tsx", import.meta.url)) })); } }] });
    await writeFile(path.join(directory, "index.html"), '<html><head><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
    const { stdout } = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/creative-workspace-ui-main.cjs", import.meta.url)), path.join(directory, "index.html")], { timeout: 35000, env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } });
    const result = JSON.parse(stdout.trim());
    assert.equal(result.passed, true, result.error);
    assert.ok(result.executionSnapshots.every(entry => entry.retainedResult), "execution result disappeared");
    t.assert.snapshot(result.executionSnapshots);
    t.assert.snapshot(result.recoverySnapshot);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test("real App routes failed domain operations to source-bound task recovery and preserves chapter publication authority", async t => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "novel-app-domain-retry-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL("./fixtures/app-domain-retry-ui.tsx", import.meta.url))], outfile: path.join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic" });
    await writeFile(path.join(directory, "index.html"), '<html><head><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
    let stdout;
    try { ({ stdout } = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/creative-workspace-ui-main.cjs", import.meta.url)), path.join(directory, "index.html")], { timeout: 35000, env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } })); }
    catch (error) { throw new Error(`${error.stdout ?? ""}\n${error.stderr ?? ""}`, { cause: error }); }
    const result = JSON.parse(stdout.trim());
    assert.equal(result.passed, true, `${result.error}\n${result.dom ?? ""}`);
    assert.equal(result.chatCalls, 7, "The three planning goals, two coordination requests, blank-chapter goal and explicit knowledge analysis use the same conversation; recovery must not replay them");
    t.assert.snapshot(result.approvalRecovery);
    t.assert.snapshot(result.coordination);
    t.assert.snapshot(result.coordinatedSuccess);
    t.assert.snapshot(result.retries);
    t.assert.snapshot(result.publication);
    t.assert.snapshot(result.isolation);
  } finally { await rm(directory, { recursive: true, force: true }); }
});


test("actual App uses one composer for chapter writing with fresh edits and safe input ownership", async t => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "novel-one-composer-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL("./fixtures/app-chapter-writing-ui.tsx", import.meta.url))], outfile: path.join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic" });
    await writeFile(path.join(directory, "index.html"), '<html><head><meta charset="utf-8"><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
    let output;
    try { output = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/creative-workspace-ui-main.cjs", import.meta.url)), path.join(directory, "index.html")], { timeout: 35000, env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } }); }
    catch (error) { throw new Error(`${error.stdout ?? ""}\n${error.stderr ?? ""}`, { cause: error }); }
    const result = JSON.parse(output.stdout.trim());
    assert.equal(result.passed, true, `${result.error}\n${result.dom}`);
    assert.deepEqual({ generations: result.generations, steers: result.steers, saves: result.saves, discussions: result.advisoryCalls }, { generations: 0, steers: 1, saves: 2, discussions: 8 });
    t.assert.snapshot(result.snapshots);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
