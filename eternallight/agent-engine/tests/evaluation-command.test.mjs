import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

import { parseBehaviorEvaluationInput, runCommandBehaviorEvaluation } from "../src/index.mjs";

const fixture = fileURLToPath(new URL("./fixture-behavior-executor.mjs", import.meta.url));

test("command behavior evaluation executes a frozen task set without granting authorization", async () => {
  const report = await runCommandBehaviorEvaluation({
    taskSet: {
      taskSetId: "portable-model-quality",
      taskSetVersion: 1,
      tasks: [{ taskId: "answer", category: "explicit-delegation", prompt: "Answer", requiredFacts: ["forty-two"], forbiddenClaims: ["invented"], metadata: { answer: "forty-two" } }],
    },
    repetitions: 2,
    executor: { command: process.execPath, args: [fixture], timeoutMs: 5_000 },
    policy: { minimumSamples: 2, minimumLowerBound: 0.3 },
    runtimeDependencies: { engineBuild: { digest: "b".repeat(64), ref: "fixture-build" } },
  }, { generatedAt: new Date("2026-09-20T00:00:00Z") });
  assert.equal(report.mode, "execute");
  assert.equal(report.authorizationEffect, "none");
  assert.equal(report.samples.length, 2);
  assert.equal(report.evaluation.status, "passed");
  assert.match(report.taskSet.digest, /^[a-f0-9]{64}$/);
  assert.match(report.executor.digest, /^[a-f0-9]{64}$/);
  assert.deepEqual(report.samples.map((sample) => sample.runRef), ["answer:0", "answer:1"]);
  assert.throws(() => parseBehaviorEvaluationInput({ mode: "samples", samples: [{ outcome: "passed" }], policy: { minimumSamples: 1, minimumLowerBound: 0.2 }, runtimeDependencies: { credential: "secret" } }), /dependency digest/);
});

test("sample import mode is explicitly distinguished from execution", async () => {
  const report = await parseBehaviorEvaluationInput({
    mode: "samples",
    samples: [{ outcome: "passed", category: "imported", blockingFailures: [] }],
    policy: { minimumSamples: 1, minimumLowerBound: 0.2 },
  }, { generatedAt: new Date("2026-09-20T00:00:00Z") });
  assert.equal(report.mode, "samples");
  assert.equal(report.executor, null);
  assert.equal(report.authorizationEffect, "none");
});
