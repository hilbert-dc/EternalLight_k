import assert from "node:assert/strict";
import { setImmediate as nextTick } from "node:timers/promises";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { CodexAppServerAdapter } from "../codex-adapter.mjs";

const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));
const boundary = () => {
  let resolve;
  const promise = new Promise(done => { resolve = done; });
  return { promise, resolve };
};

function harness(t) {
  const adapter = new CodexAppServerAdapter({ command: [process.execPath, fixture], env: { PATH: process.env.PATH } });
  const calls = [], waiters = new Set(), pending = [], releases = [];
  const request = adapter.request.bind(adapter);
  adapter.request = (method, params) => {
    calls.push({ method, params });
    if (["turn/start", "review/start", "thread/compact/start"].includes(method) && adapter.turnWaiter) waiters.add(adapter.turnWaiter);
    return request(method, params);
  };
  t.after(async () => {
    for (const release of releases) release();
    // Failed pre-fix assertions must also retire overwritten waiters, otherwise
    // their ten-minute deadlines hide the actual regression behind a hung test.
    for (const waiter of waiters) { clearTimeout(waiter.timer); waiter.reject(new Error("fixture cleanup")); }
    await adapter.finishTaskOperation();
    await Promise.allSettled(pending);
  });
  const track = promise => { pending.push(promise); void promise.catch(() => {}); return promise; };
  return { adapter, calls, releases, track };
}

test("rapid duplicate sends produce one turn and keep its original completion", async t => {
  const { adapter, calls, track } = harness(t);
  const started = boundary();
  adapter.setCallbacks({ onEvent: event => { if (event.method === "turn/started") started.resolve(); } });
  await adapter.ensureInitialized();
  const first = track(adapter.sendTurn([{ type: "text", text: "interrupt" }]));
  const second = track(adapter.sendTurn([{ type: "text", text: "interrupt" }]));
  await started.promise;
  await nextTick();
  assert.equal(calls.filter(call => call.method === "turn/start").length, 1);
  await assert.rejects(second, /正在处理上一轮请求/);
  assert.equal(adapter.status().state, "running");
  await adapter.interruptTurn();
  assert.equal((await first).turn.status, "interrupted");
  assert.equal(adapter.turnWaiter, null);
});

test("review and compaction cannot overwrite each other's completion", async t => {
  const { adapter, calls, track } = harness(t);
  await adapter.ensureInitialized();
  const first = track(adapter.reviewThread("thr-test"));
  const second = track(adapter.compactThread("thr-test"));
  const rejected = assert.rejects(second, /正在处理上一轮请求/);
  const result = await first;
  await rejected;
  assert.equal(result.turn.id, "review-turn");
  assert.deepEqual(calls.filter(call => ["review/start", "thread/compact/start"].includes(call.method)).map(call => call.method), ["review/start"]);
});

for (const lateFailure of [false, true])
test(`stopped turn's late ${lateFailure ? "failure" : "start response"} cannot alter its replacement`, async t => {
  const { adapter, calls, releases, track } = harness(t);
  const observed = boundary(), gate = boundary(), replacementStarted = boundary();
  releases.push(gate.resolve);
  const request = adapter.request.bind(adapter);
  let turns = 0;
  adapter.request = async (method, params) => {
    const old = method === "turn/start" && ++turns === 1;
    const result = await request(method, params);
    if (old) {
      observed.resolve();
      await gate.promise;
      if (lateFailure) throw new Error("fixture late start failure");
    } else if (method === "turn/start") replacementStarted.resolve();
    return result;
  };
  const first = track(adapter.sendTurn([{ type: "text", text: "interrupt" }], { threadName: "old turn" }));
  const rejected = assert.rejects(first, /已结束|重新绑定|fixture late start failure/);
  await observed.promise;
  await adapter.finishTaskOperation();
  const second = track(adapter.sendTurn([{ type: "text", text: "interrupt" }], { threadName: "new turn" }));
  await replacementStarted.promise;
  const waiter = adapter.turnWaiter;
  gate.resolve();
  await rejected;
  assert.equal(adapter.turnWaiter, waiter);
  assert.equal(adapter.turnId, "turn-test");
  assert.equal(adapter.status().state, "running");
  assert.ok(!calls.some(call => call.method === "thread/name/set" && call.params.name === "old turn"));
  await adapter.interruptTurn();
  assert.equal((await second).turn.status, "interrupted");
});

test("completed stream does not release ownership while follow-up RPC is outstanding", async t => {
  const { adapter, calls, releases, track } = harness(t);
  const observed = boundary(), gate = boundary();
  releases.push(gate.resolve);
  const request = adapter.request.bind(adapter);
  adapter.request = async (method, params) => {
    const result = await request(method, params);
    if (method === "thread/name/set" && params.name === "held naming") { observed.resolve(); await gate.promise; }
    return result;
  };
  const first = track(adapter.sendTurn([{ type: "text", text: "hello" }], { threadName: "held naming" }));
  await observed.promise;
  assert.equal(adapter.turnId, null);
  assert.equal(adapter.status().state, "running");
  assert.throws(() => adapter.assertConfigurable(), /正在处理上一轮请求/);
  await assert.rejects(adapter.sendTurn([{ type: "text", text: "hello" }]), /正在处理上一轮请求/);
  assert.equal(calls.filter(call => call.method === "turn/start").length, 1);
  gate.resolve();
  assert.equal((await first).message, "hello");
  assert.equal(adapter.status().state, "ready");
  adapter.assertConfigurable();
});

test("follow-up RPC failure retires the possibly active turn before retry", async t => {
  const { adapter, track } = harness(t);
  const request = adapter.request.bind(adapter);
  adapter.request = async (method, params) => {
    const result = await request(method, params);
    if (method === "thread/name/set" && params.name === "failed naming") throw new Error("fixture naming failure");
    return result;
  };
  const lease = { runId: "naming", leaseId: "lease", epoch: 1 };
  await adapter.bindExecution({ lease });
  adapter.prepareAdvisorySession("task-capability-naming-retry-1234567890");
  await adapter.ensureSession();
  const child = adapter.process;
  adapter.detachThread();
  await assert.rejects(track(adapter.sendTurn([{ type: "text", text: "interrupt" }], { threadName: "failed naming" })), /fixture naming failure/);
  assert.equal(adapter.status().state, "error");
  assert.equal(adapter.process, null);
  assert.equal(adapter.turnId, null);
  assert.equal(adapter.turnWaiter, null);
  assert.equal(adapter.turnOperation, null);
  assert.equal(adapter.pending.size, 0);
  assert.equal(child.exitCode !== null || child.signalCode !== null, true);
  assert.deepEqual(adapter.executionLease, lease);
  assert.equal(adapter.env.NOVEL_CODEX_TASK_CAPABILITY, "task-capability-naming-retry-1234567890");
  assert.equal((await track(adapter.sendTurn([{ type: "text", text: "hello" }]))).message, "hello");
  assert.notEqual(adapter.process, child);
});

test("a long active turn stays owned until the author interrupts it", async t => {
  const { adapter, track } = harness(t);
  const named = boundary();
  const request = adapter.request.bind(adapter);
  adapter.request = async (method, params) => {
    const result = await request(method, params);
    if (method === "thread/name/set") named.resolve();
    return result;
  };
  await adapter.ensureInitialized();
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const first = track(adapter.sendTurn([{ type: "text", text: "interrupt" }]));
  await named.promise;
  await nextTick();
  const child = adapter.process;
  t.mock.timers.tick(10 * 60 * 1000);
  assert.equal(adapter.process, child);
  assert.equal(adapter.status().state, "running");
  t.mock.timers.reset();
  await adapter.interruptTurn();
  assert.equal((await first).turn.status, "interrupted");
  await adapter.finishTaskOperation();
  assert.equal(adapter.process, null);
  assert.equal(adapter.turnWaiter, null);
  assert.equal(adapter.turnOperation, null);
  assert.equal(adapter.turnId, null);
  assert.equal(adapter.pending.size, 0);
  assert.equal(child.exitCode !== null || child.signalCode !== null, true);
  assert.equal((await track(adapter.sendTurn([{ type: "text", text: "hello" }]))).message, "hello");
  assert.notEqual(adapter.process, child);
});
