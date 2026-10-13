import assert from "node:assert/strict";
import test from "node:test";
import { WorkExecutionOwners, WorkExecutionEventConsumer } from "../work-execution-events.mjs";

function fixture(t, { admission = "claimed", status = "running", ready = true, resume = null, canResume = () => true, projectId = "project-one", workId = "work-one", threadId = "root-thread", thread = threadId } = {}) {
  const hostSessionId = "host-one";
  const path = `/api/projects/${projectId}`, calls = [], wakes = [], errors = [], invalidations = [];
  const work = { id: workId, projectId, threadId, kind: "conversation", status, version: 7,
    steps: [{ id: "step-source", operation: "advisory", status: "completed", currentAttemptId: "attempt-source",
      attempts: [{ id: "attempt-source", threadId: "root-thread", status: "completed" }] }] };
  const event = { eventId: "event-one", workTaskId: workId, sourceStepId: "step-source", sourceAttemptId: "attempt-source",
    rootThreadId: work.threadId, kind: "attemptSettled", workVersion: 7, state: "pending", claimId: null,
    hostSessionId: null, leaseExpiresAt: null, createdAt: Math.floor(Date.now() / 1000) };
  const lease = { leaseId: "lease-one", hostSessionId, workTaskId: workId, rootThreadId: work.threadId,
    expiresAt: Math.floor(Date.now() / 1000) + 30 };
  const request = async (url, options = {}) => {
    const body = options.body ? JSON.parse(options.body) : null;
    calls.push({ url, body });
    if (url === "/api/projects") return { projects: [{ id: projectId }] };
    if (url === `${path}/work-tasks/${workId}`) return structuredClone(work);
    if (url.startsWith(`${path}/work-execution-events?`)) return { projectId, items: [structuredClone(event)], nextCursor: null };
    if (url === `${path}/creative-workspace`) return { revision: 4, sourceDigest: "digest-current" };
    if (url.endsWith("/claim")) {
      assert.deepEqual(body, { hostSessionId, expectedWorkVersion: work.version, expectedProjectRevision: 4, expectedSourceDigest: "digest-current" });
      if (admission === "claimed") Object.assign(event, { state: "claimed", claimId: "claim-one", hostSessionId, leaseExpiresAt: lease.expiresAt });
      if (admission === "settled") event.state = "consumed";
      return { event: structuredClone(event), lease: admission === "claimed" ? { ...lease } : null,
        work: { id: work.id, projectId, threadId: work.threadId, kind: work.kind, status: work.status, version: work.version },
        admission, blockedReason: admission === "blocked" ? "unsettledUsage" : null };
    }
    if (url.endsWith("/execution-owner")) {
      assert.deepEqual(body, { hostSessionId, expectedWorkVersion: work.version }); return { ...lease };
    }
    if (url.endsWith("/execution-owner/renew")) {
      assert.deepEqual(body, { hostSessionId, leaseId: lease.leaseId }); return { ...lease };
    }
    if (url.endsWith("/ack")) {
      assert.deepEqual(body, { hostSessionId, claimId: "claim-one", leaseId: lease.leaseId,
        consumerStepId: work.steps.find(step => step.id === "step-next")?.currentAttemptId ? "step-next" : null });
      event.state = "consumed"; return structuredClone(event);
    }
    if (url.endsWith("/release")) {
      if (url.includes("/work-execution-events/")) {
        assert.deepEqual(body, { hostSessionId, claimId: "claim-one", leaseId: lease.leaseId }); event.state = "pending";
      } else assert.deepEqual(body, { hostSessionId, leaseId: lease.leaseId });
      return { released: true };
    }
    assert.fail(`Unexpected current endpoint: ${url}`);
  };
  const owners = new WorkExecutionOwners({ request, hostSessionId, invalidated: (...args) => invalidations.push(args) });
  const consumer = new WorkExecutionEventConsumer({ request, owners, available: async () => ready, canResume,
    readThread: async () => ({ id: thread }), resume: async (...args) => {
      wakes.push(args); if (resume) await resume(work, consumer, owners);
    }, failed: (...args) => errors.push(args) });
  t.after(async () => { await consumer.stop(); await owners.stop(); });
  return { request, owners, consumer, calls, wakes, errors, work, event, lease, invalidations };
}
const attach = work => work.steps.push({ id: "step-next", operation: "advisory", status: "running", dependsOnStepId: "step-source",
  currentAttemptId: "attempt-next", attempts: [{ id: "attempt-next", threadId: "root-thread", status: "running" }] });

test("busy or unqualified Host does not read, claim or open a model turn", async t => {
  const f = fixture(t, { ready: false }); await f.consumer.tick(); await f.consumer.settled();
  assert.deepEqual(f.calls, []); assert.deepEqual(f.wakes, []);
});
for (const status of ["paused", "cancelled"]) test(`author ${status} Work is never automatically resumed`, async t => {
  const f = fixture(t, { status }); await f.consumer.tick(); await f.consumer.settled();
  assert.equal(f.calls.some(call => call.url.endsWith("/claim")), false); assert.equal(f.wakes.length, 0);
});
for (const admission of ["blocked", "settled"]) test(`Native ${admission} event does not start another decision`, async t => {
  const f = fixture(t, { admission }); await f.consumer.tick(); await f.consumer.settled();
  assert.equal(f.wakes.length, 0); assert.equal(f.calls.some(call => call.url.endsWith("/ack")), false);
  assert.equal(f.event.state, admission === "settled" ? "consumed" : "pending");
});
test("wake uses original Work and thread and consumes only actual successor Attempt", async t => {
  const f = fixture(t, { resume: attach }); await f.consumer.tick(); await f.consumer.settled();
  assert.deepEqual(f.wakes, [["project-one", "work-one", { origin: "automatic", expectedThreadId: "root-thread",
    durableWake: { eventId: "event-one", sourceStepId: "step-source", sourceAttemptId: "attempt-source", claimId: "claim-one",
      leaseId: "lease-one", hostSessionId: "host-one" } }]]);
  assert.equal(f.calls.filter(call => call.url.endsWith("/ack")).length, 1);
  assert.equal(f.event.state, "consumed"); assert.equal(f.errors.length, 0);
  assert.equal(f.calls.some(call => /author-messages|budget-authorization|\/tasks$|\/control$/.test(call.url)), false);
});
test("ordinary blocked result is handed to Native decision only with exact durable claim identity", async t => {
  const f = fixture(t, { status: "blocked", resume: attach }); await f.consumer.tick(); await f.consumer.settled();
  assert.equal(f.wakes.length, 1); assert.equal(f.wakes[0][2].durableWake.sourceAttemptId, "attempt-source");
  assert.equal(f.calls.some(call => /\/control$|budget-authorization/.test(call.url)), false);
});
test("a genuinely blocked generation Run is refused by Native claim without a model turn", async t => {
  const f = fixture(t, { status: "blocked", admission: "blocked" }); f.work.kind = "generation"; await f.consumer.tick(); await f.consumer.settled();
  assert.equal(f.wakes.length, 0); assert.equal(f.calls.filter(call => call.url.endsWith("/claim")).length, 1);
  assert.equal(f.event.state, "pending");
});
test("a generation retryable Work failure uses only its actually claimed original lease and source", async t => {
  const f = fixture(t, { status: "blocked", resume: attach }); f.work.kind = "generation"; await f.consumer.tick(); await f.consumer.settled();
  assert.equal(f.wakes.length, 1); assert.equal(f.wakes[0][2].durableWake.sourceAttemptId, "attempt-source");
  assert.equal(f.calls.some(call => /\/control$|budget-authorization/.test(call.url)), false);
});
test("an older advisory Attempt cannot be mistaken for the actual new consumer", async t => {
  const f = fixture(t, { resume: attach });
  f.work.steps.unshift({ id: "step-old-root", operation: "advisory", status: "completed", currentAttemptId: "attempt-old-root",
    attempts: [{ id: "attempt-old-root", status: "completed", threadId: "root-thread" }] });
  await f.consumer.tick(); await f.consumer.settled();
  assert.equal(f.calls.find(call => call.url.endsWith("/ack")).body.consumerStepId, "step-next");
});
test("queued successor without an attached Attempt keeps the original event pending", async t => {
  const f = fixture(t, { resume: work => work.steps.push({ id: "step-next", operation: "advisory", status: "queued", attempts: [], currentAttemptId: null }) });
  await f.consumer.tick(); await f.consumer.settled(); assert.equal(f.event.state, "pending");
  assert.equal(f.calls.some(call => call.url.endsWith("/ack")), false);
  assert.equal(f.calls.filter(call => call.url.endsWith("/release")).length, 1);
});
test("completed Work is acked only after the original executor and Native final proof", async t => {
  const f = fixture(t, { status: "completed" }); await f.consumer.tick(); await f.consumer.settled();
  assert.equal(f.wakes.length, 1); assert.equal(f.event.state, "consumed");
  assert.equal(f.calls.find(call => call.url.endsWith("/ack")).body.consumerStepId, null);
});
test("foreign Engine thread fails before claim", async t => {
  const f = fixture(t, { thread: "foreign-thread" }); await f.consumer.tick(); await f.consumer.settled();
  assert.equal(f.wakes.length, 0); assert.equal(f.calls.some(call => call.url.endsWith("/claim")), false);
  assert.equal(f.errors.length, 1);
});
test("a failed or unknown resume releases only its exact claim and does not fabricate consumption", async t => {
  const f = fixture(t, { resume: () => { throw new Error("unknown result"); } }); await f.consumer.tick(); await f.consumer.settled();
  assert.equal(f.wakes.length, 1); assert.equal(f.event.state, "pending");
  assert.equal(f.calls.some(call => call.url.endsWith("/ack")), false); assert.equal(f.errors.length, 1);
});
test("concurrent scans share one pending wake and do not open parallel turns", async t => {
  let release, started; const beginning = new Promise(resolve => { started = resolve; });
  const waiting = new Promise(resolve => { release = resolve; });
  const f = fixture(t, { resume: async work => { started(); await waiting; attach(work); } });
  const first = f.consumer.tick(); await beginning;
  const second = f.consumer.tick(); assert.equal(f.consumer.busy, true); assert.equal(f.wakes.length, 1);
  release(); await Promise.all([first, second]); await f.consumer.settled(); assert.equal(f.wakes.length, 1); assert.equal(f.event.state, "consumed");
});
test("restart/stop after claim keeps pending evidence instead of acking an interrupted dispatch", async t => {
  const f = fixture(t, { resume: async (work, consumer) => { attach(work); await consumer.stop(); } });
  await f.consumer.tick(); await f.consumer.settled(); assert.equal(f.event.state, "pending");
  assert.equal(f.calls.some(call => call.url.endsWith("/ack")), false);
});
test("a resumed flow borrows the claim owner without acquiring or releasing a second Native lease", async t => {
  const f = fixture(t, { resume: async (work, _consumer, owners) => {
    const handle = await owners.acquire(work.projectId, work); assert.equal(handle.borrowed, true); attach(work);
  } }); await f.consumer.tick(); await f.consumer.settled();
  assert.equal(f.calls.filter(call => call.url.endsWith("/execution-owner")).length, 0);
  assert.equal(f.calls.filter(call => call.url.endsWith("/execution-owner/release")).length, 1);
  assert.equal(f.event.state, "consumed");
});
test("ordinary flow acquires, verifies and releases its exact current Native lease", async t => {
  const f = fixture(t); const handle = await f.owners.acquire("project-one", f.work);
  assert.equal(handle.borrowed, false); await f.owners.verify(handle.record); await f.owners.release(handle.record);
  assert.deepEqual(f.calls.map(call => call.url.split("/").slice(-2).join("/")), ["work-one/execution-owner", "execution-owner/renew", "execution-owner/release"]);
});
test("lease renewal failure invalidates dispatch and preserves unknown outcome semantics", async t => {
  let cancelled = 0, calls = 0;
  const work = { id: "work-one", projectId: "project-one", threadId: "root-thread", kind: "conversation", status: "running", version: 1 };
  const owners = new WorkExecutionOwners({ hostSessionId: "host-one", invalidated: () => { cancelled += 1; },
    request: async url => {
      calls += 1;
      if (url.endsWith("/renew")) throw new Error("lease changed after uncertain request");
      return { leaseId: "lease-one", hostSessionId: "host-one", workTaskId: work.id, rootThreadId: work.threadId, expiresAt: Math.floor(Date.now() / 1000) + 30 };
    } }); t.after(() => owners.stop());
  const handle = await owners.acquire(work.projectId, work); await assert.rejects(owners.verify(handle.record), { code: "WORK_EXECUTION_OWNER_INVALID" });
  await assert.rejects(owners.verify(handle.record), { code: "WORK_EXECUTION_OWNER_INVALID" });
  assert.equal(cancelled, 1); assert.equal(calls, 2);
});
test("owner acquisition rejects a foreign Native lease without sending a model request", async () => {
  const owners = new WorkExecutionOwners({ hostSessionId: "host-one", request: async () => ({ leaseId: "lease-one",
    hostSessionId: "foreign-host", workTaskId: "work-one", rootThreadId: "root-thread", expiresAt: Math.floor(Date.now() / 1000) + 30 }) });
  await assert.rejects(owners.acquire("project-one", { id: "work-one", projectId: "project-one", threadId: "root-thread",
    kind: "conversation", status: "running", version: 1 }), { code: "WORK_EXECUTION_OWNER_INVALID" }); await owners.stop();
});
test("unbound generation root cannot receive an empty-identity execution lease", async () => {
  let calls = 0;
  const owners = new WorkExecutionOwners({ request: async () => { calls += 1; } });
  await assert.rejects(owners.acquire("project-one", { id: "work-generation-one", projectId: "project-one", threadId: "",
    kind: "generation", status: "running", version: 0 }), { code: "WORK_EXECUTION_OWNER_INVALID" });
  assert.equal(calls, 0); await owners.stop();
});
test("every directory pass reads at most four projects and twenty events per project with fair rotation", async t => {
  const scans = [];
  const owners = new WorkExecutionOwners({ request: async () => assert.fail("No lease without an event") });
  const consumer = new WorkExecutionEventConsumer({ owners, available: async () => true,
    request: async url => {
      if (url === "/api/projects") return { projects: Array.from({ length: 7 }, (_, id) => ({ id: `project-${id}` })) };
      const value = new URL(url, "http://localhost"); assert.equal(value.searchParams.get("limit"), "20");
      const projectId = value.pathname.split("/")[3]; scans.push(projectId); return { projectId, items: [], nextCursor: null };
    }, readThread: () => assert.fail(), resume: () => assert.fail() });
  t.after(async () => { await consumer.stop(); await owners.stop(); });
  await consumer.tick(); assert.equal(scans.length, 4);
  await consumer.tick(); assert.equal(scans.length, 8); assert.equal(new Set(scans).size, 7);
});

for (const changedAfterClaim of [false,true]) test(`related edits prevent background dispatch, including after claim: ${changedAfterClaim}`,async t=>{
 let checks=0;
 const f=fixture(t,{canResume:(project,work)=>{
  assert.equal(project,'project-one');assert.equal(work,'work-one');
  checks++;return changedAfterClaim && checks===1;
 }});
 await f.consumer.tick(); await f.consumer.settled();
 assert.equal(f.wakes.length,0);assert.equal(f.event.state,'pending');assert.equal(f.errors.length,0);
 assert.equal(f.calls.some(c=>c.url.endsWith('/claim')),changedAfterClaim);
 assert.equal(f.calls.some(c=>c.url.endsWith('/ack')),false);
});

test('a constantly ready queue rotates between projects instead of returning to the first',async t=>{
 const dispatched=[];
 const consumer=new WorkExecutionEventConsumer({owners:{},available:()=>true,request:async()=>({projects:[{id:'a'},{id:'b'}]}),
  drainMessages:async project=>{dispatched.push(project);return true;}});
 t.after(()=>consumer.stop());for(let i=0;i<4;i++)await consumer.tick();
 assert.deepEqual(dispatched,['a','b','a','b']);
});

test('a pending Work resume does not hold the scanner or another conversation queue', async t => {
  let release, started;
  const held = new Promise(resolve => { release = resolve; });
  const beginning = new Promise(resolve => { started = resolve; });
  const f = fixture(t, { resume: async work => { started(); await held; attach(work); } });
  try {
    const scanning = f.consumer.tick(); await beginning;
    assert.equal(await Promise.race([scanning.then(() => true), new Promise(resolve => setTimeout(() => resolve(false), 50))]), true,
      'tick must finish scanning while the original Work is still executing');
    let queueScans = 0;
    f.consumer.drainMessages = async () => { queueScans++; return false; };
    await f.consumer.tick();
    assert.equal(queueScans, 1); assert.equal(f.wakes.length, 1);
    assert.equal(f.calls.filter(call => call.url.endsWith('/claim')).length, 1);
  } finally { release(); if (f.consumer.settled) await f.consumer.settled(); }
});

test('parallel wakes reserve capacity through completion and reject duplicate Work and thread events', async t => {
  let release;
  const held = new Promise(resolve => { release = resolve; });
  const fixtures = Array.from({ length: 3 }, (_, index) => fixture(t, {
    projectId: `project-${index}`, workId: `work-${index}`, threadId: `thread-${index}`,
    resume: async work => { await held; attach(work); },
  }));
  const request = async (url, options) => {
    if (url === '/api/projects') return { projects: fixtures.map(f => ({ id: f.work.projectId })) };
    const f = fixtures.find(f => url.startsWith(`/api/projects/${f.work.projectId}/`));
    assert.ok(f, url);
    const result = await f.request(url, options);
    if (url.includes('/work-execution-events?')) result.items.push(
      { ...result.items[0], eventId: 'duplicate-work' },
      { ...result.items[0], eventId: 'duplicate-thread', workTaskId: 'another-work' });
    return result;
  };
  const consumer = fixtures[0].consumer;
  consumer.request = request; consumer.owners.request = request; consumer.maxActive = 2;
  consumer.readThread = async (_project, thread) => ({ id: thread });
  consumer.resume = async (...args) => {
    const f = fixtures.find(f => f.work.id === args[1]);
    f.wakes.push(args); await held; attach(f.work);
  };
  try {
    await consumer.tick();
    for (let i = 0; i < 30 && fixtures[1].wakes.length === 0; i++) await new Promise(resolve => setTimeout(resolve, 1));
    assert.deepEqual(fixtures.map(f => f.wakes.length), [1, 1, 0]);
    await consumer.tick();
    assert.deepEqual(fixtures.map(f => f.wakes.length), [1, 1, 0]);
    release(); await consumer.settled();
    await consumer.tick(); await consumer.settled();
    assert.deepEqual(fixtures.map(f => f.wakes.length), [1, 1, 1]);
    assert.equal(fixtures.every(f => f.event.state === 'consumed'), true);
    assert.deepEqual(fixtures[0].errors, []);
  } finally { release(); await consumer.settled(); }
});

test('stop while a Native claim is pending releases it without dispatch or consumption', async t => {
  let release, claimed;
  const held = new Promise(resolve => { release = resolve; });
  const beginning = new Promise(resolve => { claimed = resolve; });
  const f = fixture(t), request = f.consumer.request;
  f.consumer.request = async (...args) => {
    const result = await request(...args);
    if (args[0].endsWith('/claim')) { claimed(); await held; }
    return result;
  };
  try {
    await f.consumer.tick(); await beginning; await f.consumer.stop();
    release(); await f.consumer.settled();
    assert.equal(f.wakes.length, 0); assert.equal(f.event.state, 'pending');
    assert.equal(f.calls.some(call => call.url.endsWith('/ack')), false);
    assert.equal(f.calls.filter(call => call.url.endsWith('/release')).length, 1);
    assert.deepEqual(f.errors, []);
  } finally { release(); await f.consumer.settled(); }
});

test('capacity exhaustion during a project page rotates the next scan to another project', async t => {
  let ready = true;
  const f = fixture(t, { canResume: () => { ready = false; return false; } });
  const scans = [], request = f.consumer.request;
  f.consumer.available = () => ready;
  f.consumer.request = async (url, options) => {
    if (url === '/api/projects') return { projects: [{ id: 'project-one' }, { id: 'project-two' }] };
    if (url.startsWith('/api/projects/project-two/')) {
      scans.push('project-two'); return { projectId: 'project-two', items: [], nextCursor: null };
    }
    const result = await request(url, options);
    if (url.includes('/work-execution-events?')) {
      scans.push('project-one'); result.items.push({ ...f.event, eventId: 'second-event' });
    }
    return result;
  };
  await f.consumer.tick(); await f.consumer.settled();
  assert.deepEqual(scans, ['project-one']); ready = true;
  await f.consumer.tick(); await f.consumer.settled();
  assert.equal(scans[1], 'project-two'); assert.equal(f.wakes.length, 0);
});

test('project scan failure is isolated, backs off, and clears only after successful reread', async t => {
  const f = fixture(t, { resume: attach });
  let now = 0, failing = true, badReads = 0;
  f.consumer.now = () => now;
  const request = f.consumer.request;
  f.consumer.request = async (path, options) => {
    if (path === '/api/projects') return { projects: [{ id: 'unreadable-project' }, { id: 'project-one' }] };
    if (path.startsWith('/api/projects/unreadable-project/work-execution-events?')) {
      badReads++;
      if (failing) throw Object.assign(new Error('scope unavailable'), { code: 'novel_scope_required' });
      return { projectId: 'unreadable-project', items: [], nextCursor: null };
    }
    return request(path, options);
  };
  await f.consumer.tick(); await f.consumer.settled();
  assert.equal(f.wakes.length, 1, 'healthy project continues through real consumer claim and resume');
  assert.equal(f.event.state, 'consumed');
  assert.deepEqual(f.consumer.health().map(value => [value.projectId, value.phase, value.code]),
    [['unreadable-project', 'scan', 'novel_scope_required']]);
  assert.equal(f.errors[0][0], 'unreadable-project');
  await f.consumer.tick(); assert.equal(badReads, 1, 'project backoff is enforced');
  now = f.consumer.health()[0].nextCheckAt; failing = false;
  await f.consumer.tick(); await f.consumer.settled();
  assert.equal(badReads, 2); assert.deepEqual(f.consumer.health(), []);
  assert.equal(f.wakes.length, 1, 'consumed work is not duplicated');
});

test('directory failure remains global and prevents scans until its scheduled check', async t => {
  const f = fixture(t);
  let reads = 0;
  f.consumer.now = () => 0;
  f.consumer.request = async () => { reads++; throw Object.assign(new Error('directory unavailable'), { code: 'directory_unavailable' }); };
  await f.consumer.tick(); await f.consumer.tick();
  assert.equal(reads, 1);
  assert.equal(f.consumer.health()[0].projectId, null);
  assert.equal(f.consumer.health()[0].code, 'directory_unavailable');
  assert.equal(f.wakes.length, 0);
});

test('Native condition wait is observable and does not become a model retry or failure', async t => {
  const f = fixture(t,{admission:'blocked'});
  let now=1000;f.consumer.now=()=>now;
  await f.consumer.tick();await f.consumer.settled();
  const state=f.consumer.health()[0];
  assert.equal(state.reason,'unsettledUsage');assert.equal(state.category,'condition');assert.equal(state.failures,0);
  assert.equal(f.errors.length,0);assert.equal(f.wakes.length,0);
  const claims=()=>f.calls.filter(c=>c.url.endsWith('/claim')).length;
  await f.consumer.tick();await f.consumer.settled();assert.equal(claims(),1);
  // Actual Work revision changes trigger rechecking Native; do not grant execution.
  f.event.workVersion++;f.work.version++;
  await f.consumer.tick();await f.consumer.settled();assert.equal(claims(),2);assert.equal(f.wakes.length,0);
  now=f.consumer.health()[0].nextCheckAt;
  await f.consumer.tick();await f.consumer.settled();assert.equal(claims(),3);
});
test('a returned executor without an actual successor remains visibly pending',async t=>{
  const f=fixture(t);
  await f.consumer.tick();await f.consumer.settled();
  assert.equal(f.consumer.health()[0].reason,'continuationPending');
  assert.equal(f.event.state,'pending');assert.equal(f.errors.length,0);
  assert.equal(f.calls.some(c=>c.url.endsWith('/ack')),false);
});

test('authoritative completed scan clears a notice consumed by another flow without resuming',async t=>{
  const f=fixture(t,{admission:'blocked'});await f.consumer.tick();await f.consumer.settled();
  assert.equal(f.consumer.health().length,1);f.event.state='consumed';
  await f.consumer.tick();await f.consumer.settled();
  assert.deepEqual(f.consumer.health(),[]);assert.equal(f.wakes.length,0);
});
