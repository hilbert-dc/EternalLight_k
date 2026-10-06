import assert from "node:assert/strict";
import test from "node:test";
import { mainHandlers, rendererEvent, runtimeFixture } from "./main-handler-harness.mjs";
const command = `${process.execPath} ${new URL("./fixture-app-server.mjs", import.meta.url).pathname}`;
const wait = async predicate => { for (let i = 0; i < 150 && !predicate(); i++) await new Promise(resolve => setTimeout(resolve, 10)); assert.ok(predicate()); };

for (const status of ["completed", "failed", "interrupted", "protocol"]) test(`active recovery preserves the original execution and settles its ${status} waiter`, async () => {
  const runtime = runtimeFixture();
  const { handlers, adapter } = await mainHandlers(runtime.request.bind(runtime), command);
  const request = adapter.request.bind(adapter); let reads = 0, starts = 0, release;
  adapter.request = async (method, params) => {
    if (["turn/start", "thread/resume"].includes(method)) starts++;
    if (method !== "thread/read") return request(method, params);
    reads++; if (reads === 1) await new Promise(resolve => { release = resolve; });
    return { thread: { id: params.threadId, status: { activeFlags: [] }, turns: [{ id: "foreign", status: "completed" },
      { id: "turn-test", status: reads === 1 ? "inProgress" : status === "protocol" ? "completed" : status, items: [{ id: "reply", type: "agentMessage", phase: "final_answer", text: status === "protocol" ? "<｜｜DSML｜｜ calls>内部协议" : "原任务结果" }] }] } };
  };
  const original = handlers.get("agent:send-prompt")(rendererEvent(), "project", "fixture-wait-for-steer").then(value => ({ value }), error => ({ error }));
  try {
    await wait(() => adapter.turnWaiter?.turnId === "turn-test");
    const waiter = adapter.turnWaiter, operation = adapter.turnOperation;
    const restore = (...args) => handlers.get("agent:reconnect-thread")({}, ...args);
    await assert.rejects(restore("other-project", "thr-test", "turn-test"), /原回合/);
    await assert.rejects(restore("project", "thr-test", "wrong-turn"), /回合已变化/);
    const checking = restore("project", "thr-test", "turn-test"); await wait(() => release);
    await assert.rejects(restore("project", "thr-test", "turn-test"), /正在核对/);
    release(); assert.equal((await checking).recovery.state, "running");
    assert.equal(adapter.turnWaiter, waiter); assert.equal(adapter.turnOperation, operation);
    await assert.rejects(handlers.get("agent:configure")({}, "deepseek", "deepseek-flash"), /已有创作任务/);
    if (status === "protocol") await assert.rejects(restore("project", "thr-test", "turn-test"), /模型未按结构化工具协议返回/);
    else {
      const recovered = await restore("project", "thr-test", "turn-test");
      assert.equal(recovered.recovery.state, status === "failed" ? "error" : status === "interrupted" ? "cancelled" : "completed");
    }
    const outcome = await original;
    if (status === "completed") { assert.equal(outcome.value.message, "原任务结果"); assert.equal(runtime.completions.length, 1); }
    else { assert.ok(outcome.error); assert.equal(runtime.completions.length, 0); }
    assert.equal(starts, 1, "recovery started or resumed another model turn");
  } finally { adapter.close(); }
});

test("a finished generation is located from Runtime without resuming its expired lease", async () => {
  const task = { id: "task-finished", projectId: "project", threadId: "thread", turnId: "turn", purpose: "writer", status: "completed" };
  const { handlers, adapter } = await mainHandlers(async path => path.endsWith("/lookup") ? { tasks: [task] } : task, command);
  try {
    const result = await handlers.get("agent:reconnect-thread")({}, "project", "thread", "turn");
    assert.deepEqual(JSON.parse(JSON.stringify(result)), { thread: { id: "thread" }, recovery: { turnId: "turn", state: "completed" } });
    assert.equal(adapter.process, null);
    await assert.rejects(handlers.get("agent:reconnect-thread")({}, "project", "thread", "wrong"), /不能恢复为普通对话/);
  } finally { adapter.close(); }
});

test("a delayed status read cannot settle the next waiter even with reused thread and turn IDs", async () => {
  const runtime = runtimeFixture();
  const { handlers, adapter } = await mainHandlers(runtime.request.bind(runtime), command);
  const request = adapter.request.bind(adapter); let release;
  adapter.request = async (method, params) => method === "thread/read" ? new Promise(resolve => { release = resolve; }) : request(method, params);
  const run = () => handlers.get("agent:send-prompt")(rendererEvent(), "project", "fixture-wait-for-steer").catch(error => error);
  let original = run();
  try {
    await wait(() => adapter.turnWaiter?.turnId === "turn-test");
    const late = handlers.get("agent:reconnect-thread")({}, "project", "thr-test", "turn-test");
    const rejected = assert.rejects(late, /回合已变化/); await wait(() => release);
    await handlers.get("agent:interrupt")({}); await original;
    original = run(); await wait(() => adapter.turnWaiter?.turnId === "turn-test");
    const waiter = adapter.turnWaiter;
    release({ thread: { id: "thr-test", turns: [{ id: "turn-test", status: "completed" }] } }); await rejected;
    assert.equal(adapter.turnWaiter, waiter);
    await handlers.get("agent:interrupt")({}); await original;
  } finally { adapter.close(); }
});
