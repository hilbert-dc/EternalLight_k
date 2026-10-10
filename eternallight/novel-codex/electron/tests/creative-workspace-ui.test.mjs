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

for (const [name, fixture, deliveryReviewOnly = false] of [
  ["execution names identify registered work without a second task ledger", "work-step-names-ui.tsx"],
  ["project conversation persists across views and isolates reads without executing writes", "app-task-continuity-ui.tsx"],
  ["conversation accurately presents partial delivery and author decisions", "work-conversation-ui.tsx", true],
  ["conversation unifies exact turns and reads durable replies without replaying execution", "work-conversation-ui.tsx"],
  ["actual workspace preserves scene identity, author intent, and save boundaries", "creative-workspace-ui.tsx"],
  ["task list serializes reads, preserves failures, and isolates project lifetimes", "work-task-list-ui.tsx"],
  ["actual empty App leaves task reading disabled without hiding real interface failures", "empty-workspace-ui.tsx"],
  ["actual conversation blocks duplicate Enter and retries transient failures", "conversation-submission-ui.tsx"],
]) test(name, async t => {
  const directory = await mkdtemp(path.join(os.tmpdir(), "novel-codex-ui-"));
  try {
    await build({ entryPoints: [fileURLToPath(new URL(`./fixtures/${fixture}`, import.meta.url))], outfile: path.join(directory, "test.js"), bundle: true, platform: "browser", jsx: "automatic", define: { "process.env.NOVEL_DELIVERY_REVIEW_UI": JSON.stringify(deliveryReviewOnly ? "1" : "0") } });
    await writeFile(path.join(directory, "index.html"), '<html><head><meta charset="utf-8"><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script src="test.js"></script></body></html>');
    let output;
    try { output = await promisify(execFile)(electron, [fileURLToPath(new URL("./fixtures/creative-workspace-ui-main.cjs", import.meta.url)), path.join(directory, "index.html")], { timeout: 35000, killSignal: "SIGKILL", env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } }); }
    catch (error) { throw new Error(`${error.stdout ?? ""}\n${error.stderr ?? ""}`, { cause: error }); }
    const { stdout } = output;
    const result = JSON.parse(stdout.trim());
    assert.equal(result.passed, true, result.error);
    if (["creative-workspace-ui.tsx", "conversation-submission-ui.tsx"].includes(fixture)) t.assert.snapshot(result.retrySnapshot);
    if (fixture === "creative-workspace-ui.tsx") t.assert.snapshot(result.chromeSnapshot);
    if (fixture === "conversation-submission-ui.tsx") {
      t.assert.snapshot(result.taskPlacementSnapshot);
      t.assert.snapshot(result.syncSnapshot);
      t.assert.snapshot(result.dirtySnapshot);
      t.assert.snapshot(result.firstWorkSnapshot);
      t.assert.snapshot(result.interactionSnapshot);
      t.assert.snapshot(result.backgroundSnapshot);
    }
    if (fixture === "app-task-continuity-ui.tsx") t.assert.snapshot(result.snapshots);
    if (fixture === "work-conversation-ui.tsx") {
      if (!deliveryReviewOnly) { t.assert.snapshot(result.timelineSnapshot); t.assert.snapshot(result.states); }
      else assert.deepEqual(result.deliveryReviewStates, [
        { goalStatus: "partial", header: "已保存成果，目标尚未全部完成", unresolvedVisible: true, authorDecisionsVisible: true, reviewVisible: true },
        { goalStatus: "needsAuthorDecision", header: "已保存成果，需你决定后续安排", unresolvedVisible: true, authorDecisionsVisible: true, reviewVisible: true },
        { goalStatus: "fulfilled", header: "结果已保存", unresolvedVisible: true, authorDecisionsVisible: true, reviewVisible: false },
      ]);
    }
    if (fixture === "work-task-list-ui.tsx") {
      t.assert.snapshot(result.deliverySnapshot);
      t.assert.snapshot(result.schedulingSnapshot);
      t.assert.snapshot(result.recoverySnapshot);
      t.assert.snapshot(result.densitySnapshot);
      assert.deepEqual(result.lifecycleCases, ["disabled empty workspace", "disabled actions and polling", "disabled missing bridge", "enabled missing bridge", "enabled missing task API", "disabled in-flight and stale actions", "re-enabled fresh read", "real failure and retry"]);
    }
    if (fixture === "empty-workspace-ui.tsx") {
      assert.deepEqual(result.cases, ["fresh empty App", "empty App without task API", "empty App after restart"]);
      assert.equal(result.calls.taskReads, 0);
      assert.equal(result.calls.models, 0);
      assert.equal(result.calls.writes, 0);
    }
  } finally { await rm(directory, { recursive: true, force: true }); }
});
