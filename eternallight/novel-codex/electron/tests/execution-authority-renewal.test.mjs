import assert from "node:assert/strict";
import { test } from "node:test";
import { maintainExecutionAuthority } from "../execution-authority-renewal.mjs";
import { renewTaskAuthority } from "../task-capability-renewal.mjs";

function clock() {
  let time = 0;
  let next = 0;
  const scheduled = new Map();
  return { now: () => time,
    schedule: (callback) => { const id = ++next; scheduled.set(id, callback); return id; },
    unschedule: id => scheduled.delete(id),
    async advance(to) {
      time = to;
      const callbacks = [...scheduled.values()]; scheduled.clear();
      callbacks.forEach(callback => callback());
      await new Promise(resolve => setImmediate(resolve));
    }, count: () => scheduled.size };
}

test("keeps a long execution alive past both original expiries without creating new task authority", async () => {
  const timer = clock();
  const capability = { hash: "private-hash", expiresAt: 1800 };
  const calls = [];
  const renewal = maintainExecutionAuthority({ ...timer, capability,
    sessionPlan: { id: "plan", expiresAt: new Date(1800_000).toISOString() }, isCurrent: () => true,
    renewTask: async expected => { calls.push(["task", expected]); return { expiresAt: timer.now() / 1000 + 1800 }; },
    renewPlan: async previous => { calls.push(["plan", previous.id]); return { id: `${previous.id}-renewed`, expiresAt: new Date(timer.now() + 1800_000).toISOString() }; },
  });
  await timer.advance(1650_000);
  await timer.advance(3300_000);
  assert.deepEqual(calls, [["task", 1800], ["plan", "plan"], ["task", 3450], ["plan", "plan-renewed"]]);
  assert.deepEqual(capability, { hash: "private-hash", expiresAt: 5100 });
  await renewal.stop();
  assert.equal(timer.count(), 0);
  await timer.advance(4800_000);
  assert.equal(calls.length, 4);
});

test("invalid or expired authority stops instead of silently increasing its deadline", async () => {
  for (const problem of ["expired", "task-invalid", "evidence-invalid"]) {
    const timer = clock();
    let requests = 0;
    const renewal = maintainExecutionAuthority({ ...timer, capability: { expiresAt: 1800 },
      sessionPlan: { expiresAt: new Date(1800_000).toISOString() }, isCurrent: () => true,
      renewTask: async () => { requests++; if (problem === "task-invalid") throw new Error("revoked"); return { expiresAt: 3450 }; },
      renewPlan: async previous => { requests++; return previous; },
    });
    const failure = assert.rejects(renewal.failure, { code: "EXECUTION_AUTHORITY_INVALID" });
    await timer.advance(problem === "expired" ? 1800_000 : 1650_000);
    await failure;
    await renewal.stop();
    assert.equal(requests, problem === "expired" ? 0 : problem === "task-invalid" ? 1 : 2);
    assert.equal(timer.count(), 0);
  }
});

test("cancelled owner and stopped in-flight renewal cannot schedule or apply a later receipt", async () => {
  const timer = clock();
  const capability = { expiresAt: 1800 };
  let resolveTask;
  let current = true;
  let plans = 0;
  const renewal = maintainExecutionAuthority({ ...timer, capability,
    sessionPlan: { expiresAt: new Date(1800_000).toISOString() }, isCurrent: () => current,
    renewTask: () => new Promise(resolve => { resolveTask = resolve; }),
    renewPlan: async () => { plans++; },
  });
  await timer.advance(1650_000);
  current = false;
  const stopping = renewal.stop();
  resolveTask({ expiresAt: 3450 });
  await stopping;
  assert.deepEqual(capability, { expiresAt: 1800 });
  assert.equal(plans, 0);
  assert.equal(timer.count(), 0);
});

test("model completion consumes no new task lease while its final reply finishes", async () => {
  const timer = clock();
  let calls = 0;
  const renewal = maintainExecutionAuthority({ ...timer, capability: { expiresAt: 1800 },
    sessionPlan: { expiresAt: new Date(86_400_000).toISOString() }, isCurrent: () => true,
    renewTask: async () => { calls++; return null; }, renewPlan: async () => { throw new Error("unexpected"); },
  });
  await timer.advance(1650_000);
  await timer.advance(3500_000);
  await renewal.stop();
  assert.equal(calls, 1);
});

test("task issuer receives only frozen task bindings, and completed vs cancelled stay distinct", async () => {
  const task = { id: "task", projectId: "project", threadId: "thread", operation: "advisory", model: "model", target: { type: "project" } };
  const execution = { task, capability: { hash: "capability-hash" } };
  const requests = [];
  assert.deepEqual(await renewTaskAuthority({ execution, expectedExpiresAt: 100,
    runtimeRequest: async (path, options) => { requests.push([path, JSON.parse(options.body)]); return { expiresAt: 1900 }; },
  }), { expiresAt: 1900 });
  assert.deepEqual(requests, [["/api/projects/project/tasks/task/capability/renew", {
    threadId: "thread", capabilityHash: "capability-hash", expectedExpiresAt: 100,
    operation: "advisory", target: { type: "project" }, model: "model",
  }]]);
  for (const status of ["completed", "cancelled"]) {
    const promise = renewTaskAuthority({ execution, expectedExpiresAt: 100,
      runtimeRequest: async (_path, options) => { if (options) throw new Error("revoked"); return { ...task, status }; },
    });
    if (status === "completed") assert.equal(await promise, null);
    else await assert.rejects(promise, /revoked/);
  }
});
