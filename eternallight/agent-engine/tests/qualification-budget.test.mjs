import test from "node:test";
import assert from "node:assert/strict";
import { nativeBinding } from "../src/native.mjs";
import { TaskBudget } from "../src/index.mjs";

test("qualification uses bounded requests and time without a second model-capacity ceiling", () => {
  const run = new (nativeBinding().NativeQualificationRun)(JSON.stringify({
    scenario: "core", command: ["unused"], baseInstructions: "Synthetic qualification", timeoutMs: 60000,
  }));
  const [start] = JSON.parse(run.apply("start", "{}"));
  assert.equal(start.method, "bridgeStart");
  const [create] = JSON.parse(run.apply("callback", JSON.stringify({id:start.id,result:{baseUrl:"http://127.0.0.1",token:"fixture"}})));
  assert.equal(create.method, "budgetCreate");
  assert.deepEqual(create.params, {maxRequests:16,maxDurationMs:60000});
  const budget = new TaskBudget(create.params);
  assert.equal(budget.snapshot().maxTokens, null);
  assert.equal(budget.snapshot().maxRequests, 16);
});
