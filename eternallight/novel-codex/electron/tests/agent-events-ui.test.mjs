import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { build } from "esbuild";
import electron from "electron";

test("actual conversation keeps final replies and late events within the current turn", async () => {
  const directory = await mkdtemp(join(tmpdir(), "novel-agent-events-ui-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL("./fixtures/agent-events-ui.tsx", import.meta.url))],
      outfile: join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic" });
    await writeFile(join(directory, "index.html"), '<html lang="zh-CN"><head><meta charset="utf-8"><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
    let output;
    try {
      output = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/creative-workspace-ui-main.cjs", import.meta.url)), join(directory, "index.html")],
        { timeout: 30000, env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } });
    } catch (error) { throw new Error(`Agent event UI failed:\n${error.stdout ?? ""}\n${error.stderr ?? ""}`, { cause: error }); }
    const result = JSON.parse(output.stdout.trim());
    assert.deepEqual(result, { passed: true, snapshots: [
      { state: "completed", prompt: "第1轮分析", answer: "第1轮最终结论", toolTitles: ["定位正文依据", "分析故事假设", "安排多视角规划检查"] },
      { state: "completed", prompt: "第2轮分析", answer: "第2轮最终结论", toolTitles: ["定位正文依据", "分析故事假设", "安排多视角规划检查"] },
    ] });
  } finally { await rm(directory, { recursive: true, force: true }); }
});
