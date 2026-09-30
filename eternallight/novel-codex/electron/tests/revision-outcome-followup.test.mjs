import assert from "node:assert/strict";
import { test } from "node:test";
import { assessQueuedRevision } from "../revision-outcome-followup.mjs";

function fixture() {
  const calls = [];
  const target = { writerTaskId: "writer", chapterId: "one", draftRevisionId: "draft", knowledgeBatchId: "batch" };
  const work = { projectId: "project", status: "running", steps: [
    { id: "model", status: "completed" },
    { id: "outcome", operation: "assessRevisionOutcome", status: "queued", dependsOnStepId: "model", target },
  ] };
  const input = { event: {}, projectId: "project", writerTask: { id: "writer", target: { desiredOutcomes: [{ id: "hide" }] } },
    currentWorkFlowState: async () => "running",
    runtimeRequest: async path => { calls.push(["read", path]); return work; },
    runCreativeAgentTask: async (...args) => { calls.push(["execute", ...args]); return { workflow: { task: { id: "assessment" } } }; },
  };
  return { input, calls, work, target };
}

test("executes only the repository's completed-predecessor target", async () => {
  const { input, calls, target } = fixture();
  assert.deepEqual(await assessQueuedRevision(input), { status: "recorded", taskId: "assessment" });
  assert.equal(calls[0][1], "/api/projects/project/tasks/writer/work-task");
  assert.deepEqual(calls[1].slice(-2), [{ operation: "assessRevisionOutcome" }, { ...target, resumeStepId: "outcome" }]);
  assert.equal(Object.hasOwn(calls[1].at(-1), "sourceTaskId"), false);
});

test("unrequested goals and paused flows do not read or execute", async () => {
  for (const state of ["absent", "paused", "cancelled"]) {
    const { input, calls } = fixture();
    if (state === "absent") input.writerTask.target = {};
    else input.currentWorkFlowState = async () => state;
    assert.deepEqual(await assessQueuedRevision(input), state === "absent" ? null : { status: state === "paused" ? "paused" : "pending" });
    assert.deepEqual(calls, []);
  }
});

test("missing, cross-project, or incomplete predecessors cannot synthesize execution", async () => {
  for (const mutation of [work => { work.steps.pop(); }, work => { work.projectId = "other"; },
    work => { work.steps[0].status = "running"; }, work => { work.steps[1].target.writerTaskId = "other"; }]) {
    const { input, calls, work } = fixture(); mutation(work);
    assert.deepEqual(await assessQueuedRevision(input), { status: "pending" });
    assert.equal(calls.length, 1);
  }
});

test("assessment lookup and execution errors stay distinct from successful knowledge modeling", async () => {
  for (const failing of ["runtimeRequest", "runCreativeAgentTask"]) {
    const { input } = fixture();
    input[failing] = async () => { throw new Error("assessment unavailable"); };
    assert.deepEqual(await assessQueuedRevision(input), { status: "pending", error: "assessment unavailable" });
  }
});

test("task cancellation preserves the host cancellation signal", async () => {
  const { input } = fixture(); const cancelled = Object.assign(new Error("cancelled"), { code: "TASK_CANCELLED" });
  input.runCreativeAgentTask = async () => { throw cancelled; };
  await assert.rejects(assessQueuedRevision(input), error => error === cancelled);
});
