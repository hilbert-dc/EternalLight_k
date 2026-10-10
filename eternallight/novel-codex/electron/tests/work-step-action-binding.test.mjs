import assert from "node:assert/strict";
import test from "node:test";
import { advanceWorkSteps } from "../work-step-continuation.mjs";

const sourceId = `task-${"a".repeat(32)}`, actionId = `action-${"b".repeat(32)}`;
function fixture(operation = "generateBrief", kind = "exactAction") {
  const target = operation === "modelKnowledge"
    ? { type: "chapter", chapterId: "chapter-one", chapterNumber: 1, sourceRevisionId: "draft-one" }
    : { type: "artifact" };
  const source = { id: sourceId, projectId: "project", threadId: "root-thread", operation: "advisory", status: "completed", inputRevision: 5 };
  const sourceStep = { id: "source-step", operation: "advisory", status: "completed", currentAttemptId: sourceId,
    attempts: [{ id: sourceId, threadId: source.threadId, status: "completed" }] };
  const step = { id: actionId, sourceTaskId: sourceId, operation, instruction: "执行原批准候选，不采用", target,
    inputRevision: 5, status: "queued", currentAttemptId: null, attempts: [], dependsOnStepId: sourceStep.id };
  const work = { id: "work-original", projectId: "project", threadId: source.threadId, status: "running", version: 4,
    goal: "原作者要求", steps: [sourceStep, step] };
  const action = { id: actionId, sourceTaskId: sourceId, operation, instruction: step.instruction,
    target: structuredClone(target), inputRevision: 5, intentRevision: 2, status: "queued", resultTaskId: null };
  const permission = { kind, workTaskId: work.id, actionId, scopePermitId: kind === "finiteScope" ? "scope-original" : null,
    grantedAt: 100, actionStatus: "queued", evidence: { authorMessageId: "author-message-original", projectId: "project",
      sourceTaskId: sourceId, intentRevision: 2, inputRevision: 5, operation, target: structuredClone(target),
      instruction: action.instruction, authorIntent: "原作者要求及实际追加要求" } };
  const workspace = { revision: 5, modeling: [{ objectId: "draft-chapter-one", sourceRevisionId: "draft-one" }] };
  const batch = { id: "batch-original", status: "extracting" }, calls = [], dispatched = [];
  let owner = work;
  const dependencies = {
    currentWorkFlowState: async () => work.status,
    runtimeRequest: async (path, options) => {
      calls.push({ path, body: options?.body ? JSON.parse(options.body) : null });
      if (path.endsWith("/decision")) { work.status = "completed"; return { workTaskId: work.id, threadId: work.threadId, step: null, stopReason: "completed" }; }
      if (path.endsWith("/creative-workspace")) return structuredClone(workspace);
      if (path.endsWith("/modeling-batches")) return structuredClone(batch);
      if (path.endsWith("/work-task")) return structuredClone(owner);
      if (path.endsWith("/actions")) return { actions: [structuredClone(action)] };
      if (path.endsWith("/permission")) return { permission: structuredClone(permission) };
      if (path.endsWith(`/tasks/${sourceId}`)) return structuredClone(source);
      assert.equal(path, `/api/projects/project/work-tasks/${work.id}`);
      return structuredClone(work);
    },
    runCreativeAgentTask: async (...args) => {
      dispatched.push(args); step.status = "completed"; return { workflow: { task: { id: "actual-child-attempt" } } };
    },
  };
  return { source, sourceStep, step, work, action, permission, workspace, batch, calls, dispatched, dependencies,
    replaceOwner: next => { owner = next; }, run: () => advanceWorkSteps({}, "project", work.id, dependencies) };
}

for (const kind of ["exactAction", "finiteScope"]) test(`restores only the exact original ${kind} action binding`, async () => {
  const f = fixture("generateBrief", kind);
  assert.equal((await f.run()).status, "completed");
  assert.deepEqual(f.dispatched[0].slice(1), ["project", f.step.instruction, { operation: "generateBrief" },
    { ...f.action.target, requestedByTaskId: sourceId, requestedActionId: actionId }]);
  assert.equal(f.calls.some(call => /\/approve$|\/control$|\/budget-authorization$|\/finish$/.test(call.path)), false);
  assert.equal(f.dispatched.length, 1);
});

test("current chapter action establishes its exact source batch without confusing it with resumeStepId", async () => {
  const f = fixture("modelKnowledge"); await f.run();
  assert.deepEqual(f.dispatched[0][4], { ...f.action.target, requestedByTaskId: sourceId, requestedActionId: actionId, batchId: f.batch.id });
  assert.deepEqual(f.calls.find(call => call.path.endsWith("/modeling-batches")).body, {
    objectId: "draft-chapter-one", sourceRevisionId: "draft-one", expectedRevision: 5,
    extractorVersion: `novel-knowledge-v1:${actionId}`,
  });
});

test("rejects foreign Work, source, thread, action, target and permission before any model or new grant", async () => {
  const mutations = [
    f => f.replaceOwner({ ...f.work, id: "work-foreign" }),
    f => f.replaceOwner({ ...f.work, projectId: "foreign" }),
    f => f.replaceOwner({ ...f.work, threadId: "foreign-thread" }),
    f => { f.source.id = "foreign-source"; },
    f => { f.source.projectId = "foreign"; },
    f => { f.source.threadId = "foreign-thread"; },
    f => { f.sourceStep.attempts[0].threadId = "foreign-thread"; },
    f => { f.source.status = "running"; },
    f => { f.action.id = "foreign-action"; },
    f => { f.action.sourceTaskId = "foreign-source"; },
    f => { f.action.operation = "generateOutline"; },
    f => { f.action.target = { type: "project" }; },
    f => { f.action.instruction = "未知指令"; },
    f => { f.action.inputRevision++; },
    f => { f.action.status = "running"; },
    f => { f.action.resultTaskId = "already-attempted"; },
    f => { f.permission.workTaskId = "foreign-work"; },
    f => { f.permission.actionId = "foreign-action"; },
    f => { f.permission.actionStatus = "completed"; },
    f => { f.permission.evidence.sourceTaskId = "foreign-source"; },
    f => { f.permission.evidence.operation = "generateOutline"; },
    f => { f.permission.evidence.target = { type: "project" }; },
    f => { f.permission.evidence.intentRevision++; },
    f => { f.permission.evidence.authorIntent = ""; },
    f => { f.permission.kind = "unknown-permission"; },
    f => { f.permission.scopePermitId = "unexpected-scope"; },
    f => { f.step.currentAttemptId = "already-attempted"; },
    f => { f.step.attempts.push({ id: "already-attempted" }); },
    f => { const owner = structuredClone(f.work); owner.steps[1].attempts.push({ id: "concurrently-attached" }); f.replaceOwner(owner); },
  ];
  for (const mutate of mutations) {
    const f = fixture(); mutate(f); await assert.rejects(f.run(), { code: "WORK_ACTION_BINDING_INVALID" });
    assert.deepEqual(f.dispatched, []); assert.equal(f.calls.some(call => call.body), false);
  }
});

test("stale or already-saved chapter analysis never sends the model again", async () => {
  for (const change of [f => { f.workspace.revision++; }, f => { f.workspace.modeling[0].sourceRevisionId = "different-draft"; },
    f => { f.batch.status = "ready"; }]) {
    const f = fixture("modelKnowledge"); change(f);
    await assert.rejects(f.run(), { code: "WORK_ACTION_BINDING_INVALID" }); assert.deepEqual(f.dispatched, []);
  }
});

test("a Native model decision retains its current action target and resume identity", async () => {
  const f = fixture(); f.step.target = { type: "artifact", workDecisionAction: {
    workTaskId: f.work.id, decisionTaskId: sourceId, decisionStepId: f.sourceStep.id, actionKey: "brief" } };
  await f.run(); assert.deepEqual(f.dispatched[0][4], { ...f.step.target, resumeStepId: actionId });
  assert.equal(f.calls.some(call => call.path.includes(`/tasks/${sourceId}`)), false);
});
