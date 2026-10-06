import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { ModelBridge, ModelBridgeProcess, PersistentTaskBudget, TaskBudget, createExecutionLease, defineConnection, resolveSessionPlan } from "../src/index.mjs";
import { verifiedBaselineEvidence } from "../src/testing/index.mjs";

test("metering records long request trees, unknown usage and terminal replay without a quota", async t => {
  const directory = await mkdtemp(join(tmpdir(), "unrestricted-metering-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const databasePath = join(directory, "budget.sqlite3");
  const budget = new PersistentTaskBudget({ databasePath, runId: "unrestricted", startedAt: 1000 });
  for (let index = 0; index < 70; index++) {
    const id = budget.reserveRequest({ estimatedInputTokens: 20000 }, 86_400_000);
    budget.settleReservation(id, index === 69 ? null : { total_tokens: 20000 });
  }
  assert.equal(budget.remainingDurationMs(86_400_000), null);
  assert.deepEqual(budget.snapshot(), { id: budget.id, requests: 70, tokens: 1380000,
    unknownTokens: 20000, reservedTokens: 0, maxRequests: null, maxTokens: null,
    maxDurationMs: null, startedAt: 1000, overrun: false });
  budget.complete("completed");
  const receipt = PersistentTaskBudget.receipt(databasePath, "unrestricted");
  assert.deepEqual(receipt.limits, { maxRequests: null, maxTokens: null, maxDurationMs: null });
  assert.deepEqual(receipt.usage, { requests: 70, tokens: 1380000, unknownTokens: 20000, overrun: false });
  assert.deepEqual(PersistentTaskBudget.receipt(databasePath, "unrestricted"), receipt);
  budget.close();
});

for (const Bridge of [ModelBridge, ModelBridgeProcess]) {
  test(`${Bridge.name} forwards past the former author limits and preserves metering`, async t => {
    const provider = createServer(async (request, response) => {
      for await (const chunk of request) void chunk;
      response.setHeader("content-type", "application/json");
      response.end(JSON.stringify({ id: "metered", choices: [{ message: { role: "assistant", content: "done" }, finish_reason: "stop" }], usage: { prompt_tokens: 14000, completion_tokens: 1000, total_tokens: 15000 } }));
    });
    await new Promise(resolve => provider.listen(0, "127.0.0.1", resolve));
    t.after(() => new Promise(resolve => provider.close(resolve)));
    const connection = defineConnection({ id: "fixture", provider: "fixture", model: "fixture-v1",
      endpoint: `http://127.0.0.1:${provider.address().port}`, credentialRef: "FIXTURE_KEY", protocol: "chat-completions",
      declaredLimits: { contextTokens: 64000, outputTokens: 4000 } });
    const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: {} });
    const bridge = new Bridge({ sessionPlan, connections: [connection], credentialResolver: async () => "fixture" });
    t.after(() => bridge.close());
    const budget = new TaskBudget({ startedAt: Date.now() - 86_400_000 });
    const address = await bridge.start();
    await bridge.beginTaskBudget(budget);
    await bridge.beginExecutionLease(createExecutionLease({ sessionPlan, runId: "unrestricted", budgetId: budget.id }));
    for (let index = 0; index < 20; index++) {
      const response = await fetch(`${address.baseUrl}/v1/responses`, { method: "POST",
        headers: { authorization: `Bearer ${address.token}`, "content-type": "application/json",
          "x-codex-turn-metadata": JSON.stringify({ thread_id: "root" }) },
        body: JSON.stringify({ model: "fixture", stream: false, input: "hello", max_output_tokens: 1000 }) });
      const body = await response.json();
      assert.equal(response.status, 200, JSON.stringify(body));
    }
    await bridge.endTaskBudget(budget);
    assert.deepEqual({ requests: budget.snapshot().requests, tokens: budget.snapshot().tokens, overrun: budget.snapshot().overrun },
      { requests: 20, tokens: 300000, overrun: false });
  });
}
