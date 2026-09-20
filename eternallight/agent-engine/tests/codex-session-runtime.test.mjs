import assert from "node:assert/strict";
import { test } from "node:test";

import { CodexSessionRuntime } from "../src/index.mjs";

function fixture() {
  const calls = [];
  const appServer = {
    ensureSession: async () => "thread-1",
    resumeSession: async (threadId) => threadId,
    bindExecution: async () => calls.push("app:bind"),
    sendTurn: async () => ({ turn: { id: "turn-1", status: "completed" }, message: "done" }),
    revokeExecution: async ({ revokedEpoch }) => { calls.push("app:revoke"); return { revokedEpoch }; },
    interruptTurn: async () => { calls.push("app:interrupt"); return true; },
    close: async () => calls.push("app:close"),
    subscribeEvents: () => () => {},
  };
  const bridge = {
    start: async () => calls.push("bridge:start"),
    beginTaskBudget: async () => calls.push("bridge:budget:start"),
    beginExecutionLease: async () => calls.push("bridge:lease:start"),
    cancelActiveRequests: async () => { calls.push("bridge:cancel"); return 1; },
    endExecutionLease: async () => { calls.push("bridge:lease:end"); return true; },
    endTaskBudget: async () => { calls.push("bridge:budget:end"); return true; },
    close: async () => calls.push("bridge:close"),
  };
  const budget = {};
  const runtime = new CodexSessionRuntime({ appServer, bridge, budgetFactory: async () => budget });
  const lease = { runId: "run-1", leaseId: "lease-1", budgetId: "budget-1", epoch: 0 };
  return { runtime, lease, calls, appServer, bridge };
}

test("Codex session runtime owns bridge, budget, lease, and turn ordering", async () => {
  const { runtime, lease, calls } = fixture();
  await runtime.bindExecution({ lease, sessionPlan: { id: "plan-1" } });
  assert.equal((await runtime.sendTurn([{ type: "text", text: "hello" }])).message, "done");
  assert.deepEqual(calls, [
    "bridge:start",
    "bridge:budget:start",
    "bridge:lease:start",
    "app:bind",
    "bridge:lease:end",
    "bridge:budget:end",
  ]);
});

test("Codex session runtime revokes provider work before App Server execution", async () => {
  const { runtime, lease, calls } = fixture();
  await runtime.bindExecution({ lease, sessionPlan: { id: "plan-1" } });
  assert.deepEqual(await runtime.revokeExecution({ runId: "run-1", leaseId: "lease-1", expectedEpoch: 0, revokedEpoch: 1 }), { revokedEpoch: 1 });
  assert.deepEqual(calls.slice(-2), ["bridge:cancel", "app:revoke"]);
  await assert.rejects(runtime.sendTurn([{ type: "text", text: "hello" }]), (error) => error.code === "TASK_CANCELLED");
  assert.deepEqual(calls.slice(-2), ["bridge:lease:end", "bridge:budget:end"]);
});

test("Codex session runtime closes both isolated processes", async () => {
  const { runtime, calls } = fixture();
  await runtime.close();
  assert.deepEqual(calls, ["app:close", "bridge:close"]);
  assert.throws(() => runtime.ensureSession(), /closed/);
});

test("Codex session runtime applies the same lease to non-standard App Server turns", async () => {
  const { runtime, lease, calls } = fixture();
  await runtime.bindExecution({ lease, sessionPlan: { id: "plan-1" } });
  const result = await runtime.sendTurn([], { dispatch: async (appServer) => ({ reviewed: Boolean(appServer) }) });
  assert.deepEqual(result, { reviewed: true });
  assert.deepEqual(calls.slice(-2), ["bridge:lease:end", "bridge:budget:end"]);
});

test("Codex session runtime can revoke while the budget is being prepared", async () => {
  const { runtime, lease, calls, appServer } = fixture();
  appServer.revokeExecution = async () => { throw new Error("App Server execution lease mismatch"); };
  let releaseBudget;
  let budgetStarted;
  const budgetBarrier = new Promise((resolve) => { releaseBudget = resolve; });
  const budgetReady = new Promise((resolve) => { budgetStarted = resolve; });
  runtime.budgetFactory = async () => {
    budgetStarted();
    await budgetBarrier;
    return {};
  };
  const binding = runtime.bindExecution({ lease, sessionPlan: { id: "plan-1" } });
  await budgetReady;
  const revocation = runtime.revokeExecution({ runId: "run-1", leaseId: "lease-1", expectedEpoch: 0, revokedEpoch: 1 });
  releaseBudget();
  assert.deepEqual(await revocation, { revokedEpoch: 1 });
  await assert.rejects(binding, (error) => error.code === "TASK_CANCELLED");
  assert.deepEqual(calls, ["bridge:cancel"]);
});

test("close accepts synchronous cancellation and closes the bridge even if the app closer throws", async () => {
  const { runtime, lease, calls, bridge } = fixture();
  bridge.cancelActiveRequests = () => { calls.push("bridge:cancel"); return 0; };
  runtime.closeAppServer = () => { calls.push("app:close"); throw new Error("close failed"); };
  await runtime.bindExecution({ lease, sessionPlan: { id: "plan-1" } });
  await assert.rejects(runtime.close(), /close failed/);
  assert.deepEqual(calls.slice(-6), ["bridge:cancel", "app:interrupt", "bridge:lease:end", "bridge:budget:end", "app:close", "bridge:close"]);
});

test("revocation during App Server binding precedes clearing the bound lease", async () => {
  const { runtime, lease, calls, appServer } = fixture();
  let releaseBinding;
  let startedBinding;
  const started = new Promise((resolve) => { startedBinding = resolve; });
  const barrier = new Promise((resolve) => { releaseBinding = resolve; });
  let bound = false;
  appServer.bindExecution = async () => { startedBinding(); await barrier; bound = true; };
  appServer.revokeExecution = async () => { assert.equal(bound, true); calls.push("app:revoke"); return { revokedEpoch: 1 }; };
  appServer.endExecution = async () => { bound = false; calls.push("app:end"); };
  const binding = runtime.bindExecution({ lease, sessionPlan: {} });
  const rejected = assert.rejects(binding, (error) => error.code === "TASK_CANCELLED");
  await started;
  const revocation = runtime.revokeExecution({ runId: lease.runId, leaseId: lease.leaseId, expectedEpoch: 0, revokedEpoch: 1 });
  releaseBinding();
  assert.deepEqual(await revocation, { revokedEpoch: 1 });
  await rejected;
  assert.ok(calls.indexOf("app:revoke") < calls.indexOf("app:end"));
  assert.equal(bound, false);
});
