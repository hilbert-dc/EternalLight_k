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
  let sourceTask, writerTask, modelingTask, assessmentStep, knowledgeReady = false, proposalVisible = false, writerFailed = false;
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
    if (path.endsWith("/actions")) return { actions: sourceTask && proposalVisible ? [proposal()] : [] };
    if (path.endsWith(`/${actionId}/approve`)) return { ...proposal(), status: "queued" };
    if (path.endsWith(`/${actionId}/finish`)) return { status: "completed" };
    if (failure === "writer" && path.endsWith("/decision") && writerFailed) {
      const work = [...runtime.workTasks.values()][0];
      const blocked = work.steps.find(step => step.currentAttemptId === writerTask.id && step.status === "blocked");
      let admitted = null;
      if (blocked) {
        blocked.status = "superseded";
        work.status = "running";
        admitted = { id: "failure-decision-step", operation: "advisory", status: "queued",
          instruction: "根据实际失败回执说明未保存内容，不重写历史", target: { workDecision: {
            workTaskId: work.id, originTaskId: sourceTask.id, sourceTaskId: writerTask.id } } };
        work.steps.push(admitted);
      } else if (work.steps.every(step => ["completed", "superseded"].includes(step.status))) work.status = "completed";
      return { workTaskId: work.id, threadId: work.threadId, step: admitted, decisionStepId: admitted?.id ?? null,
        stopReason: work.status === "completed" ? "completed" : null };
    }
    if (failure === "writer" && writerTask && path === `/api/projects/project/tasks/${writerTask.id}`
      && options.method === "PATCH") {
      writerTask.status = "failed";
      const work = [...runtime.workTasks.values()][0];
      work.status = "blocked";
      const step = work.steps.find(item => item.currentAttemptId === writerTask.id);
      step.status = "blocked";
      step.attempts[0].status = "failed";
      step.attempts[0].error = "writer format rejected";
      return writerTask;
    }
    if (failure === "writer" && writerTask && path === `/api/projects/project/tasks/${writerTask.id}`) return writerTask;
    if (failure === "lookup" && knowledgeReady && /\/work-tasks\/[^/]+$/.test(path)) {
      throw new Error("assessment lookup unavailable");
    }
    const result = await runtime.request(path, body);
    if (path.endsWith("/tasks") && options.method === "POST") {
      created.push(result);
      const work = [...runtime.workTasks.values()].find(item => item.steps.some(step => step.currentAttemptId === result.id));
      const step = work.steps.find(item => item.currentAttemptId === result.id);
      Object.assign(step, { operation: result.operation, status: "running" });
      if (!body.target?.resumeStepId) step.target = result.target;
      step.attempts = [{ id: result.id, threadId: result.threadId, status: "running" }];
      if (result.operation === "advisory" && !sourceTask) sourceTask = result;
      if (result.target?.workDecision) {
        result.inputManifest = { ...result.inputManifest, workDecision: { ...result.target.workDecision,
          rootThreadId: work.threadId, decisionStepId: step.id, decisionTaskId: result.id } };
      }
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
      step.attempts[0].status = "completed";
      if (completed.operation === "assessRevisionOutcome") {
        work.steps.push({ id: "final-decision-step", operation: "advisory", status: "queued", dependsOnStepId: step.id,
          instruction: "核对已保存结果后简洁报告", target: { workDecision: { workTaskId: work.id,
            originTaskId: sourceTask.id, sourceTaskId: writerTask.id } } });
      }
      if (completed.operation === "generateChapter") {
        work.steps.push({ id: "admitted-knowledge-step", operation: "modelKnowledge", status: "queued",
          instruction: "读取实际保存的新稿并提取知识", dependsOnStepId: step.id,
          target: { batchId: "batch-one", sourceTaskId: writerTask.id } });
      }
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
    if (failure === "writer" && this.activeOperation === "generateChapter" && !writerFailed) {
      writerFailed = true;
      throw new Error("writer format rejected");
    }
    return sendTurn.call(this, input, options);
  };
  return { handlers, adapter, runtime, calls, created, turns, state, exposeProposal: () => { proposalVisible = true; },
    get sourceTask() { return sourceTask; }, get writerTask() { return writerTask; },
    get modelingTask() { return modelingTask; }, get knowledgeReady() { return knowledgeReady; }, get assessmentStep() { return assessmentStep; } };
}

for (const failure of [null, "lookup", "turn"]) test(`production exact approval preserves ready knowledge when assessment ${failure ? `${failure} fails` : "succeeds"}`, async () => {
  const fixture = await approvalFixture(failure);
  const { handlers, adapter, runtime, calls, created, turns, state } = fixture;
  try {
    const event = rendererEvent();
    await handlers.get("agent:send-prompt")(event, "project", authorIntent);
    assert.deepEqual(created.map(task => task.operation), ["advisory"]);
    assert.equal(calls.some(call => call.path.endsWith("/approve")), false);
    fixture.exposeProposal();
    const result = await handlers.get("agent:approve-action")(event, "project", fixture.sourceTask.id, actionId);
    assert.equal(fixture.knowledgeReady, true);
    assert.equal(result.taskId, fixture.writerTask.id);
    assert.deepEqual(plain(result.action.target), target);
    assert.deepEqual(calls.find(call => call.path.endsWith(`/${actionId}/approve`)).body, { expectedIntentRevision: 0 });
    assert.deepEqual(calls.find(call => call.path.endsWith(`/${actionId}/finish`)).body, { resultTaskId: fixture.writerTask.id });

    const writerCreate = calls.find(call => call.path.endsWith("/tasks") && call.body?.operation === "generateChapter");
    assert.deepEqual(writerCreate.body.target, { ...target, requestedByTaskId: fixture.sourceTask.id, requestedActionId: actionId });
    assert.equal(writerCreate.body.initialBudget, undefined);
    assert.equal(calls.filter(call => call.path.endsWith("/modeling-batches")).length, 0, "Host must dispatch Native-admitted batch without choosing or creating another");
    assert.equal(calls.filter(call => call.path.endsWith("/next-step")).length, 0, "Host must not select a fixed followup");
    assert.equal(calls.filter(call => call.path.endsWith("/tasks") && call.body?.operation === "modelKnowledge").length, 1);
    assert.equal(turns.filter(turn => turn.operation === "modelKnowledge").length, 1, "assessment failure must not repeat knowledge modeling");
    assert.equal(runtime.workTasks.size, 1, "followups must keep the original work owner");
    assert.deepEqual(state.confirmations, [], "author submission and funded followups start directly");
    assert.deepEqual(turns.map(turn => turn.operation), failure === "lookup"
      ? ["advisory", "generateChapter", "modelKnowledge"]
      : failure === "turn" ? ["advisory", "generateChapter", "modelKnowledge", "assessRevisionOutcome"]
        : ["advisory", "generateChapter", "modelKnowledge", "assessRevisionOutcome", "advisory"]);

    assert.equal(result.candidateResults.some(item => item.taskId === fixture.writerTask.id), true, "Original saved writer must remain in the aggregate");
    if (failure) {
      assert.match(result.executionFailure?.message ?? "", new RegExp(`assessment ${failure} unavailable`));
      assert.equal(result.executionFailure?.workTaskId, [...runtime.workTasks.keys()][0]);
      assert.equal(result.candidateResults.some(item => item.taskId === fixture.modelingTask.id), true);
    } else {
      assert.equal(result.executionFailure ?? null, null);
      assert.equal(result.workTask.status, "completed");
      assert.notEqual(result.workflow.task.id, fixture.writerTask.id, "Final decision must not replace the approved writer's receipt identity");
      assert.equal(result.candidateResults.some(item => item.taskId === created.find(task => task.operation === "assessRevisionOutcome").id), true);
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
      const binding = prompt.match(/Novel Runtime任务绑定：\n(\{[^\n]+\})\n\n/);
      assert.ok(binding, "production runner must send the compact frozen assessment handoff");
      const handoff = JSON.parse(binding[1]);
      assert.equal(handoff.task.purpose, "critic");
      assert.deepEqual(handoff.task.target, expectedTarget);
      assert.equal(handoff.task.taskId, assessment.id);
      assert.equal(handoff.task.requiredOutputKind, "revisionOutcome");
      assert.equal(prompt.includes("byteBudget=60000"), false);

    }
  } finally { adapter.close(); }
});

test("production exact approval lets the original Work decide after its persisted initial writer failure", async () => {
  const fixture = await approvalFixture("writer");
  try {
    const event = rendererEvent();
    await fixture.handlers.get("agent:send-prompt")(event, "project", authorIntent);
    fixture.exposeProposal();
    const result = await fixture.handlers.get("agent:approve-action")(event, "project", fixture.sourceTask.id, actionId);
    assert.equal(result.taskId, fixture.writerTask.id);
    assert.equal(fixture.writerTask.status, "failed");
    assert.deepEqual(fixture.calls.find(call => call.path.endsWith(`/${actionId}/finish`)).body, { error: "writer format rejected" });
    assert.deepEqual(fixture.turns.map(turn => turn.operation), ["advisory", "generateChapter", "advisory"]);
    assert.equal(fixture.runtime.workTasks.size, 1);
    assert.equal(result.workTask.status, "completed");
    assert.deepEqual(plain(result.candidateResults), [], "A failed writer is not a saved result");
    assert.notEqual(result.workflow.task.id, fixture.writerTask.id);
    assert.equal(Object.hasOwn(result, "originalFailedTaskId"), false);
    assert.equal(fixture.calls.filter(call => call.path.endsWith("/approve")).length, 1);
    assert.equal(fixture.calls.filter(call => call.path.endsWith("/budget-authorization") && call.method === "POST").length, 0);
    assert.deepEqual(fixture.state.confirmations, []);
  } finally { fixture.adapter.close(); }
});
