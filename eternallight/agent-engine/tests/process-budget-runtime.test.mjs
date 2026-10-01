import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { CodexSessionRuntime, ModelBridgeProcess, PersistentTaskBudget, createExecutionLease, defineConnection, resolveSessionPlan } from "../src/index.mjs";
import { verifiedBaselineEvidence } from "../src/testing/index.mjs";

for (const outcome of ["completed", "failed"]) {
  test(`process bridge releases a persistent budget for one runtime-owned ${outcome} finalization`, async t => {
    const directory = await mkdtemp(join(tmpdir(), "engine-process-budget-"));
    const databasePath = join(directory, "budget.sqlite3");
    const connection = defineConnection({ id: "fixture", provider: "fixture", model: "fixture-v1",
      endpoint: "https://fixture.invalid", credentialRef: "FIXTURE_KEY", protocol: "chat-completions",
      declaredLimits: { contextTokens: 64000, outputTokens: 4000 } });
    const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: {} });
    const bridge = new ModelBridgeProcess({ sessionPlan, connections: [connection], credentialResolver: async () => "fixture", heartbeatIntervalMs: 0 });
    const runId = `persistent-${outcome}`;
    const budget = new PersistentTaskBudget({ databasePath, runId, maxRequests: 2, maxTokens: 10000 });
    let finalizations = 0;
    const complete = budget.complete.bind(budget);
    budget.complete = status => { finalizations++; complete(status); };
    const appServer = {
      ensureSession: async () => "thread", resumeSession: async id => id,
      bindExecution: async () => {}, revokeExecution: async () => {}, interruptTurn: async () => {},
      close: async () => {}, subscribeEvents: () => () => {},
      sendTurn: async () => {
        if (outcome === "failed") throw new Error("fixture turn failed");
        return { turn: { status: "completed" }, message: "done" };
      },
    };
    const runtime = new CodexSessionRuntime({ appServer, bridge, budgetFactory: async () => budget });
    t.after(async () => { await runtime.close(); budget.close(); await rm(directory, { recursive: true, force: true }); });
    const lease = createExecutionLease({ sessionPlan, runId, budgetId: budget.id });
    await runtime.bindExecution({ lease, sessionPlan });
    if (outcome === "failed") await assert.rejects(runtime.sendTurn([]), /fixture turn failed/);
    else assert.equal((await runtime.sendTurn([])).message, "done");
    assert.equal(finalizations, 1);
    const receipt = PersistentTaskBudget.receipt(databasePath, runId);
    assert.equal(receipt.status, outcome);
    const first = structuredClone(receipt);
    await runtime.close();
    assert.equal(finalizations, 1);
    assert.deepEqual(PersistentTaskBudget.receipt(databasePath, runId), first);
  });
}

test("a crashed process bridge records outstanding persistent usage as unknown, not completed", async t => {
  const directory = await mkdtemp(join(tmpdir(), "engine-process-unknown-"));
  const databasePath = join(directory, "budget.sqlite3");
  const connection = defineConnection({ id: "fixture", provider: "fixture", model: "fixture-v1",
    endpoint: "https://fixture.invalid", credentialRef: "FIXTURE_KEY", protocol: "chat-completions",
    declaredLimits: { contextTokens: 64000, outputTokens: 4000 } });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: {} });
  const bridge = new ModelBridgeProcess({ sessionPlan, connections: [connection], credentialResolver: async () => "fixture", heartbeatIntervalMs: 0 });
  const budget = new PersistentTaskBudget({ databasePath, runId: "crashed", maxRequests: 2, maxTokens: 10000 });
  t.after(async () => { await bridge.close(); budget.close(); await rm(directory, { recursive: true, force: true }); });
  await bridge.start();
  await bridge.beginTaskBudget(budget);
  budget.reserveRequest({ estimatedInputTokens: 100 });
  process.kill(bridge.processId, "SIGTERM");
  const deadline = Date.now() + 5000;
  while (bridge.processId !== null && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 10));
  assert.equal(bridge.processId, null);
  await assert.rejects(bridge.start(), /budget outcome is unknown/);
  assert.equal(await bridge.endTaskBudget(budget), false);
  const receipt = PersistentTaskBudget.receipt(databasePath, "crashed");
  assert.equal(receipt.status, "outcome-unknown");
  assert.deepEqual(receipt.usage, { requests: 1, tokens: 0, unknownTokens: 100, overrun: false });
});
