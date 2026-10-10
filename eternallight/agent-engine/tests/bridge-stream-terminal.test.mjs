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

for (const Bridge of [ModelBridge, ModelBridgeProcess]) {
  for (const mode of ["reported", "reported-usage-only", "missing", "nonterminal", "postfinish-delta", "invalid", "cancelled"]) {
    test(`${Bridge.name}: ${mode} chat usage retains the correct charge after an interrupted stream`, { timeout: 10000 }, async t => {
      let providerResponse;
      let providerCalls = 0;
      const marker = "usage-boundary";
      const provider = createServer((request, response) => {
        const chunks = [];
        request.on("data", chunk => chunks.push(chunk));
        request.on("end", () => {
          providerCalls++;
          const body = JSON.parse(Buffer.concat(chunks));
          assert.equal(body.stream, true);
          assert.equal(body.stream_options.include_usage, true);
          providerResponse = response;
          const frame = { id: "usage-fixture", choices: [{ index: 0,
            delta: { role: "assistant", content: marker },
            finish_reason: ["nonterminal", "postfinish-delta", "reported-usage-only"].includes(mode) ? null : "stop" }] };
          if (["reported", "nonterminal", "postfinish-delta", "invalid"].includes(mode)) {
            frame.usage = { prompt_tokens: 11, completion_tokens: 7,
              total_tokens: mode === "invalid" ? "invalid" : 18 };
          }
          response.writeHead(200, { "content-type": "text/event-stream" });
          if (["postfinish-delta", "reported-usage-only"].includes(mode)) {
            response.write(`data: ${JSON.stringify({ id: "usage-fixture",
              choices: [{ index: 0, delta: {}, finish_reason: "stop" }] })}\n\n`);
          }
          if (mode === "reported-usage-only") {
            response.write(`data: ${JSON.stringify({ id: "usage-fixture", choices: [],
              usage: { prompt_tokens: 11, completion_tokens: 7, total_tokens: 18 } })}\n\n`);
          }
          // No [DONE] or EOF: the client-observed delta proves the native decoder
          // accepted this frame before the transport is interrupted below.
          response.write(`data: ${JSON.stringify(frame)}\n\n`);
        });
      });
      await new Promise(resolve => provider.listen(0, "127.0.0.1", resolve));
      t.after(() => new Promise(resolve => { provider.closeAllConnections(); provider.close(resolve); }));
      const connection = defineConnection({ id: "fixture", provider: "fixture", model: "fixture",
        endpoint: `http://127.0.0.1:${provider.address().port}`, protocol: "chat-completions", credentialRef: "FIXTURE",
        declaredLimits: { contextTokens: 64000, outputTokens: 4000 } });
      const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: {} });
      const bridge = new Bridge({ sessionPlan, connections: [connection], credentialResolver: async () => "fixture" });
      t.after(() => bridge.close());
      const budget = new TaskBudget({ maxRequests: 1, maxTokens: 10000 });
      const reservations = [], settlements = [];
      const reserve = budget.reserveRequest.bind(budget);
      const settle = budget.settleReservation.bind(budget);
      budget.reserveRequest = parameters => {
        const id = reserve(parameters);
        reservations.push({ id, parameters: structuredClone(parameters) });
        return id;
      };
      budget.settleReservation = (id, usage, options) => {
        settlements.push({ id, usage: structuredClone(usage), options: structuredClone(options) });
        return settle(id, usage, options);
      };
      const address = await bridge.start();
      await bridge.beginTaskBudget(budget);
      await bridge.beginExecutionLease(createExecutionLease({ sessionPlan, runId: `interrupted-${mode}`, budgetId: budget.id }));
      const result = await fetch(`${address.baseUrl}/v1/responses`, { method: "POST", headers: {
        authorization: `Bearer ${address.token}`, "x-codex-turn-metadata": JSON.stringify({ thread_id: "root" }),
      }, body: JSON.stringify({ model: "fixture", stream: true, max_output_tokens: 128,
        input: [{ type: "message", role: "user", content: [{ type: "input_text", text: "bounded request" }] }] }) });
      assert.equal(result.status, 200);
      const reader = result.body.getReader();
      const decoder = new TextDecoder();
      let received = "";
      while (!received.includes(marker)) {
        const { value, done } = await reader.read();
        assert.equal(done, false, "decoder acceptance must precede interruption");
        received += decoder.decode(value, { stream: true });
      }
      assert.equal(providerCalls, 1);
      const interrupted = assert.rejects(async () => {
        for (;;) {
          const { value, done } = await reader.read();
          if (done) return;
          received += decoder.decode(value, { stream: true });
        }
      });
      if (mode === "cancelled") assert.equal(await bridge.cancelActiveRequests(), 1);
      else providerResponse.socket.destroy();
      await interrupted;
      await bridge.cancelActiveRequests();
      assert.doesNotMatch(received, /response\.completed|response\.output_item\.done/);
      assert.equal(reservations.length, 1);
      assert.equal(settlements.length, 1, "one original reservation settles exactly once");
      assert.equal(settlements[0].id, reservations[0].id);
      const reported = ["reported", "reported-usage-only"].includes(mode);
      const expectedUsage = reported ? { total_tokens: 18, input_tokens: 11, output_tokens: 7 } : null;
      assert.deepEqual(settlements[0].usage, expectedUsage);
      assert.equal(settlements[0].options.outcome, "unknown");
      const snapshot = budget.snapshot();
      const reserved = reservations[0].parameters.estimatedInputTokens + (reservations[0].parameters.maxOutputTokens ?? 0);
      assert.equal(snapshot.requests, 1, "an already sent request is never refunded");
      assert.equal(snapshot.tokens, reported ? 18 : 0);
      assert.equal(snapshot.unknownTokens, reported ? 0 : reserved);
      assert.equal(snapshot.reservedTokens, 0);
      assert.equal(snapshot.overrun, false);
      const settledSnapshot = structuredClone(snapshot);
      await bridge.cancelActiveRequests();
      assert.equal(settlements.length, 1);
      assert.deepEqual(budget.snapshot(), settledSnapshot);
    });
  }
}
