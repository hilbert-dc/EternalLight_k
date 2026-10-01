import assert from "node:assert/strict";
import { createServer } from "node:http";
import { test } from "node:test";
import { ModelBridge, ModelBridgeProcess, TaskBudget, createExecutionLease, defineConnection, resolveSessionPlan } from "../src/index.mjs";
import { verifiedBaselineEvidence } from "../src/testing/index.mjs";

for (const Bridge of [ModelBridge, ModelBridgeProcess]) {
  for (const [reason, diagnostic, retryable] of [
    ["max_output_tokens", "provider_output_limit", false],
    ["server_error", "provider_server_error", true],
    ["unknown_vendor_reason", "provider_stream_failed", false],
  ]) {
    test(`${Bridge.name}: ${reason} retains usage and never releases a failed stream's tool`, async t => {
      const provider = createServer((request, response) => {
        const chunks = [];
        request.on("data", chunk => chunks.push(chunk));
        request.on("end", () => {
          const body = JSON.parse(Buffer.concat(chunks));
          const tool = { type: "function_call", id: "pending", call_id: "write-once",
            name: body.tools[0].name, arguments: "{}" };
          const events = [
            { type: "response.created", response: { id: "response-fixture" } },
            { type: "response.output_item.done", output_index: 0, item: tool },
            { type: reason === "max_output_tokens" ? "response.incomplete" : "response.failed", response: {
              id: "response-fixture", status: "failed", output: [tool],
              incomplete_details: reason === "max_output_tokens" ? { reason } : null,
              error: reason === "max_output_tokens" ? null : { code: reason, message: "PRIVATE_PROVIDER_TEXT" },
              usage: { input_tokens: 11, output_tokens: 7, total_tokens: 18 },
            } },
          ];
          response.writeHead(200, { "content-type": "text/event-stream" });
          response.end(events.map(event => `data: ${JSON.stringify(event)}\n\n`).join(""));
        });
      });
      await new Promise(resolve => provider.listen(0, "127.0.0.1", resolve));
      t.after(() => new Promise(resolve => { provider.closeAllConnections(); provider.close(resolve); }));
      const connection = defineConnection({ id: "fixture", provider: "fixture", model: "fixture",
        endpoint: `http://127.0.0.1:${provider.address().port}`, protocol: "responses", credentialRef: "FIXTURE",
        declaredLimits: { contextTokens: 64000, outputTokens: 4000 } });
      const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: {} });
      const observations = [];
      const bridge = new Bridge({ sessionPlan, connections: [connection], credentialResolver: async () => "fixture",
        responseObserver: value => observations.push(value) });
      t.after(() => bridge.close());
      const budget = new TaskBudget();
      const settled = [];
      const settle = budget.settleReservation.bind(budget);
      budget.settleReservation = (id, usage, options) => { settled.push({ usage, options }); return settle(id, usage, options); };
      const address = await bridge.start();
      await bridge.beginTaskBudget(budget);
      await bridge.beginExecutionLease(createExecutionLease({ sessionPlan, runId: "terminal-test", budgetId: budget.id }));
      const result = await fetch(`${address.baseUrl}/v1/responses`, { method: "POST", headers: {
        authorization: `Bearer ${address.token}`, "x-codex-turn-metadata": JSON.stringify({ thread_id: "root" }),
      }, body: JSON.stringify({ model: "fixture", stream: true, input: [],
        tools: [{ type: "function", name: "write", parameters: { type: "object", properties: {} } }] }) });
      const text = await result.text();
      assert.doesNotMatch(text, /write-once|function_call|PRIVATE_PROVIDER_TEXT|unknown_vendor_reason/);
      const terminal = text.split("\n").filter(line => line.startsWith("data: "))
        .map(line => JSON.parse(line.slice(6))).find(event => event.type === "response.failed");
      assert.deepEqual(terminal.response.error, { code: retryable ? "server_error" : "invalid_prompt", message: diagnostic });
      assert.deepEqual(settled.map(value => value.usage), [{ input_tokens: 11, output_tokens: 7, total_tokens: 18 }]);
      assert.equal(budget.snapshot().reservedTokens, 0);
      assert.deepEqual(observations.map(({ errorCategory, errorCode, retryable }) => ({ errorCategory, errorCode, retryable })),
        [{ errorCategory: "provider", errorCode: diagnostic, retryable }]);
    });
  }
}
