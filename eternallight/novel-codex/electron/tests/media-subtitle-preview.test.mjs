import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, rm, writeFile, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import { build } from "esbuild";
import test from "node:test";
import electron from "electron";

test("real animation preview loads the private Host font under production CSP and rejects missing glyphs", { timeout: 90_000 }, async () => {
  assert.ok(process.env.NOVEL_CODEX_MEDIA_FONT, "NOVEL_CODEX_MEDIA_FONT must be explicitly configured");
  const directory = await mkdtemp(join(tmpdir(), "media-subtitle-preview-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL("./fixtures/media-subtitle-preview-ui.tsx", import.meta.url))], outfile: join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic" });
    const productionHtml = await readFile(fileURLToPath(new URL("../../src/index.html", import.meta.url)), "utf8");
    const csp = productionHtml.match(/http-equiv="Content-Security-Policy"\s+content="([^"]+)"/)[1];
    await writeFile(join(directory, "index.html"), `<html lang="zh-CN"><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="${csp}"><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>`);
    await writeFile(join(directory, "preload.cjs"), 'const {contextBridge,ipcRenderer}=require("electron");contextBridge.exposeInMainWorld("readSubtitleFont",()=>ipcRenderer.invoke("test:subtitle-font"));');
    const { stdout } = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/media-subtitle-preview-main.cjs", import.meta.url)),
      join(directory, "index.html"), fileURLToPath(new URL("../", import.meta.url)), directory], { timeout: 80_000, maxBuffer: 64 * 1024,
      env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } });
    const result = JSON.parse(stdout);
    assert.equal(result.passed, true); assert.equal(result.csp, true);
    assert.ok(result.fontRequests > 0); assert.ok(result.fonts.length > 0 && result.fonts.every(font => font.isCustomFont && font.glyphCount > 0));
    assert.equal(result.missingGlyphRejected, true); assert.equal(result.overflowRejected, true);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
