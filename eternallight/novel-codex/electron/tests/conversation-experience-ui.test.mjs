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

test("App sends natural author intent, queues exact messages once and restores without paid replay", async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "novel-multi-conversation-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL("./fixtures/conversation-experience-ui.tsx", import.meta.url))], outfile: path.join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic" });
    await writeFile(path.join(directory, "index.html"), '<html><head><meta charset="utf-8"><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
    let output;
    try {
      output = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/creative-workspace-ui-main.cjs", import.meta.url)), path.join(directory, "index.html")], { timeout: 35000, killSignal: "SIGKILL", env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } });
    } catch (error) {
      throw new Error(`${error.stdout ?? ""}\n${error.stderr ?? ""}`, { cause: error });
    }
    const result = JSON.parse(output.stdout.trim());
    assert.equal(result.passed, true, `${result.error}\n${result.dom ?? ""}`);
    assert.equal(result.sends, 6);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});
