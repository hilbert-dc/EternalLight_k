import assert from "node:assert/strict";
import test from "node:test";
import { ExecutionRequests } from "../execution-requests.mjs";

const deferred = () => { let resolve; const promise = new Promise(r => { resolve = r; }); return { promise, resolve }; };

test("request context survives awaits and stop never addresses another sender/project/request", async () => {
  const requests = new ExecutionRequests(), a = { sender: {} }, b = { sender: {} }, hold = deferred();
  const start = requests.wrap("agent:send-prompt", async (_event, project) => {
    const original = requests.current();
    await hold.promise;
    assert.equal(requests.current(), original);
    assert.equal(original.projectId, project);
    return original.stopRequested;
  });
  const first = start(a, { requestId: "first", args: ["project-a"] });
  const second = start(b, { requestId: "second", args: ["project-b"] });
  const target = { requestId: "first", projectId: "project-a" };
  let interrupts = 0;
  const stop = () => { interrupts++; return { interrupted: true }; };
  await assert.rejects(requests.stop(b, target, stop), /原执行/);
  await assert.rejects(requests.stop(a, { ...target, projectId: "project-b" }, stop), /原执行/);
  assert.deepEqual(await requests.stop(a, { ...target, requestId: "missing" }, stop), { interrupted: false });
  await Promise.all([requests.stop(a, target, stop), requests.stop(a, target, stop)]);
  assert.equal(interrupts, 1);
  hold.resolve();
  assert.deepEqual(await Promise.all([first, second]), [true, false]);
  assert.deepEqual(await requests.stop(a, target, stop), { interrupted: false });
});

test("startup stop fences late admission, rejects missing identity and preserves actual stop errors", async () => {
  const requests = new ExecutionRequests(), event = { sender: {} }, gate = deferred();
  const start = requests.wrap("agent:generate-work", async () => { await gate.promise; requests.assertActive(); assert.fail("stopped request admitted"); });
  await assert.rejects(start(event, { args: ["project"] }), /身份/);
  const target = { requestId: "request", projectId: "project" };
  const pending = start(event, { requestId: target.requestId, args: [target.projectId] });
  const rejected = assert.rejects(pending, error => error.code === "TASK_CANCELLED");
  await assert.rejects(start(event, { requestId: target.requestId, args: [target.projectId] }), /已提交/);
  await assert.rejects(requests.stop(event, undefined, () => {}), /指定原执行/);
  await assert.rejects(requests.stop(event, target, () => { throw Error("Native unavailable"); }), /Native unavailable/);
  assert.deepEqual(await requests.stop(event, target, () => ({ interrupted: true })), { interrupted: true });
  gate.resolve(); await rejected;
});


test("background work does not inherit the originating stopped request", async () => {
  const requests = new ExecutionRequests(), event = { sender: {} }, hold = deferred();
  const run = requests.wrap("agent:send-prompt", async () => {
    await hold.promise;
    assert.throws(() => requests.assertActive(), error => error.code === "TASK_CANCELLED");
    await requests.detached(async () => {
      await Promise.resolve();
      assert.equal(requests.assertActive(), null);
    });
  });
  const pending = run(event, { requestId: "author", args: ["project"] });
  await requests.stop(event, { requestId: "author", projectId: "project" }, () => ({ interrupted: true }));
  hold.resolve(); await pending;
});
