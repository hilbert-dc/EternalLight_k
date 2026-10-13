import assert from "node:assert/strict";
import { createServer } from "node:http";
import { test } from "node:test";
import { ModelBridge, TaskBudget, createExecutionLease, defineConnection, resolveSessionPlan } from "../src/index.mjs";
import { verifiedBaselineEvidence } from "../src/testing/index.mjs";

const sse = events => events.map(event => `data: ${JSON.stringify(event)}\n\n`).join("");
const responseUsage = { input_tokens: 40, output_tokens: 10, total_tokens: 50 };
const chatUsage = { prompt_tokens: 40, completion_tokens: 10, total_tokens: 50 };

const diagnosticPrefix = "invalid_tool_arguments; toolValidation=";
function diagnostic(message) {
  assert.ok(Buffer.byteLength(message) <= 400);
  assert.ok(message.startsWith(diagnosticPrefix));
  assert.equal(message.includes("never-execute-private-value"), false);
  assert.equal(message.includes("private-key"), false);
  return JSON.parse(message.slice(diagnosticPrefix.length));
}

for (const [protocol, mode, variant = "required"] of [
  ["responses", "json"], ["responses", "stream"], ["responses", "delayed"], ["responses", "tail"],
  ["responses", "missing"], ["responses", "timeout"], ["responses", "cancel"], ["responses", "provider-http"],
  ["responses", "provider-stream"], ["responses", "invalid-usage"],
  ["chat-completions", "json"], ["chat-completions", "stream"], ["chat-completions", "missing"],
  ...["type", "enum", "additionalProperties", "bounded"].flatMap(variant => [
    ["responses", "json", variant], ["responses", "stream", variant],
  ]),
  ...["json", "stream", "delayed", "missing", "timeout", "cancel"].map(mode => ["responses", mode, "unauthorized"]),
  ...["duplicate", "choice-none", "choice-other", "oversize"].flatMap(variant => ["json", "stream", "delayed", "missing"].map(mode => ["responses", mode, variant])),
  ...["json", "stream", "missing"].map(mode => ["chat-completions", mode, "unauthorized"]),
  ["chat-completions", "json", "type"], ["chat-completions", "stream", "additionalProperties"],
]) test(`${protocol} ${mode} ${variant} preserves schema rejection, exact usage and zero execution`, { timeout: 10000 }, async t => {
  const field = "declared".repeat(30);
  const parameters = variant === "bounded" ? { type: "object", properties: { [field]: { type: "string" } } }
    : { type: "object", required: ["text"], properties: { text: { type: "string", ...(variant === "enum" ? { enum: ["allowed"] } : {}) } }, additionalProperties: false };
  const argumentsValue = variant === "oversize" ? { text: "x".repeat(270000) }
    : ["duplicate", "choice-none", "choice-other"].includes(variant) ? { text: "allowed" } : variant === "type" ? { text: 123 } : variant === "enum" ? { text: "never-execute-private-value" }
    : variant === "additionalProperties" ? { text: "allowed", "private-key": "never-execute-private-value" }
    : variant === "bounded" ? { [field]: false } : {};
  const expectedDiagnostic = { tool: "write", reason: variant === "bounded" ? "type" : variant,
    schemaPath: variant === "bounded" ? "unknown" : variant === "required" ? "/required" : variant === "additionalProperties" ? "/additionalProperties" : `/properties/text/${variant}`,
    ...(variant === "required" ? { missingField: "text", requiredFields: ["text"] } : {}) };
  const errorCode = variant === "unauthorized" ? "unauthorized_tool_call" : variant === "oversize" ? "provider_output_item_limit"
    : ["duplicate", "choice-none", "choice-other"].includes(variant) ? "invalid_tool_output" : "invalid_tool_arguments";
  const checkDiagnostic = message => {
    if (variant === "oversize") assert.ok(message.includes("outputItem="));
    else if (variant === "unauthorized") {
      const prefix = "unauthorized_tool_call; toolValidation=";
      assert.ok(message.startsWith(prefix));
      assert.deepEqual(JSON.parse(message.slice(prefix.length)), {tool:"n20__mcp__novel_workspace__read_creative_strategy",reason:"not_in_current_request",schemaPath:"/tools"});
      assert.ok(Buffer.byteLength(message) <= 400);
      assert.ok(!message.includes("never-execute-private-value"));
    }
    else if (errorCode !== "invalid_tool_arguments") assert.equal(message, errorCode);
    else assert.deepEqual(diagnostic(message), expectedDiagnostic);
  };
  let requests = 0;
  const provider = createServer(async (incoming, response) => {
    const chunks = [];
    for await (const chunk of incoming) chunks.push(chunk);
    const outbound = JSON.parse(Buffer.concat(chunks));
    requests++;
    const name = variant === "unauthorized" ? "n20__mcp__novel_workspace__read_creative_strategy"
      : protocol === "responses" ? outbound.tools[0].name : outbound.tools[0].function.name;
    const usage = mode === "missing" ? undefined : mode === "invalid-usage"
      ? { input_tokens: 40, output_tokens: 10, total_tokens: -1 } : responseUsage;
    const item = { type: "function_call", id: "fc_invalid", call_id: "invalid-once", name,
      arguments: JSON.stringify(argumentsValue), status: "completed" };
    const terminal = { type: "response.completed", response: { id: "response-invalid", status: "completed", output: variant === "duplicate" ? [item, { ...item, id: "fc_duplicate" }] : [item], usage } };
    if (mode === "provider-http") {
      response.writeHead(503, { "content-type": "application/json" });
      response.end(JSON.stringify({ error: { message: "arguments for write violate the declared schema: required property text" } }));
    } else if (mode === "provider-stream") {
      response.writeHead(200, { "content-type": "text/event-stream" });
      response.end(sse([{ type: "response.failed", response: { status: "failed", error: {
        code: "server_error", message: "arguments for write violate the declared schema: required property text" } } }]));
    } else if (mode === "json") {
      response.writeHead(200, { "content-type": "application/json" });
      response.end(JSON.stringify(protocol === "responses" ? terminal.response : { id: "chat-invalid", usage: chatUsage,
        choices: [{ finish_reason: "tool_calls", message: { tool_calls: [{ id: item.call_id, type: "function", function: { name, arguments: item.arguments } }] } }] }));
    } else if (protocol === "chat-completions") {
      response.writeHead(200, { "content-type": "text/event-stream" });
      response.end(sse([{ id: "chat-invalid", choices: [{ finish_reason: "tool_calls", delta: {
        tool_calls: [{ index: 0, id: item.call_id, type: "function", function: { name, arguments: item.arguments } }] } }] },
        { id: "chat-invalid", choices: [], ...(mode !== "missing" ? { usage: chatUsage } : {}) }]) + "data: [DONE]\n\n");
    } else {
      response.writeHead(200, { "content-type": "text/event-stream" });
      response.write(sse([{ type: "response.created", response: { id: "response-invalid" } },
        { type: "response.output_item.done", item }]));
      if (["timeout", "cancel"].includes(mode)) return;
      if (mode === "delayed") await new Promise(resolve => setTimeout(resolve, 20));
      response.end(mode === "tail" ? `data: ${JSON.stringify(terminal)}` : sse([terminal]) + "data: [DONE]\n\n");
    }
  });
  await new Promise(resolve => provider.listen(0, "127.0.0.1", resolve));
  t.after(() => new Promise(resolve => { provider.closeAllConnections(); provider.close(resolve); }));
  const connection = defineConnection({ id: "fixture", provider: "fixture", model: "fixture", protocol,
    endpoint: `http://127.0.0.1:${provider.address().port}`, credentialRef: "FIXTURE",
    declaredLimits: { contextTokens: 64000, outputTokens: 8192 } });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: {} });
  const observations = [], settlements = [];
  const bridge = new ModelBridge({ connections: [connection], sessionPlan, credentialResolver: async () => "fixture",
    requestTimeoutMs: mode === "timeout" ? 100 : 2000, responseObserver: value => observations.push(value) });
  t.after(() => bridge.close());
  const budget = new TaskBudget();
  const settle = budget.settleReservation.bind(budget);
  budget.settleReservation = (id, usage, options) => { settlements.push({ usage, options }); return settle(id, usage, options); };
  const address = await bridge.start();
  await bridge.beginTaskBudget(budget);
  await bridge.beginExecutionLease(createExecutionLease({ sessionPlan, runId: "invalid-arguments", budgetId: budget.id }));
  const stream = mode !== "json" && mode !== "provider-http";
  const result = await fetch(`${address.baseUrl}/v1/responses`, { method: "POST", headers: {
    authorization: `Bearer ${address.token}`, "x-codex-turn-metadata": JSON.stringify({ thread_id: "root" }),
  }, body: JSON.stringify({ model: "fixture", input: [], stream, tools: [{ type: "function", name: "write",
    parameters }], ...(variant === "choice-none" ? { tool_choice: "none" } : variant === "choice-other" ? { tool_choice: { type: "function", name: "other" }, tools: [{ type: "function", name: "write", parameters }, { type: "function", name: "other", parameters }] } : {}) }) });
  const cancellation = mode === "cancel" ? new Promise(resolve => setTimeout(() => resolve(bridge.cancelActiveRequests()), 20)) : null;
  if (!stream) {
    const payload = await result.json();
    assert.equal(result.status, mode === "provider-http" ? 502 : 400);
    assert.equal(payload.error.code, mode === "provider-http" ? undefined : errorCode);
    assert.equal(payload.error.retryable, mode === "provider-http" ? undefined : false);
    if (mode !== "provider-http") checkDiagnostic(payload.error.message);
  } else {
    const text = await result.text();
    const events = text.split("\n").filter(line => line.startsWith("data: ") && line !== "data: [DONE]").map(line => JSON.parse(line.slice(6)));
    assert.equal(events.some(event => event.type === "response.output_item.done" || event.type === "response.completed"), false);
    if (protocol === "responses") assert.equal(text.includes("never-execute-private-value"), false);
    const error = events.at(-1).response.error;
    if (mode === "provider-stream") assert.deepEqual(error, { code: "server_error", message: "provider_server_error" });
    else { assert.equal(error.code, "invalid_prompt"); checkDiagnostic(error.message); }
  }
  if (cancellation) assert.equal(await cancellation, 1);
  const deadline = Date.now() + 2000;
  while (budget.snapshot().reservedTokens && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 5));
  const known = !["missing", "timeout", "cancel", "provider-http", "provider-stream", "invalid-usage"].includes(mode);
  assert.equal(requests, 1);
  assert.equal(budget.snapshot().requests, 1);
  assert.equal(budget.snapshot().reservedTokens, 0);
  assert.equal(budget.snapshot().tokens, known ? 50 : 0);
  assert.equal(budget.snapshot().unknownTokens > 0, !known);
  assert.equal(settlements.length, 1);
  assert.equal(settlements[0].usage?.total_tokens ?? null, known ? 50 : null);
  assert.deepEqual(observations.map(({ status, errorCode, retryable }) => ({ status, errorCode, retryable })), [{ status: "failed",
    errorCode: mode === "provider-http" ? "provider_http_error" : mode === "provider-stream" ? "provider_server_error" : errorCode,
    retryable: ["provider-http", "provider-stream"].includes(mode) }]);
  if (!["provider-http", "provider-stream"].includes(mode)) checkDiagnostic(observations[0].errorMessage);
});
