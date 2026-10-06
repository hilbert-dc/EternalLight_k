import assert from "node:assert/strict";
import { test } from "node:test";
import { mainHandlers, rendererEvent } from "./main-handler-harness.mjs";

for (const remainingWork of [false, true]) test(`offline saved-result recovery never starts model work (${remainingWork ? "follow-up paused" : "finished"})`, async () => {
  const requests = [];
  const state = { confirmations: [], budgets: [], leases: [] };
  const work = { id: "work-saved", projectId: "project", kind: "direct", status: "blocked", version: 7,
    resumeMode: "recoverSavedResult", resumeBlockReason: null, steps: [{ id: "model", operation: "modelKnowledge", status: "blocked" }] };
  const restored = { ...work, status: remainingWork ? "paused" : "completed", version: 9,
    resumeMode: remainingWork ? "continueExecution" : null, steps: [{ ...work.steps[0], status: "completed" }] };
  const { handlers, adapter } = await mainHandlers(async (path, body) => {
    requests.push({ path, body });
    if (path.endsWith("/provider-usage")) return { pending: null };
    if (path.endsWith("/control")) {
      assert.deepEqual(body, { expectedVersion: 7, command: "recoverSavedResult" });
      return restored;
    }
    if (path.endsWith("/work-tasks/work-saved")) return work;
    assert.fail(`Recovery attempted another operation: ${path}`);
  }, null, state);
  try {
    assert.equal(adapter.enabled, false);
    assert.deepEqual(await handlers.get("agent:recover-work-task")(rendererEvent(), "project", work.id), restored);
    assert.deepEqual(requests.map(({ path }) => path), [
      "/api/projects/project/work-tasks/work-saved",
      "/api/projects/project/work-tasks/work-saved/provider-usage",
      "/api/projects/project/work-tasks/work-saved/control",
    ]);
    assert.deepEqual(state.confirmations, []);
    assert.deepEqual(state.budgets, []);
    assert.deepEqual(state.leases, []);
    assert.equal(adapter.process, null);
  } finally { adapter.close(); }
});

test("offline execution fails before changing task state or requesting new funds", async () => {
  const work = { id: "work-paused", projectId: "project", kind: "direct", status: "paused", version: 3,
    resumeMode: "continueExecution", steps: [{ operation: "advisory", status: "queued" }] };
  const requests = [];
  const state = { confirmations: [], budgets: [] };
  const { handlers, adapter } = await mainHandlers(async (path) => {
    requests.push(path);
    if (path.endsWith("/provider-usage")) return { pending: null };
    if (path.endsWith("/work-tasks/work-paused")) return work;
    assert.fail(`Offline execution mutated state: ${path}`);
  }, null, state);
  try {
    await assert.rejects(handlers.get("agent:resume-work-task")(rendererEvent(), "project", work.id), /需要可用模型/);
    assert.equal(work.status, "paused");
    assert.equal(requests.length, 2);
    assert.deepEqual(state.confirmations, []);
    assert.deepEqual(state.budgets, []);
  } finally { adapter.close(); }
});

test("stale local recovery fails closed without falling through to model retry", async () => {
  const work = { id: "work-stale", projectId: "project", kind: "direct", status: "blocked", version: 7,
    resumeMode: "recoverSavedResult", steps: [] };
  const state = { confirmations: [], budgets: [] };
  const { handlers, adapter } = await mainHandlers(async (path) => {
    if (path.endsWith("/provider-usage")) return { pending: null };
    if (path.endsWith("/control")) throw new Error("WORK_TASK_STALE");
    if (path.endsWith("/work-tasks/work-stale")) return work;
    assert.fail(`Stale recovery retried work: ${path}`);
  }, null, state);
  try {
    await assert.rejects(handlers.get("agent:recover-work-task")(rendererEvent(), "project", work.id), /WORK_TASK_STALE/);
    assert.equal(work.status, "blocked");
    assert.deepEqual(state.confirmations, []);
    assert.deepEqual(state.budgets, []);
  } finally { adapter.close(); }
});

test("a local-recovery click cannot become paid execution after task conditions change", async () => {
  const work = { id: "work-changed", projectId: "project", kind: "direct", status: "paused", version: 8,
    resumeMode: "continueExecution", steps: [{ operation: "advisory", status: "queued" }] };
  const state = { confirmations: [], budgets: [], leases: [] };
  const { handlers, adapter } = await mainHandlers(async (path) => {
    if (path.endsWith("/work-tasks/work-changed")) return work;
    assert.fail(`Recovery turned into execution: ${path}`);
  }, `${process.execPath} unused-fixture-engine`, state);
  try {
    assert.equal(adapter.enabled, true);
    await assert.rejects(handlers.get("agent:recover-work-task")(rendererEvent(), "project", work.id), /不会改为调用模型/);
    assert.deepEqual(state.confirmations, []);
    assert.deepEqual(state.budgets, []);
    assert.deepEqual(state.leases, []);
    assert.equal(adapter.process, null);
  } finally { adapter.close(); }
});

test("usage reconciliation cannot change local recovery into model continuation", async () => {
  const work = { id: "work-settlement", projectId: "project", kind: "direct", status: "blocked", version: 7,
    resumeMode: "recoverSavedResult", steps: [] };
  const refreshed = { ...work, version: 8, resumeMode: "continueExecution" };
  const state = { confirmations: [], budgets: [], leases: [] };
  const requests = [];
  let reconciled = false;
  const { handlers, adapter } = await mainHandlers(async (path, body) => {
    requests.push({ path, body });
    if (path.endsWith("/provider-usage")) {
      if (body) { reconciled = true; return {}; }
      return { pending: { taskId: "ended-attempt", status: "failed" } };
    }
    if (path.endsWith("/work-tasks/work-settlement")) return reconciled ? refreshed : work;
    assert.fail(`Reconciled recovery attempted execution: ${path}`);
  }, `${process.execPath} unused-fixture-engine`, state);
  try {
    await assert.rejects(handlers.get("agent:recover-work-task")(rendererEvent(), "project", work.id), /不会改为调用模型/);
    assert.equal(requests.length, 4);
    assert.deepEqual(requests[2].body, { taskId: "ended-attempt" });
    assert.deepEqual(state.confirmations, []);
    assert.deepEqual(state.budgets, []);
    assert.deepEqual(state.leases, []);
    assert.equal(adapter.process, null);
  } finally { adapter.close(); }
});
