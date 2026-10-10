import assert from "node:assert/strict";
import { test } from "node:test";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { CodexAppServerAdapter } from "../codex-adapter.mjs";
import { novelRecoveryOperationHash } from "../novel-recovery-contract.mjs";

const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));
const command = `${process.execPath} ${join("", fixture)}`;

test("read-state receipts preserve the observed revision and conflicts are never retried", async () => {
  const adapter = new CodexAppServerAdapter({ command });
  adapter.ensureInitialized = async options => assert.deepEqual(options, { localOnly: true });
  const calls = [];
  const readState = { revision: "seen-revision", firstUnread: { type: "turn", turnId: "turn" } };
  adapter.request = async (method, params) => {
    calls.push({ method, params });
    if (method === "thread/read") return { thread: { id: "thread", turns: [] }, readState };
    if (method === "thread/list") return { data: [{ id: "thread" }], readStates: { thread: readState } };
    throw Object.assign(new Error("revision conflict"), { code: -32000 });
  };
  assert.deepEqual((await adapter.readThread("thread")).thread.readState, readState);
  assert.deepEqual((await adapter.listThreads()).threads[0].readState, readState);
  assert.equal(calls.length, 2, "reading history must not mark it read");
  await assert.rejects(adapter.updateThreadReadState("thread", { operation: "read", expectedRevision: readState.revision }), /revision conflict/);
  assert.deepEqual(calls.at(-1), { method: "thread/readState/update", params: {
    threadId: "thread", operation: { type: "read" }, expectedRevision: "seen-revision" } });
  assert.equal(calls.length, 3);
  await assert.rejects(adapter.updateThreadReadState("thread", { operation: "read", expectedRevision: "" }));
  assert.equal(calls.length, 3);
});

test("Host turn dispatch forwards explicitly retained results without changing ordinary turns", async t => {
  const adapter = new CodexAppServerAdapter({ command });
  t.after(() => adapter.close());
  const calls = [], request = adapter.request.bind(adapter);
  adapter.request = (method, params) => { if (method === "turn/start") calls.push(params); return request(method, params); };
  const toolOutputs = [{ name: "receipt", namespace: null, output: "confirmed", retain: true }];
  await adapter.sendTurn([{ type: "text", text: "completed-message" }], { toolOutputs });
  await adapter.sendTurn([{ type: "text", text: "completed-message" }]);
  assert.deepEqual(calls[0].toolOutputs, toolOutputs);
  assert.equal(Object.hasOwn(calls[1], "toolOutputs"), false);
});

test("matched output admission failure retains the healthy process and original thread", async t => {
  const adapter = new CodexAppServerAdapter({ command });
  t.after(() => adapter.close());
  await adapter.ensureSession(); const child = adapter.process, thread = adapter.threadId;
  await assert.rejects(adapter.sendTurn([{ type: "text", text: "bounded-output" }]), /provider_output_item_limit/);
  assert.equal(adapter.process, child); assert.equal(adapter.threadId, thread);
  assert.equal(child.killed, false); assert.equal(adapter.turnWaiter, null); assert.equal(adapter.turnOperation, null);
  await adapter.sendTurn([{ type: "text", text: "completed-message" }]);
  assert.equal(adapter.process, child); assert.equal(adapter.threadId, thread);
});

test("ordinary failed turns still terminate the process", async t => {
  const adapter = new CodexAppServerAdapter({ command });
  t.after(() => adapter.close()); await adapter.ensureSession(); const child = adapter.process;
  await assert.rejects(adapter.sendTurn([{ type: "text", text: "failed-turn" }]), /unrelated failure/);
  assert.equal(adapter.process, null); assert.equal(adapter.threadId, null); assert.equal(child.killed, true);
});

test("supplier diagnostics retain their original protocol and model identity", () => {
  const adapter = new CodexAppServerAdapter({ command: null, provider: "openai", model: "gpt-5.3-codex" });
  const events = [];
  adapter.setCallbacks({ onEvent: event => events.push(event) });
  const diagnostic = "Codex app-server v2: gpt-5.3-codex upstream request failed (429)";
  adapter.fail(new Error(diagnostic));
  assert.equal(adapter.status().error, diagnostic);
  assert.equal(adapter.status().model, "gpt-5.3-codex");
  assert.equal(events.at(-1).params.message, diagnostic);
  adapter.close();
});

test("bounded recent history retains final phase and real timing after many tool steps", async () => {
  const adapter = new CodexAppServerAdapter({ command });
  adapter.ensureInitialized = async () => {};
  adapter.request = async () => ({ thread: { id: "recent-thread", turns: Array.from({ length: 20 }, (_, index) => ({
    id: `turn-${index}`, status: "completed", startedAt: 100 + index, completedAt: 110 + index,
    items: [{ id: "user", type: "userMessage", text: "作者指令" },
      ...Array.from({ length: 30 }, (_, i) => ({ id: `tool-${i}`, type: "mcpToolCall", text: "读取" })),
      { id: "final", type: "agentMessage", phase: "final_answer", text: "长回复".repeat(4000) },
      { id: "process", type: "agentMessage", phase: "commentary", text: "后续过程" }],
  })) } });
  const detail = await adapter.readThread("recent-thread");
  assert.deepEqual(detail.turns.map(turn => turn.id), Array.from({ length: 12 }, (_, index) => `turn-${index + 8}`));
  assert.equal(detail.turns.at(-1).startedAt, 119);
  assert.equal(detail.turns.at(-1).completedAt, 129);
  assert.equal(detail.turns.at(-1).items.length, 8);
  assert.equal(detail.turns.at(-1).items[0].type, "userMessage");
  const final = detail.turns.at(-1).items.find(item => item.phase === "final_answer");
  assert.ok(final.text.length <= 8001);
  assert.equal(detail.turns.at(-1).items.at(-1).phase, "commentary");
});

test("recovery reads the unique original MCP request beyond display history without starting inference", async () => {
  const adapter = new CodexAppServerAdapter({ command, configurationError: "inference unavailable" });
  const identity = { projectId: "project", taskId: "task", threadId: "thread", turnId: "original-turn", callId: "original-call", toolName: "submit_revision_outcome", requestHash: "a".repeat(64) };
  const arguments_ = { projectId: "project", taskId: "task", threadId: "thread", expectedRevision: 2,
    expectedCandidateRevisionId: "null", idempotencyKey: "original-key", writerTaskId: "writer", payload: { nested: [null, "原始引用", 0] } };
  identity.requestHash = novelRecoveryOperationHash({ tool: identity.toolName, arguments: arguments_ });
  const original = { id: identity.callId, type: "mcpToolCall", server: "novel_workspace", tool: identity.toolName, arguments: arguments_ };
  const history = { thread: { id: identity.threadId, turns: [{ id: identity.turnId,
    items: [original, ...Array.from({ length: 20 }, (_, i) => ({ id: `later-item-${i}`, type: "agentMessage", text: "later" }))] },
    ...Array.from({ length: 20 }, (_, i) => ({ id: `later-turn-${i}`, items: [] }))] } };
  const calls = [];
  adapter.threadId = "selected-thread";
  adapter.ensureInitialized = async options => { assert.deepEqual(options, { localOnly: true }); };
  adapter.request = async (method, params) => {
    calls.push({ method, params });
    return method === "thread/items/read" ? { data: [{ turnId: identity.turnId, item: original }] } : history;
  };
  assert.equal((await adapter.readThread(identity.threadId)).turns.some(turn => turn.id === identity.turnId), false);
  const evidence = await adapter.readRecoveryOperationEvidence(identity);
  assert.deepEqual(evidence, { taskId: "task", threadId: "thread", turnId: identity.turnId, callId: identity.callId,
    toolName: identity.toolName, requestHash: identity.requestHash, arguments: arguments_ });
  assert.equal(evidence.arguments.expectedCandidateRevisionId, "null", "the string must not be changed into JSON null");
  evidence.arguments.payload.nested[1] = "changed returned copy";
  assert.equal(arguments_.payload.nested[1], "原始引用");
  assert.equal(adapter.threadId, "selected-thread");
  assert.equal(adapter.turnId, null);
  assert.deepEqual(calls.at(-1), { method: "thread/items/read", params: {
    threadId: identity.threadId, turnId: identity.turnId, itemIds: [identity.callId] } });
});

test("recovery rejects missing, ambiguous, foreign, incomplete and oversized original request evidence", async () => {
  const adapter = new CodexAppServerAdapter({ command });
  const identity = { projectId: "project", taskId: "task", threadId: "thread", turnId: "turn", callId: "call", toolName: "save_brief", requestHash: "b".repeat(64) };
  const args = { projectId: "project", taskId: "task", threadId: "thread", expectedRevision: 0, expectedCandidateRevisionId: null,
    idempotencyKey: "key", payload: { text: "unchanged" } };
  identity.requestHash = novelRecoveryOperationHash({ tool: identity.toolName, arguments: args });
  const item = { id: "call", type: "mcpToolCall", server: "novel_workspace", tool: "save_brief", arguments: args };
  const original = { data: [{ turnId: "turn", item }] };
  let history = original, requests = 0;
  adapter.ensureInitialized = async options => { assert.deepEqual(options, { localOnly: true }); };
  adapter.request = async method => { assert.equal(method, "thread/items/read"); requests++; return history; };
  const rejected = [
    value => { value.data[0].turnId = "foreign"; },
    value => { value.data = []; },
    value => { value.data.push(structuredClone(value.data[0])); },
    value => { value.data[0].item.id = "foreign"; },
    value => { value.data[0].item.displayTruncated = true; },
    value => { value.data[0].item.type = "agentMessage"; },
    value => { value.data[0].item.server = "foreign_workspace"; },
    value => { value.data[0].item.tool = "save_outline"; },
    value => { value.data[0].item.arguments = JSON.stringify(args); },
    value => { value.data[0].item.arguments.projectId = "foreign"; },
    value => { value.data[0].item.arguments.taskId = "foreign"; },
    value => { value.data[0].item.arguments.threadId = "foreign"; },
    value => { value.data[0].item.arguments.expectedRevision = "0"; },
    value => { delete value.data[0].item.arguments.expectedCandidateRevisionId; },
    value => { value.data[0].item.arguments.expectedCandidateRevisionId = false; },
    value => { delete value.data[0].item.arguments.idempotencyKey; },
    value => { value.data[0].item.arguments.payload.text = "changed request"; },
    value => { value.data[0].item.arguments.expectedCandidateRevisionId = "null"; },
    value => { value.data[0].item.arguments.payload.text = "字".repeat(44_000); },
  ];
  for (const corrupt of rejected) {
    history = structuredClone(original); corrupt(history);
    await assert.rejects(adapter.readRecoveryOperationEvidence(identity));
  }
  history = original;
  const beforeInvalidIdentity = requests;
  await assert.rejects(adapter.readRecoveryOperationEvidence({ ...identity, requestHash: "not-a-hash" }));
  assert.equal(requests, beforeInvalidIdentity);
  const evidence = await adapter.readRecoveryOperationEvidence(identity);
  assert.equal(evidence.arguments.expectedCandidateRevisionId, null);
  history = structuredClone(original);
  const boundedArgs = history.data[0].item.arguments;
  boundedArgs.payload.text = "";
  const { projectId, ...binding } = identity;
  const overhead = Buffer.byteLength(JSON.stringify({ ...binding, arguments: boundedArgs }), "utf8");
  boundedArgs.payload.text = "x".repeat(128 * 1024 - overhead);
  identity.requestHash = novelRecoveryOperationHash({ tool: identity.toolName, arguments: boundedArgs });
  assert.equal(Buffer.byteLength(JSON.stringify(await adapter.readRecoveryOperationEvidence(identity)), "utf8"), 128 * 1024);
  boundedArgs.payload.text += "x";
  identity.requestHash = novelRecoveryOperationHash({ tool: identity.toolName, arguments: boundedArgs });
  await assert.rejects(adapter.readRecoveryOperationEvidence(identity), /上限/);
});

test("adapter audit lease renews only the live same execution and cannot revive cancellation", async () => {
  const adapter = new CodexAppServerAdapter({ command });
  const lease = { runId: "run", leaseId: "lease", epoch: 1, revoked: false, planId: "old", threadId: "thread",
    deadline: new Date(Date.now() + 60_000).toISOString() };
  await adapter.bindExecution({ lease });
  const next = { ...lease, planId: "fresh", deadline: new Date(Date.now() + 120_000).toISOString() };
  await assert.rejects(adapter.renewExecution({ lease: { ...next, threadId: "other" }, sessionPlan: { id: "fresh" } }), /改变了执行身份/);
  assert.deepEqual(await adapter.renewExecution({ lease: next, sessionPlan: { id: "fresh" } }), next);
  await adapter.revokeExecution({ runId: "run", leaseId: "lease", expectedEpoch: 1, revokedEpoch: 2 });
  await assert.rejects(adapter.renewExecution({ lease: { ...next, deadline: new Date(Date.now() + 180_000).toISOString() }, sessionPlan: { id: "fresh" } }), /无法续期/);
});

test("logical model routes are independent of supplier model labels and update on configuration", async () => {
  const adapter = new CodexAppServerAdapter({ command, model: "supplier-model", modelRoute: "connection-one", provider: "fixture" });
  const requested = [];
  const request = adapter.request.bind(adapter);
  adapter.request = (method, params) => { if (method === "thread/start") requested.push(params.model); return request(method, params); };
  try {
    await adapter.ensureSession();
    assert.equal(adapter.status().model, "supplier-model");
    adapter.configure({ command, cwd: adapter.cwd });
    await adapter.ensureSession();
    assert.equal(adapter.status().model, "supplier-model");
    adapter.configure({ command, model: "supplier-model-two", modelRoute: "connection-two", provider: "fixture" });
    await adapter.ensureSession();
    assert.equal(adapter.status().model, "supplier-model-two");
    assert.deepEqual(requested, ["connection-one", "connection-one", "connection-two"]);
  } finally { await adapter.close(); }
});

test("starts a fresh DeepSeek App Server without desktop authentication", async () => {
  const adapter = new CodexAppServerAdapter({ command, model: "deepseek-flash", provider: "deepseek", env: { PATH: process.env.PATH } });
  await adapter.ensureInitialized();
  assert.equal(adapter.initialized, true);
  adapter.close();
});

test("performs app-server handshake and streams a completed turn", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-deepseek-key", command });
  const initialization = [];
  const request = adapter.request;
  adapter.request = function(method, params) {
    if (method === "initialize") initialization.push(params.clientInfo);
    return request.call(this, method, params);
  };
  const events = [];
  adapter.setCallbacks({ onEvent: (event) => events.push(event) });
  const result = await adapter.sendTurn([{ type: "text", text: "hello" }]);
  assert.equal(result.mode, "novel-agent-engine");
  assert.equal(result.message, "hello");
  assert.equal(result.turn.id, "turn-test");
  assert.equal(adapter.threadId, "thr-test");
  assert.deepEqual(initialization, [{ name: "novel_codex", title: "小说工坊", version: "0.1.0" }]);
  assert.ok(events.some((event) => event.method === "item/agentMessage/delta"));
  assert.ok(events.some((event) => event.method === "item/commandExecution/outputDelta"));
  assert.ok(events.some((event) => event.method === "turn/plan/updated"));
  assert.ok(events.some((event) => event.method === "turn/diff/updated"));
  assert.ok(events.some((event) => event.method === "thread/status/changed" && event.params.status.type === "active"));
  assert.ok(events.some((event) => event.method === "thread/status/changed" && event.params.status.type === "idle"));
  assert.ok(events.some((event) => event.method === "item/completed" && event.params.item?.type === "fileChange"));
  assert.ok(events.some((event) => event.method === "turn/completed"));
  adapter.close();
});

test("uses the completed agent message as the bounded turn result", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-deepseek-key", command });
  const result = await adapter.sendTurn([{ type: "text", text: "completed-message" }]);
  assert.equal(result.message, "最终讨论结论");
  adapter.close();
});

test("rejects DSML protocol text without exposing it as an agent response", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-deepseek-key", command });
  const events = [];
  adapter.setCallbacks({ onEvent: (event) => events.push(event) });
  try {
    await assert.rejects(adapter.sendTurn([{ type: "text", text: "protocol-leak" }]), /结构化工具协议/);
    assert.ok(events.some((event) => event.method === "agent/error" && /结构化工具协议/.test(event.params.message)));
    assert.ok(events.every((event) => !JSON.stringify(event).includes("DSML")));
  } finally {
    adapter.close();
  }
});

test("protocol failure before follow-up thread naming rejects without a timeout", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-deepseek-key", command });
  const request = adapter.request.bind(adapter);
  adapter.request = async (method, params) => {
    const result = await request(method, params);
    if (method === "turn/start") {
      adapter.handleLine(JSON.stringify({ method: "item/completed", params: { threadId: params.threadId, turnId: "turn-test", item: { id: "early-protocol-leak", type: "agentMessage", text: "<｜｜DSML｜｜ calls>" } } }));
    }
    return result;
  };
  try {
    await assert.rejects(adapter.sendTurn([{ type: "text", text: "hello" }]), /结构化工具协议/);
    assert.equal(adapter.pending.size, 0);
  } finally {
    adapter.close();
  }
});

test("caps a streamed agent message before returning it", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-deepseek-key", command });
  const rawEvents = [];
  const visibleEvents = [];
  adapter.setCallbacks({ onRawEvent: (event) => rawEvents.push(event), onEvent: (event) => visibleEvents.push(event) });
  const result = await adapter.sendTurn([{ type: "text", text: "long-message" }]);
  assert.equal(result.message.length, 64 * 1024);
  adapter.handleLine(JSON.stringify({ method: "item/completed", params: { item: { id: "large-audit-event", type: "mcpToolCall", result: "x".repeat(70 * 1024) } } }));
  assert.ok(rawEvents.some((event) => JSON.stringify(event).length > 64 * 1024));
  const visible = visibleEvents.find((event) => event.params?.item?.id === "large-audit-event");
  assert.equal(visible?.method, "item/completed");
  assert.ok(JSON.stringify(visible).length <= 64 * 1024);
  assert.match(visible.params.item.result, /内容过长，已截断显示/);
  assert.ok(visibleEvents.every((event) => event.method !== "agent/error"));
  adapter.close();
});

test("starts domain generation without an execution environment or approvals", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-deepseek-key", command });
  adapter.prepareTaskOperation("generateOutline", "task-capability-outline-1234567890");
  const result = await adapter.sendTurn([{ type: "text", text: "isolated" }], { disableEnvironment: true });
  assert.deepEqual(result.turn.receivedParams.environments, []);
  assert.equal(result.turn.receivedParams.approvalPolicy, "never");
  assert.equal(adapter.env.NOVEL_CODEX_OPERATION, "generateOutline");
  adapter.close();
});

test("terminates the app-server and erases the task capability after each task", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-deepseek-key", command });
  adapter.prepareTaskOperation("generateOutline", "task-capability-outline-erase-1234567890");
  await adapter.sendTurn([{ type: "text", text: "isolated" }], { disableEnvironment: true });
  const child = adapter.process;
  await adapter.finishTaskOperation();
  assert.equal(adapter.process, null);
  assert.equal(adapter.threadId, null);
  assert.equal(adapter.activeOperation, null);
  assert.equal(adapter.env.NOVEL_CODEX_OPERATION, undefined);
  assert.equal(adapter.env.NOVEL_CODEX_TASK_CAPABILITY, undefined);
  assert.equal(child.killed, true);
});

test("force-terminates an active task and rejects its pending turn", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-deepseek-key", command });
  adapter.prepareTaskOperation("generateOutline", "task-capability-outline-stop-1234567890");
  let started;
  const startedPromise = new Promise((resolve) => { started = resolve; });
  adapter.setCallbacks({ onEvent: (event) => { if (event.method === "turn/started") started(); } });
  const turn = adapter.sendTurn([{ type: "text", text: "approval" }], { disableEnvironment: true });
  await startedPromise;
  const child = adapter.process;
  const rejected = assert.rejects(turn, /任务会话已结束/);
  await adapter.finishTaskOperation();
  await rejected;
  assert.equal(child.killed, true);
  assert.equal(adapter.env.NOVEL_CODEX_TASK_CAPABILITY, undefined);
});

test("selects the read-only advisory operation for a free discussion", () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-deepseek-key", command, env: { ...process.env, NOVEL_CODEX_OPERATION: "generatePlan" } });
  adapter.prepareAdvisorySession("task-capability-advisory-123456789");
  assert.equal(adapter.env.NOVEL_CODEX_OPERATION, "advisory");
  assert.equal(adapter.env.NOVEL_CODEX_TASK_CAPABILITY, "task-capability-advisory-123456789");
  adapter.close();
});

test("rotates the capability and process between advisory tasks", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-deepseek-key", command });
  adapter.prepareAdvisorySession("task-capability-advisory-a-123456789");
  await adapter.resumeThread("thr-history");
  const child = adapter.process;
  adapter.prepareAdvisorySession("task-capability-advisory-b-123456789");
  assert.notEqual(adapter.process, child);
  assert.equal(adapter.threadId, null);
  assert.equal(adapter.env.NOVEL_CODEX_TASK_CAPABILITY, "task-capability-advisory-b-123456789");
  adapter.close();
});

test("automatically denies software permission requests without exposing an approval", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-deepseek-key", command });
  const events = [];
  const userInputRequests = [];
  adapter.setCallbacks({ onUserInputRequest: (request) => userInputRequests.push(request), onEvent: (event) => events.push(event) });
  const result = await adapter.sendTurn([{ type: "text", text: "approval" }]);
  assert.equal(result.mode, "novel-agent-engine");
  assert.deepEqual(userInputRequests, []);
  assert.ok(events.some((event) => event.method === "agent/softwarePermissionDenied" && event.params.requestMethod === "item/commandExecution/requestApproval"));
  assert.ok(!events.some((event) => event.method === "item/commandExecution/requestApproval"));
  assert.ok(events.some((event) => event.method === "serverRequest/resolved"));
  adapter.close();
});

test("collects structured user input and resumes the turn", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-deepseek-key", command });
  const requests = [];
  let resolveSecondRequest;
  const secondRequest = new Promise((resolve) => { resolveSecondRequest = resolve; });
  const firstRequest = new Promise((resolve) => adapter.setCallbacks({ onUserInputRequest: (request) => { requests.push(request); if (requests.length === 1) resolve(request); if (requests.length === 2) resolveSecondRequest(request); } }));
  const turn = adapter.sendTurn([{ type: "text", text: "user-input" }]);
  const request = await firstRequest;
  assert.equal(request.method, "item/tool/requestUserInput");
  assert.equal(request.params.questions[0].id, "direction");
  assert.deepEqual(adapter.resolveUserInput(request.requestId, { answers: { direction: "白塔谜团" } }), { resolved: true });
  const nextRequest = await secondRequest;
  assert.equal(nextRequest.method, "item/tool/requestUserInput");
  assert.equal(nextRequest.params.questions[0].id, "tone");
  assert.deepEqual(adapter.resolveUserInput(nextRequest.requestId, { answers: { tone: "克制" } }), { resolved: true });
  const result = await turn;
  assert.equal(result.mode, "novel-agent-engine");
  adapter.close();
});

test("preserves bounded multiline author answers and keeps invalid requests available", { timeout: 5000 }, async t => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-deepseek-key", command });
  t.after(() => adapter.close());
  let resolveFirst, resolveSecond;
  const first = new Promise(resolve => { resolveFirst = resolve; });
  const second = new Promise(resolve => { resolveSecond = resolve; });
  adapter.setCallbacks({ onUserInputRequest: request => {
    if (request.params.questions[0].id === "direction") resolveFirst(request);
    else resolveSecond(request);
  } });
  const turn = adapter.sendTurn([{ type: "text", text: "user-input" }]);
  const request = await first;
  const pendingRequest = adapter.userInputs.get(request.requestId);
  const answer = "节奏\t克制\r\n推进白塔\n保留悬念".padEnd(240, "月");
  for (const invalid of [answer + "月", "\t\r\n", "内容\0", "内容\u0007", "内容\u001b", "内容\u007f", "内容\u0085"]) {
    assert.throws(() => adapter.resolveUserInput(request.requestId, { answers: { direction: invalid } }), /用户回答内容无效或过长/);
    assert.equal(adapter.userInputs.get(request.requestId), pendingRequest);
  }
  assert.deepEqual(adapter.resolveUserInput(request.requestId, { answers: { direction: answer } }), { resolved: true });
  const next = await second;
  assert.deepEqual(adapter.resolveUserInput(next.requestId, { answers: { tone: "克制" } }), { resolved: true });
  const result = await turn;
  assert.equal(result.message, answer);
});

test("delivers a bounded free answer even when the model lists closed options", { timeout: 5000 }, async t => {
  const adapter = new CodexAppServerAdapter({ command });
  t.after(() => adapter.close());
  const requested = new Promise(resolve => adapter.setCallbacks({ onUserInputRequest: resolve }));
  const turn = adapter.sendTurn([{ type: "text", text: "user-input-free-answer" }]);
  const request = await requested;
  const question = request.params.questions[0];
  assert.equal(question.isOther, false);
  assert.deepEqual(question.options.map(option => option.label), ["白塔谜团", "墨鸦线"]);
  const pending = adapter.userInputs.get(request.requestId);
  for (const invalid of [
    null, { answers: [] }, { answers: {} }, { answers: { foreign: "白塔谜团" } },
    { answers: { direction: "白塔谜团", extra: "墨鸦线" } },
    ...["", "\t\r\n", "月".repeat(241), "内容\0", "内容\u0007", "内容\u001b", "内容\u007f", "内容\u0085", [], Array(7).fill("白塔谜团")]
      .map(direction => ({ answers: { direction } })),
  ]) {
    assert.throws(() => adapter.resolveUserInput(request.requestId, invalid), /用户回答格式无效|必须回答所有问题|用户回答内容无效或过长/);
    assert.equal(adapter.userInputs.get(request.requestId), pending);
  }
  assert.throws(() => adapter.resolveUserInput("foreign-request", { answers: { direction: "白塔谜团" } }), /作者问题已过期/);
  assert.equal(adapter.userInputs.get(request.requestId), pending);
  const answer = "把两条线结合：\n先揭示墨鸦与白塔的关系，保持克制，但不要直接揭底。";
  assert.deepEqual(adapter.resolveUserInput(request.requestId, { answers: { direction: answer } }), { resolved: true });
  assert.equal(adapter.userInputs.has(request.requestId), false);
  const result = await turn;
  assert.equal(result.message, answer, "The app-server must receive the author's complete custom answer");
});

test("steers an active turn without creating a new turn", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-deepseek-key", command });
  let started;
  const startedPromise = new Promise((resolve) => { started = resolve; });
  adapter.setCallbacks({ onEvent: (event) => { if (event.method === "turn/started") started(); } });
  const lease = {runId:"steer-run", leaseId:"steer-lease", epoch:0};
  await adapter.bindExecution({lease});
  const turn = adapter.sendTurn([{ type: "text", text: "steer" }]);
  await startedPromise;
  const target = {...lease,expectedThreadId:adapter.threadId,expectedTurnId:adapter.turnId};
  assert.equal((await adapter.steerTurn([{type:"text",text:"wrong turn"}], {...target,expectedTurnId:"old-turn"})).delivery,"notSent");
  assert.equal((await adapter.steerTurn([{type:"text",text:"wrong epoch"}], {...target,epoch:1})).delivery,"notSent");
  assert.deepEqual(await adapter.steerTurn([{ type: "text", text: "focus on the lighthouse" }], target), { steered: true });
  const result = await turn;
  assert.equal(result.mode, "novel-agent-engine");
  adapter.close();
});

test("lists, pins, archives, restores and resumes an isolated app-server thread", async t => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-deepseek-key", command });
  t.after(() => adapter.close());
  const listed = await adapter.listThreads();
  assert.deepEqual(listed.threads[0], {
    id: "thr-history",
    name: "整理第一章",
    preview: "继续完善灯塔场景",
    modelProvider: "deepseek",
    status: "notLoaded",
    createdAt: 1730831111,
    updatedAt: 1730832111,
    isPinned: false,
    ephemeral: false,
    readState: null,
  });
  const nextPage = await adapter.listThreads({ cursor: listed.nextCursor });
  assert.equal(nextPage.threads[0].id, "thr-history-2");
  assert.equal(nextPage.nextCursor, null);
  const archived = await adapter.listThreads({ archived: true });
  assert.equal(archived.threads[0].id, "thr-archived");
  assert.equal(archived.threads[0].isPinned, true);
  const detail = await adapter.readThread("thr-history");
  assert.equal(detail.thread.id, "thr-history");
  assert.deepEqual(detail.turns[0], {
    id: "turn-history",
    status: "completed",
    startedAt: null,
    completedAt: null,
    items: [
      { id: "item-user", type: "userMessage", status: "completed", text: "整理第一章", phase: null },
      { id: "item-agent", type: "agentMessage", status: "completed", text: "我会先检查白塔灯光的伏笔。", phase: null },
      { id: "item-tool", type: "commandExecution", status: "completed", text: "echo test", phase: null },
    ],
  });
  assert.equal(adapter.threadId, null);
  const pinned = await adapter.setThreadPinned("thr-history", true);
  assert.equal(pinned.thread.id, "thr-history");
  assert.equal(pinned.thread.isPinned, true);
  assert.deepEqual(await adapter.archiveThread("thr-history"), { archived: true });
  const restored = await adapter.unarchiveThread("thr-archived");
  assert.equal(restored.thread.id, "thr-archived");
  assert.equal(restored.thread.name, "旧稿回收站");
  const resumed = await adapter.resumeThread("thr-history");
  assert.equal(resumed.thread.id, "thr-history");
  assert.equal(adapter.threadId, "thr-history");
  adapter.close();
});

test("reads local history while inference credentials are unavailable", async () => {
  const adapter = new CodexAppServerAdapter({ command, configurationError: "capability evidence expired" });
  const listed = await adapter.listThreads();
  assert.equal(listed.threads[0].id, "thr-history");
  const detail = await adapter.readThread("thr-history");
  assert.equal(detail.thread.id, "thr-history");
  await assert.rejects(adapter.sendTurn([{ type: "text", text: "must not reach a model" }]), /未配置/);
  assert.equal(adapter.status().mode, "local-history");
  adapter.close();
});

test("saves the actual conversation name and verifies same-thread metadata before returning", async () => {
  const adapter = new CodexAppServerAdapter({ command });
  const calls = [];
  adapter.ensureInitialized = async options => { assert.deepEqual(options, { localOnly: true }); };
  adapter.request = async (method, params) => {
    calls.push({ method, params });
    return method === "thread/name/set" ? {} : { thread: { id: "named-thread", name: "新对话", turns: [] } };
  };
  const saved = await adapter.saveThreadName("named-thread", "新对话");
  assert.equal(saved.thread.id, "named-thread");
  assert.equal(saved.thread.name, "新对话");
  assert.deepEqual(calls, [
    { method: "thread/name/set", params: { threadId: "named-thread", name: "新对话" } },
    { method: "thread/read", params: { threadId: "named-thread", includeTurns: false } },
  ]);
  for (const name of ["", " leading", "trailing ", "line\nfeed", "x".repeat(121)]) {
    await assert.rejects(adapter.saveThreadName("named-thread", name), /名称/);
  }
  assert.equal(calls.length, 2);
});

test("empty conversation creation disables only task MCP and cannot directly execute", async t => {
  const adapter = new CodexAppServerAdapter({ command, configurationError: "inference credentials unavailable", env: { PATH: process.env.PATH } });
  t.after(() => adapter.close());
  const calls = [], request = adapter.request.bind(adapter);
  adapter.request = (method, params) => { calls.push({ method, params }); return request(method, params); };
  const created = await adapter.createThread();
  await adapter.saveThreadName(created.thread.id, "新对话");
  assert.deepEqual((await adapter.readThread(created.thread.id)).turns, []);
  const start = calls.find(call => call.method === "thread/start");
  assert.deepEqual(start.params.config, { "mcp_servers.novel_workspace.enabled": false });
  assert.equal(adapter.env.NOVEL_CODEX_TASK_CAPABILITY, undefined);
  assert.equal(adapter.activeOperation, null);
  const count = calls.length;
  await assert.rejects(adapter.ensureSession(), /先绑定创作任务/);
  assert.equal(calls.length, count, "execution cannot reuse a disabled management session");
  assert.equal(calls.filter(call => call.method === "turn/start").length, 0);
});

test("new thread is published only after durable full history read succeeds", async t => {
  for (const failRead of [false, true]) {
    const adapter = new CodexAppServerAdapter({ command, env: { PATH: process.env.PATH } });
    t.after(() => adapter.close());
    const events = [], request = adapter.request.bind(adapter);
    let named = false, read = false;
    adapter.setCallbacks({ onEvent: event => {
      if (event.method === "thread/started") {
        assert.equal(read, true, "in-memory creation must not become visible");
        events.push(event);
      }
    } });
    adapter.request = async (method, params) => {
      if (method === "thread/name/set") named = true;
      if (method === "thread/read") {
        assert.equal(named, true);
        assert.equal(params.includeTurns, true);
        if (failRead) throw new Error("history persistence unavailable");
        read = true;
      }
      return request(method, params);
    };
    if (failRead) {
      await assert.rejects(adapter.createThread(), /history persistence unavailable/);
      assert.equal(events.length, 0);
    } else {
      const result = await adapter.createThread();
      assert.equal(events.length, 1);
      assert.equal(events[0].params.thread.id, result.thread.id);
    }
  }
});

test("task binding rotates even the same capability after empty creation and requires MCP on exact resume", async t => {
  const capability = "unit-task-capability-advisory-123456789";
  const adapter = new CodexAppServerAdapter({ command, env: { PATH: process.env.PATH, NOVEL_CODEX_OPERATION: "advisory", NOVEL_CODEX_TASK_CAPABILITY: capability } });
  t.after(() => adapter.close());
  const calls = [], request = adapter.request.bind(adapter);
  adapter.request = (method, params) => { calls.push({ method, params }); return request(method, params); };
  const { thread } = await adapter.createThread();
  await adapter.saveThreadName(thread.id, "新对话");
  const managementProcess = adapter.process;
  adapter.prepareAdvisorySession(capability);
  assert.equal(managementProcess.killed, true);
  assert.equal(adapter.process, null);
  await adapter.resumeThread(thread.id);
  assert.notEqual(adapter.process, managementProcess);
  assert.equal(adapter.threadId, thread.id);
  assert.deepEqual(calls.find(call => call.method === "thread/resume").params, {
    threadId: thread.id,
    config: { "mcp_servers.novel_workspace.enabled": true, "mcp_servers.novel_workspace.required": true },
  });
  assert.equal(adapter.env.NOVEL_CODEX_TASK_CAPABILITY, capability);
  assert.equal(calls.filter(call => call.method === "turn/start").length, 0);
});

test("conversation-only resume has no task authority and task MCP startup failures remain fatal", async t => {
  const adapter = new CodexAppServerAdapter({ command, env: { PATH: process.env.PATH } });
  t.after(() => adapter.close());
  const calls = [], request = adapter.request.bind(adapter);
  adapter.request = (method, params) => {
    calls.push({ method, params });
    if (method === "thread/resume" && params.config["mcp_servers.novel_workspace.enabled"] === true) {
      throw Error("required MCP novel_workspace: capability rejected");
    }
    return request(method, params);
  };
  await adapter.resumeThread("thr-history");
  assert.deepEqual(calls.find(call => call.method === "thread/resume").params.config, { "mcp_servers.novel_workspace.enabled": false });
  await assert.rejects(adapter.ensureSession(), /先绑定创作任务/);
  adapter.prepareAdvisorySession("unit-task-capability-rejected-123456789");
  await assert.rejects(adapter.resumeThread("thr-history"), /required MCP novel_workspace: capability rejected/);
  assert.equal(adapter.process, null);
  assert.equal(adapter.threadId, null);
  assert.equal(calls.filter(call => call.method === "turn/start").length, 0);
  assert.equal(calls.filter(call => call.method === "thread/resume").length, 2, "no automatic retry or success fallback");
});

test("generic execution preserves explicit servers while a bound task cannot fall back without its workspace transport", async t => {
  const adapter = new CodexAppServerAdapter({ command, env: { PATH: process.env.PATH } });
  t.after(() => adapter.close());
  const calls = [], request = adapter.request.bind(adapter);
  adapter.request = (method, params) => {
    calls.push({ method, params });
    if (method === "thread/resume") throw Error("invalid transport for mcp_servers.novel_workspace");
    return request(method, params);
  };
  const originalThread = await adapter.ensureSession();
  const start = calls.find(call => call.method === "thread/start");
  assert.equal(Object.hasOwn(start.params, "config"), false, "unbound generic execution must not inject a domain server");
  assert.equal(adapter.activeOperation, null);
  adapter.prepareAdvisorySession("unit-task-capability-missing-transport-123456789");
  await assert.rejects(adapter.resumeThread(originalThread), /invalid transport for mcp_servers.novel_workspace/);
  assert.deepEqual(calls.find(call => call.method === "thread/resume").params.config, {
    "mcp_servers.novel_workspace.enabled": true, "mcp_servers.novel_workspace.required": true,
  });
  assert.equal(adapter.process, null);
  assert.equal(adapter.threadId, null);
  assert.equal(calls.filter(call => call.method === "thread/resume").length, 1, "no unbound retry after task initialization fails");
  assert.equal(calls.filter(call => call.method === "turn/start").length, 0);
});

test("name save rejects mismatched metadata and propagates native save failure", async () => {
  const adapter = new CodexAppServerAdapter({ command });
  adapter.ensureInitialized = async () => {};
  for (const thread of [{ id: "foreign-thread", name: "新对话" }, { id: "named-thread", name: "未保存名称" }]) {
    adapter.request = async method => method === "thread/name/set" ? {} : { thread };
    await assert.rejects(adapter.saveThreadName("named-thread", "新对话"), /回执不匹配/);
  }
  const calls = [];
  adapter.request = async method => { calls.push(method); throw Error("native name save failed"); };
  await assert.rejects(adapter.saveThreadName("named-thread", "新对话"), /native name save failed/);
  assert.deepEqual(calls, ["thread/name/set"]);
  adapter.turnOperation = {};
  await assert.rejects(adapter.saveThreadName("named-thread", "新对话"), /正在执行/);
  assert.deepEqual(calls, ["thread/name/set"]);
});

test("interrupts an active turn", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-deepseek-key", command });
  let started;
  const startedPromise = new Promise((resolve) => { started = resolve; });
  adapter.setCallbacks({ onEvent: (event) => { if (event.method === "turn/started") started(); } });
  const turn = adapter.sendTurn([{ type: "text", text: "interrupt" }]);
  await startedPromise;
  assert.deepEqual(await adapter.interruptTurn(), { interrupted: true });
  const result = await turn;
  assert.equal(result.mode, "novel-agent-engine");
  adapter.close();
});

test("supports task review, context compaction and background terminal controls", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-deepseek-key", command });
  const events = [];
  adapter.setCallbacks({ onEvent: (event) => events.push(event) });
  adapter.prepareAdvisorySession("unit-task-capability-review-123456789");
  await adapter.resumeThread("thr-test");
  const review = await adapter.reviewThread("thr-test");
  assert.equal(review.started, true);
  assert.equal(review.turn.id, "review-turn");
  await new Promise((resolve) => setTimeout(resolve, 10));
  assert.ok(events.some((event) => event.method === "agent/review"));
  assert.ok(events.some((event) => event.method === "item/completed" && event.params.item?.type === "exitedReviewMode"));
  assert.deepEqual(await adapter.compactThread("thr-test"), { started: true, turn: { id: "compact-turn", status: "completed", items: [] } });
  await new Promise((resolve) => setTimeout(resolve, 10));
  assert.ok(events.some((event) => event.method === "item/completed" && event.params.item?.type === "contextCompaction"));
  const terminals = await adapter.listBackgroundTerminals("thr-test");
  assert.deepEqual(terminals.terminals, [{ processId: "process-42", command: "npm run watch", cwd: "/tmp/novel", status: "running" }]);
  assert.deepEqual(await adapter.terminateBackgroundTerminal("thr-test", "process-42"), { terminated: true });
  adapter.close();
});

test("rejects pending work when app-server exits cleanly", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-deepseek-key", command });
  await assert.rejects(adapter.sendTurn([{ type: "text", text: "clean-exit" }]), /已退出 \(0\)/);
  adapter.close();
});
