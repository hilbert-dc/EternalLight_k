import test from "node:test";
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { mainHandlers, rendererEvent } from "./main-handler-harness.mjs";
import { readPlanningReviewContinuation } from "../planning-review-resumption.mjs";
import { reviewTask } from "./fixtures/planning-review-task.mjs";

function reviewFixture() {
  const run = { sourceTaskId: null, sourceIntentRevision: null, focus: "", id: "planning-review-fixture", workTaskId: "work-persisted-review-owner", projectId: "project", roles: ["structure", "character"],
    status: "running", artifactKind: "outline", revisionId: "outline-one", inputRevision: 0,
    authorIntent: "检查规划", authorMessageId: "author-review", sourcePacketHash: "a".repeat(64), seats: [{ role: "structure", taskId: "task-first", status: "completed" },
      { role: "character", taskId: "task-old", status: "failed" }] };
  const work = { id: run.workTaskId, projectId: run.projectId, threadId: "review-root", kind: "direct", goal: run.authorIntent,
    status: "blocked", version: 2, controlRevision: 0, steps: run.roles.map((role, index) => ({
      id: `review-seat-${run.id}-${role}`, operation: "reviewPlanningSeat", inputRevision: 0,
      status: index ? "blocked" : "completed", currentAttemptId: index ? "task-old" : "task-first",
      target: { type: "artifact", artifactKind: run.artifactKind, revisionId: run.revisionId, reviewRunId: run.id, role },
    })) };
  return { run, work };
}

for (const outcome of ["resume", "finalize", "finalizePaused", "settlementFailure", "paused", "conversationResume", "conversationFinalize"]) {
  test(`production review recovery keeps original run and skips completed seats: ${outcome}`, async () => {
    const { run, work } = reviewFixture();
    const reviewSteps = [...work.steps];
    if (outcome.startsWith("conversation")) {
      run.sourceTaskId = "task-source"; run.sourceIntentRevision = 1;
      work.kind = "conversation";
      work.steps.unshift({ id: "original-dialogue", operation: "advisory", currentAttemptId: run.sourceTaskId, status: "completed", target: {} });
    }
    const state = { confirmations: [], budgets: [] };
    const deadlineAt = Math.floor(Date.now() / 1000) + 600;
    let pending = { taskId: "task-old", status: "failed" };
    let settled = 0;
    let created = 0;
    let finalized = 0;
    let controls = 0;
    const replies = new Map();
    const onlyFinalize = outcome.startsWith("finalize") || outcome === "conversationFinalize";
    if (onlyFinalize) {
      pending = null; work.status = outcome === "finalizePaused" ? "paused" : "blocked";
      reviewSteps[1].status = "completed"; run.seats[1].status = "completed";
    }
    if (outcome === "paused") work.status = "paused";
    const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));
    const { handlers, adapter } = await mainHandlers(async (path, body, options) => {
      if (path.endsWith("/final-reply")) {
        const taskId = path.split("/").at(-2);
        if (body) replies.set(taskId, { taskId, projectId: run.projectId, binding: body });
        return replies.get(taskId) ?? null;
      }
      if (path.endsWith("/provider-usage")) {
        if (!body) return { pending };
        if (outcome === "settlementFailure") throw new Error("receipt unavailable");
        settled++; pending = null; work.version++; return {};
      }
      if (path.endsWith("/budget-authorization")) {
        assert.equal(body, null, "must not grant a new allowance");
        return { authorization: { deadlineAt } };
      }
      if (path.endsWith("/control")) {
        assert.deepEqual({ expectedControlRevision: body.expectedControlRevision, command: body.command }, { expectedControlRevision: work.controlRevision, command: "resume" });
        controls++; work.status = "running";
        if (!onlyFinalize) { reviewSteps[1].status = "queued"; reviewSteps[1].currentAttemptId = null; }
        work.version++; work.controlRevision++; return work;
      }
      if (path.endsWith(`/work-tasks/${work.id}`) || path.endsWith("/work-task")) return work;
      if (path.endsWith(`/planning-reviews/${run.id}`)) return run;
      if (path.endsWith("/decision")) { work.status = "completed"; return { workTaskId: work.id, threadId: work.threadId, step: null, stopReason: "completed" }; }
      if (path.endsWith("/planning-reviews")) assert.fail("must not create another review run");
      if (path.endsWith("/creative-workspace")) return { revision: 0, headSnapshotId: "snapshot", sourceDigest: "source" };
      if (path.endsWith("/tasks")) {
        assert.ok(path.endsWith("/seats/character/tasks"), "completed structure seat cannot be rerun");
        created++;
        const task = reviewTask(run, "character", body, "task-retried");
        reviewSteps[1].currentAttemptId = task.id; reviewSteps[1].status = "running";
        run.seats[1] = { role: "character", taskId: task.id, status: "queued" }; return task;
      }
      if (path.endsWith("/provider-budget")) return { taskId: "task-retried", workTaskId: work.id,
        budgetId: "budget-task-retried", maxRequests: 2, maxTokens: 800, deadlineAt };
      if (path.endsWith("/complete")) {
        reviewSteps[1].status = "completed"; work.status = "completed";
        run.seats[1].status = "completed"; return {};
      }
      if (path.endsWith("/finalize")) { finalized++; run.status = "completed"; work.status = "completed"; return run; }
      if (path.endsWith("/cancel") || options?.method === "PATCH") assert.fail("successful recovery cannot cancel/fail the original work");
      return {};
    }, `${process.execPath} ${fixture}`, state);
    try {
      const result = handlers.get("agent:resume-work-task")(rendererEvent(), "project", work.id);
      if (outcome === "settlementFailure") {
        await assert.rejects(result, /receipt unavailable/);
        assert.deepEqual([created, finalized, controls, settled], [0, 0, 0, 0]);
      } else {
        assert.equal((await result).id, work.id);
        assert.deepEqual([created, finalized, controls, settled], onlyFinalize ? [0, 1, outcome === "finalizePaused" ? 1 : 0, 0] : [1, 1, 1, 2]);
      }
      assert.equal(state.confirmations.length, 0);
      assert.equal(state.budgets.length, onlyFinalize || outcome === "settlementFailure" ? 0 : 1);
      assert.equal(reviewSteps[0].currentAttemptId, "task-first");
    } finally { adapter.close(); }
  });
}

test("review continuation rejects cross-owner, mixed steps and live attempts before model execution", async () => {
  for (const change of [
    (run) => { delete run.authorMessageId; },
    (run) => { delete run.workTaskId; },
    (run) => { run.workTaskId = "work-other-owner"; },
    (run) => { run.authorIntent = "host wrapper"; },
    (run) => { run.projectId = "other"; },
    (_run, work) => { work.steps[1].operation = "generateChapter"; },
    (_run, work) => { work.steps[1].target.revisionId = "other-outline"; },
    (_run, work) => { work.steps[1].status = "running"; },
    (_run, work) => { work.steps[0].currentAttemptId = "different-attempt"; },
  ]) {
    const { run, work } = reviewFixture();
    work.status = "running"; work.steps[1].status = "queued";
    change(run, work);
    await assert.rejects(readPlanningReviewContinuation(async () => run, "project", work), /无效|不能重跑/);
  }
});


test("one requested review view resumes without adding other seats", async () => {
  const {run,work}=reviewFixture();
  run.roles=["character"]; run.seats=[{role:"character",taskId:null,status:"queued"}];
  work.steps=[work.steps[1]]; work.steps[0].status="queued"; work.steps[0].currentAttemptId=null; work.status="running";
  const calls=[];
  const result=await readPlanningReviewContinuation(async path=>{ calls.push(path); return run; },"project",work,run.id);
  assert.deepEqual(result.roles,["character"]);
  assert.equal(calls.length,1);
  assert.equal(result.run,run);
});

function savedReviewFixture(kind, status) {
  const { run, work } = reviewFixture();
  work.kind = kind;
  work.status = status;
  work.resumeMode = "recoverSavedResult";
  work.resumeBlockReason = "unknownUsage";
  for (const step of work.steps) step.status = "completed";
  for (const seat of run.seats) seat.status = "completed";
  if (kind === "conversation") {
    run.sourceTaskId = "task-source";
    run.sourceIntentRevision = 1;
    work.steps.unshift({ id: "original-dialogue", operation: "advisory", currentAttemptId: run.sourceTaskId,
      status: "completed", target: {}, attempts: [{ id: run.sourceTaskId, status: "completed", threadId: work.threadId }] });
  }
  return { run, work };
}

async function savedRecoveryHost(work, { resultStatus = "completed", cancelledBeforeControl = false } = {}) {
  const requests = [];
  const state = { confirmations: [], budgets: [], leases: [] };
  const event = rendererEvent();
  state.rendererWebContents = event.sender;
  const path = `/api/projects/${work.projectId}/work-tasks/${work.id}`;
  const readVersion = work.version;
  const sourceTask = { id: "task-source", projectId: work.projectId, threadId: work.threadId,
    operation: "advisory", status: "completed", target: {} };
  // This is the current durable handoff shape accepted by executeWorkContinuation.
  // A saved-result-only action must not read or consume it, even after completion.
  const handoff = work.kind === "conversation" ? {
    task: sourceTask,
    proposal: { proposalId: "generation-proposal-source", projectId: work.projectId,
      sourceTaskId: sourceTask.id, threadId: work.threadId,
      resumeRun: { id: `generation-${"a".repeat(32)}`, expectedVersion: 4 } },
  } : null;
  const main = await mainHandlers(async (requestPath, body, options) => {
    requests.push({ path: requestPath, body, method: options?.method ?? "GET" });
    if (requestPath === path && !body) return structuredClone(work);
    if (requestPath === `${path}/provider-usage` && !body) {
      if (cancelledBeforeControl) {
        work.status = "cancelled";
        work.resumeMode = null;
        work.resumeBlockReason = null;
        work.version++;
      }
      return { pending: null };
    }
    if (requestPath === `${path}/control`) {
      assert.equal(options?.method, "POST");
      assert.deepEqual({ expectedControlRevision: body.expectedControlRevision, command: body.command }, { expectedControlRevision: work.controlRevision, command: "recoverSavedResult" });
      if (cancelledBeforeControl) throw Object.assign(new Error("原任务状态已变化，尚未核对保存结果。"), { code: "work_task_stale" });
      work.version++;
      work.controlRevision++;
      work.status = resultStatus;
      work.resumeMode = null;
      work.resumeBlockReason = resultStatus === "completed" ? null : "unknownUsage";
      for (const step of work.steps) {
        step.status = "completed";
        for (const attempt of step.attempts ?? []) {
          if (attempt.id === step.currentAttemptId) attempt.status = "completed";
        }
      }
      return structuredClone(work);
    }
    if (requestPath === `${path}/generation-handoff` && !body) return { handoff };
    assert.fail(`Saved-result-only recovery must not request ${requestPath}`);
  }, `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`, state);
  const modelCalls = [];
  main.adapter.ensureSession = async () => {
    modelCalls.push("ensureSession");
    assert.fail("Saved-result-only recovery must not initialize a model session");
  };
  main.adapter.request = async method => {
    modelCalls.push(method);
    assert.fail(`Saved-result-only recovery must not send ${method}`);
  };
  return { main, event, requests, state, modelCalls, path };
}

function assertSavedRecoveryOnly(fixture) {
  const { requests, path, state, modelCalls } = fixture;
  assert.equal(requests.filter(call => call.method === "POST").length, 1);
  assert.equal(requests.filter(call => call.path === `${path}/control`).length, 1);
  assert.ok(requests.every(call => call.path === `${path}/control`
    || call.method === "GET" && [path, `${path}/provider-usage`].includes(call.path)));
  assert.deepEqual(modelCalls, []);
  assert.deepEqual(state.confirmations, []);
  assert.deepEqual(state.budgets, []);
  assert.deepEqual(state.leases, []);
  assert.deepEqual(state.ownerRequests, []);
}

for (const kind of ["direct", "conversation"]) {
  for (const status of ["paused", "blocked"]) {
    test(`explicit saved review recovery performs only Native control: ${kind}/${status}`, async () => {
      const { work } = savedReviewFixture(kind, status);
      const originalSteps = structuredClone(work.steps);
      const fixture = await savedRecoveryHost(work);
      try {
        const result = await fixture.main.handlers.get("agent:recover-work-task")(fixture.event, work.projectId, work.id);
        assert.deepEqual(JSON.parse(JSON.stringify(result)), work);
        assert.equal(result.status, "completed");
        assert.deepEqual(work.steps, originalSteps);
        assertSavedRecoveryOnly(fixture);
      } finally { fixture.main.adapter.close(); }
    });
  }
}

test("saved review aggregation preserves a paused unfinished conversation without a follow-up", async () => {
  const { work } = savedReviewFixture("conversation", "paused");
  const fixture = await savedRecoveryHost(work, { resultStatus: "paused" });
  try {
    const result = await fixture.main.handlers.get("agent:recover-work-task")(fixture.event, work.projectId, work.id);
    assert.equal(result.status, "paused");
    assert.equal(result.resumeBlockReason, "unknownUsage");
    assert.equal(result.resumeMode, null);
    assertSavedRecoveryOnly(fixture);
  } finally { fixture.main.adapter.close(); }
});

test("known-usage modeling saved-result recovery keeps its original no-model handler path", async () => {
  const { work } = savedReviewFixture("conversation", "blocked");
  work.resumeBlockReason = null;
  work.steps = [work.steps[0], { id: "knowledge-step", operation: "modelKnowledge", status: "blocked",
    currentAttemptId: "task-model", target: { batchId: "batch-published", chapterId: "chapter-one" },
    attempts: [{ id: "task-model", status: "failed", threadId: work.threadId }] }];
  const fixture = await savedRecoveryHost(work);
  try {
    const result = await fixture.main.handlers.get("agent:recover-work-task")(fixture.event, work.projectId, work.id);
    assert.equal(result.status, "completed");
    assert.equal(result.id, work.id);
    assertSavedRecoveryOnly(fixture);
  } finally { fixture.main.adapter.close(); }
});

test("explicit saved-result recovery cannot substitute another Native mode or blocked reason", async () => {
  for (const [mode, reason] of [[null, "unknownUsage"], ["continueExecution", null],
    [null, "pendingRecovery"], [null, "inputChanged"], [null, "partialOutput"]]) {
    const { work } = savedReviewFixture("conversation", "paused");
    work.resumeMode = mode;
    work.resumeBlockReason = reason;
    const before = structuredClone(work);
    const fixture = await savedRecoveryHost(work);
    try {
      await assert.rejects(fixture.main.handlers.get("agent:recover-work-task")(fixture.event, work.projectId, work.id), /恢复条件已变化/);
      assert.deepEqual(work, before);
      assert.deepEqual(fixture.requests.map(call => [call.path, call.method]), [[fixture.path, "GET"]]);
      assert.deepEqual(fixture.modelCalls, []);
      assert.deepEqual(fixture.state.budgets, []);
    } finally { fixture.main.adapter.close(); }
  }
});

test("concurrent Native cancellation rejects saved-result control without substituting continuation", async () => {
  const { work } = savedReviewFixture("conversation", "paused");
  const fixture = await savedRecoveryHost(work, { cancelledBeforeControl: true });
  try {
    await assert.rejects(fixture.main.handlers.get("agent:recover-work-task")(fixture.event, work.projectId, work.id), /原任务状态已变化/);
    assert.equal(work.status, "cancelled");
    assert.equal(work.resumeMode, null);
    assert.equal(work.resumeBlockReason, null);
    assertSavedRecoveryOnly(fixture);
  } finally { fixture.main.adapter.close(); }
});

for (const status of ["paused", "blocked"]) {
  test(`automatic production resume cannot invoke author saved-result recovery: ${status}`, async () => {
    const { work } = savedReviewFixture("conversation", status);
    const before = structuredClone(work);
    const fixture = await savedRecoveryHost(work);
    try {
      const result = await fixture.main.workExecutionConsumer.resume(work.projectId, work.id,
        { origin: "automatic", expectedThreadId: work.threadId });
      assert.deepEqual(JSON.parse(JSON.stringify(result)), before);
      assert.deepEqual(work, before);
      assert.deepEqual(fixture.requests.map(call => [call.path, call.method]), [[fixture.path, "GET"]]);
      assert.deepEqual(fixture.modelCalls, []);
      assert.deepEqual(fixture.state.budgets, []);
      assert.deepEqual(fixture.state.confirmations, []);
      assert.deepEqual(fixture.state.ownerRequests, []);
    } finally { fixture.main.adapter.close(); }
  });
}
