import assert from "node:assert/strict";
import { createServer } from "node:http";
import { test } from "node:test";
import { ModelBridge, ModelBridgeProcess, TaskBudget, createExecutionLease, defineConnection, resolveSessionPlan } from "../src/index.mjs";
import { nativeFetch } from "../src/http-transport.mjs";
import { verifiedBaselineEvidence } from "../src/testing/index.mjs";

for (const [name, Bridge, custom] of [["native", ModelBridge, false], ["callback", ModelBridge, true], ["process", ModelBridgeProcess, false]]) {
  for (const mode of ["stream", "buffered", "nonstream", "stalled", "author-deadline", "cancelled"]) {
    test(`${name}: ${mode} keeps liveness separate from total duration`, { timeout: 15000 }, async t => {
      const provider = createServer((request, response) => {
        request.resume();
        request.on("end", () => {
          response.writeHead(200, { "content-type": mode === "nonstream" ? "application/json" : "text/event-stream" });
          let count = 0;
          const timer = setInterval(() => {
            if (mode === "stalled" && count) return;
            if (mode === "nonstream") response.write(" ");
            else response.write(`data: ${JSON.stringify({ id: "progress", choices: [{ index: 0, delta: { content: "章" }, finish_reason: null }] })}\n\n`);
            if (++count === 12) {
              clearInterval(timer);
              const usage = { prompt_tokens: 11, completion_tokens: 12, total_tokens: 23 };
              response.end(mode === "nonstream" ? JSON.stringify({ id: "progress", choices: [{ index: 0, message: { role: "assistant", content: "章".repeat(12) }, finish_reason: "stop" }], usage })
                : `data: ${JSON.stringify({ id: "progress", choices: [{ index: 0, delta: {}, finish_reason: "stop" }], usage })}\n\ndata: [DONE]\n\n`);
            }
          }, 40);
          response.on("close", () => clearInterval(timer));
        });
      });
      await new Promise(resolve => provider.listen(0, "127.0.0.1", resolve));
      t.after(() => new Promise(resolve => { provider.closeAllConnections(); provider.close(resolve); }));
      const connection = defineConnection({ id: "fixture", provider: "fixture", model: "fixture", endpoint: `http://127.0.0.1:${provider.address().port}`,
        protocol: "chat-completions", credentialRef: "FIXTURE", declaredLimits: { contextTokens: 64000, outputTokens: 4000 } });
      const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: {} });
      const observations = [];
      const bridge = new Bridge({ sessionPlan, connections: [connection], credentialResolver: async () => "fixture", requestTimeoutMs: 200,
        ...(custom ? { fetchImpl: (...args) => nativeFetch(...args) } : {}),
        responseObserver: value => observations.push(value),
        ...(mode === "buffered" ? { responseBarrier: { rootTool: "unused", releaseFile: "/unused", timeoutMs: 1000 } } : {}) });
      t.after(() => bridge.close());
      const address = await bridge.start();
      const budget = new TaskBudget(mode === "author-deadline" ? { maxDurationMs: 300 } : {});
      await bridge.beginTaskBudget(budget);
      await bridge.beginExecutionLease(createExecutionLease({ sessionPlan, runId: "idle-test", budgetId: budget.id }));
      const started = Date.now();
      const result = await fetch(`${address.baseUrl}/v1/responses`, { method: "POST", headers: { authorization: `Bearer ${address.token}`,
        "x-codex-turn-metadata": JSON.stringify({ thread_id: "root" }) }, body: JSON.stringify({ model: "fixture", input: [], stream: mode !== "nonstream" }) });
      assert.equal(result.status, 200);
      const reading = result.text();
      if (mode === "cancelled") {
        const rejected = assert.rejects(reading);
        await bridge.cancelActiveRequests();
        await rejected;
      } else if (["stalled", "author-deadline"].includes(mode)) {
        await assert.rejects(reading);
      } else {
        const body = await reading;
        assert.match(body, /completed/);
        assert.ok(Date.now() - started > 400, "healthy response exceeds the idle window");
      }
      await bridge.close();
      if (["stream", "buffered", "nonstream"].includes(mode)) {
        assert.equal(budget.snapshot().reservedTokens, 0);
        assert.equal(observations.at(-1).normalizedUsage.totalTokens, 23);
      }
      if (mode === "stalled") assert.equal(observations.at(-1).errorCategory, "timeout");
    });
  }
}
