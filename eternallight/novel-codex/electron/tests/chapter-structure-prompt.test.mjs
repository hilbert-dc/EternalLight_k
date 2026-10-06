import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import test from "node:test";
import { build } from "esbuild";
import { withFrozenMethods } from "./fixtures/frozen-methods.mjs";
import { createTaskBoundHandoff, taskBoundInstructions, taskExecutionPrompt } from "../workflow-handoff.mjs";
import { modelInputTokenBudget } from "../../shared/author-input-budget.mjs";

test("structure handoff uses the bound event, keeps removed prose out of prompts and does not turn check intent into story writing", () => {
  const task = { id: "task", threadId: "thread", operation: "assessChapterStructure", purpose: "critic", inputRevision: 3,
    target: { type: "project", structureChangeId: "change" }, inputManifest: { authorIntent: "只检查删除的影响", chapterStructureChange: { id: "change", projectId: "project", chapterId: "removed" } } };
  withFrozenMethods(task);
  const before = structuredClone(task);
  const handoff = createTaskBoundHandoff("project", { revision: 3, chapters: [{ body: "REMOVED_PROSE_SENTINEL" }] }, task);
  const prompt = taskExecutionPrompt(task), bound = taskBoundInstructions(handoff);
  assert.deepEqual(task, before);
  assert.match(prompt, /自行选择合适的查询工具/);
  assert.match(prompt, /本任务额度/);
  assert.match(prompt, /不得生成候选、修改正文或规划/);
  assert.match(prompt, /未读取的章节或规划列为未覆盖/);
  for (const fixed of ["一次 query_story", "byteBudget=6000", "ranking={type:'literal'}", "最多4个steps", "text.limit=3"]) assert.ok(!prompt.includes(fixed));
  assert.deepEqual(handoff.task.target, task.target);
  assert.match(bound, /read_chapter_structure_source/);
  assert.match(bound, /sourceRole/);
  assert.ok(!bound.includes("REMOVED_PROSE_SENTINEL"));
  assert.ok(!bound.includes("save_draft_checkpoint") && !bound.includes("checkpoint_reading"));
  assert.ok(modelInputTokenBudget(prompt) < 1_000);
  assert.ok(modelInputTokenBudget(bound) < 2_000);
  assert.throws(() => taskExecutionPrompt({ ...task, target: { type: "project" } }), /结构变化/);
});

test("automatic start binds real task identity, preserves prior author history and rejects foreign or duplicate starts", async () => {
  const directory = await mkdtemp(join(tmpdir(), "structure-events-"));
  try {
    await build({ entryPoints: { automatic: new URL("../../src/chapter-structure-events.ts", import.meta.url).pathname,
      events: new URL("../../src/agent-events.ts", import.meta.url).pathname }, outdir: directory, outExtension: { ".js": ".mjs" }, bundle: true, platform: "node", format: "esm" });
    const { applyChapterStructureEvent } = await import(pathToFileURL(join(directory, "automatic.mjs")));
    const { createAgentRun, reduceAgentEvent, summarizeAgentRun } = await import(pathToFileURL(join(directory, "events.mjs")));
    const previous = { ...createAgentRun("上轮作者要求"), state: "completed", answer: "原回答", taskId: "author-task", threadId: "author-thread" };
    const event = { method: "agent/structure-started", params: { projectId: "project", operation: "assessChapterStructure", taskId: "check-task", threadId: "check-thread" } };
    assert.equal(applyChapterStructureEvent(previous, event, "other"), previous);
    assert.equal(applyChapterStructureEvent({ ...previous, state: "running" }, event, "project").taskId, "author-task");
    const run = applyChapterStructureEvent(previous, event, "project");
    assert.equal(run.origin, "chapterStructure"); assert.equal(run.taskId, "check-task");
    assert.equal(run.history[0].prompt, "上轮作者要求"); assert.equal(run.history[0].answer, "原回答");
    assert.equal(applyChapterStructureEvent(run, event, "project"), run);
    const streaming = reduceAgentEvent(run, { method: "turn/started", params: { threadId: "check-thread", turn: { id: "check-turn", status: "inProgress" } } });
    assert.equal(streaming.turnId, "check-turn");
    const failed = applyChapterStructureEvent(streaming, { method: "structure/error", params: { projectId: "project", taskId: "check-task", message: "检查未完成" } }, "project");
    assert.equal(failed.state, "error"); assert.equal(summarizeAgentRun(failed).origin, "chapterStructure");
  } finally { await rm(directory, { recursive: true, force: true }); }
});
