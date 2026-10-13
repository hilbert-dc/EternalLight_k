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
  assert.match(instructions, /read_context核对冻结target、coverage和taskSources/);
  assert.match(instructions, /绑定operation确定purpose与预算/);
  assert.match(instructions, /read_source_content/);
  assert.match(instructions, /revisionOutcomeInput的desiredOutcomes、beforeScenes（无原稿时为空）、afterScenes、coverage及必要前文/);
  assert.match(instructions, /每个目标id恰好一次supported、unmet或insufficientEvidence/);
  assert.match(instructions, /supported\/unmet须有精确原文依据/);
  assert.match(instructions, /仅用after及必要前文证据评估/);
  assert.match(instructions, /不能改目标或以旧稿充当修改结果/);
  assert.match(instructions, /同一次逻辑写重试复用idempotencyKey/);
  assert.match(instructions, /最后complete_task核对真实产物/);
  assert.match(instructions, /不修改正文、规划、承诺或Canon/);
  assert.doesNotMatch(instructions, /save_draft_checkpoint|submit_scene_assessment|publish_knowledge_batch|request_candidate_task|read_reading_progress|checkpoint_reading/);
  assert.ok(modelInputTokenBudget(instructions) < 10_000);
  assert.deepEqual(task, before);
});

test("queued assessment executes once through the production creative task runner", async () => {
  const event = {};
  const target = assessmentTarget();
  const step = { id: "outcome-step", operation: "assessRevisionOutcome", status: "queued",
    dependsOnStepId: "model-step", target };
  const work = { id: "work-one", projectId: "project-one", threadId: "thread-root", status: "running", goal: "让指控有依据", steps: [
    { id: "writer-step", operation: "generateChapter", status: "completed" },
    { id: "model-step", operation: "modelKnowledge", status: "completed" }, step,
  ] };
  const calls = [];
  const requests = [];
  const result = await advanceWorkSteps(event, "project-one", work.id, {
    runtimeRequest: async (path, options) => { requests.push({ path, options });
      if (path.endsWith("/decision")) return { workTaskId: work.id, threadId: work.threadId, step: null, stopReason: "completed" };
      return work; },
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
  assert.equal(requests.filter(({ path }) => path.endsWith("/decision")).length, 1);
});

test("assessment waits for knowledge completion and respects a paused work flow", async () => {
  const work = { id: "work-one", projectId: "project-one", threadId: "thread-root", status: "running", goal: "检查结果", steps: [
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


test("adapter prepares a revision assessment with the ordinary tool-loop capability profile", () => {
  const adapter = new CodexAppServerAdapter({ env: {} });
  adapter.prepareTaskOperation("assessRevisionOutcome", "revision-capability-123456789");
  assert.equal(adapter.activeOperation, "assessRevisionOutcome");
  assert.equal(adapter.env.NOVEL_CODEX_OPERATION, "assessRevisionOutcome");
  assert.equal(adapter.env.NOVEL_CODEX_TASK_CAPABILITY, "revision-capability-123456789");
  assert.equal(operationRequirement("assessRevisionOutcome"), operationRequirement("assessSceneOutcome"));
});
