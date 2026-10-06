import test from "node:test";
import assert from "node:assert/strict";
import { executeConversationPlanningReviews, registerPlanningReviewReadHandler } from "../conversation-planning-reviews.mjs";
import { readPlanningReviewContinuation } from "../planning-review-resumption.mjs";

function fixture() {
  const source = { id: "task-source", projectId: "project", operation: "advisory", inputRevision: 4 };
  const run = { id: "planning-review-one", projectId: "project", workTaskId: "work-dialogue", sourceTaskId: source.id,
    sourceIntentRevision: 1, authorIntent: "请从人物和结构分别检查第二卷", authorMessageId: "author-original", focus: "保留结局",
    artifactKind: "outline", revisionId: "outline-original", inputRevision: 4, status: "running", roles: ["structure", "character"], reports: [],
    seats: ["structure", "character"].map(role => ({ role, taskId: null, status: "queued" })) };
  const work = { id: run.workTaskId, projectId: source.projectId, kind: "conversation", goal: "最初的对话要求", status: "running", steps: [
    { id: "dialogue", operation: "advisory", currentAttemptId: source.id, status: "completed", target: {} },
    ...run.roles.map(role => ({ id: `review-seat-${run.id}-${role}`, operation: "reviewPlanningSeat", status: "queued", inputRevision: 4,
      target: { type: "artifact", reviewRunId: run.id, role, artifactKind: run.artifactKind, revisionId: run.revisionId } })),
    { id: "unrelated", operation: "generateBrief", status: "queued", target: {} },
  ] };
  const calls = [];
  const dependencies = {
    currentWorkFlowState: async () => "running",
    runtimeRequest: async (path, options) => {
      calls.push({ path, options });
      if (path.endsWith(`/tasks/${source.id}/planning-reviews`)) return { runs: [structuredClone(run)] };
      if (path.endsWith(`/work-tasks/${work.id}`)) return structuredClone(work);
      if (path.endsWith(`/planning-reviews/${run.id}`)) return structuredClone(run);
      if (path.endsWith("/finalize")) { run.status = "completed"; return structuredClone(run); }
      assert.fail(path);
    },
    notifyReviewStart: params => calls.push({ started: params }),
    runPlanningReview: async (_event, projectId, exact, continuation) => {
      assert.equal(projectId, source.projectId); assert.deepEqual(exact, run); assert.deepEqual(continuation.roles, run.roles);
      calls.push({ execute: exact.id, owner: exact.workTaskId });
      run.status = "completed"; run.reports = run.roles.map(role => ({ id: `report-${role}`, role }));
      return { run: structuredClone(run) };
    },
  };
  return { source, run, work, calls, dependencies, discussion: { message: "正在按你指定的范围安排检查。", workflow: { task: source } } };
}

test("same-send deep review runs only registered seats in the original conversation owner", async () => {
  const f = fixture();
  const result = await executeConversationPlanningReviews({}, "project", f.discussion, f.dependencies);
  assert.match(result.message, /2\/2/);
  assert.equal(result.planningReviews[0].id, f.run.id);
  assert.deepEqual(f.calls.filter(item => item.execute), [{ execute: f.run.id, owner: f.work.id }]);
  assert.equal(f.work.steps.at(-1).status, "queued");
  assert.equal(f.calls.filter(item => item.options?.method).length, 0, "host must not register another run or budget");
});

test("foreign source, owner, role source, or live parent never launches seats", async () => {
  for (const damage of [f => { f.run.sourceTaskId = "task-other"; }, f => { f.run.inputRevision++; },
    f => { f.work.projectId = "other"; }, f => { f.work.steps[0].status = "running"; },
    f => { f.work.steps[1].target.revisionId = "newer-outline"; }, f => { f.run.workTaskId = "other-owner"; },
    f => { delete f.run.sourceIntentRevision; }, f => { f.run.sourceIntentRevision = -1; }]) {
    const f = fixture(); damage(f);
    await assert.rejects(executeConversationPlanningReviews({}, "project", f.discussion, f.dependencies));
    assert.deepEqual(f.calls.filter(item => item.execute), []);
  }
});

test("completed reports finalize without a model; pause and unknown receipts never replay", async () => {
  const f = fixture();
  for (let index = 0; index < f.run.roles.length; index++) {
    const taskId = `task-seat-${index}`;
    Object.assign(f.work.steps[index + 1], { status: "completed", currentAttemptId: taskId });
    Object.assign(f.run.seats[index], { taskId, status: "completed" });
  }
  await executeConversationPlanningReviews({}, "project", f.discussion, f.dependencies);
  assert.equal(f.calls.filter(item => item.path?.endsWith("/finalize")).length, 1);
  assert.deepEqual(f.calls.filter(item => item.execute), []);
  const paused = fixture(); paused.dependencies.currentWorkFlowState = async () => "paused";
  assert.deepEqual((await executeConversationPlanningReviews({}, "project", paused.discussion, paused.dependencies)).planningReviews, []);
  const failed = fixture(); failed.dependencies.runPlanningReview = async () => { throw Error("saved report receipt unavailable"); };
  await assert.rejects(executeConversationPlanningReviews({}, "project", failed.discussion, failed.dependencies), /receipt unavailable/);
  assert.equal(failed.calls.filter(item => item.started).length, 1);
});

test("conversation continuation requires an explicit run and completed original author turn", async () => {
  const f = fixture();
  await assert.rejects(readPlanningReviewContinuation(f.dependencies.runtimeRequest, "project", f.work), /无效/);
  const continuation = await readPlanningReviewContinuation(f.dependencies.runtimeRequest, "project", f.work, f.run.id);
  assert.deepEqual(continuation.roles, ["structure", "character"]);
});

test("saved report IPC only reads an exact project resource", async () => {
  let handler; const calls = [];
  registerPlanningReviewReadHandler({ handle: (_name, value) => { handler = value; } }, async path => { calls.push(path); return {}; });
  await handler({}, "project", "planning-review-one");
  assert.deepEqual(calls, ["/api/projects/project/planning-reviews/planning-review-one"]);
  for (const id of ["../other", "", "bad\nkey"]) assert.throws(() => handler({}, "project", id));
});
