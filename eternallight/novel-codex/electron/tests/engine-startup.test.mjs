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
