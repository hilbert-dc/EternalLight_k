import assert from "node:assert/strict";
import test from "node:test";
import { mainHandlers, rendererEvent, runtimeFixture } from "./main-handler-harness.mjs";

const command = `${process.execPath} ${new URL("./fixture-app-server.mjs", import.meta.url).pathname}`;
const deferred = () => { let resolve; const promise = new Promise(r => { resolve = r; }); return { promise, resolve }; };
const ready = async predicate => { for (let i = 0; i < 500; i++) { if (predicate()) return; await new Promise(r => setTimeout(r, 10)); } assert.fail("execution did not become ready"); };

test("stop during task creation fences the late task and preserves its original Work", async () => {
  const runtime = runtimeFixture(), entered = deferred(), release = deferred(), controls = [];
  const host = await mainHandlers(async (path, body) => {
    const result = await runtime.request(path, body);
    if (path.endsWith("/tasks") && body) { entered.resolve(); await release.promise; }
    if (path.endsWith("/control")) controls.push(body.command);
    assert.ok(!path.endsWith("/cancel"), "pause must not terminally cancel Work");
    return result;
  }, command);
  const event = rendererEvent();
  const pending = host.handlers.get("agent:send-prompt")(event, "project", "fixture-wait-for-steer");
  const rejected = assert.rejects(pending, error => error.code === "TASK_CANCELLED");
  try {
    await entered.promise;
    const target = host.executionTargets.get(event);
    await assert.rejects(host.handlers.get("agent:interrupt")(event), /指定原执行/);
    await assert.rejects(host.handlers.get("agent:interrupt")(rendererEvent(), target), /原执行/);
    assert.equal((await host.handlers.get("agent:interrupt")(event, target)).interrupted, true);
    release.resolve(); await rejected;
    assert.deepEqual(controls, ["pause"]);
    assert.equal([...runtime.workTasks.values()][0].status, "paused");
    assert.equal(runtime.completions.length, 0);
    assert.equal(host.adapter.turnWaiter, null);
  } finally { release.resolve(); host.adapter.close(); }
});

test("a stale execution request cannot stop a new turn in the same conversation", async () => {
  const runtime = runtimeFixture(), host = await mainHandlers(runtime.request.bind(runtime), command), event = rendererEvent();
  const start = () => host.handlers.get("agent:send-prompt")(event, "project", "fixture-wait-for-steer").catch(error => error);
  try {
    let pending = start(); await ready(() => host.adapter.turnWaiter?.turnId);
    const old = host.executionTargets.get(event);
    await host.handlers.get("agent:interrupt")(event, old); await pending;
    pending = start(); await ready(() => host.adapter.turnWaiter?.turnId);
    const waiter = host.adapter.turnWaiter;
    assert.deepEqual(await host.handlers.get("agent:interrupt")(event, old), { interrupted: false });
    assert.equal(host.adapter.turnWaiter, waiter);
    await host.handlers.get("agent:interrupt")(event, host.executionTargets.get(event)); await pending;
    assert.ok([...runtime.workTasks.values()].every(work => work.status === "paused"));
  } finally { host.adapter.close(); }
});

test("Native pause failure still stops the exact local turn and is never reported as a pause receipt", async () => {
  const runtime = runtimeFixture();
  const host = await mainHandlers((path, body) => {
    if (path.endsWith("/control") && body?.command === "pause") throw Error("Native control unavailable");
    return runtime.request(path, body);
  }, command), event = rendererEvent();
  const pending = host.handlers.get("agent:send-prompt")(event, "project", "fixture-wait-for-steer").catch(error => error);
  try {
    await ready(() => host.adapter.turnWaiter?.turnId);
    await assert.rejects(host.handlers.get("agent:interrupt")(event, host.executionTargets.get(event)), /暂停尚未确认.*Native control unavailable/);
    await pending;
    assert.equal(host.adapter.turnWaiter, null);
    assert.notEqual([...runtime.workTasks.values()][0].status, "paused");
    assert.equal(runtime.completions.length, 0);
  } finally { host.adapter.close(); }
});
