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

for (const [name, fixture] of [
  ["independent comic and animation editing preserve exact targets, failed drafts and real API receipts", "multimedia-workspace-ui.tsx"],
  ["media creative design uses the existing draft, save, recovery and conflict contracts", "media-creative-design-ui.tsx"],
]) test(name, async () => {
  const directory = await mkdtemp(join(tmpdir(), "multimedia-workspace-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL(`./fixtures/${fixture}`, import.meta.url))], outfile: join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic" });
    await writeFile(join(directory, "index.html"), '<html lang="zh-CN"><head><meta charset="utf-8"><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
    let output;
    try { output = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/creative-workspace-ui-main.cjs", import.meta.url)), join(directory, "index.html")], { timeout: 30000, env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } }); }
    catch (error) { throw new Error(`${error.stdout ?? ""}\n${error.stderr ?? ""}`, { cause: error }); }
    const result = JSON.parse(output.stdout.trim());
    assert.equal(result.passed, true, result.error);
    if (fixture === "multimedia-workspace-ui.tsx") {
    assert.equal(result.modelCalls, 0);
    assert.equal(result.savedComic, true);
    assert.equal(result.savedAnimation, true);
    assert.equal(result.imports, 1);
    assert.equal(result.openedExports, 1);
    }
  } finally { await rm(directory, { recursive: true, force: true }); }
});
