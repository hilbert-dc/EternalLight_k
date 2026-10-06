import assert from "node:assert/strict";
import { createServer } from "node:http";
import test from "node:test";
import { ModelBridge, ModelBridgeProcess, TaskBudget, createExecutionLease, defineConnection, resolveSessionPlan } from "../src/index.mjs";
import { verifiedBaselineEvidence } from "../src/testing/index.mjs";

const waitFor = async predicate => {
  const deadline = Date.now() + 5000;
  while (!predicate()) { if (Date.now() >= deadline) throw Error("condition timed out"); await new Promise(resolve => setTimeout(resolve, 10)); }
};

test("in-flight token reservations defer without charging and resume after settlement", () => {
  const budget = new TaskBudget({ maxTokens: 100 });
  const first = budget.reserveRequest({ estimatedInputTokens: 80 });
  const before = budget.snapshot();
  assert.deepEqual(budget.reserveRequest({ estimatedInputTokens: 60, deferIfBusy: true }), { waiting: true });
  assert.deepEqual(budget.snapshot(), before);
  budget.settleReservation(first, { total_tokens: 10 });
  assert.equal(typeof budget.reserveRequest({ estimatedInputTokens: 60, deferIfBusy: true }), "string");
  assert.throws(() => budget.reserveRequest({ estimatedInputTokens: 95, deferIfBusy: true }), /token budget exhausted/);
});

test("invalid resource policies and child capacities fail before starting a session", () => {
  const connection = defineConnection({ id: "fixture", model: "test", endpoint: "http://localhost:9", credentialRef: "TEST_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64000, outputTokens: 100 } });
  for (const policy of [{ maxActiveChildren: 0 }, { maxActiveChildren: 65 }, { resourcePolicy: { maxConcurrentRequests: -1 } }, { resourcePolicy: { connectionLimits: [] } }]) {
    assert.throws(() => resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy }), /maxActiveChildren|resourcePolicy/);
  }
});

for (const Bridge of [ModelBridge, ModelBridgeProcess]) {
 for (const [constraint, maxTokens, providerLimit] of [["provider", 10000, 1], ["budget", 150, 3]]) {
  test(`${Bridge.name} queues by ${constraint}, cancels a waiter, and reuses capacity for more than three tasks`, { timeout: 15000 }, async t => {
    let calls = 0, active = 0, peak = 0, release, holdAll = false, releaseAll;
    const held = new Promise(resolve => { release = resolve; });
    const heldAll = new Promise(resolve => { releaseAll = resolve; });
    const provider = createServer(async (request, response) => {
      for await (const chunk of request) { /* drain request */ }
      calls++; active++; peak = Math.max(peak, active);
      if (calls === 1) await held;
      if (holdAll) await heldAll;
      await new Promise(resolve => setTimeout(resolve, 25));
      active--;
      response.writeHead(200, { "content-type": "application/json" });
      response.end(JSON.stringify({ id: `reply-${calls}`, choices: [{ message: { role: "assistant", content: "ok" }, finish_reason: "stop" }], usage: { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 } }));
    });
    await new Promise(resolve => provider.listen(0, "127.0.0.1", resolve));
    t.after(() => { release(); releaseAll(); provider.closeAllConnections(); provider.close(); });
    const connection = defineConnection({ id: "fixture", model: "test", endpoint: `http://127.0.0.1:${provider.address().port}`, credentialRef: "TEST_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64000, outputTokens: 100 } });
    const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection),
      policy: { maxActiveChildren: 12, resourcePolicy: { maxConcurrentRequests: 3, maxRequestsPerProvider: providerLimit, maxQueuedRequests: 12 } } });
    const observed = [];
    const bridge = new Bridge({ sessionPlan, connections: [connection], credentialResolver: async () => "fixture", requestObserver: event => observed.push(event) });
    t.after(() => bridge.close());
    const address = await bridge.start();
    const budget = new TaskBudget({ maxRequests: 20, maxTokens });
    await bridge.beginTaskBudget(budget);
    await bridge.beginExecutionLease(createExecutionLease({ sessionPlan, budgetId: budget.id, runId: "scheduled" }));
    const request = async signal => {
      const response = await fetch(`${address.baseUrl}/v1/responses`, { method: "POST", signal,
        headers: { authorization: `Bearer ${address.token}`, "content-type": "application/json", "x-codex-turn-metadata": JSON.stringify({ thread_id: "root" }) },
        body: JSON.stringify({ model: "fixture", input: [], stream: false, max_output_tokens: 100 }) });
      const payload = await response.json(); assert.equal(response.status, 200, JSON.stringify(payload)); return payload;
    };
    const first = request(); await waitFor(() => calls === 1);
    const controller = new AbortController();
    const cancelled = request(controller.signal).catch(error => error);
    const rest = Array.from({ length: 5 }, () => request());
    await waitFor(() => observed.length === 7);
    assert.equal(calls, 1); assert.equal(budget.snapshot().requests, 1);
    controller.abort(); assert.equal((await cancelled).name, "AbortError");
    // Give the server cancellation observer a chance to drop the queued client.
    await new Promise(resolve => setTimeout(resolve, 50));
    release(); await Promise.all([first, ...rest]);
    assert.equal(calls, 6); assert.equal(peak, 1);
    assert.equal(budget.snapshot().requests, 6); assert.equal(budget.snapshot().reservedTokens, 0);
    await request(); assert.equal(calls, 7);
    holdAll = true;
    const cancelling = Promise.allSettled([request(), request(), request(), request()]);
    await waitFor(() => observed.length === 12 && calls >= 8);
    await bridge.cancelActiveRequests();
    const cancelledTree = await cancelling;
    assert.ok(cancelledTree.every(result => result.status === "rejected"));
    assert.equal(budget.snapshot().reservedTokens, 0);
    const callsAfterCancel = calls;
    releaseAll(); await new Promise(resolve => setTimeout(resolve, 50));
    assert.equal(calls, callsAfterCancel, "queued requests must not start after tree cancellation");
  });
 }
}
