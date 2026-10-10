import test from "node:test";
import assert from "node:assert/strict";
import { ExecutionDomains } from "../execution-domains.mjs";
const deferred = () => { let resolve; const promise = new Promise(r => resolve = r); return { promise, resolve }; };
test("parallel conversations retain their own adapter, callback context and cancellation domain", async () => {
  const a = deferred(), b = deferred(), prepared = [], disposed = [], callbacks = [];
  const domains = new ExecutionDomains({ base: {}, prepare: async domain => { domain.adapter = { id: domain.id }; prepared.push(domain); }, dispose: async domain => { disposed.push(domain.id); } });
  const first = domains.run({ projectId: "p", threadId: "a" }, async () => {
    const owner = domains.current(); callbacks.push(domains.bind(() => domains.current())); await a.promise; assert.equal(domains.current(), owner);
  });
  const second = domains.run({ projectId: "p", threadId: "b" }, async () => {
    const owner = domains.current(); callbacks.push(domains.bind(() => domains.current())); await b.promise; assert.equal(domains.current(), owner);
  });
  await Promise.resolve();
  assert.equal(domains.active().length, 2); assert.notEqual(prepared[0].adapter, prepared[1].adapter);
  assert.equal(callbacks[0](), prepared[0]); assert.equal(callbacks[1](), prepared[1]);
  await assert.rejects(domains.run({ projectId: "p", threadId: "a" }, async () => {}), /已有任务/);
  a.resolve(); await first; assert.equal(domains.active().length, 1); assert.equal(disposed.length, 1);
  assert.equal(callbacks[0](), undefined); assert.equal(callbacks[1](), prepared[1]);
  b.resolve(); await second; assert.equal(domains.active().length, 0);
});
test("preparation failure cleans only the failed domain and allows a new same-thread attempt", async () => {
  let failed = true, disposed = 0;
  const domains = new ExecutionDomains({ base: {}, prepare: async () => { if (failed) throw Error("bridge start failed"); }, dispose: async () => { disposed++; } });
  await assert.rejects(domains.run({ projectId: "p", threadId: "a" }, async () => {}), /bridge start failed/);
  failed = false; await domains.run({ projectId: "p", threadId: "a" }, async () => {});
  assert.equal(disposed, 2); assert.equal(domains.active().length, 0);
});

test("conflicting selectors never retarget an execution and fresh request IDs do not bypass a thread owner", async () => {
  const gate = deferred();
  const domains = new ExecutionDomains({base: {}, prepare: async () => {}, dispose: async () => {}});
  const run = domains.run({projectId: "p", threadId: "a", requestId: "original", workId: "work-a"}, () => gate.promise);
  try {
    assert.equal(domains.find({projectId: "p", threadId: "a", requestId: "different"}), null);
    assert.equal(domains.find({projectId: "p", threadId: "other", workId: "work-a"}), null);
    assert.equal(domains.find({projectId: "other", requestId: "original"}), null);
    assert.equal(domains.find({projectId: "p"}), null);
    assert.equal(domains.find({projectId: "p", threadId: "a", requestId: "original"})?.workId, "work-a");
    await assert.rejects(domains.run({projectId: "p", threadId: "a", requestId: "new"}, async () => {}), /已有任务/);
  } finally { gate.resolve(); await run; }
});

test('capacity includes preparation and cleanup and is reclaimed after failure', async()=>{
  const preparing=deferred(),cleaning=deferred();let prepares=0;
  const domains=new ExecutionDomains({base:{},maxActive:1,prepare:async()=>{prepares++;await preparing.promise;throw Error('prepare failed');},dispose:()=>cleaning.promise});
  const pending=domains.run({projectId:'p',threadId:'a'},async()=>{});
  const outcome=assert.rejects(pending,/prepare failed/);
  assert.equal(domains.hasCapacity(),false);
  await assert.rejects(domains.run({projectId:'p',threadId:'b'},async()=>{}),{code:'EXECUTION_CAPACITY'});
  preparing.resolve();await new Promise(resolve=>setImmediate(resolve));
  assert.equal(domains.hasCapacity(),false);assert.equal(prepares,1);
  cleaning.resolve();await outcome;assert.equal(domains.hasCapacity(),true);
});

test('shutdown while preparing never enters the provider callback after preparation finishes',async()=>{
 const gate=deferred();let called=false,resource=false,disposed=0;
 const domains=new ExecutionDomains({base:{},prepare:async()=>{await gate.promise;resource=true;},dispose:async()=>{resource=false;disposed++;}});
 const running=domains.run({projectId:'p',threadId:'a'},async()=>{called=true;});
 const rejected=assert.rejects(running,{code:'TASK_CANCELLED'});
 const closing=domains.closeAll();assert.equal(disposed,0);gate.resolve();await rejected;await closing;
 assert.equal(called,false);assert.equal(resource,false);assert.equal(disposed,1);assert.equal(domains.active().length,0);
});
