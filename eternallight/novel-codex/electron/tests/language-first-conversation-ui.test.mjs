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

test("App accepts direct follow-ups in the selected thread and optional exact references without modes or writes", async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "novel-language-conversation-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL("./fixtures/language-first-conversation-ui.tsx", import.meta.url))], outfile: path.join(directory, "test.js"),
      bundle: true, platform: "browser", jsx: "automatic" });
    await writeFile(path.join(directory, "index.html"), '<html><head><meta charset="utf-8"><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
    let output;
    try { output = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/creative-workspace-ui-main.cjs", import.meta.url)), path.join(directory, "index.html")],
      { timeout: 35000, killSignal: "SIGKILL", env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } }); }
    catch (error) { throw Error(`${error.stdout ?? ""}\n${error.stderr ?? ""}`, { cause: error }); }
    const result = JSON.parse(output.stdout.trim());
    assert.equal(result.passed, true, `${result.error}\n${result.dom}\n${JSON.stringify(result.writes)}`);
    assert.deepEqual(result.prompts, ["还有其他方案吗？", "把前两个合并，先不保存。", "现在按这个方案修改结尾。", "先分析，别续写。", "只调整这条建议的意象，先给我两个方向。"]);
    assert.equal(result.writes, 0); assert.equal(result.controlCalls, 0);
    assert.deepEqual(result.checks, ["single input", "no duplicate discussion entry", "no implicit author prompt", "exact proposal", "preserved draft", "unquoted multi-turn language", "optional versioned reference", "input race", "ABA isolation", "latest reference wins"]);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
