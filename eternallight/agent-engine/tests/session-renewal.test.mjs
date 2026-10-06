import assert from "node:assert/strict";
import { createServer } from "node:http";
import test from "node:test";
import { AgentEngineHost, CodexSessionRuntime, ModelBridge, ModelBridgeProcess, TaskBudget,
  createExecutionLease, defineConnection, resolveSessionPlan, renewSessionPlan } from "../src/index.mjs";
import { nativeCall } from "../src/native.mjs";
import { trustedTestEvidence, verifiedBaselineEvidence } from "../src/testing/index.mjs";

const connection = defineConnection({ id: "fixture", provider: "fixture", model: "writer", endpoint: "https://fixture.invalid",
  credentialRef: "FIXTURE_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64000, outputTokens: 4000 } });
const policy = { defaultConnectionId: "fixture", capabilityProfileId: "text-v1" };

function plans(conn = connection, now = Date.now(), oldTtl = 60000) {
  const compile = (offset, ttl, prefix) => resolveSessionPlan({ connections: [conn], policy,
    evidence: verifiedBaselineEvidence(conn, { checkedAt: new Date(now + offset), ttlMs: ttl, evidenceRefPrefix: prefix }),
    now: new Date(now) });
  return { previous: compile(-2000, oldTtl + 2000, "old"), next: compile(-1000, 120000, "fresh") };
}
function renewLease(lease, previous, next, now = Date.now()) {
  return nativeCall("lease.renew", { currentLease: lease, previousPlan: previous, sessionPlan: next, epoch: lease.epoch, now });
}

test("renewal requires a genuine fresh compiled plan and freezes every execution-scope field", () => {
  const now = Date.now();
  const { previous, next } = plans(connection, now);
  assert.equal(renewSessionPlan(previous, next, { now: new Date(now) }), next);
  assert.throws(() => renewSessionPlan(previous, { ...next }, { now: new Date(now) }), /trusted evidence compiled/);
  assert.throws(() => renewSessionPlan(previous, previous, { now: new Date(now) }), /later fresh evidence/);
  for (const key of ["rootRoute", "routes", "engineContractHash", "policyDigest", "defaultModel", "authorizedCapabilityProfiles",
    "productExtensions", "requiredExtensions", "allowedChildModels", "allowedChildEdges", "allowedMcpServerIds",
    "maxActiveChildren", "budgetPolicyRef", "permissionsRef"]) {
    const changed = { ...next, [key]: key === "routes" ? next.routes.map(route => ({ ...route, credentialRef: "OTHER" })) : "changed" };
    assert.throws(() => nativeCall("session.renew", { previousPlan: previous, sessionPlan: changed, now }), /frozen execution scope/, key);
  }
  for (const key of ["environmentDigest", "model", "endpoint", "protocol", "authorizedExtensions", "inputBudget"]) {
    const changed = { ...next, routes: next.routes.map(route => ({ ...route, [key]: "changed" })) };
    assert.throws(() => nativeCall("session.renew", { previousPlan: previous, sessionPlan: changed, now }), /frozen execution scope/, key);
  }
  assert.throws(() => renewSessionPlan(previous, next, { now: new Date(Date.parse(previous.expiresAt)) }), /unexpired/);
});

test("renewed leases preserve execution identity, explicit deadline caps and epochs", () => {
  const now = Date.now();
  const { previous, next } = plans(connection, now);
  const lease = createExecutionLease({ sessionPlan: previous, runId: "run", budgetId: "budget", now: new Date(now) });
  assert.deepEqual(renewLease(lease, previous, next, now), { ...lease, planId: next.id, deadline: next.expiresAt });
  const capped = createExecutionLease({ sessionPlan: previous, runId: "run-cap", budgetId: "budget", now: new Date(now), deadlineAtMs: now + 90000 });
  assert.deepEqual(renewLease(capped, previous, next, now), { ...capped, planId: next.id, deadline: new Date(now + 90000).toISOString() });
  const exhausted = createExecutionLease({ sessionPlan: previous, runId: "run-exhausted", budgetId: "budget", now: new Date(now), maxDurationMs: 10000 });
  assert.throws(() => renewLease(exhausted, previous, next, now), /explicit execution deadline/);
  assert.throws(() => nativeCall("lease.renew", { currentLease: lease, previousPlan: previous, sessionPlan: next, epoch: 2, now }), /revoked/);
  assert.throws(() => renewLease(lease, previous, next, Date.parse(lease.deadline)), /unexpired|expired/);
});

function controlledRuntime() {
  let finishTurn, releaseRenewal;
  const calls = [];
  const runtime = {
    subscribeEvents: () => () => {}, ensureSession: async () => "thread", resumeSession: async thread => thread,
    bindExecution: async () => {}, sendTurn: () => new Promise(resolve => { finishTurn = resolve; }),
    renewExecution: async params => { calls.push(params); if (releaseRenewal) await releaseRenewal.promise; return params.lease; },
    revokeExecution: async ({ revokedEpoch }) => ({ revokedEpoch }), interruptTurn: async () => true, close: async () => {},
  };
  return { runtime, calls, finish: () => finishTurn({ turn: { id: "turn", status: "completed" } }),
    pauseRenewal: () => { let release; const promise = new Promise(resolve => { release = resolve; }); releaseRenewal = { promise }; return release; } };
}

test("host commits fresh plan only after runtime acknowledgement, retaining run and budget", async () => {
  const { previous, next } = plans();
  const control = controlledRuntime();
  const host = new AgentEngineHost({ connections: [connection], evidenceProvider: async () => trustedTestEvidence([]), runtimeFactory: async () => control.runtime });
  await host.startSession({ sessionId: "session", sessionPlan: previous });
  const run = await host.startRun({ sessionId: "session", runId: "run", budgetId: "budget", input: [] });
  const renewed = await host.renewRun({ runId: "run", sessionPlan: next });
  assert.deepEqual(renewed, { runId: "run", planId: next.id, lease: { ...run.lease, planId: next.id, deadline: next.expiresAt } });
  assert.equal(control.calls[0].sessionPlan, next);
  assert.equal(host.sessionStatus("session").planId, next.id);
  assert.equal(host.runStatus("run").state, "running");
  await assert.rejects(host.renewRun({ runId: "run", sessionPlan: { ...next } }), /trusted evidence compiled/);
  control.finish(); await run.completion;
  await assert.rejects(host.renewRun({ runId: "run", sessionPlan: next }), /live running/);
  await host.closeSession("session");
});

test("host cancellation wins a late renewal acknowledgement without resurrecting authority", async () => {
  const { previous, next } = plans();
  const control = controlledRuntime();
  const release = control.pauseRenewal();
  const host = new AgentEngineHost({ connections: [connection], evidenceProvider: async () => trustedTestEvidence([]), runtimeFactory: async () => control.runtime });
  await host.startSession({ sessionId: "session", sessionPlan: previous });
  await host.startRun({ sessionId: "session", runId: "run", budgetId: "budget", input: [] });
  const renewing = host.renewRun({ runId: "run", sessionPlan: next });
  const rejected = assert.rejects(renewing, /REVOKED/);
  await assert.rejects(host.renewRun({ runId: "run", sessionPlan: next }), /already pending/);
  await host.cancelRun("run");
  release(); await rejected;
  assert.equal(host.sessionStatus("session").planId, previous.id);
  assert.equal(host.runStatus("run").state, "cancelled");
  assert.equal(host.sessionStatus("session").state, "recovery-blocked");
  await host.closeSession("session");
});

test("host does not commit a renewal acknowledged after expiry or resume a completed run", async () => {
  let now = Date.now();
  const { previous, next } = plans(connection, now);
  const control = controlledRuntime();
  const release = control.pauseRenewal();
  const host = new AgentEngineHost({ connections: [connection], now: () => new Date(now),
    evidenceProvider: async () => trustedTestEvidence([]), runtimeFactory: async () => control.runtime });
  await host.startSession({ sessionId: "session", sessionPlan: previous });
  await host.startRun({ sessionId: "session", runId: "run", budgetId: "budget", input: [] });
  const renewing = host.renewRun({ runId: "run", sessionPlan: next });
  const rejected = assert.rejects(renewing, /unexpired/);
  now = Date.parse(previous.expiresAt);
  release(); await rejected;
  assert.equal(host.sessionStatus("session").planId, previous.id);
  // Reconciliation is asynchronous; closing waits for revocation/drain.
  await host.closeSession("session");
  assert.equal(host.runStatus("run").state, "cancelled");
});

test("host fences terminal completion until a late renewal settles", async () => {
  const { previous, next } = plans();
  const control = controlledRuntime();
  const release = control.pauseRenewal();
  const host = new AgentEngineHost({ connections: [connection], evidenceProvider: async () => trustedTestEvidence([]), runtimeFactory: async () => control.runtime });
  await host.startSession({ sessionId: "session", sessionPlan: previous });
  const run = await host.startRun({ sessionId: "session", runId: "run", budgetId: "budget", input: [] });
  const renewing = host.renewRun({ runId: "run", sessionPlan: next });
  const rejected = assert.rejects(renewing, /REVOKED/);
  let completed = false;
  void run.completion.then(() => { completed = true; });
  control.finish();
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(completed, false);
  assert.throws(() => host.closeRun("run"), /pending execution renewal/);
  release(); await rejected; await run.completion;
  assert.equal(host.sessionStatus("session").planId, previous.id);
  assert.equal(host.sessionStatus("session").state, "recovery-blocked");
  host.closeRun("run"); await host.closeSession("session");
});

test("host rejects a missing runtime renewal receipt and blocks session reuse", async () => {
  const { previous, next } = plans();
  const control = controlledRuntime();
  control.runtime.renewExecution = async () => undefined;
  const host = new AgentEngineHost({ connections: [connection], evidenceProvider: async () => trustedTestEvidence([]), runtimeFactory: async () => control.runtime });
  await host.startSession({ sessionId: "session", sessionPlan: previous });
  await host.startRun({ sessionId: "session", runId: "run", budgetId: "budget", input: [] });
  await assert.rejects(host.renewRun({ runId: "run", sessionPlan: next }), /did not acknowledge/);
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(host.sessionStatus("session").state, "recovery-blocked");
  assert.equal(host.sessionStatus("session").planId, previous.id);
  await assert.rejects(host.startRun({ sessionId: "session", runId: "another", budgetId: "budget-2", input: [] }), /not idle/);
  await host.closeSession("session");
});

test("runtime renewal never reacquires budget or cancels an active turn", async () => {
  const { previous, next } = plans();
  const control = controlledRuntime();
  const calls = [];
  const bridge = { start: async () => {}, beginTaskBudget: async () => calls.push("budget"), beginExecutionLease: async () => calls.push("lease"),
    renewExecutionLease: async params => { assert.equal(params.sessionPlan, next); calls.push("renew"); return params.lease; },
    cancelActiveRequests: async () => calls.push("cancel"), endExecutionLease: async () => true, endTaskBudget: async () => true, close: async () => {} };
  const runtime = new CodexSessionRuntime({ appServer: control.runtime, bridge, budgetFactory: async () => ({}) });
  const lease = createExecutionLease({ sessionPlan: previous, runId: "run", budgetId: "budget" });
  await runtime.bindExecution({ lease, sessionPlan: previous });
  const turn = runtime.sendTurn([]);
  const renewed = renewLease(lease, previous, next);
  assert.deepEqual(await runtime.renewExecution({ lease: renewed, sessionPlan: next }), renewed);
  assert.equal(control.calls[0].sessionPlan, next);
  assert.deepEqual(control.calls[0].lease, renewed);
  assert.deepEqual(calls, ["budget", "lease", "renew"]);
  control.finish(); await turn; await runtime.close();
});

test("runtime cancellation wins a late bridge renewal acknowledgement", async () => {
  const { previous, next } = plans();
  const control = controlledRuntime();
  let release;
  const barrier = new Promise(resolve => { release = resolve; });
  const bridge = { start: async () => {}, beginTaskBudget: async () => {}, beginExecutionLease: async () => {},
    renewExecutionLease: async params => { await barrier; return params.lease; }, cancelActiveRequests: async () => 0,
    endExecutionLease: async () => true, endTaskBudget: async () => true, close: async () => {} };
  const runtime = new CodexSessionRuntime({ appServer: control.runtime, bridge, budgetFactory: async () => ({}) });
  const lease = createExecutionLease({ sessionPlan: previous, runId: "run", budgetId: "budget" });
  await runtime.bindExecution({ lease, sessionPlan: previous });
  const turn = runtime.sendTurn([]);
  const renewing = runtime.renewExecution({ lease: renewLease(lease, previous, next), sessionPlan: next });
  const rejected = assert.rejects(renewing, error => error.code === "TASK_CANCELLED");
  await runtime.revokeExecution({ runId: lease.runId, leaseId: lease.leaseId, expectedEpoch: lease.epoch, revokedEpoch: lease.epoch + 1 });
  release(); await rejected;
  control.finish(); await turn; await runtime.close();
});

for (const Bridge of [ModelBridge, ModelBridgeProcess]) {
  test(`${Bridge.name} renews across evidence expiry without cancelling requests, resetting metering or lineage`, async t => {
    let unblock, started;
    const ready = new Promise(resolve => { started = resolve; });
    const barrier = new Promise(resolve => { unblock = resolve; });
    const provider = createServer(async (request, response) => {
      for await (const chunk of request) void chunk;
      started(); await barrier;
      response.setHeader("content-type", "application/json");
      response.end(JSON.stringify({ id: "response", choices: [{ message: { role: "assistant", content: "done" }, finish_reason: "stop" }],
        usage: { prompt_tokens: 5, completion_tokens: 2, total_tokens: 7 } }));
    });
    await new Promise(resolve => provider.listen(0, "127.0.0.1", resolve));
    t.after(() => new Promise(resolve => { provider.closeAllConnections(); provider.close(resolve); }));
    const conn = defineConnection({ ...connection, endpoint: `http://127.0.0.1:${provider.address().port}` });
    const { previous, next } = plans(conn, Date.now(), 2500);
    const bridge = new Bridge({ sessionPlan: previous, connections: [conn], credentialResolver: async () => "fixture" });
    t.after(() => bridge.close());
    const budget = new TaskBudget();
    const address = await bridge.start();
    await bridge.beginTaskBudget(budget);
    const lease = createExecutionLease({ sessionPlan: previous, runId: "run", budgetId: budget.id });
    await bridge.beginExecutionLease(lease);
    const headers = parent => ({ authorization: `Bearer ${address.token}`, "content-type": "application/json",
      "x-codex-turn-metadata": JSON.stringify({ thread_id: "root", ...(parent ? { parent_thread_id: parent } : {}) }) });
    const request = parent => fetch(`${address.baseUrl}/v1/responses`, { method: "POST", headers: headers(parent),
      body: JSON.stringify({ model: "fixture", stream: false, input: "hello", max_output_tokens: 100 }) });
    const active = request(); await ready;
    const renewed = renewLease(lease, previous, next);
    assert.deepEqual(await bridge.renewExecutionLease({ lease: renewed, sessionPlan: next }), renewed);
    await new Promise(resolve => setTimeout(resolve, Math.max(0, Date.parse(previous.expiresAt) - Date.now() + 20)));
    unblock();
    const response = await active; assert.equal(response.status, 200, await response.text());
    const afterExpiry = await request(); assert.equal(afterExpiry.status, 200, await afterExpiry.text());
    // Existing thread lineage must not be reset: it remains recognized even if
    // a subsequent envelope names an unknown parent (new threads cannot do so).
    const retained = await request("unknown-parent"); assert.equal(retained.status, 200, await retained.text());
    assert.deepEqual({ requests: budget.snapshot().requests, tokens: budget.snapshot().tokens }, { requests: 3, tokens: 21 });
    await bridge.cancelActiveRequests();
    const later = plans(conn, Date.now() + 1000).next;
    await assert.rejects(Promise.resolve().then(() => bridge.renewExecutionLease({ lease: renewLease(renewed, next, later), sessionPlan: later })), /revoked|later fresh evidence/);
  });
}
