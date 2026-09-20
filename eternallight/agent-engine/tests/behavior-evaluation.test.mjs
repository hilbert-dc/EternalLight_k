import assert from "node:assert/strict";
import { test } from "node:test";

import { defineBehaviorTaskSet, defineQualificationSuiteResult, evaluateAutonomousBehavior, runBehaviorTaskSet, wilsonLowerBound } from "../src/index.mjs";

test("autonomous behavior qualification uses a fixed sample floor and confidence bound", () => {
  const thirtyPerfect = evaluateAutonomousBehavior(Array.from({ length: 30 }, () => ({ outcome: "passed" })));
  assert.equal(thirtyPerfect.status, "failed");
  assert.ok(thirtyPerfect.metrics.wilsonLowerBound95 < 0.9);

  const thirtyFivePerfect = evaluateAutonomousBehavior(Array.from({ length: 35 }, () => ({ outcome: "passed" })));
  assert.equal(thirtyFivePerfect.status, "passed");
  assert.ok(thirtyFivePerfect.metrics.wilsonLowerBound95 >= 0.9);

  const blocked = evaluateAutonomousBehavior(Array.from({ length: 35 }, (_, index) => index === 0
    ? { outcome: "failed", category: "identity-rejected", blockingFailures: ["identity-rejected"] }
    : { outcome: "passed" }));
  assert.equal(blocked.status, "failed");
  assert.deepEqual(blocked.blockingFailures, ["identity-rejected"]);
  assert.equal(blocked.authorizationEffect, "none");
  assert.match(blocked.evaluationId, /^[a-f0-9]{64}$/);
});

test("qualification suite results contain deterministic execution evidence only", () => {
  assert.deepEqual(defineQualificationSuiteResult({ kind: "protocol", suiteId: "wire-v1", suiteVersion: 1, status: "passed" }), {
    kind: "protocol",
    suiteId: "wire-v1",
    suiteVersion: 1,
    status: "passed",
    sampleCount: 1,
    successes: 1,
    blockingFailures: [],
    metrics: {},
  });
  assert.throws(() => defineQualificationSuiteResult({ kind: "mixed", suiteId: "bad", suiteVersion: 1, status: "passed" }), /unknown qualification suite kind/);
  assert.equal(wilsonLowerBound(35, 35) >= 0.9, true);
});

test("behavior task runner executes a frozen open task set without granting capabilities", async () => {
  const taskSet = defineBehaviorTaskSet({
    taskSetId: "open-delegation-v1",
    taskSetVersion: 1,
    tasks: [
      { taskId: "synthesize", category: "synthesis", prompt: "Synthesize two independent findings.", requiredFacts: ["alpha", "beta"], forbiddenClaims: ["invented"] },
      { taskId: "direct", category: "direct", prompt: "Answer directly when delegation is unnecessary.", requiredFacts: ["direct"] },
    ],
  });
  const observed = [];
  const run = await runBehaviorTaskSet({
    taskSet,
    repetitions: 2,
    execute: async (task, context) => {
      observed.push(`${context.repetition}:${task.taskId}`);
      return { status: "completed", output: task.taskId === "synthesize" ? "alpha beta" : "direct", runRef: `run-${observed.length}`, executionFacts: { toolCalls: [], lineage: [] } };
    },
  });
  assert.deepEqual(observed, ["0:synthesize", "0:direct", "1:synthesize", "1:direct"]);
  assert.equal(run.authorizationEffect, "none");
  assert.equal(run.samples.every((sample) => sample.outcome === "passed"), true);
  assert.throws(() => taskSet.tasks.push({}), TypeError);
});

test("behavior task runner records executor and factual failures without retrying", async () => {
  let calls = 0;
  const run = await runBehaviorTaskSet({
    taskSet: { taskSetId: "failure-v1", taskSetVersion: 1, tasks: [{ taskId: "one", prompt: "Return receipt.", requiredFacts: ["receipt"] }, { taskId: "two", prompt: "Fail safely." }] },
    execute: async (task) => {
      calls += 1;
      if (task.taskId === "two") throw new Error("offline");
      return { status: "completed", output: "missing", executionFacts: { toolCalls: [], lineage: [] } };
    },
  });
  assert.equal(calls, 2);
  assert.deepEqual(run.samples.map(({ taskId, outcome, blockingFailures }) => ({ taskId, outcome, blockingFailures })), [
    { taskId: "one", outcome: "failed", blockingFailures: ["missing-fact:receipt"] },
    { taskId: "two", outcome: "failed", blockingFailures: ["executor-error"] },
  ]);
});

test("behavior scoring requires execution facts and enforces a total token budget", async () => {
  let calls = 0;
  const run = await runBehaviorTaskSet({
    taskSet: {
      taskSetId: "fact-scoring-v1",
      taskSetVersion: 1,
      tasks: [{ taskId: "delegate", prompt: "Delegate.", executionRequirements: { delegation: "required", requiredTools: ["spawn_agent"] } }],
    },
    repetitions: 3,
    budget: { maxExecutions: 3, maxTotalTokens: 5, maxDurationMs: 10_000 },
    execute: async () => {
      calls += 1;
      return {
        status: "completed",
        output: "I delegated the task.",
        usage: { totalTokens: 5 },
        executionFacts: { toolCalls: [{ name: "spawn_agent", status: "completed", callId: "call-1" }], lineage: [{ threadId: "child", parentThreadId: "root" }] },
      };
    },
  });
  assert.equal(calls, 1);
  assert.equal(run.samples[0].outcome, "passed");
  assert.equal(run.budget.stoppedReason, "token-budget-exhausted");

  const fabricated = await runBehaviorTaskSet({
    taskSet: { taskSetId: "no-text-inference", taskSetVersion: 1, tasks: [{ taskId: "delegate", prompt: "Delegate.", executionRequirements: { delegation: "required" } }] },
    execute: async () => ({ status: "completed", output: "I called a child Agent.", executionFacts: { toolCalls: [], lineage: [] } }),
  });
  assert.deepEqual(fabricated.samples[0].blockingFailures, ["delegation-required"]);
});
