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
for (const fixture of ["app-model-connection-ui.tsx", "app-turn-recovery-ui.tsx"]) test(`actual App connection journey: ${fixture}`, async t => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "novel-model-connection-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL(`./fixtures/${fixture}`, import.meta.url))], outfile: path.join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic" });
    await writeFile(path.join(directory, "index.html"), '<html><head><meta charset="utf-8"><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
    // Native focus styling requires a focused BrowserWindow; reuse the key/resize driver.
    const driver = await readFile(new URL("./fixtures/project-information-ui-main.cjs", import.meta.url), "utf8");
    await writeFile(path.join(directory, "driver.cjs"), driver.replace("show: false", "show: true").replace("await window.loadFile", "window.focus(); await window.loadFile")
      .replace('window.webContents.sendInputEvent({ type: "keyUp", ...control.key });', 'if (control.key.keyCode === "Return" || control.key.keyCode === "Space") window.webContents.sendInputEvent({ type: "char", keyCode: control.key.keyCode === "Return" ? "\\r" : " " });\n        window.webContents.sendInputEvent({ type: "keyUp", ...control.key });'));
    let stdout;
    try { ({ stdout } = await promisify(execFile)(electron, [path.join(directory, "driver.cjs"), path.join(directory, "index.html")], { timeout: 35000, env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } })); }
    catch (error) { throw new Error(`${error.stdout ?? ""}\n${error.stderr ?? ""}`, { cause: error }); }
    const result = JSON.parse(stdout.trim()); assert.equal(result.passed, true, result.error);
    assert.deepEqual(result.calls, fixture === "app-model-connection-ui.tsx" ? { verify: 3, apply: 2, model: 0 } : { model: 6, reconnect: 5 });
    t.assert.snapshot(result.snapshots.map(item => typeof item === "string" ? item : ({ ...item, text: item.text.replace(/已等待.*?秒/g, "已等待 N 秒") })));
  } finally { await rm(directory, { recursive: true, force: true }); }
});
