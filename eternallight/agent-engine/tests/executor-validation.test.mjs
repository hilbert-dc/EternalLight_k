import assert from "node:assert/strict";
import { createServer } from "node:http";
import { test } from "node:test";
import { ModelBridge, TaskBudget, createExecutionLease, defineConnection, resolveSessionPlan } from "../src/index.mjs";
import { verifiedBaselineEvidence } from "../src/testing/index.mjs";

for (const protocol of ["responses", "chat-completions"]) for (const stream of [false, true]) {
  for (const delegate of [false, true]) test(`${protocol} stream=${stream} delegates schema only with trusted exact executor binding=${delegate}`, async t => {
    const provider = createServer(async (req, res) => {
      let raw = ""; for await (const chunk of req) raw += chunk;
      const body = JSON.parse(raw);
      assert.equal(JSON.stringify(body).includes("executorValidatedTools"), false, "host policy must never enter provider input");
      const name = protocol === "responses" ? body.tools[0].name : body.tools[0].function.name;
      const item = { type: "function_call", id: "item-one", call_id: "bad-arguments", name, arguments: "{}" };
      const response = { id: "response-one", status: "completed", output: [item], usage: { input_tokens: 4, output_tokens: 2, total_tokens: 6 } };
      res.writeHead(200, { "content-type": stream ? "text/event-stream" : "application/json" });
      const chat = { id: "chat-one", choices: [{ finish_reason: "tool_calls", message: { tool_calls: [{ id: item.call_id, type: "function", function: { name, arguments: item.arguments } }] } }], usage: { prompt_tokens: 4, completion_tokens: 2, total_tokens: 6 } };
      if (!stream) return res.end(JSON.stringify(protocol === "responses" ? response : chat));
      const events = protocol === "responses" ? [{ type: "response.created", response: { id: response.id } },
        { type: "response.output_item.done", output_index: 0, item }, { type: "response.completed", response }]
        : [{ id: "chat-one", choices: [{ finish_reason: "tool_calls", delta: { tool_calls: [{ index: 0, id: item.call_id, type: "function", function: { name, arguments: "{}" } }] } }] }, { id: "chat-one", choices: [], usage: chat.usage }];
      res.end(events.map(event => `data: ${JSON.stringify(event)}\n\n`).join("") + (protocol === "chat-completions" ? "data: [DONE]\n\n" : ""));
    });
    await new Promise(resolve => provider.listen(0, "127.0.0.1", resolve));
    t.after(() => { provider.closeAllConnections(); provider.close(); });
    const connection = defineConnection({ id: "fixture", provider: "fixture", model: "fixture", endpoint: `http://127.0.0.1:${provider.address().port}`, credentialRef: "FIXTURE_KEY", protocol,
      declaredLimits: { contextTokens: 64000, outputTokens: 1000 }, protocolOptions: { namespaceToolsMode: "flatten" } });
    const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection) });
    const bridge = new ModelBridge({ sessionPlan, connections: [connection], credentialResolver: async () => "fixture",
      executorValidatedTools: delegate ? [{ namespace: "mcp__fixture", name: "write" }] : [{ namespace: "mcp__other", name: "write" }] });
    t.after(() => bridge.close());
    const budget = new TaskBudget(); const address = await bridge.start();
    await bridge.beginTaskBudget(budget);
    await bridge.beginExecutionLease(createExecutionLease({ sessionPlan, runId: "executor-validation", budgetId: budget.id }));
    const response = await fetch(`${address.baseUrl}/v1/responses`, { method: "POST", headers: { authorization: `Bearer ${address.token}`, "x-codex-turn-metadata": JSON.stringify({ thread_id: "root" }) },
      body: JSON.stringify({ model: "fixture", input: [], stream, tools: [{ type: "namespace", name: "mcp__fixture", tools: [{ type: "function", name: "write", parameters: { type: "object", properties: { value: { type: "string" } }, required: ["value"], additionalProperties: false } }] }] }) });
    const raw = await response.text();
    const result = stream ? raw.split("\n").filter(line => line.startsWith("data: ") && line !== "data: [DONE]").map(line => JSON.parse(line.slice(6))).at(-1).response : JSON.parse(raw);
    if (delegate) {
      assert.equal(result.status, "completed");
      assert.equal(result.output[0].call_id, "bad-arguments");
      assert.equal(result.output[0].namespace, "mcp__fixture");
      assert.equal(result.output[0].name, "write");
      assert.equal(result.output[0].arguments, "{}"); // original call, never fake a tool result
    } else assert.match(JSON.stringify(result.error), /invalid_tool_arguments/);
    for (let i = 0; i < 100 && budget.snapshot().reservedTokens; i++) await new Promise(resolve => setTimeout(resolve, 10));
    assert.equal(budget.snapshot().tokens, 6);
    assert.equal(budget.snapshot().requests, 1);
    assert.equal(budget.snapshot().reservedTokens, 0);
  });
}
