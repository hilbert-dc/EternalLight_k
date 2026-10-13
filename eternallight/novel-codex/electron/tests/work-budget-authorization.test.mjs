import test from "node:test";
import assert from "node:assert/strict";
import { ensureWorkBudgetAuthorization } from "../work-budget-authorization.mjs";

const work = { id: "work-modeling", projectId: "project", kind: "direct", version: 3, goal: "建模当前正文" };
const path = "/api/projects/project/work-tasks/work-modeling";
const inputs = { projectId: "project", work, origin: "author", databasePath: "fixture.sqlite3",
  getWindow: () => ({ isDestroyed: () => false }) };

test("automatic work with no allowance cannot confirm or run", async () => {
  const calls = [];
  await assert.rejects(ensureWorkBudgetAuthorization({ ...inputs, origin: "automatic",
    runtimeRequest: async (url) => { calls.push(url); return { authorization: null }; },
  }), /作者确认/);
  assert.deepEqual(calls, [`${path}/budget-authorization`]);
});

test("author continue authorizes the existing owner and reloads its version", async () => {
  const calls = [];
  const refreshed = { ...work, version: 4 };
  const result = await ensureWorkBudgetAuthorization({ ...inputs,
    runtimeRequest: async (url, options) => {
      calls.push([url, options]);
      if (calls.length === 1) return { authorization: null };
      if (calls.length === 2) return { revision: 7, headSnapshotId: "snapshot", sourceDigest: "source" };
      if (calls.length === 3) return {};
      return refreshed;
    },
  });
  assert.equal(result, refreshed);
  assert.deepEqual(calls.map(([url]) => url), [`${path}/budget-authorization`, "/api/projects/project/creative-workspace", `${path}/budget-authorization`, path]);
  const body = JSON.parse(calls[2][1].body);
  assert.deepEqual(body, { expectedVersion: 3, expectedProjectRevision: 7,
    expectedSourceDigest: "source", maxRequests: null, maxTokens: null, deadlineAt: null });
});

test("existing authorization is reused without replenishment", async () => {
  let calls = 0;
  assert.equal(await ensureWorkBudgetAuthorization({ ...inputs, origin: "automatic",
    runtimeRequest: async () => { calls++; return { authorization: { deadlineAt: null } }; },
  }), work);
  assert.equal(calls, 1);
});

test("corrupted authority and expired authority cannot write or resume", async () => {
  for (const response of [{}, { authorization: {} }, { authorization: { deadlineAt: 1 } }]) {
    let calls = 0;
    await assert.rejects(ensureWorkBudgetAuthorization({ ...inputs, runtimeRequest: async () => { calls++; return response; } }));
    assert.equal(calls, 1);
  }
});
