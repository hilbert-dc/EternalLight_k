import assert from "node:assert/strict";
import { createServer, request as httpRequest } from "node:http";
import { fork } from "node:child_process";
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
  assert.equal((await request()).status, 502);
  assert.equal(await bridge.endExecutionLease(firstActive.lease), true);
  assert.equal(bridge.endTaskBudget(first), true);
  assert.deepEqual({ ...first.snapshot(), id: "[BOUND]", startedAt: "[BOUND]" }, { id: "[BOUND]", requests: 1, tokens: 3, unknownTokens: 0, reservedTokens: 0, maxRequests: 1, maxTokens: 10_000, maxDurationMs: 1_800_000, startedAt: "[BOUND]", overrun: false });

  const second = new TaskBudget({ maxRequests: 1, maxTokens: 10_000 });
  const secondActive = activateBridge(bridge, sessionPlan, second);
  assert.equal((await request()).status, 200);
  assert.equal(await bridge.endExecutionLease(secondActive.lease), true);
  assert.equal(bridge.endTaskBudget(second), true);
  assert.deepEqual({ ...second.snapshot(), id: "[BOUND]", startedAt: "[BOUND]" }, { id: "[BOUND]", requests: 1, tokens: 3, unknownTokens: 0, reservedTokens: 0, maxRequests: 1, maxTokens: 10_000, maxDurationMs: 1_800_000, startedAt: "[BOUND]", overrun: false });
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
  assert.equal((await request()).status, 502);
  assert.equal(await bridge.endExecutionLease(lease), true);
  assert.equal(await bridge.endTaskBudget(budget), true);
  assert.deepEqual({ ...budget.snapshot(), id: "[BOUND]", startedAt: "[BOUND]" }, { id: "[BOUND]", requests: 1, tokens: 3, unknownTokens: 0, reservedTokens: 0, maxRequests: 1, maxTokens: 10_000, maxDurationMs: 1_800_000, startedAt: "[BOUND]", overrun: false });

  const firstProcessId = bridge.processId;
  await new Promise((resolve) => setTimeout(resolve, 35));
  assert.equal(bridge.processId, firstProcessId);
  process.kill(firstProcessId, "SIGTERM");
  await waitUntil(() => bridge.processId === null);
  const restarted = await bridge.start();
  assert.equal(restarted.baseUrl, listening.baseUrl);
  assert.notEqual(bridge.processId, firstProcessId);
  assert.equal((await request()).status, 403);
});

test("bridge process resets after start failure and can attempt a fresh worker", async () => {
  const connection = defineConnection({ id: "writer", provider: "test", endpoint: "https://provider.invalid", model: "writer-v1", credentialRef: "WRITER_API_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 } });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: {} });
  let forks = 0;
  const bridge = new ModelBridgeProcess({
    sessionPlan,
    connections: [connection],
    credentialResolver: async () => "secret",
    workerPath: fileURLToPath(new URL("./fixture-failing-worker.mjs", import.meta.url)),
    forkImpl: (...arguments_) => { forks += 1; return fork(...arguments_); },
    lifecycleTimeoutMs: 100,
    heartbeatIntervalMs: 0,
  });
  await assert.rejects(bridge.start(), /fixture start failed/);
  assert.equal(bridge.processId, null);
  await assert.rejects(bridge.start(), /fixture start failed/);
  assert.equal(forks, 2);
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
  const bridge = new ModelBridgeProcess({ sessionPlan, connections: [connection], credentialResolver: async () => "secret", workerPath: fileURLToPath(new URL("./fixture-unresponsive-worker.mjs", import.meta.url)), lifecycleTimeoutMs: 30, heartbeatIntervalMs: 0 });
  await bridge.start();
  const processId = bridge.processId;
  await assert.rejects(bridge.close(), /bridge close timed out/);
  assert.equal(bridge.processId, null);
  await waitUntil(() => {
    try { process.kill(processId, 0); return false; } catch { return true; }
  });
});
