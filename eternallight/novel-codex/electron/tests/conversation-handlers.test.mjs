import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { mainHandlers, rendererEvent, runtimeFixture } from "./main-handler-harness.mjs";
import { registerConversationHandlers } from "../conversation-handlers.mjs";
import { readFile } from "node:fs/promises";
import { runInNewContext } from "node:vm";

const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;
const focus = { view: "outline", excerpts: [{ id: "quote", projectId: "project", view: "outline", label: "全书大纲", text: "未保存选区", state: "unsaved", locator: "outline", snapshotId: null, revisionId: null, objectId: null, start: 0, end: 5 }] };
const invoke = async (handlers, channel, ...args) => handlers.get(channel)(rendererEvent(), ...args);
const readRuntime = runtime => (path, body, options, url) => runtime.request(path, body, options, url);

test("real preload forwards every explicit conversation identity and typed channel argument", async () => {
  let api;
  const calls = [];
  runInNewContext(await readFile(new URL("../preload.cjs", import.meta.url), "utf8"), {
    crypto: { randomUUID: () => "request-test" }, process: { platform: "test" },
    require: name => {
      assert.equal(name, "electron");
      return { contextBridge: { exposeInMainWorld: (key, value) => { if (key === "novelCodex") api = value; } },
        ipcRenderer: { invoke: async (...args) => { calls.push(args); return {}; } } };
    },
  });
  await api.agent.sendPrompt("project", "问题", null, focus, "selected-thread");
  await api.agent.createThread("project", { sourceThreadId: "source-thread", lastTurnId: "source-turn" });
  await api.agent.renameThread("project", "selected-thread", "人物讨论");
  await api.agent.readConversation("project", "selected-thread");
  await api.agent.steer("project", "补充", "running-thread");
  await api.agent.setThreadPinned("project", "selected-thread", true);
  await api.agent.archiveThread("project", "selected-thread");
  await api.agent.unarchiveThread("project", "selected-thread");
  await api.agent.updateThreadReadState("project", "selected-thread", { operation: "read", expectedRevision: "observed" });
  assert.deepEqual(JSON.parse(JSON.stringify(calls)), [
    ["agent:send-prompt", { requestId: "request-test", args: ["project", "问题", null, focus, "selected-thread", null] }],
    ["agent:create-thread", "project", { sourceThreadId: "source-thread", lastTurnId: "source-turn" }],
    ["agent:rename-thread", "project", "selected-thread", "人物讨论"],
    ["agent:read-conversation", "project", "selected-thread"],
    ["agent:steer", "project", "补充", "running-thread"],
    ["agent:set-thread-pinned", "project", "selected-thread", true],
    ["agent:archive-thread", "project", "selected-thread"],
    ["agent:unarchive-thread", "project", "selected-thread"],
    ["agent:update-thread-read-state", "project", "selected-thread", { operation: "read", expectedRevision: "observed" }],
  ]);
});

test("production read-state IPC checks conversation ownership before forwarding observed revision", async t => {
  const runtime = runtimeFixture();
  const { handlers, adapter } = await mainHandlers(readRuntime(runtime), command);
  t.after(() => adapter.close());
  const { thread } = await invoke(handlers, "agent:create-thread", "project", {});
  const requests = [];
  adapter.updateThreadReadState = async (threadId, request) => {
    requests.push({ threadId, request });
    return { readState: { revision: "updated", firstUnread: null } };
  };
  const request = { operation: "read", expectedRevision: "observed" };
  assert.deepEqual(await invoke(handlers, "agent:update-thread-read-state", "project", thread.id, request),
    { readState: { revision: "updated", firstUnread: null } });
  await assert.rejects(invoke(handlers, "agent:update-thread-read-state", "other-project", thread.id, request));
  assert.deepEqual(requests, [{ threadId: thread.id, request }]);
});

test("production empty conversations persist ownership and restore exact independent engine threads", async () => {
  const runtime = runtimeFixture(), state = {};
  const first = await mainHandlers(readRuntime(runtime), command, state);
  const creationCalls = [], request = first.adapter.request.bind(first.adapter);
  first.adapter.request = (method, params) => { creationCalls.push({ method, params }); return request(method, params); };
  let second;
  try {
    const a = await invoke(first.handlers, "agent:create-thread", "project");
    const b = await invoke(first.handlers, "agent:create-thread", "project");
    assert.notEqual(a.thread.id, b.thread.id);
    assert.equal(runtime.tasks.size, 0);
    assert.deepEqual(creationCalls.filter(call => call.method === "thread/start").map(call => call.params.config), [
      { "mcp_servers.novel_workspace.enabled": false }, { "mcp_servers.novel_workspace.enabled": false },
    ]);
    assert.equal(creationCalls.filter(call => call.method === "turn/start").length, 0);
    const list = await invoke(first.handlers, "agent:list-threads", "project");
    assert.deepEqual(new Set(list.threads.map(thread => thread.id)), new Set([a.thread.id, b.thread.id]));
    const empty = await invoke(first.handlers, "agent:read-conversation", "project", a.thread.id);
    assert.deepEqual(empty.turns, []);
    await first.adapter.close();
    second = await mainHandlers(readRuntime(runtime), command, state);
    assert.equal((await invoke(second.handlers, "agent:list-threads", "project")).threads.length, 2);
    const restored = await invoke(second.handlers, "agent:resume-thread", "project", b.thread.id);
    assert.equal(restored.thread.id, b.thread.id);
    assert.equal(second.adapter.threadId, b.thread.id);
    const aAgain = await invoke(second.handlers, "agent:read-conversation", "project", a.thread.id);
    assert.equal(aAgain.thread.id, a.thread.id);
    assert.deepEqual(aAgain.turns, []);
    await assert.rejects(invoke(second.handlers, "agent:read-conversation", "foreign", a.thread.id), /不属于/);
  } finally { await first.adapter.close(); await second?.adapter.close(); }
});

test("production send uses explicitly selected conversation and blocks foreign or archived threads before inference", async () => {
  const runtime = runtimeFixture();
  const turns = [], taskResumes = [];
  const { handlers, adapter } = await mainHandlers(readRuntime(runtime), command, {
    onPrepareDomain(domain) {
      const request = domain.codexAdapter.request.bind(domain.codexAdapter);
      domain.codexAdapter.request = async (method, params) => {
        if (method === "turn/start") turns.push(params);
        if (method === "thread/resume") taskResumes.push(params);
        return request(method, params);
      };
    },
  });
  try {
    const a = await invoke(handlers, "agent:create-thread", "project");
    const b = await invoke(handlers, "agent:create-thread", "project");
    assert.equal(adapter.threadId, b.thread.id);
    const sent = await invoke(handlers, "agent:send-prompt", "project", "只讨论 A", null, focus, a.thread.id);
    assert.equal(sent.workflow.task.threadId, a.thread.id);
    assert.equal(turns.length, 1);
    assert.equal(turns[0].threadId, a.thread.id);
    assert.deepEqual(taskResumes.at(-1), { threadId: a.thread.id,
      config: { "mcp_servers.novel_workspace.enabled": true, "mcp_servers.novel_workspace.required": true } });
    assert.equal(runtime.tasks.has(b.thread.id), false);
    const foreign = await invoke(handlers, "agent:create-thread", "foreign");
    await assert.rejects(invoke(handlers, "agent:send-prompt", "project", "不能发送", null, focus, foreign.thread.id), /不属于/);
    await invoke(handlers, "agent:archive-thread", "project", a.thread.id);
    await assert.rejects(invoke(handlers, "agent:send-prompt", "project", "不能发送", null, focus, a.thread.id), /已归档/);
    assert.equal(turns.length, 1);
  } finally { await adapter.close(); }
});

test("production registration failure retries the same created engine thread without duplicate start", async () => {
  const runtime = runtimeFixture(); let failRegistration = true;
  const { handlers, adapter } = await mainHandlers(async (path, body, options, url) => {
    if (path.endsWith("/conversations") && body && failRegistration) { failRegistration = false; throw Error("登记暂时失败"); }
    return runtime.request(path, body, options, url);
  }, command);
  let starts = 0;
  const request = adapter.request.bind(adapter);
  adapter.request = async (method, params) => { if (method === "thread/start") starts++; return request(method, params); };
  try {
    await assert.rejects(invoke(handlers, "agent:create-thread", "project"), /登记暂时失败/);
    const retry = await invoke(handlers, "agent:create-thread", "project");
    assert.equal(starts, 1);
    assert.equal(retry.thread.id, "thr-test");
    assert.equal(runtime.conversations.size, 1);
    const next = await invoke(handlers, "agent:create-thread", "project");
    assert.notEqual(next.thread.id, retry.thread.id);
    assert.equal(starts, 2);
  } finally { await adapter.close(); }
});

test("production name-save failure keeps the same pending thread and registers only after verified persistence", async () => {
  const runtime = runtimeFixture();
  let verifiedName = false, registrations = 0;
  const { handlers, adapter } = await mainHandlers(async (path, body, options, url) => {
    if (path.endsWith("/conversations") && body) {
      assert.equal(verifiedName, true, "registration must follow actual same-thread name verification");
      assert.equal(body.title, "新对话");
      registrations++;
    }
    return runtime.request(path, body, options, url);
  }, command);
  const calls = [], request = adapter.request.bind(adapter);
  let failName = true;
  adapter.request = async (method, params) => {
    calls.push({ method, params });
    if (method === "thread/name/set" && failName) { failName = false; throw Error("新对话名称保存失败"); }
    const result = await request(method, params);
    if (method === "thread/read" && params.includeTurns === true) {
      assert.equal(result.thread.id, params.threadId);
      assert.equal(result.thread.name, "新对话");
      verifiedName = true;
    }
    return result;
  };
  try {
    await assert.rejects(invoke(handlers, "agent:create-thread", "project"), /名称保存失败/);
    assert.equal(registrations, 0);
    assert.equal(runtime.conversations.size, 0);
    const retry = await invoke(handlers, "agent:create-thread", "project");
    assert.equal(registrations, 1);
    assert.equal(adapter.status().error, null);
    assert.equal(runtime.conversations.size, 1);
    assert.equal(runtime.tasks.size, 0);
    assert.equal(calls.filter(call => call.method === "thread/start").length, 1);
    const saves = calls.filter(call => call.method === "thread/name/set");
    assert.equal(saves.length, 2);
    assert.ok(saves.every(call => call.params.threadId === retry.thread.id && call.params.name === "新对话"));
    assert.equal(calls.filter(call => call.method === "turn/start").length, 0);
    assert.deepEqual((await invoke(handlers, "agent:read-conversation", "project", retry.thread.id)).turns, []);
  } finally { await adapter.close(); }
});

test("production mismatched saved name cannot register or replace the pending engine thread", async () => {
  const runtime = runtimeFixture();
  const { handlers, adapter } = await mainHandlers(readRuntime(runtime), command);
  const request = adapter.request.bind(adapter);
  let starts = 0, corruptName = true;
  adapter.request = async (method, params) => {
    if (method === "thread/start") starts++;
    const result = await request(method, params);
    if (method === "thread/read" && params.includeTurns === true && corruptName) {
      corruptName = false;
      return { ...result, thread: { ...result.thread, name: "未保存名称" } };
    }
    return result;
  };
  try {
    await assert.rejects(invoke(handlers, "agent:create-thread", "project"), /持久化核对/);
    assert.equal(runtime.conversations.size, 0);
    const retry = await invoke(handlers, "agent:create-thread", "project");
    assert.equal(starts, 1);
    assert.equal(retry.thread.id, adapter.threadId);
    assert.equal(retry.thread.name, "新对话");
    assert.equal(runtime.conversations.size, 1);
    assert.equal(runtime.tasks.size, 0);
  } finally { await adapter.close(); }
});

test("production fork sends exact source turn and restores original author text in copied history", async () => {
  const runtime = runtimeFixture();
  const { handlers, adapter } = await mainHandlers(readRuntime(runtime), command);
  const forks = [];
  const request = adapter.request.bind(adapter);
  adapter.request = async (method, params) => { if (method === "thread/fork") forks.push(params); return request(method, params); };
  try {
    const source = await invoke(handlers, "agent:create-thread", "project");
    await invoke(handlers, "agent:send-prompt", "project", "原作者问题  😀", null, focus, source.thread.id);
    const branch = await invoke(handlers, "agent:create-thread", "project", { sourceThreadId: source.thread.id, lastTurnId: "turn-test" });
    assert.notEqual(branch.thread.id, source.thread.id);
    assert.deepEqual(forks[0], { threadId: source.thread.id, lastTurnId: "turn-test" });
    const stored = runtime.conversations.get(branch.thread.id);
    assert.equal(stored.forkedFromId, source.thread.id);
    assert.equal(stored.lastTurnId, "turn-test");
    const history = await invoke(handlers, "agent:read-conversation", "project", branch.thread.id);
    assert.equal(history.turns[0].items.find(item => item.type === "userMessage").text, "原作者问题  😀");
    assert.equal(JSON.stringify(history).includes("不登记候选修改"), false);
    await assert.rejects(invoke(handlers, "agent:create-thread", "foreign", { sourceThreadId: source.thread.id, lastTurnId: "turn-test" }), /不属于/);
    assert.equal(forks.length, 1);
  } finally { await adapter.close(); }
});

test("metadata failures keep persisted truth and active execution permits independent metadata edits", async () => {
  const runtime = runtimeFixture(), handlers = new Map(); let failure = null, busy = false, writes = 0;
  await runtime.request("/api/projects/project/conversations", { threadId: "thread-a", title: "新对话", forkedFromId: null, lastTurnId: null });
  registerConversationHandlers({ handle: (channel, callback) => handlers.set(channel, callback) }, {
    runtimeRequest: async (path, options) => {
      if (options?.method === "PATCH") { writes++; if (failure) throw Error(failure); }
      return runtime.request(path.split("?")[0], options?.body ? JSON.parse(options.body) : null, options, new URL(path, "http://fixture"));
    }, adapter: () => { throw Error("metadata must not mutate engine thread"); }, isBusy: () => busy,
    exclusive: async callback => callback(), readHistory: async () => { throw Error("unused"); },
  });
  const original = { ...runtime.conversations.get("thread-a") };
  for (const [channel, args] of [["agent:rename-thread", ["人物塑造"]], ["agent:set-thread-pinned", [true]], ["agent:archive-thread", []]]) {
    failure = "乐观版本冲突";
    await assert.rejects(invoke(handlers, channel, "project", "thread-a", ...args), /乐观版本冲突/);
    assert.deepEqual(runtime.conversations.get("thread-a"), original);
  }
  failure = null;
  const renamed = await invoke(handlers, "agent:rename-thread", "project", "thread-a", "人物塑造");
  assert.equal(renamed.thread.name, "人物塑造");
  const pinned = await invoke(handlers, "agent:set-thread-pinned", "project", "thread-a", true);
  assert.equal(pinned.thread.isPinned, true);
  const archived = await invoke(handlers, "agent:archive-thread", "project", "thread-a");
  assert.equal(archived.thread.isArchived, true);
  const restored = await invoke(handlers, "agent:unarchive-thread", "project", "thread-a");
  assert.equal(restored.thread.isArchived, false);
  assert.equal(runtime.conversations.get("thread-a").version, 4);
  const previousWrites = writes;
  await assert.rejects(invoke(handlers, "agent:rename-thread", "project", "thread-a", "x".repeat(121)), /名称/);
  await assert.rejects(invoke(handlers, "agent:set-thread-pinned", "project", "thread-a", "true"), /参数/);
  await assert.rejects(invoke(handlers, "agent:create-thread", "project", { sourceThreadId: "thread-a", lastTurnId: "turn", injected: true }), /参数/);
  busy = true;
  assert.equal(writes, previousWrites);
  const activeRename = await invoke(handlers, "agent:rename-thread", "project", "thread-a", "运行中改名");
  assert.equal(activeRename.thread.name, "运行中改名");
  const activePin = await invoke(handlers, "agent:set-thread-pinned", "project", "thread-a", false);
  assert.equal(activePin.thread.isPinned, false);
  assert.equal(writes, previousWrites + 2);
});
