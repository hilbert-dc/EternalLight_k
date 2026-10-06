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
    status: "blocked", version: 2, steps: run.roles.map((role, index) => ({
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
        assert.deepEqual(body, { expectedVersion: work.version, command: "resume" });
        controls++; work.status = "running";
        if (!onlyFinalize) { reviewSteps[1].status = "queued"; reviewSteps[1].currentAttemptId = null; }
        work.version++; return work;
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
