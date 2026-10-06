import assert from "node:assert/strict";
import { test } from "node:test";
import { ModelBridge, ModelBridgeProcess, TaskBudget, createExecutionLease, defineConnection, resolveSessionPlan } from "../src/index.mjs";
import { verifiedBaselineEvidence } from "../src/testing/index.mjs";

for (const Bridge of [ModelBridge, ModelBridgeProcess]) {
  for (const reason of ["tokens", "requests", "duration", "host"]) {
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
      // Reproduce the real modeler's ledger, including non-refundable unknown usage.
      if (reason === "tokens") {
        budget.observeUsage({ total_tokens: 130979 });
        const reservation = budget.reserveRequest({ estimatedInputTokens: 24614 });
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
      assert.deepEqual(payload, { error: { message: `task ${label} budget exhausted`,
        type: "budget_exhausted", code: `task_${label}_budget_exhausted`, retryable: false } });
      assert.deepEqual(budget.snapshot(), before);
      assert.deepEqual(observations.map(({ status, errorCategory, errorCode, errorStatusCode }) => ({ status, errorCategory, errorCode, errorStatusCode })),
        [{ status: "failed", errorCategory: "budget", errorCode: payload.error.code, errorStatusCode: 400 }]);
    });
  }
}
