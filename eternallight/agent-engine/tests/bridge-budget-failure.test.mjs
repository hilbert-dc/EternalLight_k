import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { test } from "node:test";
import { ModelBridge, ModelBridgeProcess, PersistentTaskBudget, TaskBudget, createExecutionLease, defineConnection, resolveSessionPlan } from "../src/index.mjs";
import { verifiedBaselineEvidence } from "../src/testing/index.mjs";

for (const Bridge of [ModelBridge, ModelBridgeProcess]) {
  for (const reason of ["tokens", "requests", "duration", "unknown", "host"]) {
    test(`${Bridge.name} distinguishes ${reason} failure before a provider request`, async t => {
      const connection = defineConnection({ id: "fixture", model: "fixture-v1", provider: "fixture",
        endpoint: "http://127.0.0.1:9", credentialRef: "FIXTURE_KEY", protocol: "chat-completions",
        declaredLimits: { contextTokens: 131072, outputTokens: 8192 } });
      const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: {} });
      const observations = [];
      const bridge = new Bridge({ sessionPlan, connections: [connection], credentialResolver: async () => "fixture",
        responseObserver: value => observations.push(value) });
      t.after(() => bridge.close());
      const budget = new TaskBudget({ maxRequests: reason === "requests" ? 1 : 8, maxTokens: 170000, maxDurationMs: 1000 });
      if (reason === "tokens") {
        budget.observeUsage({ total_tokens: 155593 });
      } else if (reason === "unknown") {
        const reservation = budget.reserveRequest({ estimatedInputTokens: 100 });
        budget.settleReservation(reservation, null, { outcome: "unknown" });
      } else if (reason === "requests") {
        const reservation = budget.reserveRequest({ estimatedInputTokens: 1 });
        budget.settleReservation(reservation, { total_tokens: 1 });
      } else if (reason === "duration") {
        const reserve = budget.reserveRequest.bind(budget);
        budget.reserveRequest = parameters => reserve(parameters, budget.startedAt + budget.maxDurationMs);
      } else {
        budget.reserveRequest = () => { throw new Error("fixture host failure"); };
      }
      const before = budget.snapshot();
      const address = await bridge.start();
      await bridge.beginTaskBudget(budget);
      await bridge.beginExecutionLease(createExecutionLease({ sessionPlan, runId: "budget-denial", budgetId: budget.id }));
      const response = await fetch(`${address.baseUrl}/v1/responses`, { method: "POST",
        headers: { authorization: `Bearer ${address.token}`, "content-type": "application/json",
          "x-codex-turn-metadata": JSON.stringify({ thread_id: "root" }) },
        body: JSON.stringify({ model: "fixture", stream: false, max_output_tokens: 8192,
          input: Array.from({ length: 8 }, () => ({ type: "message", role: "user", content: [{ type: "input_text", text: "bounded ".repeat(1000) }] })) }) });
      const payload = await response.json();
      if (reason === "host") {
        assert.equal(response.status, 502);
        assert.deepEqual(payload, { error: { message: "fixture host failure", type: "bridge_error" } });
        assert.deepEqual(budget.snapshot(), before);
        assert.deepEqual(observations.map(({ errorCategory, errorCode }) => ({ errorCategory, errorCode })),
          [{ errorCategory: "bridge", errorCode: "bridge_error" }]);
        return;
      }
      assert.equal(response.status, 400, JSON.stringify(payload));
      const label = { tokens: "token", requests: "request", duration: "duration" }[reason];
      assert.deepEqual(payload, reason === "unknown"
        ? { error: { message: "task provider usage is unknown", type: "budget_unknown", code: "task_provider_usage_unknown", retryable: false } }
        : { error: { message: `task ${label} budget exhausted`, type: "budget_exhausted", code: `task_${label}_budget_exhausted`, retryable: false } });
      assert.deepEqual(budget.snapshot(), before);
      assert.deepEqual(observations.map(({ status, errorCategory, errorCode, errorStatusCode }) => ({ status, errorCategory, errorCode, errorStatusCode })),
        [{ status: "failed", errorCategory: "budget", errorCode: payload.error.code, errorStatusCode: 400 }]);
    });
  }
}

const waitFor = async predicate => {
  const deadline = Date.now() + 5000;
  while (!predicate()) {
    if (Date.now() >= deadline) throw Error("unknown usage settlement did not arrive");
    await new Promise(resolve => setTimeout(resolve, 10));
  }
};

for (const Bridge of [ModelBridge, ModelBridgeProcess]) {
  for (const [label, limits] of [["finite", { maxRequests: 8, maxTokens: 200000 }], ["unlimited", {}]]) {
    test(`${Bridge.name} blocks a trusted request replay after a real ${label} stream disconnect`, { timeout: 15000 }, async t => {
      let providerCalls = 0, providerResponse, bridge;
      const marker = "usage-disconnect-boundary";
      const provider = createServer(async (request, response) => {
        const chunks = [];
        for await (const chunk of request) chunks.push(chunk);
        const body = JSON.parse(Buffer.concat(chunks));
        assert.equal(body.stream, true);
        assert.equal(body.stream_options.include_usage, true);
        providerCalls++;
        providerResponse = response;
        response.writeHead(200, { "content-type": "text/event-stream" });
        response.write(`data: ${JSON.stringify({ id: "interrupted-request", choices: [{ index: 0,
          delta: { role: "assistant", content: marker }, finish_reason: null }] })}\n\n`);
      });
      await new Promise(resolve => provider.listen(0, "127.0.0.1", resolve));
      t.after(async () => {
        await bridge?.close();
        await new Promise(resolve => { provider.closeAllConnections(); provider.close(resolve); });
      });
      const connection = defineConnection({ id: "fixture", provider: "fixture", model: "fixture-v1",
        endpoint: `http://127.0.0.1:${provider.address().port}`, credentialRef: "FIXTURE_KEY", protocol: "chat-completions",
        declaredLimits: { contextTokens: 64000, outputTokens: 4000 } });
      const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: {} });
      const observations = [];
      bridge = new Bridge({ sessionPlan, connections: [connection], credentialResolver: async () => "fixture",
        responseObserver: value => observations.push(value) });
      const directory = Bridge === ModelBridgeProcess ? await mkdtemp(join(tmpdir(), "stream-failure-evidence-")) : null;
      const databasePath = directory ? join(directory, "engine.sqlite3") : null;
      if (directory) t.after(() => rm(directory, { recursive: true, force: true }));
      const budget = databasePath ? new PersistentTaskBudget({ ...limits, databasePath, runId: "interrupted-usage" }) : new TaskBudget(limits);
      const reservations = [], settlements = [];
      const reserve = budget.reserveRequest.bind(budget), settle = budget.settleReservation.bind(budget);
      // This Host-owned logical identity exercises the formerly successful
      // replay branch. It is never a provider argument or model-supplied id.
      budget.reserveRequest = parameters => {
        const admitted = { ...parameters, reservationId: "trusted:interrupted-stream" };
        const id = reserve(admitted);
        reservations.push({ id, parameters: admitted });
        return id;
      };
      budget.settleReservation = (id, usage, options) => {
        const result = settle(id, usage, options);
        settlements.push({ id, usage, options });
        return result;
      };
      const address = await bridge.start();
      await bridge.beginTaskBudget(budget);
      await bridge.beginExecutionLease(createExecutionLease({ sessionPlan, runId: "interrupted-usage", budgetId: budget.id }));
      const request = () => fetch(`${address.baseUrl}/v1/responses`, { method: "POST", headers: {
        authorization: `Bearer ${address.token}`, "content-type": "application/json",
        "x-codex-turn-metadata": JSON.stringify({ thread_id: "root" }),
      }, body: JSON.stringify({ model: "fixture", stream: true, input: "hello", max_output_tokens: 128 }) });
      const first = await request();
      assert.equal(first.status, 200);
      const reader = first.body.getReader(), decoder = new TextDecoder();
      let received = "";
      while (!received.includes(marker)) {
        const { done, value } = await reader.read();
        assert.equal(done, false, "the native proxy accepted a frame before interruption");
        received += decoder.decode(value, { stream: true });
      }
      providerResponse.socket.destroy();
      await assert.rejects(async () => { while (!(await reader.read()).done) {} });
      await waitFor(() => budget.snapshot().unknownTokens > 0);
      assert.equal(reservations.length, 1);
      assert.equal(settlements.length, 1);
      assert.equal(settlements[0].id, reservations[0].id);
      assert.equal(settlements[0].usage, null);
      assert.equal(settlements[0].options.outcome, "unknown");
      const before = budget.state;
      assert.equal(before.requests, 1);
      assert.equal(before.tokens, 0);
      assert.equal(before.unknownTokens, reservations[0].parameters.estimatedInputTokens + reservations[0].parameters.maxOutputTokens);
      assert.equal(before.reservedTokens, 0);
      assert.equal(before.overrun, false);
      for (let retry = 0; retry < 2; retry++) {
        const next = await request();
        assert.equal(next.status, 400);
        assert.deepEqual(await next.json(), { error: { message: "task provider usage is unknown", type: "budget_unknown",
          code: "task_provider_usage_unknown", retryable: false } });
        assert.equal(providerCalls, 1, "the original unknown request cannot authorize another dispatch");
        assert.deepEqual(budget.state, before);
      }
      assert.equal(reservations.length, 1);
      assert.equal(settlements.length, 1);
      assert.deepEqual(observations.slice(-2).map(({ errorCategory, errorCode, retryable }) => ({ errorCategory, errorCode, retryable })),
        Array.from({ length: 2 }, () => ({ errorCategory: "budget", errorCode: "task_provider_usage_unknown", retryable: false })));
      await bridge.endTaskBudget(budget);
      assert.deepEqual(budget.state, before, "terminal aggregation preserves the original unknown cost");
      if (databasePath) {
        budget.complete("failed");
        const receipt = PersistentTaskBudget.receipt(databasePath, "interrupted-usage");
        assert.equal(receipt.failureObservations.length, 2);
        assert.equal(receipt.failureObservations[0].errorStatusCode, 502);
        assert.equal(receipt.failureObservations[1].errorCode, "task_provider_usage_unknown");
        assert.notEqual(receipt.failureObservations[0].requestId, receipt.failureObservations[1].requestId);
      }
    });
  }
}
