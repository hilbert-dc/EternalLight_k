import assert from "node:assert/strict";
import { test } from "node:test";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { CodexAppServerAdapter } from "../codex-adapter.mjs";

const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));
const command = `${process.execPath} ${join("", fixture)}`;

test("logs a fresh OpenAI App Server into ephemeral API-key auth", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-openai-key", command, env: { PATH: process.env.PATH } });
  await adapter.ensureInitialized();
  assert.equal(adapter.initialized, true);
  adapter.close();
});

test("fails closed before starting a fresh OpenAI App Server without an API key", async () => {
  const adapter = new CodexAppServerAdapter({ command });
  await assert.rejects(adapter.ensureInitialized(), /缺少 API Key/);
  assert.equal(adapter.process, null);
});

test("performs app-server handshake and streams a completed turn", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-openai-key", command });
  const events = [];
  adapter.setCallbacks({ onEvent: (event) => events.push(event) });
  const result = await adapter.sendTurn([{ type: "text", text: "hello" }]);
  assert.equal(result.mode, "codex-app-server");
  assert.equal(result.message, "hello");
  assert.equal(result.turn.id, "turn-test");
  assert.equal(adapter.threadId, "thr-test");
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
  const adapter = new CodexAppServerAdapter({ apiKey: "test-openai-key", command });
  const result = await adapter.sendTurn([{ type: "text", text: "completed-message" }]);
  assert.equal(result.message, "最终讨论结论");
  adapter.close();
});

test("caps a streamed agent message before returning it", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-openai-key", command });
  const result = await adapter.sendTurn([{ type: "text", text: "long-message" }]);
  assert.equal(result.message.length, 64 * 1024);
  adapter.close();
});

test("starts domain generation without an execution environment or approvals", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-openai-key", command });
  adapter.prepareTaskOperation("generateOutline", "task-capability-outline-1234567890");
  const result = await adapter.sendTurn([{ type: "text", text: "isolated" }], { disableEnvironment: true });
  assert.deepEqual(result.turn.receivedParams.environments, []);
  assert.equal(result.turn.receivedParams.approvalPolicy, "never");
  assert.equal(adapter.env.NOVEL_CODEX_OPERATION, "generateOutline");
  adapter.close();
});

test("terminates the app-server and erases the task capability after each task", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-openai-key", command });
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
  const adapter = new CodexAppServerAdapter({ apiKey: "test-openai-key", command });
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
  const adapter = new CodexAppServerAdapter({ apiKey: "test-openai-key", command, env: { ...process.env, NOVEL_CODEX_OPERATION: "generatePlan" } });
  adapter.prepareAdvisorySession("task-capability-advisory-123456789");
  assert.equal(adapter.env.NOVEL_CODEX_OPERATION, "advisory");
  assert.equal(adapter.env.NOVEL_CODEX_TASK_CAPABILITY, "task-capability-advisory-123456789");
  adapter.close();
});

test("rotates the capability and process between advisory tasks", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-openai-key", command });
  adapter.prepareAdvisorySession("task-capability-advisory-a-123456789");
  await adapter.resumeThread("thr-history");
  const child = adapter.process;
  adapter.prepareAdvisorySession("task-capability-advisory-b-123456789");
  assert.notEqual(adapter.process, child);
  assert.equal(adapter.threadId, null);
  assert.equal(adapter.env.NOVEL_CODEX_TASK_CAPABILITY, "task-capability-advisory-b-123456789");
  adapter.close();
});

test("pauses for explicit approval and resumes after resolution", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-openai-key", command });
  const events = [];
  const approval = new Promise((resolve) => adapter.setCallbacks({ onApprovalRequest: resolve, onEvent: (event) => events.push(event) }));
  const turn = adapter.sendTurn([{ type: "text", text: "approval" }]);
  const request = await approval;
  assert.equal(request.method, "item/commandExecution/requestApproval");
  assert.equal(request.params.command, "echo test");
  assert.deepEqual(adapter.resolveApproval(request.requestId, { decision: "decline" }), { resolved: true });
  const result = await turn;
  assert.equal(result.mode, "codex-app-server");
  assert.ok(events.some((event) => event.method === "serverRequest/resolved"));
  adapter.close();
});

test("collects structured user input and resumes the turn", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-openai-key", command });
  const requests = [];
  let resolveSecondRequest;
  const secondRequest = new Promise((resolve) => { resolveSecondRequest = resolve; });
  const firstRequest = new Promise((resolve) => adapter.setCallbacks({ onApprovalRequest: (request) => { requests.push(request); if (requests.length === 1) resolve(request); if (requests.length === 2) resolveSecondRequest(request); } }));
  const turn = adapter.sendTurn([{ type: "text", text: "user-input" }]);
  const request = await firstRequest;
  assert.equal(request.method, "item/tool/requestUserInput");
  assert.equal(request.params.questions[0].id, "direction");
  assert.deepEqual(adapter.resolveApproval(request.requestId, { answers: { direction: "白塔谜团" } }), { resolved: true });
  const nextRequest = await secondRequest;
  assert.equal(nextRequest.method, "item/tool/requestUserInput");
  assert.equal(nextRequest.params.questions[0].id, "tone");
  assert.deepEqual(adapter.resolveApproval(nextRequest.requestId, { answers: { tone: "克制" } }), { resolved: true });
  const result = await turn;
  assert.equal(result.mode, "codex-app-server");
  adapter.close();
});

test("steers an active turn without creating a new turn", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-openai-key", command });
  let started;
  const startedPromise = new Promise((resolve) => { started = resolve; });
  adapter.setCallbacks({ onEvent: (event) => { if (event.method === "turn/started") started(); } });
  const turn = adapter.sendTurn([{ type: "text", text: "steer" }]);
  await startedPromise;
  assert.deepEqual(await adapter.steerTurn([{ type: "text", text: "focus on the lighthouse" }]), { steered: true });
  const result = await turn;
  assert.equal(result.mode, "codex-app-server");
  adapter.close();
});

test("lists, pins, archives, restores and resumes an isolated app-server thread", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-openai-key", command });
  const listed = await adapter.listThreads();
  assert.deepEqual(listed.threads[0], {
    id: "thr-history",
    name: "整理第一章",
    preview: "继续完善灯塔场景",
    modelProvider: "openai",
    status: "notLoaded",
    createdAt: 1730831111,
    updatedAt: 1730832111,
    isPinned: false,
    ephemeral: false,
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
    items: [
      { id: "item-user", type: "userMessage", status: "completed", text: "整理第一章" },
      { id: "item-agent", type: "agentMessage", status: "completed", text: "我会先检查白塔灯光的伏笔。" },
      { id: "item-tool", type: "commandExecution", status: "completed", text: "echo test" },
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

test("interrupts an active turn", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-openai-key", command });
  let started;
  const startedPromise = new Promise((resolve) => { started = resolve; });
  adapter.setCallbacks({ onEvent: (event) => { if (event.method === "turn/started") started(); } });
  const turn = adapter.sendTurn([{ type: "text", text: "interrupt" }]);
  await startedPromise;
  assert.deepEqual(await adapter.interruptTurn(), { interrupted: true });
  const result = await turn;
  assert.equal(result.mode, "codex-app-server");
  adapter.close();
});

test("supports task review, context compaction and background terminal controls", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-openai-key", command });
  const events = [];
  adapter.setCallbacks({ onEvent: (event) => events.push(event) });
  await adapter.resumeThread("thr-test");
  const review = await adapter.reviewThread("thr-test");
  assert.equal(review.started, true);
  assert.equal(review.turn.id, "review-turn");
  await new Promise((resolve) => setTimeout(resolve, 10));
  assert.ok(events.some((event) => event.method === "agent/review"));
  assert.ok(events.some((event) => event.method === "item/completed" && event.params.item?.type === "exitedReviewMode"));
  assert.deepEqual(await adapter.compactThread("thr-test"), { started: true });
  await new Promise((resolve) => setTimeout(resolve, 10));
  assert.ok(events.some((event) => event.method === "item/completed" && event.params.item?.type === "contextCompaction"));
  const terminals = await adapter.listBackgroundTerminals("thr-test");
  assert.deepEqual(terminals.terminals, [{ processId: "process-42", command: "npm run watch", cwd: "/tmp/novel", status: "running" }]);
  assert.deepEqual(await adapter.terminateBackgroundTerminal("thr-test", "process-42"), { terminated: true });
  adapter.close();
});

test("rejects pending work when app-server exits cleanly", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-openai-key", command });
  await assert.rejects(adapter.sendTurn([{ type: "text", text: "clean-exit" }]), /exited \(0\)/);
  adapter.close();
});
