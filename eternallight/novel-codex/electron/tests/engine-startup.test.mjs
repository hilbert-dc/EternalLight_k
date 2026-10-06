import assert from "node:assert/strict";
import { setImmediate as nextTick } from "node:timers/promises";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { CodexAppServerAdapter } from "../codex-adapter.mjs";

const fixture = fileURLToPath(new URL("./fixture-delayed-engine.mjs", import.meta.url));
const create = (delay, timeouts) => new CodexAppServerAdapter({command:[process.execPath,fixture],
  env:{PATH:process.env.PATH,FIXTURE_INITIAL_DELAY:String(delay)},timeouts});

test("cold startup has a separate deadline and concurrent callers share one handshake", async t => {
  const adapter = create(150,{startupMs:2000,requestMs:50});
  t.after(() => adapter.finishTaskOperation());
  const first = adapter.ensureInitialized();
  await nextTick();
  const child = adapter.process;
  assert.equal(adapter.status().state,"starting");
  const second = adapter.ensureInitialized();
  assert.equal(adapter.process,child);
  assert.equal(adapter.pending.size,1);
  assert.deepEqual(await Promise.all([first,second]),[true,true]);
  assert.equal(adapter.status().state,"ready");
  adapter.env.FIXTURE_REQUEST_DELAY = "150";
  // The child inherited only its initial environment; rotate to exercise the
  // ordinary RPC timeout independently of the already successful handshake.
  await adapter.finishTaskOperation();
  await adapter.ensureInitialized();
  await assert.rejects(adapter.request("thread/list",{}),/请求超时：thread\/list/);
  assert.equal(adapter.pending.size,0);
});

test("startup failure closes its child and can be retried without losing the owning lease", async t => {
  const adapter = create(5000,{startupMs:100,requestMs:100});
  t.after(() => adapter.finishTaskOperation());
  const lease = {runId:"run",leaseId:"lease",epoch:1};
  await adapter.bindExecution({lease});
  const failed = adapter.ensureInitialized();
  const child = adapter.process;
  await assert.rejects(failed,/请求超时：initialize/);
  assert.equal(adapter.process,null);
  assert.equal(adapter.pending.size,0);
  assert.equal(adapter.initializing,null);
  assert.equal(child.exitCode !== null || child.signalCode !== null,true);
  assert.deepEqual(adapter.executionLease,lease);
  await adapter.endExecution(lease);
  adapter.env.FIXTURE_INITIAL_DELAY = "0";
  adapter.timeouts.startupMs = 2000;
  const retry = adapter.ensureInitialized();
  assert.equal(adapter.status().state,"starting");
  await retry;
  assert.equal(adapter.status().state,"ready");
  assert.equal(adapter.lastError,null);
});

test("rotating a task during startup cannot let the old initialization close the new child", async t => {
  const adapter = create(1000,{startupMs:2000,requestMs:100});
  t.after(() => adapter.finishTaskOperation());
  const first = adapter.ensureInitialized();
  const rejected = assert.rejects(first,/会话已重新绑定/);
  const old = adapter.process;
  adapter.env.FIXTURE_INITIAL_DELAY = "0";
  adapter.prepareTaskOperation("modelKnowledge","new-task-capability-1234567890");
  const second = adapter.ensureInitialized();
  const current = adapter.process;
  await rejected;
  assert.notEqual(current,old);
  assert.equal(adapter.process,current);
  assert.notEqual(adapter.initializing,null);
  await second;
  assert.equal(adapter.process,current);
  assert.equal(adapter.env.NOVEL_CODEX_TASK_CAPABILITY,"new-task-capability-1234567890");
  assert.equal(adapter.lastError,null);
});

test("session startup waits for required tools beyond the ordinary RPC deadline", async t => {
  const adapter = create(0, { startupMs: 2000, requestMs: 50 });
  adapter.env.FIXTURE_REQUEST_DELAY = "200";
  t.after(() => adapter.finishTaskOperation());
  await adapter.ensureInitialized();
  const first = adapter.ensureSession();
  const second = adapter.ensureSession();
  await nextTick();
  assert.equal(adapter.status().state, "starting");
  assert.equal(adapter.pending.size, 1);
  assert.deepEqual(await Promise.all([first, second]), ["delayed-thread", "delayed-thread"]);
  assert.equal(adapter.status().state, "ready");
  adapter.detachThread();
  assert.equal((await adapter.resumeThread("delayed-thread")).thread.id, "delayed-thread");
  await assert.rejects(adapter.request("thread/list", {}), /请求超时：thread\/list/);
  assert.equal(adapter.pending.size, 0);
});

test("a stalled session still expires and task cleanup retires the child", async t => {
  const adapter = create(0, { startupMs: 200, requestMs: 50 });
  adapter.env.FIXTURE_REQUEST_DELAY = "5000";
  t.after(() => adapter.finishTaskOperation());
  await adapter.ensureInitialized();
  const child = adapter.process;
  await assert.rejects(adapter.ensureSession(), /请求超时：thread\/start/);
  assert.equal(adapter.pending.size, 0);
  assert.equal(adapter.threadStarting, null);
  assert.equal(adapter.threadId, null);
  await adapter.finishTaskOperation();
  assert.equal(adapter.process, null);
  assert.equal(child.exitCode !== null || child.signalCode !== null, true);
});

test("concurrent restore and session callers share readiness and cannot switch its target", async t => {
  const adapter = create(0, { startupMs: 2000, requestMs: 50 });
  adapter.env.FIXTURE_REQUEST_DELAY = "150";
  t.after(() => adapter.finishTaskOperation());
  await adapter.ensureInitialized();
  const first = adapter.resumeThread("saved-thread");
  const second = adapter.resumeThread("saved-thread");
  const joined = adapter.ensureSession();
  await nextTick();
  assert.equal(adapter.status().state, "starting");
  assert.equal(adapter.pending.size, 1);
  await assert.rejects(adapter.resumeThread("different-thread"), /会话正在准备/);
  assert.throws(() => adapter.detachThread(), /会话正在准备/);
  const [restored, duplicate, id] = await Promise.all([first, second, joined]);
  assert.deepEqual(duplicate, restored);
  assert.equal(id, "saved-thread");
  assert.equal(adapter.status().state, "ready");
});

for (const restoring of [false, true])
test(`a stale ${restoring ? "restore" : "creation"} response cannot publish into or clear a new session`, async t => {
  const adapter = create(0, { startupMs: 2000, requestMs: 500 });
  adapter.env.FIXTURE_REQUEST_DELAY = "150";
  const events = [];
  adapter.setCallbacks({ onEvent: event => events.push(event) });
  let release, observed;
  const boundary = new Promise(resolve => { observed = resolve; });
  const gate = new Promise(resolve => { release = resolve; });
  t.after(async () => { release(); await adapter.finishTaskOperation(); });
  const request = adapter.request.bind(adapter);
  let sessions = 0;
  adapter.request = async (method, params) => {
    const session = ["thread/start", "thread/resume"].includes(method);
    const first = session && ++sessions === 1;
    const result = await request(method, params);
    if (first) { observed(); await gate; }
    return result;
  };
  const old = restoring ? adapter.resumeThread("old-thread") : adapter.ensureSession();
  const rejected = assert.rejects(old, /会话已重新绑定/);
  await boundary;
  adapter.env.FIXTURE_THREAD_ID = "new-thread";
  adapter.prepareTaskOperation("modelKnowledge", "new-task-capability-1234567890");
  await adapter.ensureInitialized();
  const current = adapter.ensureSession();
  await nextTick();
  const opening = adapter.threadStarting;
  release();
  await rejected;
  assert.equal(adapter.threadId, null);
  assert.equal(adapter.threadStarting, opening);
  assert.equal(adapter.status().state, "starting");
  assert.deepEqual(await Promise.all([current, adapter.ensureSession()]), ["new-thread", "new-thread"]);
  assert.equal(sessions, 2);
  assert.equal(events.filter(event => event.method === "agent/ready").length, 1);
  assert.equal(adapter.lastError, null);
});

for (const restoring of [false, true])
test(`failed session ${restoring ? "restore" : "creation"} retires its child and can be retried`, async t => {
  const adapter = create(0, { startupMs: 2000, requestMs: 500 });
  adapter.env.FIXTURE_SESSION_FAILURE = "1";
  adapter.prepareAdvisorySession("task-capability-advisory-retry-123456789");
  const lease = { runId: "session-retry", leaseId: "lease", epoch: 1 };
  await adapter.bindExecution({ lease });
  t.after(() => adapter.finishTaskOperation());
  await adapter.ensureInitialized();
  const child = adapter.process;
  const open = () => restoring ? adapter.resumeThread("delayed-thread") : adapter.ensureSession();
  await assert.rejects(open(), /required MCP servers failed/);
  assert.equal(adapter.status().state, "error");
  assert.equal(adapter.process, null);
  assert.equal(adapter.initialized, false);
  assert.equal(adapter.threadId, null);
  assert.equal(adapter.threadStarting, null);
  assert.equal(adapter.pending.size, 0);
  assert.equal(child.exitCode !== null || child.signalCode !== null, true);
  assert.deepEqual(adapter.executionLease, lease);
  assert.equal(adapter.env.NOVEL_CODEX_TASK_CAPABILITY, "task-capability-advisory-retry-123456789");
  delete adapter.env.FIXTURE_SESSION_FAILURE;
  await open();
  assert.equal(adapter.status().state, "ready");
  assert.equal(adapter.lastError, null);
});

test("stopping during session readiness does not poison an immediate restart", async t => {
  const adapter = create(0, { startupMs: 2000, requestMs: 500 });
  adapter.env.FIXTURE_REQUEST_DELAY = "1000";
  t.after(() => adapter.finishTaskOperation());
  await adapter.ensureInitialized();
  const old = adapter.ensureSession();
  const rejected = assert.rejects(old, /任务会话已结束/);
  await nextTick();
  assert.equal(adapter.pending.size, 1);
  const oldChild = adapter.process;
  const finishing = adapter.finishTaskOperation();
  adapter.env.FIXTURE_REQUEST_DELAY = "150";
  adapter.prepareAdvisorySession("task-capability-advisory-restart-123456789");
  const restart = adapter.ensureSession();
  await Promise.all([rejected, finishing]);
  assert.equal(await restart, "delayed-thread");
  assert.notEqual(adapter.process, oldChild);
  assert.equal(oldChild.exitCode !== null || oldChild.signalCode !== null, true);
  assert.equal(adapter.env.NOVEL_CODEX_TASK_CAPABILITY, "task-capability-advisory-restart-123456789");
  assert.equal(adapter.status().state, "ready");
  assert.equal(adapter.pending.size, 0);
});
