import assert from "node:assert/strict";
import { test } from "node:test";

import { CodexAppServerAdapter } from "../codex-adapter.mjs";
import { operationRequirement } from "../operation-requirements.mjs";
import { advanceWorkSteps } from "../work-step-continuation.mjs";
import { createTaskBoundHandoff, taskBoundInstructions, taskExecutionPrompt } from "../workflow-handoff.mjs";
import { modelInputTokenBudget } from "../../shared/author-input-budget.mjs";
import { withFrozenMethods } from "./fixtures/frozen-methods.mjs";

function assessmentTarget() {
  return { writerTaskId: "writer-one", chapterId: "chapter-one", chapterNumber: 1,
    originalRevisionId: "original-one", draftRevisionId: "draft-one", knowledgeBatchId: "batch-one" };
}

test("assessment handoff preserves sources and the existing frozen author prompt", () => {
  const task = { id: "assessment-one", threadId: "critic-one", inputRevision: 8,
    operation: "assessRevisionOutcome", purpose: "critic", requiredOutputKind: "revisionOutcome",
    target: { ...assessmentTarget(), resumeStepId: "outcome-step" },
    inputManifest: { authorIntent: "  让第二场的指控有正文依据 🌙\n",
      authorIntentBinding: { sourceTaskId: "writer-one", intentRevision: 0, actionId: null },
      expectedCandidateRevisionId: null } };
  withFrozenMethods(task, "project-one");
  const before = structuredClone(task);
  assert.equal(taskExecutionPrompt(task), task.inputManifest.authorIntent);
  const handoff = createTaskBoundHandoff("project-one", { revision: 8 }, task);
  assert.deepEqual(handoff.task.target, task.target);
  assert.deepEqual(handoff.task.inputManifest.authorIntentBinding, task.inputManifest.authorIntentBinding);
  assert.equal(handoff.task.inputManifest.authorIntent, undefined);
  assert.equal(handoff.task.inputManifest.expectedCandidateRevisionId, null);
  const instructions = taskBoundInstructions(handoff);
  assert.match(instructions, /read_context\(purpose=critic,budget=24,byteBudget=60000\)/);
  assert.match(instructions, /operationContext\.revisionOutcomeInput/);
  assert.match(instructions, /desiredOutcomes、beforeScenes、afterScenes 和 coverage/);
  assert.match(instructions, /每个目标 id 恰好一次/);
  assert.match(instructions, /supported、unmet 或 insufficientEvidence/);
  assert.match(instructions, /supported\/unmet 必须有证据/);
  assert.match(instructions, /Unicode 字符偏移/);
  assert.match(instructions, /evidence:\[\{groupId,quote\}\]/);
  assert.match(instructions, /唯一连续摘录由 Runtime 生成偏移/);
  assert.match(instructions, /start\/end 可选且必须成对/);
  assert.match(instructions, /实际改动的新场景/);
  assert.match(instructions, /同一逻辑写入重试保持 payload 和 idempotencyKey 不变/);
  assert.match(instructions, /保存后 complete_task/);
  assert.match(instructions, /不修改正文、规划、承诺或 Canon/);
  assert.doesNotMatch(instructions, /save_draft_checkpoint|submit_scene_assessment|publish_knowledge_batch|request_candidate_task|read_reading_progress|checkpoint_reading/);
  assert.ok(modelInputTokenBudget(instructions) < 10_000);
  assert.deepEqual(task, before);
});

test("queued assessment executes once through the production creative task runner", async () => {
  const event = {};
  const target = assessmentTarget();
  const step = { id: "outcome-step", operation: "assessRevisionOutcome", status: "queued",
    dependsOnStepId: "model-step", target };
  const work = { id: "work-one", status: "running", goal: "让指控有依据", steps: [
    { id: "writer-step", operation: "generateChapter", status: "completed" },
    { id: "model-step", operation: "modelKnowledge", status: "completed" }, step,
  ] };
  const calls = [];
  const requests = [];
  const result = await advanceWorkSteps(event, "project-one", work.id, {
    runtimeRequest: async (path, options) => { requests.push({ path, options }); return work; },
    currentWorkFlowState: async () => "running",
    runCreativeAgentTask: async (...args) => { calls.push(args); step.status = "completed"; },
    runAdvisoryAgentTask: async () => assert.fail("assessment must run as a creative task"),
    runCandidateWorkflow: async () => assert.fail("assessment must not enter chapter followup"),
    modelChapterKnowledge: async () => assert.fail("completed modeling must not be repeated"),
  });
  assert.equal(result, work);
  assert.deepEqual(calls, [[event, "project-one", work.goal, { operation: "assessRevisionOutcome" },
    { ...target, resumeStepId: "outcome-step" }]]);
  assert.deepEqual(step.target, target);
  assert.ok(requests.every(({ path, options }) => path === "/api/projects/project-one/work-tasks/work-one" && options === undefined));
});

test("assessment waits for knowledge completion and respects a paused work flow", async () => {
  const work = { id: "work-one", status: "running", goal: "检查结果", steps: [
    { id: "model-step", operation: "modelKnowledge", status: "running" },
    { id: "outcome-step", operation: "assessRevisionOutcome", status: "queued",
      dependsOnStepId: "model-step", target: assessmentTarget() },
  ] };
  const deps = {
    runtimeRequest: async () => work,
    currentWorkFlowState: async () => "running",
    runCreativeAgentTask: async () => assert.fail("assessment dependency is incomplete"),
  };
  assert.equal(await advanceWorkSteps({}, "project-one", work.id, deps), work);
  work.steps[0].status = "completed";
  assert.equal(await advanceWorkSteps({}, "project-one", work.id,
    { ...deps, currentWorkFlowState: async () => "paused" }), work);
});

test("host does not create an assessment when Core has queued none", async () => {
  const work = { id: "work-one", status: "running", goal: "普通局部改稿", steps: [
    { id: "writer-step", operation: "generateChapter", status: "completed" },
    { id: "model-step", operation: "modelKnowledge", status: "completed" },
  ] };
  const requests = [];
  assert.equal(await advanceWorkSteps({}, "project-one", work.id, {
    runtimeRequest: async (...args) => { requests.push(args); return work; },
    currentWorkFlowState: async () => "running",
    runCreativeAgentTask: async () => assert.fail("Core did not queue assessment"),
  }), work);
  assert.deepEqual(requests, [["/api/projects/project-one/work-tasks/work-one"]]);
});

test("adapter prepares a revision assessment with the ordinary tool-loop capability profile", () => {
  const adapter = new CodexAppServerAdapter({ env: {} });
  adapter.prepareTaskOperation("assessRevisionOutcome", "revision-capability-123456789");
  assert.equal(adapter.activeOperation, "assessRevisionOutcome");
  assert.equal(adapter.env.NOVEL_CODEX_OPERATION, "assessRevisionOutcome");
  assert.equal(adapter.env.NOVEL_CODEX_TASK_CAPABILITY, "revision-capability-123456789");
  assert.equal(operationRequirement("assessRevisionOutcome"), operationRequirement("assessSceneOutcome"));
});
