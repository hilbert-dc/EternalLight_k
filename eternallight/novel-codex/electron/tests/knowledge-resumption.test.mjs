import assert from "node:assert/strict";
import { test } from "node:test";
import { mainHandlers, rendererEvent } from "./main-handler-harness.mjs";

for (const [status, stepStatus, reason] of [
  ["paused", "queued", null],
  ["blocked", "blocked", null],
  ["paused", "queued", "attemptBudget"],
]) test(`chapter analysis keeps its existing work and authorization: ${status}/${reason}`, async () => {
  const work = { id: "work-source", projectId: "project", kind: "direct", status, version: 3,
    resumeMode: "continueExecution", resumeBlockReason: reason,
    steps: [{ operation: "modelKnowledge", status: stepStatus, target: { sourceTaskId: "source" } }] };
  const requests = [];
  const state = { budgets: [], confirmations: [] };
  const { handlers, adapter } = await mainHandlers(async (path, body) => {
    requests.push({ path, body });
    if (path.endsWith("/chapters/chapter/next-step")) return { action: {
      kind: "modelKnowledge", chapterId: "chapter", sourceRevisionId: "exact-source", sourceTaskId: "source",
    } };
    if (path.endsWith("/tasks/source/work-task") || path.endsWith("/work-tasks/work-source")) return work;
    if (path.endsWith("/provider-usage")) return { pending: null };
    if (path.endsWith("/budget-authorization")) return { authorization: { deadlineAt: null } };
    if (path.endsWith("/control")) {
      assert.deepEqual(body, { expectedVersion: 3, command: "resume" });
      throw new Error("Existing work reached authoritative resume validation");
    }
    assert.fail(`Chapter analysis escaped its existing work: ${path}`);
  }, `${process.execPath} unused-fixture-engine`, state);
  try {
    await assert.rejects(handlers.get("agent:resume-knowledge")(rendererEvent(), "project", "chapter"),
      /Existing work reached authoritative resume validation/);
    assert.equal(requests.filter(({ path }) => path.endsWith("/control")).length, 1);
    assert.deepEqual(state.budgets, []);
    assert.deepEqual(state.confirmations, []);
    assert.equal(adapter.process, null);
  } finally { adapter.close(); }
});
