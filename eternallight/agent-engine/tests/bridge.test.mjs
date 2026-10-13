import assert from "node:assert/strict";
import { createServer, request as httpRequest } from "node:http";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

import { CAPABILITIES, DELEGATION_EXECUTION_CAPABILITIES, ModelBridge, ModelBridgeProcess, TaskBudget, createExecutionLease, defineConnection, resolveSessionPlan } from "../src/index.mjs";
import { verifiedBaselineEvidence } from "../src/testing/index.mjs";

function codexHeaders(token = "bridge-secret", threadId = "thread-root", parentThreadId = null, metadata = {}) {
  return {
    authorization: `Bearer ${token}`,
    "content-type": "application/json",
    "x-codex-turn-metadata": JSON.stringify({ thread_id: threadId, ...(parentThreadId ? { parent_thread_id: parentThreadId } : {}), ...metadata }),
  };
}

function activateBridge(bridge, sessionPlan, budget = new TaskBudget()) {
  bridge.beginTaskBudget(budget);
  const lease = createExecutionLease({ sessionPlan, runId: `test-${budget.id}`, budgetId: budget.id });
  bridge.beginExecutionLease(lease);
  return { budget, lease };
}

async function listen(server) {
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  return `http://127.0.0.1:${server.address().port}`;
}

async function waitUntil(predicate, timeoutMs = 2_000) {
  const deadline = Date.now() + timeoutMs;
  while (!predicate()) {
    if (Date.now() >= deadline) throw new Error("condition timed out");
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
}

test("bridge authenticates Codex, routes by logical model, and emits Responses SSE", async (t) => {
  const downstreamRequests = [];
  const requestObservations = [];
  const responseObservations = [];
  const downstream = createServer((request, response) => {
    const chunks = [];
    request.on("data", (chunk) => chunks.push(chunk));
    request.on("end", () => {
      downstreamRequests.push({ headers: request.headers, path: request.url, body: JSON.parse(Buffer.concat(chunks).toString("utf8")) });
      response.writeHead(200, { "content-type": "text/event-stream" });
      response.write('data: {"id":"chat-1","choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_9","function":{"name":"f__read_context","arguments":"{\\"projectId\\":"}}]}}]}\n\n');
      setTimeout(() => response.end('data: {"id":"chat-1","choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\\"p1\\"}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":5,"completion_tokens":2,"total_tokens":7}}\n\ndata: [DONE]\n\n'), 10);
    });
  });
  const endpoint = await listen(downstream);
  t.after(() => new Promise((resolve) => { downstream.closeAllConnections(); downstream.close(resolve); }));
  const connection = defineConnection({ id: "writer", provider: "test", endpoint, model: "writer-v1", credentialRef: "WRITER_API_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 } });
  const evidence = verifiedBaselineEvidence(connection, { protocol: connection.protocol });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence, policy: { requiredCapabilities: DELEGATION_EXECUTION_CAPABILITIES } });
  const bridge = new ModelBridge({
    sessionPlan,
    connections: [connection],
    credentialResolver: async (reference) => reference === "WRITER_API_KEY" ? "downstream-secret" : null,
    token: "bridge-secret",
    requestObserver: async (observation) => requestObservations.push(observation),
    responseObserver: async (observation) => responseObservations.push(observation),
  });
  const started = await bridge.start();
  activateBridge(bridge, sessionPlan);
  t.after(() => bridge.close());

  const unauthorized = await fetch(`${started.baseUrl}/v1/responses`, { method: "POST", body: "{}", headers: { "content-type": "application/json" } });
  assert.equal(unauthorized.status, 401);
  const response = await fetch(`${started.baseUrl}/v1/responses`, {
    method: "POST",
    headers: codexHeaders("bridge-secret", "thread-root", null, { turn_id: "turn-1", root_turn_id: "turn-root", session_id: "session-1" }),
    body: JSON.stringify({ model: "writer", input: [{ type: "message", role: "user", content: [{ type: "input_text", text: "Go" }] }], tools: [{ type: "function", name: "read_context", parameters: { type: "object" } }], stream: true }),
  });
  const stream = await response.text();
  assert.equal(response.status, 200, stream);
  assert.match(stream, /event: response\.created/);
  assert.match(stream, /"call_id":"call_9"/);
  assert.match(stream, /event: response\.completed/);
  assert.equal(downstreamRequests.length, 1);
  assert.equal(downstreamRequests[0].headers.authorization, "Bearer downstream-secret");
  assert.equal(downstreamRequests[0].body.model, "writer-v1");
  assert.equal(downstreamRequests[0].body.stream, true);
  assert.equal(requestObservations.length, 1);
  assert.equal(requestObservations[0].requestMetrics.maxTokens, downstreamRequests[0].body.max_tokens);
  assert.equal(requestObservations[0].requestMetrics.bodyBytes, Buffer.byteLength(JSON.stringify(downstreamRequests[0].body)));
  assert.equal(responseObservations[0].normalizedUsage.totalTokens, 7);
  assert.deepEqual({
    connectionId: requestObservations[0].connectionId,
    logicalModel: requestObservations[0].logicalModel,
    threadId: requestObservations[0].threadId,
    parentThreadId: requestObservations[0].parentThreadId,
    turnId: requestObservations[0].turnId,
    rootTurnId: requestObservations[0].rootTurnId,
    sessionId: requestObservations[0].sessionId,
  }, {
    connectionId: "writer",
    logicalModel: "writer",
    threadId: "thread-root",
    parentThreadId: null,
    turnId: "turn-1",
    rootTurnId: "turn-root",
    sessionId: "session-1",
  });
  assert.match(requestObservations[0].runId, /^test-/);
  assert.equal(typeof requestObservations[0].requestId, "string");
  assert.equal(responseObservations.length, 1);
  assert.equal(responseObservations[0].requestId, requestObservations[0].requestId);
  assert.deepEqual(responseObservations[0].toolCalls.map(({ argumentValueDigests, ...call }) => call), [{ callId: "call_9", name: "read_context", namespace: null }]);
  assert.match(responseObservations[0].toolCalls[0].argumentValueDigests.projectId, /^[a-f0-9]{64}$/);
});

test("bridge applies immutable root and child tool policies before provider encoding", async (t) => {
  const root = mkdtempSync(join(tmpdir(), "agent-engine-tool-policy-"));
  const childPhaseFile = join(root, "child-phase.json");
  const childCompletionFile = join(root, "child-complete.json");
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const connection = defineConnection({ id: "writer", provider: "test", endpoint: "https://provider.invalid", model: "writer-v1", credentialRef: "WRITER_API_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 } });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: { requiredCapabilities: DELEGATION_EXECUTION_CAPABILITIES, allowedChildConnectionIds: [connection.id] } });
  const downstreamTools = [];
  const downstreamToolChoices = [];
  const bridge = new ModelBridge({
    sessionPlan,
    connections: [connection],
    credentialResolver: async () => "downstream-secret",
    token: "bridge-secret",
    toolPolicy: { rootAllowed: ["root_tool"], childPhaseFile, childCompletionFile, childBeforePhaseAllowed: ["child_tool"], childAfterPhaseAllowed: ["root_tool"], childToolChoiceRequired: true },
    fetchImpl: async (_url, options) => {
      const body = JSON.parse(options.body);
      downstreamTools.push((body.tools ?? []).map((tool) => tool.function.name));
      downstreamToolChoices.push(body.tool_choice ?? null);
      const message = body.tool_choice === "required"
        ? { role: "assistant", tool_calls: [{ id: `call-${downstreamTools.length}`, type: "function", function: { name: body.tools[0].function.name, arguments: "{}" } }] }
        : { role: "assistant", content: "done" };
      return new Response(JSON.stringify({ id: "chat-policy", choices: [{ message, finish_reason: body.tool_choice === "required" ? "tool_calls" : "stop" }], usage: { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 } }), { status: 200, headers: { "content-type": "application/json" } });
    },
  });
  const listening = await bridge.start();
  activateBridge(bridge, sessionPlan);
  t.after(() => bridge.close());
  const tools = ["root_tool", "child_tool", "unrelated"].map((name) => ({ type: "function", name, parameters: { type: "object" } }));
  const invoke = (threadId, parentThreadId = null) => fetch(`${listening.baseUrl}/v1/responses`, {
    method: "POST",
    headers: codexHeaders("bridge-secret", threadId, parentThreadId),
    body: JSON.stringify({ model: "writer", input: [], tools, stream: false }),
  });
  assert.equal((await invoke("thread-root")).status, 200);
  assert.equal((await invoke("thread-child", "thread-root")).status, 200);
  writeFileSync(childPhaseFile, "{}\n");
  assert.equal((await invoke("thread-child", "thread-root")).status, 200);
  writeFileSync(childCompletionFile, "{}\n");
  const completedResponse = await invoke("thread-child", "thread-root");
  assert.equal(completedResponse.status, 200, await completedResponse.text());
  assert.deepEqual(downstreamTools, [["f__root_tool"], ["f__child_tool"], ["f__root_tool"], []]);
  assert.deepEqual(downstreamToolChoices, [null, "required", "required", "none"]);
});

test("bridge can hold a selected root tool until a deterministic execution barrier opens", async (t) => {
  const root = mkdtempSync(join(tmpdir(), "agent-engine-response-barrier-"));
  const releaseFile = join(root, "release.json");
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const connection = defineConnection({ id: "writer", provider: "test", endpoint: "https://provider.invalid", model: "writer-v1", credentialRef: "WRITER_API_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 } });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: {} });
  const bridge = new ModelBridge({
    sessionPlan,
    connections: [connection],
    credentialResolver: async () => "downstream-secret",
    token: "bridge-secret",
    responseBarrier: { rootTool: "send_message", releaseFile, timeoutMs: 2_000 },
    fetchImpl: async () => new Response(JSON.stringify({
      id: "chat-barrier",
      choices: [{ message: { role: "assistant", tool_calls: [{ id: "call-send", type: "function", function: { name: "f__send_message", arguments: "{}" } }] }, finish_reason: "tool_calls" }],
      usage: { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 },
    }), { status: 200, headers: { "content-type": "application/json" } }),
  });
  const listening = await bridge.start();
  activateBridge(bridge, sessionPlan);
  t.after(() => bridge.close());
  const pending = fetch(`${listening.baseUrl}/v1/responses`, {
    method: "POST",
    headers: codexHeaders(),
    body: JSON.stringify({ model: "writer", input: [], tools: [{ type: "function", name: "send_message", parameters: { type: "object" } }], stream: false }),
  });
  const beforeRelease = await Promise.race([pending.then(() => "completed"), new Promise((resolve) => setTimeout(() => resolve("pending"), 50))]);
  assert.equal(beforeRelease, "pending");
  writeFileSync(releaseFile, "{}\n");
  assert.equal((await pending).status, 200);
});

test("closing the bridge aborts an in-flight downstream request", async () => {
  const connection = defineConnection({ id: "writer", provider: "test", endpoint: "https://provider.invalid", model: "writer-v1", credentialRef: "WRITER_API_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 } });
  const evidence = verifiedBaselineEvidence(connection, { protocol: connection.protocol });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence, policy: {} });
  let downstreamStarted;
  const started = new Promise((resolve) => { downstreamStarted = resolve; });
  let aborted = false;
  const bridge = new ModelBridge({
    sessionPlan,
    connections: [connection],
    credentialResolver: async () => "downstream-secret",
    token: "bridge-secret",
    fetchImpl: async (_url, { signal }) => new Promise((_resolve, reject) => {
      downstreamStarted();
      signal.addEventListener("abort", () => {
        aborted = true;
        reject(Object.assign(new Error("aborted"), { name: "AbortError" }));
      }, { once: true });
    }),
  });
  const listening = await bridge.start();
  activateBridge(bridge, sessionPlan);
  const client = fetch(`${listening.baseUrl}/v1/responses`, {
    method: "POST",
    headers: codexHeaders(),
    body: JSON.stringify({ model: "writer", input: [], stream: true }),
  });
  await started;
  await bridge.close();
  const response = await client;
  assert.equal(response.status, 499);
  assert.equal(aborted, true);
});

test("cancelling active bridge requests aborts the entire in-flight task tree without closing the bridge", async (t) => {
  const connection = defineConnection({ id: "writer", provider: "test", endpoint: "https://provider.invalid", model: "writer-v1", credentialRef: "WRITER_API_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 } });
  const evidence = verifiedBaselineEvidence(connection, { protocol: connection.protocol });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence, policy: {} });
  let downstreamStarted;
  const started = new Promise((resolve) => { downstreamStarted = resolve; });
  let aborted = false;
  const bridge = new ModelBridge({
    sessionPlan,
    connections: [connection],
    credentialResolver: async () => "downstream-secret",
    token: "bridge-secret",
    fetchImpl: async (_url, { signal }) => new Promise((_resolve, reject) => {
      downstreamStarted();
      signal.addEventListener("abort", () => {
        aborted = true;
        reject(Object.assign(new Error("aborted"), { name: "AbortError" }));
      }, { once: true });
    }),
  });
  const listening = await bridge.start();
  const { budget } = activateBridge(bridge, sessionPlan);
  let settlementStarted;
  let releaseSettlement;
  const settling = new Promise((resolve) => { settlementStarted = resolve; });
  const settlementBarrier = new Promise((resolve) => { releaseSettlement = resolve; });
  const settleReservation = budget.settleReservation.bind(budget);
  budget.settleReservation = async (...args) => {
    settlementStarted();
    await settlementBarrier;
    return settleReservation(...args);
  };
  t.after(() => bridge.close());
  const client = fetch(`${listening.baseUrl}/v1/responses`, {
    method: "POST",
    headers: codexHeaders(),
    body: JSON.stringify({ model: "writer", input: [], stream: true }),
  });
  await started;
  let cancellationFinished = false;
  const cancellation = bridge.cancelActiveRequests().then((count) => { cancellationFinished = true; return count; });
  await settling;
  assert.equal(cancellationFinished, false);
  assert.equal(bridge.controllers.size, 1);
  releaseSettlement();
  assert.equal(await cancellation, 1);
  assert.equal(bridge.controllers.size, 0);
  const response = await client;
  assert.equal(response.status, 499);
  assert.equal(aborted, true);
  assert.equal(bridge.server.listening, true);
  const afterCancellation = await fetch(`${listening.baseUrl}/v1/responses`, {
    method: "POST",
    headers: codexHeaders(),
    body: JSON.stringify({ model: "writer", input: [], stream: true }),
  });
  assert.equal(afterCancellation.status, 403);
  assert.match((await afterCancellation.json()).error.message, /revoked/);
});

test("runtime authentication failure revokes the execution before another provider request", async (t) => {
  const connection = defineConnection({ id: "writer", provider: "test", endpoint: "https://provider.invalid", model: "writer-v1", credentialRef: "WRITER_API_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 } });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: {} });
  let providerRequests = 0;
  const failures = [];
  const bridge = new ModelBridge({
    sessionPlan,
    connections: [connection],
    credentialResolver: async () => "rejected-secret",
    token: "bridge-secret",
    fetchImpl: async () => {
      providerRequests += 1;
      return new Response(JSON.stringify({ error: { type: "authentication_error", message: "expired credential" } }), { status: 401, headers: { "content-type": "application/json" } });
    },
    qualificationObserver: async (failure) => failures.push(failure),
  });
  const listening = await bridge.start();
  activateBridge(bridge, sessionPlan);
  t.after(() => bridge.close());
  const invoke = () => fetch(`${listening.baseUrl}/v1/responses`, {
    method: "POST",
    headers: codexHeaders(),
    body: JSON.stringify({ model: "writer", input: [], stream: false }),
  });
  assert.equal((await invoke()).status, 401);
  assert.equal((await invoke()).status, 403);
  assert.equal(providerRequests, 1);
  assert.equal(failures.length, 1);
  assert.equal(failures[0].connectionId, "writer");
});

test("bridge bounds concurrent downstream work and times out stalled providers", async (t) => {
  const connection = defineConnection({ id: "writer", provider: "test", endpoint: "https://provider.invalid", model: "writer-v1", credentialRef: "WRITER_API_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 } });
  const evidence = verifiedBaselineEvidence(connection, { protocol: connection.protocol });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence, policy: {} });
  let downstreamStarted;
  const started = new Promise((resolve) => { downstreamStarted = resolve; });
  const bridge = new ModelBridge({
    sessionPlan,
    connections: [connection],
    credentialResolver: async () => "downstream-secret",
    token: "bridge-secret",
    maxConcurrentRequests: 1,
    requestTimeoutMs: 25,
    fetchImpl: async (_url, { signal }) => new Promise((_resolve, reject) => {
      downstreamStarted();
      signal.addEventListener("abort", () => reject(Object.assign(new Error("aborted"), { name: "AbortError" })), { once: true });
    }),
  });
  const listening = await bridge.start();
  activateBridge(bridge, sessionPlan);
  t.after(() => bridge.close());
  const request = () => fetch(`${listening.baseUrl}/v1/responses`, {
    method: "POST",
    headers: codexHeaders(),
    body: JSON.stringify({ model: "writer", input: [], stream: true }),
  });

  const first = request();
  await started;
  const rejected = await request();
  assert.equal(rejected.status, 429);
  const timedOut = await first;
  assert.equal(timedOut.status, 504);
  assert.equal((await timedOut.json()).error.type, "timeout");
});

test("bridge times out an incomplete upload and releases its concurrency slot", async (t) => {
  const connection = defineConnection({ id: "writer", provider: "test", endpoint: "https://provider.invalid", model: "writer-v1", credentialRef: "WRITER_API_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 } });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: {} });
  const bridge = new ModelBridge({ sessionPlan, connections: [connection], credentialResolver: async () => "secret", token: "bridge-secret", maxConcurrentRequests: 1, requestTimeoutMs: 30, fetchImpl: async () => { throw new Error("downstream must not run"); } });
  const listening = await bridge.start();
  activateBridge(bridge, sessionPlan);
  t.after(() => bridge.close());
  const url = new URL("/v1/responses", listening.baseUrl);
  const result = new Promise((resolve, reject) => {
    const request = httpRequest({ hostname: url.hostname, port: url.port, path: url.pathname, method: "POST", headers: { ...codexHeaders(), "content-length": "1000" } }, (response) => {
      const chunks = [];
      response.on("data", (chunk) => chunks.push(chunk));
      response.on("end", () => resolve({ status: response.statusCode, body: Buffer.concat(chunks).toString("utf8") }));
    });
    request.on("error", reject);
    request.write('{"model":"writer"');
  });
  const timedOut = await result;
  assert.equal(timedOut.status, 504);
  await waitUntil(() => bridge.controllers.size === 0);
  const malformed = await fetch(`${listening.baseUrl}/v1/responses`, { method: "POST", headers: codexHeaders(), body: "{" });
  assert.equal(malformed.status, 400);
});

test("bridge rejects oversized downstream JSON responses", async (t) => {
  const connection = defineConnection({ id: "writer", provider: "test", endpoint: "https://provider.invalid", model: "writer-v1", credentialRef: "WRITER_API_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 } });
  const evidence = verifiedBaselineEvidence(connection, { protocol: connection.protocol });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence, policy: {} });
  const bridge = new ModelBridge({
    sessionPlan,
    connections: [connection],
    credentialResolver: async () => "downstream-secret",
    token: "bridge-secret",
    maxDownstreamBytes: 16,
    fetchImpl: async () => new Response(JSON.stringify({ choices: [{ message: { role: "assistant", content: "too large" } }] }), { status: 200, headers: { "content-type": "application/json" } }),
  });
  const listening = await bridge.start();
  activateBridge(bridge, sessionPlan);
  t.after(() => bridge.close());
  const response = await fetch(`${listening.baseUrl}/v1/responses`, {
    method: "POST",
    headers: codexHeaders(),
    body: JSON.stringify({ model: "writer", input: [], stream: false }),
  });
  assert.equal(response.status, 502);
  assert.match((await response.json()).error.message, /exceeds bridge limit/);
});

test("bridge task budgets are isolated between sequential task trees", async (t) => {
  const downstream = createServer((_request, response) => {
    response.writeHead(200, { "content-type": "text/event-stream" });
    response.end('data: {"id":"usage","choices":[{"delta":{"content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":2,"completion_tokens":1,"total_tokens":3}}\n\ndata: [DONE]\n\n');
  });
  const endpoint = await listen(downstream);
  t.after(() => new Promise((resolve) => { downstream.closeAllConnections(); downstream.close(resolve); }));
  const connection = defineConnection({ id: "writer", provider: "test", endpoint, model: "writer-v1", credentialRef: "WRITER_API_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 } });
  const evidence = verifiedBaselineEvidence(connection, { protocol: connection.protocol });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence, policy: {} });
  const bridge = new ModelBridge({ sessionPlan, connections: [connection], credentialResolver: async () => "downstream-secret", token: "bridge-secret" });
  const listening = await bridge.start();
  t.after(() => bridge.close());
  const request = () => fetch(`${listening.baseUrl}/v1/responses`, {
    method: "POST",
    headers: codexHeaders(),
    body: JSON.stringify({ model: "writer", input: [], stream: true }),
  });

  const first = new TaskBudget({ maxRequests: 1, maxTokens: 10_000 });
  const firstActive = activateBridge(bridge, sessionPlan, first);
  assert.equal((await request()).status, 200);
  assert.equal((await request()).status, 400);
  assert.equal(await bridge.endExecutionLease(firstActive.lease), true);
  assert.equal(bridge.endTaskBudget(first), true);
  assert.deepEqual({ ...first.snapshot(), id: "[BOUND]", startedAt: "[BOUND]" }, { id: "[BOUND]", requests: 1, tokens: 3, unknownTokens: 0, reservedTokens: 0, maxRequests: 1, maxTokens: 10_000, maxDurationMs: null, startedAt: "[BOUND]", overrun: false });

  const second = new TaskBudget({ maxRequests: 1, maxTokens: 10_000 });
  const secondActive = activateBridge(bridge, sessionPlan, second);
  assert.equal((await request()).status, 200);
  assert.equal(await bridge.endExecutionLease(secondActive.lease), true);
  assert.equal(bridge.endTaskBudget(second), true);
  assert.deepEqual({ ...second.snapshot(), id: "[BOUND]", startedAt: "[BOUND]" }, { id: "[BOUND]", requests: 1, tokens: 3, unknownTokens: 0, reservedTokens: 0, maxRequests: 1, maxTokens: 10_000, maxDurationMs: null, startedAt: "[BOUND]", overrun: false });
});

test("bridge process isolates transport and preserves cancellation-safe task budgets", async (t) => {
  const downstream = createServer((_request, response) => {
    response.writeHead(200, { "content-type": "text/event-stream" });
    response.end('data: {"id":"usage","choices":[{"delta":{"content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":2,"completion_tokens":1,"total_tokens":3}}\n\ndata: [DONE]\n\n');
  });
  const endpoint = await listen(downstream);
  t.after(() => new Promise((resolve) => { downstream.closeAllConnections(); downstream.close(resolve); }));
  const connection = defineConnection({ id: "writer", provider: "test", endpoint, model: "writer-v1", credentialRef: "WRITER_API_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 } });
  const evidence = verifiedBaselineEvidence(connection, { protocol: connection.protocol });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence, policy: {} });
  const bridge = new ModelBridgeProcess({ sessionPlan, connections: [connection], credentialResolver: async () => "downstream-secret", token: "process-bridge-secret", heartbeatIntervalMs: 10, heartbeatTimeoutMs: 1_000 });
  const listening = await bridge.start();
  t.after(() => bridge.close());
  assert.notEqual(bridge.processId, process.pid);
  const request = () => fetch(`${listening.baseUrl}/v1/responses`, {
    method: "POST",
    headers: codexHeaders("process-bridge-secret"),
    body: JSON.stringify({ model: "writer", input: [], stream: true }),
  });

  const budget = new TaskBudget({ maxRequests: 1, maxTokens: 10_000 });
  await bridge.beginTaskBudget(budget);
  const lease = createExecutionLease({ sessionPlan, runId: "process-test", budgetId: budget.id });
  await bridge.beginExecutionLease(lease);
  assert.equal((await request()).status, 200);
  assert.equal((await request()).status, 400);
  assert.equal(await bridge.endExecutionLease(lease), true);
  assert.equal(await bridge.endTaskBudget(budget), true);
  assert.deepEqual({ ...budget.snapshot(), id: "[BOUND]", startedAt: "[BOUND]" }, { id: "[BOUND]", requests: 1, tokens: 3, unknownTokens: 0, reservedTokens: 0, maxRequests: 1, maxTokens: 10_000, maxDurationMs: null, startedAt: "[BOUND]", overrun: false });

  const firstProcessId = bridge.processId;
  await new Promise((resolve) => setTimeout(resolve, 35));
  assert.equal(bridge.processId, firstProcessId);
  process.kill(firstProcessId, "SIGTERM");
  // A missing pid reports exit, not completion of asynchronous retirement.
  await waitUntil(() => bridge.processId === null && JSON.parse(bridge.native.apply("inspect", "{}")).acceptingStart);
  const restarted = await bridge.start();
  assert.equal(restarted.baseUrl, listening.baseUrl);
  assert.notEqual(bridge.processId, firstProcessId);
  assert.equal((await request()).status, 403);
});

test("bridge restart waits for transport retirement even after its process id clears", async (t) => {
  const connection = defineConnection({ id: "writer", provider: "test", endpoint: "https://provider.invalid", model: "writer-v1", credentialRef: "WRITER_API_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 } });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: {} });
  const bridge = new ModelBridgeProcess({ sessionPlan, connections: [connection], credentialResolver: async () => "fixture-secret", heartbeatIntervalMs: 0 });
  const listening = await bridge.start();
  let releaseRetirement;
  const barrier = new Promise(resolve => { releaseRetirement = resolve; });
  let retirementStarted;
  const retiring = new Promise(resolve => { retirementStarted = resolve; });
  t.after(async () => { releaseRetirement(); await bridge.close(); });
  const transport = bridge.transports.get(bridge.generation);
  const original = transport.native;
  transport.native = new Proxy(original, { get(target, property) {
    if (property === "close") return async (...args) => {
      retirementStarted();
      await barrier;
      return original.close(...args);
    };
    const value = Reflect.get(target, property);
    return typeof value === "function" ? value.bind(target) : value;
  } });
  const firstProcessId = bridge.processId;
  process.kill(firstProcessId, "SIGTERM");
  await retiring;
  assert.equal(bridge.processId, null);
  assert.equal(JSON.parse(bridge.native.apply("inspect", "{}")).acceptingStart, false);
  await assert.rejects(bridge.start(), /bridge process is closing/);
  releaseRetirement();
  await waitUntil(() => JSON.parse(bridge.native.apply("inspect", "{}")).acceptingStart);
  const restarted = await bridge.start();
  assert.equal(restarted.baseUrl, listening.baseUrl);
  assert.notEqual(bridge.processId, firstProcessId);
});

test("bridge process resets after start failure and can attempt a fresh worker", async () => {
  const connection = defineConnection({ id: "writer", provider: "test", endpoint: "https://provider.invalid", model: "writer-v1", credentialRef: "WRITER_API_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 } });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: {} });
  const bridge = new ModelBridgeProcess({
    sessionPlan,
    connections: [connection],
    credentialResolver: async () => "secret",
    workerPath: fileURLToPath(new URL("./fixture-failing-worker.mjs", import.meta.url)),
    lifecycleTimeoutMs: 100,
    heartbeatIntervalMs: 0,
  });
  await assert.rejects(bridge.start(), /fixture start failed/);
  assert.equal(bridge.processId, null);
  await assert.rejects(bridge.start(), /fixture start failed/);
  assert.equal(bridge.generation, 2);
});

test("native bridge startup is single-flight and close waits for late credential resolution", async () => {
  const connection = defineConnection({ id: "writer", provider: "test", endpoint: "https://provider.invalid", model: "writer-v1", credentialRef: "WRITER_API_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 } });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: {} });
  let release;
  const credentials = new Promise(resolve => { release = resolve; });
  let resolutions = 0;
  const bridge = new ModelBridgeProcess({ sessionPlan, connections: [connection], credentialResolver: async () => { resolutions += 1; return credentials; }, heartbeatIntervalMs: 0 });
  const first = bridge.start();
  const second = bridge.start();
  assert.equal(first, second);
  const close = bridge.close();
  await assert.rejects(bridge.start(), /closing/);
  release("fixture-secret");
  await first;
  await close;
  assert.equal(resolutions, 1);
  assert.equal(bridge.processId, null);
  assert.equal(bridge.pending.size, 0);
});

test("bridge process cannot restart an active task after its budget worker crashes", async (t) => {
  const connection = defineConnection({ id: "writer", provider: "test", endpoint: "https://provider.invalid", model: "writer-v1", credentialRef: "WRITER_API_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 } });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: {} });
  const bridge = new ModelBridgeProcess({ sessionPlan, connections: [connection], credentialResolver: async () => "secret", heartbeatIntervalMs: 0 });
  await bridge.start();
  t.after(() => bridge.close());
  const budget = new TaskBudget({ maxRequests: 4, maxTokens: 1_000 });
  await bridge.beginTaskBudget(budget);
  process.kill(bridge.processId, "SIGTERM");
  await waitUntil(() => bridge.processId === null);
  await assert.rejects(bridge.start(), /budget outcome is unknown/);
  assert.equal(await bridge.endTaskBudget(budget), false);
  assert.equal(budget.snapshot().unknownTokens, 0);
  await bridge.start();
});

test("bridge process kills a worker that does not acknowledge close", async () => {
  const connection = defineConnection({ id: "writer", provider: "test", endpoint: "https://provider.invalid", model: "writer-v1", credentialRef: "WRITER_API_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 } });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: {} });
  const bridge = new ModelBridgeProcess({ sessionPlan, connections: [connection], credentialResolver: async () => "secret", workerPath: fileURLToPath(new URL("./fixture-unresponsive-worker.mjs", import.meta.url)), lifecycleTimeoutMs: 1_000, heartbeatIntervalMs: 0 });
  await bridge.start();
  bridge.lifecycleTimeoutMs = 30;
  const processId = bridge.processId;
  await assert.rejects(bridge.close(), /bridge close timed out/);
  assert.equal(bridge.processId, null);
  await waitUntil(() => {
    try { process.kill(processId, 0); return false; } catch { return true; }
  });
});

test("worker exit rejects pending RPCs even when durable budget recovery throws", async (t) => {
  const connection = defineConnection({ id: "writer", provider: "test", endpoint: "https://provider.invalid", model: "writer-v1", credentialRef: "WRITER_API_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 } });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: {} });
  const bridge = new ModelBridgeProcess({ sessionPlan, connections: [connection], credentialResolver: async () => "secret", heartbeatIntervalMs: 0, workerPath: fileURLToPath(new URL("./fixture-stalled-cancel-worker.mjs", import.meta.url)) });
  await bridge.start();
  t.after(() => bridge.close());
  const budget = new TaskBudget({ maxRequests: 4, maxTokens: 1_000 });
  await bridge.beginTaskBudget(budget);
  const recover = budget.markOutstandingUnknown.bind(budget);
  budget.markOutstandingUnknown = () => { throw new Error("disk write failed"); };
  const pending = assert.rejects(bridge.cancelActiveRequests(), /process exited/);
  process.kill(bridge.processId, "SIGTERM");
  await pending;
  await bridge.budgetRecovery;
  assert.match(bridge.budgetRecoveryError.message, /disk write failed/);
  await assert.rejects(bridge.start(), /budget outcome is unknown/);
  await assert.rejects(bridge.endTaskBudget(budget), /disk write failed/);
  budget.markOutstandingUnknown = recover;
  assert.equal(await bridge.endTaskBudget(budget), false);
  await bridge.start();
});

for (const protocol of ["responses", "chat-completions", "messages"]) {
for (const [boundary, items] of [["route", 5], ["connection", 25]]) {
test(`${protocol} ${boundary} input overflow is terminal before provider dispatch or budget reservation`, async t => {
  let dispatched = 0;
  const downstream = createServer((_request, response) => {
    dispatched++;
    response.writeHead(200, { "content-type": "application/json" });
    response.end(JSON.stringify({ id: "small-response", status: "completed", output: [
      { type: "message", id: "small-answer", role: "assistant", status: "completed",
        content: [{ type: "output_text", text: "accepted", annotations: [] }] }],
      usage: { input_tokens: 10, output_tokens: 2, total_tokens: 12 } }));
  });
  const endpoint = await listen(downstream);
  t.after(() => new Promise(resolve => { downstream.closeAllConnections(); downstream.close(resolve); }));
  const connection = defineConnection({ id: "writer", provider: "test", endpoint, model: "writer-v1",
    credentialRef: "WRITER_API_KEY", protocol, declaredLimits: { contextTokens: 24_000, outputTokens: 4_000 } });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: {} });
  const observations = [];
  const bridge = new ModelBridge({ sessionPlan, connections: [connection], credentialResolver: async () => "fixture-secret", token: "bridge-secret",
    responseObserver: async observation => observations.push(observation) });
  const started = await bridge.start();
  t.after(() => bridge.close());
  const { budget } = activateBridge(bridge, sessionPlan);
  const response = await fetch(`${started.baseUrl}/v1/responses`, { method: "POST", headers: codexHeaders(),
    body: JSON.stringify({ model: "writer", stream: protocol !== "messages",
      input: Array.from({ length: items }, () => ({ type: "message", role: "user", content: [{ type: "input_text", text: "bounded source context ".repeat(500) }] })) }) });
  const streaming = protocol !== "messages";
  const wire = await response.text();
  const event = streaming ? JSON.parse(wire.split("\n").find(line => line.startsWith("data: ")).slice(6)) : null;
  const value = streaming ? event.response : JSON.parse(wire);
  assert.equal(response.status, streaming ? 200 : 400, wire);
  if (streaming) {
    assert.match(response.headers.get("content-type"), /text\/event-stream/);
    assert.equal(event.type, "response.failed");
    assert.equal(value.status, "failed");
    assert.deepEqual(value.output, []);
    assert.equal(value.usage, null);
    assert.equal(value.error.diagnosticCode, "context_input_budget_exceeded");
  }
  assert.equal(value.error.code, streaming ? "context_length_exceeded" : "context_input_budget_exceeded");
  assert.equal(value.error.retryable, false);
  const capacity = value.error.contextCapacity;
  assert.equal(capacity.boundary, boundary);
  assert.equal(capacity.inputLimitTokens, boundary === "route" ? 3616 : 20000);
  assert.ok(capacity.estimatedInputTokens > capacity.inputLimitTokens);
  assert.equal(capacity.excessTokens, capacity.estimatedInputTokens - capacity.inputLimitTokens);
  assert.equal(observations.length, 1);
  assert.deepEqual(observations[0].contextCapacity, capacity);
  assert.ok(observations[0].requestId);
  assert.equal(dispatched, 0);
  assert.equal(budget.snapshot().requests, 0);
  assert.equal(budget.snapshot().reservedTokens, 0);
  if (protocol === "responses") {
    const resumed = await fetch(`${started.baseUrl}/v1/responses`, { method: "POST", headers: codexHeaders(),
      body: JSON.stringify({ model: "writer", stream: false,
        input: [{ type: "message", role: "user", content: [{ type: "input_text", text: "smaller context" }] }] }) });
    const result = await resumed.json();
    assert.equal(resumed.status, 200, JSON.stringify(result));
    assert.equal(dispatched, 1);
    assert.equal(budget.snapshot().requests, 1, "same budget admits a smaller request after rejection");
    assert.equal(budget.snapshot().reservedTokens, 0);
  }
});

}
}
