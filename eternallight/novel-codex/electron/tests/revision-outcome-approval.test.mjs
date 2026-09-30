import assert from "node:assert/strict";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { mainHandlers, rendererEvent, runtimeFixture } from "./main-handler-harness.mjs";

const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;
const actionId = `action-${"a".repeat(32)}`;
const authorIntent = "只修改密函场景，把烧信改为藏信。";
const desiredOutcomes = [{ id: "hide_letter", criterion: "主角藏起密函而不是烧毁它。", groupIds: ["letter"] }];
const target = { type: "chapter", chapterId: "chapter-one", chapterNumber: 1,
  scope: { type: "selectedScenes", sourceRevisionId: "original-one", groupIds: ["letter"] }, desiredOutcomes };
const plain = value => JSON.parse(JSON.stringify(value));

// Only Runtime responses and the local fixture's failing turn are controlled.
// Main's approval, creative runner, adapter, handoff and budget lifecycle all execute.
async function approvalFixture(failure) {
  const runtime = runtimeFixture();
  const calls = [], created = [], turns = [];
  const state = { confirmations: [], budgets: [] };
  let sourceTask, writerTask, modelingTask, assessmentStep, knowledgeReady = false;
  const proposal = () => ({ id: actionId, sourceTaskId: sourceTask.id, status: "proposed", inputRevision: 0,
    intentRevision: 0, operation: "generateChapter", target, instruction: authorIntent });
  const { handlers, adapter } = await mainHandlers(async (path, body, options = {}) => {
    calls.push({ path, body, method: options.method ?? "GET" });
    if (path.endsWith("/creative-workspace")) return {
      revision: 0, headSnapshotId: "snapshot-fixture", sourceDigest: "source-fixture",
      chapters: [{ id: target.chapterId, number: target.chapterNumber }],
      modeling: [{ objectId: `draft-${target.chapterId}`, sourceRevisionId: "draft-one",
        status: knowledgeReady ? "ready" : "model_pending" }],
    };
    if (path.endsWith("/actions")) return { actions: sourceTask ? [proposal()] : [] };
    if (path.endsWith(`/${actionId}/approve`)) return { ...proposal(), status: "queued" };
    if (path.endsWith(`/${actionId}/finish`)) return { status: "completed" };
    if (path.endsWith("/next-step")) {
      assert.equal(path, `/api/projects/project/tasks/${writerTask.id}/next-step`);
      return { action: { kind: "modelKnowledge", chapterId: target.chapterId,
        sourceRevisionId: "draft-one", sourceTaskId: writerTask.id } };
    }
    if (path.endsWith("/modeling-batches")) return { id: "batch-one" };
    if (failure === "lookup" && knowledgeReady && path === `/api/projects/project/tasks/${writerTask.id}/work-task`) {
      throw new Error("assessment lookup unavailable");
    }
    const result = await runtime.request(path, body);
    if (path.endsWith("/tasks") && options.method === "POST") {
      created.push(result);
      const work = [...runtime.workTasks.values()].find(item => item.steps.some(step => step.currentAttemptId === result.id));
      const step = work.steps.find(item => item.currentAttemptId === result.id);
      Object.assign(step, { operation: result.operation, status: "running" });
      if (!body.target?.resumeStepId) step.target = result.target;
      if (result.operation === "advisory") sourceTask = result;
      if (result.operation === "generateChapter") {
        writerTask = result;
        result.purpose = "writer";
        // Core's normalized Writer target supplies the exact read position.
        result.target = { ...result.target, readingChapter: 1, sceneOrder: 1 };
      }
      if (result.operation === "modelKnowledge") { modelingTask = result; result.purpose = "critic"; }
      if (result.operation === "assessRevisionOutcome") {
        result.purpose = "critic";
        result.requiredOutputKind = "revisionOutcome";
        result.inputManifest = { authorIntent,
          authorIntentBinding: { sourceTaskId: sourceTask.id, intentRevision: 0, actionId } };
      }
    }
    if (path.endsWith("/complete")) {
      const completed = created.find(task => path === `/api/projects/project/tasks/${task.id}/complete`);
      assert.ok(completed, `Unexpected completion ${path}`);
      completed.status = "completed";
      const work = [...runtime.workTasks.values()].find(item => item.steps.some(step => step.currentAttemptId === completed.id));
      const step = work.steps.find(item => item.currentAttemptId === completed.id);
      step.status = "completed";
      if (completed.operation === "modelKnowledge") {
        knowledgeReady = true;
        assessmentStep = { id: "repository-outcome-step", operation: "assessRevisionOutcome", status: "queued",
          dependsOnStepId: step.id, target: { writerTaskId: writerTask.id, chapterId: target.chapterId,
            chapterNumber: target.chapterNumber, originalRevisionId: "original-one", draftRevisionId: "draft-one",
            knowledgeBatchId: "batch-one" } };
        work.steps.push(assessmentStep);
      }
    }
    return result;
  }, command, state);
  const sendTurn = adapter.sendTurn;
  adapter.sendTurn = async function (input, options) {
    turns.push({ operation: this.activeOperation,
      prompt: typeof input === "string" ? input : input.map(item => item.text ?? "").join("") });
    if (failure === "turn" && this.activeOperation === "assessRevisionOutcome") throw new Error("assessment turn unavailable");
    return sendTurn.call(this, input, options);
  };
  return { handlers, adapter, runtime, calls, created, turns, state,
    get sourceTask() { return sourceTask; }, get writerTask() { return writerTask; },
    get modelingTask() { return modelingTask; }, get assessmentStep() { return assessmentStep; } };
}

for (const failure of [null, "lookup", "turn"]) test(`production exact approval preserves ready knowledge when assessment ${failure ? `${failure} fails` : "succeeds"}`, async () => {
  const fixture = await approvalFixture(failure);
  const { handlers, adapter, runtime, calls, created, turns, state } = fixture;
  try {
    const event = rendererEvent();
    await handlers.get("agent:send-prompt")(event, "project", authorIntent);
    assert.deepEqual(created.map(task => task.operation), ["advisory"]);
    assert.equal(calls.some(call => call.path.endsWith("/approve")), false);
    const result = await handlers.get("agent:approve-action")(event, "project", fixture.sourceTask.id, actionId);
    assert.deepEqual(plain(result.modeling), { status: "ready", batchId: "batch-one" });
    assert.equal(result.taskId, fixture.writerTask.id);
    assert.deepEqual(plain(result.action.target), target);
    assert.deepEqual(calls.find(call => call.path.endsWith(`/${actionId}/approve`)).body, { expectedIntentRevision: 0 });
    assert.deepEqual(calls.find(call => call.path.endsWith(`/${actionId}/finish`)).body, { resultTaskId: fixture.writerTask.id });

    const writerCreate = calls.find(call => call.path.endsWith("/tasks") && call.body?.operation === "generateChapter");
    assert.deepEqual(writerCreate.body.target, { ...target, requestedByTaskId: fixture.sourceTask.id, requestedActionId: actionId });
    assert.equal(writerCreate.body.initialBudget, undefined);
    assert.equal(calls.filter(call => call.path.endsWith("/modeling-batches")).length, 1);
    assert.equal(calls.filter(call => call.path.endsWith("/next-step")).length, 1);
    assert.equal(calls.filter(call => call.path.endsWith("/tasks") && call.body?.operation === "modelKnowledge").length, 1);
    assert.equal(turns.filter(turn => turn.operation === "modelKnowledge").length, 1, "assessment failure must not repeat knowledge modeling");
    assert.equal(runtime.workTasks.size, 1, "followups must keep the original work owner");
    assert.equal(state.confirmations.length, 1, "followups must share the original approved budget");
    assert.deepEqual(turns.map(turn => turn.operation), failure === "lookup"
      ? ["advisory", "generateChapter", "modelKnowledge"]
      : ["advisory", "generateChapter", "modelKnowledge", "assessRevisionOutcome"]);

    if (failure) {
      const expected = { status: "pending", error: `assessment ${failure} unavailable` };
      assert.deepEqual(plain(result.outcome), expected);
      assert.deepEqual(plain(result.followup), expected);
    } else {
      const assessment = created.find(task => task.operation === "assessRevisionOutcome");
      assert.deepEqual(plain(result.outcome), { status: "recorded", taskId: assessment.id });
      assert.equal(result.followup, null);
    }
    if (failure !== "lookup") {
      const assessment = created.find(task => task.operation === "assessRevisionOutcome");
      const expectedTarget = { ...fixture.assessmentStep.target, resumeStepId: fixture.assessmentStep.id };
      assert.equal(Object.hasOwn(fixture.assessmentStep.target, "resumeStepId"), false);
      assert.deepEqual(calls.find(call => call.path.endsWith("/tasks") && call.body?.operation === "assessRevisionOutcome").body.target, expectedTarget);
      assert.equal(Object.hasOwn(expectedTarget, "sourceTaskId"), false);
      assert.equal(assessment.purpose, "critic");
      assert.equal(assessment.initialBudget, undefined);
      const prompt = turns.find(turn => turn.operation === "assessRevisionOutcome").prompt;
      assert.match(prompt, /read_context\(purpose=critic,budget=24,byteBudget=60000\)/);
      const binding = prompt.match(/局部改稿目标评估任务绑定：\n(\{[\s\S]*?\})\n\n先 novel_workspace/);
      assert.ok(binding, "production creative runner must send a bound assessment prompt");
      const handoff = JSON.parse(binding[1]);
      assert.equal(handoff.task.purpose, "critic");
      assert.deepEqual(handoff.task.target, expectedTarget);
      assert.equal(handoff.task.taskId, assessment.id);
      assert.equal(handoff.task.requiredOutputKind, "revisionOutcome");
    }
  } finally { adapter.close(); }
});
