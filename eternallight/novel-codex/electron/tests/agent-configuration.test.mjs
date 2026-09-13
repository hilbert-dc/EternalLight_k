import assert from "node:assert/strict";
import { test } from "node:test";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { applyAgentConfiguration } from "../agent-configuration.mjs";
import { CodexAppServerAdapter } from "../codex-adapter.mjs";
import { modelInputTokenBudget } from "../../shared/author-input-budget.mjs";

const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));
const command = `${process.execPath} ${join("", fixture)}`;

test("main forwards author export and metric requests with their methods and selected payloads", async () => {
  const requests = [];
  const { handlers, adapter } = await mainHandlers(async (path, body, options) => {
    requests.push({ path, body, method: options.method ?? "GET" });
    return { content: "正式正文", projectRevision: 7 };
  }, command);
  try {
    const selected = { scopes: ["manuscript"], format: "markdown" };
    const result = await handlers.get("runtime:export-project")(rendererEvent(), "project", selected);
    assert.equal(result.content, "正式正文");
    await handlers.get("runtime:metrics")(rendererEvent());
    await handlers.get("runtime:metrics-abandon")(rendererEvent(), "brief", "00000000-0000-4000-8000-000000000001");
    assert.deepEqual(requests, [
      { path: "/api/projects/project/export", body: selected, method: "POST" },
      { path: "/api/metrics", body: null, method: "GET" },
      { path: "/api/metrics/abandon", body: { stage: "brief", idempotencyKey: "00000000-0000-4000-8000-000000000001" }, method: "POST" },
    ]);
  } finally { adapter.close(); }
});

function deepSeekConfiguration() {
  return {
    command,
    cwd: process.cwd(),
    env: { PATH: process.env.PATH },
    model: "deepseek-flash",
    provider: "deepseek",
  };
}

test("reconfigures an idle app-server that was started by listing threads", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-openai-key", command });
  await adapter.listThreads();
  const child = adapter.process;
  let persisted = false;
  const status = applyAgentConfiguration({
    adapter,
    configuration: deepSeekConfiguration(),
    persist: () => { persisted = true; },
  });
  assert.equal(persisted, true);
  assert.equal(child.killed, true);
  assert.equal(adapter.process, null);
  assert.equal(status.provider, "deepseek");
  assert.equal(status.model, "deepseek-flash");
});

test("rejects an active-turn reconfiguration before persistence", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-openai-key", command });
  let started;
  const turnStarted = new Promise((resolve) => { started = resolve; });
  adapter.setCallbacks({ onEvent: (event) => { if (event.method === "turn/started") started(); } });
  const turn = adapter.sendTurn([{ type: "text", text: "interrupt" }]);
  await turnStarted;
  let persisted = false;
  assert.throws(() => applyAgentConfiguration({
    adapter,
    configuration: deepSeekConfiguration(),
    persist: () => { persisted = true; },
  }), /正在处理上一轮请求/);
  assert.equal(persisted, false);
  assert.equal(adapter.provider, "openai");
  await adapter.interruptTurn();
  await turn;
  adapter.close();
});

test("keeps the running configuration when persistence fails", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-openai-key", command });
  await adapter.listThreads();
  const child = adapter.process;
  assert.throws(() => applyAgentConfiguration({
    adapter,
    configuration: deepSeekConfiguration(),
    persist: () => { throw new Error("disk full"); },
  }), /disk full/);
  assert.equal(adapter.provider, "openai");
  assert.equal(adapter.process, child);
  assert.equal(child.killed, false);
  adapter.close();
});

import { mainHandlers, rendererEvent, runtimeFixture } from "./main-handler-harness.mjs";

test("main rejects model changes throughout Runtime preparation", async () => {
  const runtime = runtimeFixture();
  let release;
  let entered;
  const preparing = new Promise((resolve) => { entered = resolve; });
  const gate = new Promise((resolve) => { release = resolve; });
  const {handlers,adapter} = await mainHandlers(async (path, body) => {
    if (path.endsWith('/creative-workspace')) { entered(); await gate; }
    return runtime.request(path, body);
  }, command);
  try {
    const turn = handlers.get('agent:send-prompt')(rendererEvent(),'project','hello');
    await preparing;
    assert.equal(adapter.turnWaiter,null);
    assert.throws(()=>handlers.get('agent:configure')({},'deepseek','deepseek-flash'),/已有 Codex 任务正在运行/);
    assert.equal(adapter.provider,'openai');
    release();
    await turn;
  } finally { release(); adapter.close(); }
});

test("main persists a bounded Unicode summary and returns the complete discussion", async () => {
  const runtime = runtimeFixture();
  const {handlers,adapter} = await mainHandlers(runtime.request.bind(runtime),command);
  try {
    const result = await handlers.get('agent:send-prompt')(rendererEvent(),'project','integration-long-reply');
    assert.equal(result.message,'🌙'.repeat(5000));
    assert.equal(runtime.completions[0].summary,'🌙'.repeat(4000));
  } finally {adapter.close();}
});

test("main hands a long Chinese discussion to planning in one bounded turn without losing text", async () => {
  const runtime = runtimeFixture();
  const { handlers, adapter } = await mainHandlers(runtime.request.bind(runtime), command);
  const intent = "作者原始意图：" + "父女在饭桌上互相试探，最后一起看月亮。".repeat(220) + "🌙终点";
  try {
    for (const [name, args] of [
      ["agent:send-prompt", ["project", intent]],
      ["agent:generate-planning", ["project", "generateBrief", intent]],
    ]) {
      const result = await handlers.get(name)(rendererEvent(), ...args);
      const input = result.turn.receivedParams.input;
      assert.ok(input.length > 1);
      assert.ok(input.every((item) => item.type === "text" && Buffer.byteLength(item.text, "utf8") <= 9500));
      assert.ok(input.every((item) => Buffer.from(item.text, "utf8").toString("utf8") === item.text));
      const full = input.map((item) => item.text).join("");
      assert.ok(full.endsWith(intent));
      assert.match(full, /novel-workflow/);
      assert.match(full, /taskId/);
      assert.ok(modelInputTokenBudget(full) <= 9000);
      assert.equal(full.includes("\ufffd"), false);
      assert.deepEqual(result.turn.receivedParams.environments, []);
    }
  } finally { adapter.close(); }
});

test("main rejects an oversized Unicode intent before creating a task", async () => {
  const runtime = runtimeFixture();
  const { handlers, adapter } = await mainHandlers(runtime.request.bind(runtime), command);
  try {
    await assert.rejects(handlers.get("agent:send-prompt")(rendererEvent(), "project", "🌙 ".repeat(5000)), /文本预算/);
    assert.equal(runtime.tasks.size, 0);
    assert.equal(adapter.process, null);
  } finally { adapter.close(); }
});

test("main steers an existing turn with all long Unicode instructions", async () => {
  const runtime = runtimeFixture();
  const { handlers, adapter } = await mainHandlers(runtime.request.bind(runtime), command);
  let started;
  const ready = new Promise((resolve) => { started = resolve; });
  adapter.setCallbacks({ onEvent: (event) => { if (event.method === "turn/started") started(); } });
  const intent = "请保留人物的克制和犹豫。".repeat(330) + "🌙结束";
  try {
    const turn = adapter.sendTurn([{ type: "text", text: "steer" }]);
    await ready;
    await handlers.get("agent:steer")({}, intent);
    const result = await turn;
    const input = result.turn.receivedParams.input;
    assert.ok(input.length > 1);
    assert.ok(input.every((item) => Buffer.byteLength(item.text, "utf8") <= 9500));
    assert.ok(input.map((item) => item.text).join("").endsWith(intent));
    assert.equal(result.turn.receivedParams.expectedTurnId, "turn-test");
  } finally { adapter.close(); }
});

for (const restore of ['agent:resume-thread','agent:reconnect-thread']) {
  test(`main ${restore} keeps the selected historical advisory on the next turn`, async () => {
    const runtime = runtimeFixture();
    runtime.tasks.set('thr-history',{threadId:'thr-history',purpose:'advisory',target:{type:'project'}});
    const {handlers,adapter} = await mainHandlers(runtime.request.bind(runtime),command);
    try {
      await handlers.get('agent:send-prompt')(rendererEvent(),'project','first discussion');
      await handlers.get(restore)({},'project','thr-history');
      const result = await handlers.get('agent:send-prompt')(rendererEvent(),'project','continue selected');
      assert.equal(result.workflow.task.threadId,'thr-history');
      assert.equal(result.message,'hello');
      const page = await handlers.get('agent:list-threads')({},'project');
      assert.equal(page.threads[0].id,'thr-history');
      await assert.rejects(handlers.get('agent:resume-thread')({},'other-project','foreign-thread'),/该任务不属于当前作品/);
    } finally {adapter.close();}
  });
}

test("main restores an advisory after application restart without an in-memory preference", async () => {
  const runtime = runtimeFixture();
  runtime.tasks.set('thr-history',{threadId:'thr-history',purpose:'advisory',target:{type:'project'}});
  const {handlers,adapter} = await mainHandlers(runtime.request.bind(runtime),command);
  try {
    await handlers.get('agent:resume-thread')({},'project','thr-history');
    const result = await handlers.get('agent:send-prompt')(rendererEvent(),'project','continue after restart');
    assert.equal(result.workflow.task.threadId,'thr-history');
  } finally {adapter.close();}
});


test("main returns an empty thread page without an invalid ownership lookup", async () => {
  const runtime = runtimeFixture();
  const {handlers,adapter} = await mainHandlers(runtime.request.bind(runtime),command);
  try {
    const page = await handlers.get('agent:list-threads')({},'project',false,'empty-page');
    assert.equal(page.threads.length,0);
    assert.equal(runtime.lookup.length,0);
  } finally {adapter.close();}
});
