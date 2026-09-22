import assert from "node:assert/strict";
import { test } from "node:test";
import { ModelBridge, TaskBudget, createExecutionLease, defineConnection, resolveSessionPlan } from "../src/index.mjs";
import { verifiedBaselineEvidence } from "../src/testing/index.mjs";

function fixture(options) {
  const connection = defineConnection({ id: "writer", provider: "fixture", endpoint: "https://provider.invalid", model: "fixture", credentialRef: "FIXTURE_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64000, outputTokens: 4000 } });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: {} });
  const bridge = new ModelBridge({ sessionPlan, connections: [connection], credentialResolver: async () => "fixture", token: "fixture-token", ...options });
  const budget = new TaskBudget();
  const lease = createExecutionLease({ sessionPlan, runId: `run-${budget.id}`, budgetId: budget.id });
  return { bridge, budget, lease };
}
async function invoke(bridge) {
  const { baseUrl, token } = await bridge.start();
  return fetch(`${baseUrl}/responses`, { method: "POST", headers: { authorization: `Bearer ${token}`, "x-codex-turn-metadata": JSON.stringify({ thread_id: "root" }) }, body: JSON.stringify({ model: "writer", input: [], stream: false }) });
}

test("native bridge remembers a late reservation and refunds it before cancellation finishes", async t => {
  let invoked;
  const reserving = new Promise(resolve => { invoked = resolve; });
  let release;
  const gate = new Promise(resolve => { release = resolve; });
  let providerCalls = 0;
  const { bridge, budget, lease } = fixture({ fetchImpl: async () => { providerCalls++; throw new Error("must not call provider"); } });
  t.after(() => bridge.close());
  const reserve = budget.reserveRequest.bind(budget);
  budget.reserveRequest = async params => { invoked(); await gate; return reserve(params); };
  bridge.beginTaskBudget(budget); bridge.beginExecutionLease(lease);
  const response = invoke(bridge);
  await reserving;
  let drained = false;
  const cancellation = bridge.cancelActiveRequests().then(value => { drained = true; return value; });
  await new Promise(resolve => setTimeout(resolve, 20));
  assert.equal(drained, false);
  release();
  assert.equal(await cancellation, 1);
  assert.equal((await response).status, 499);
  assert.equal(providerCalls, 0);
  assert.equal(budget.snapshot().reservedTokens, 0);
});

test("native bridge refuses new execution after an unresolved settlement and still closes its listener", async () => {
  const { bridge, budget, lease } = fixture({ fetchImpl: async () => { throw new Error("provider unavailable"); } });
  budget.settleReservation = async () => { throw new Error("fixture settlement persistence failed"); };
  bridge.beginTaskBudget(budget); bridge.beginExecutionLease(lease);
  assert.equal((await invoke(bridge)).status, 502);
  await assert.rejects(bridge.cancelActiveRequests(), /settlement is unresolved/);
  bridge.endTaskBudget(budget);
  assert.throws(() => bridge.beginTaskBudget(new TaskBudget()), /settlement is unresolved/);
  await assert.rejects(bridge.close(), /settlement is unresolved/);
  assert.equal(bridge.server, null);
});
