import { nativeBinding, nativeCall } from "../native.mjs";

import { digest } from "../contracts/capabilities.mjs";
import { runBehaviorTaskSet } from "../capabilities/behavior-evaluation.mjs";

const EXECUTOR_ENVIRONMENT_KEYS = ["PATH", "PATHEXT", "SystemRoot", "WINDIR", "ComSpec", "TEMP", "TMP", "TMPDIR", "LANG", "LC_ALL"];

function isolatedEnvironment(extra) {
  return Object.fromEntries([
    ...EXECUTOR_ENVIRONMENT_KEYS.filter((key) => typeof process.env[key] === "string").map((key) => [key, process.env[key]]),
    ...Object.entries(extra),
  ]);
}

export function defineBehaviorEvaluationReport({ generatedAt = new Date(), ...input }) {
  return Object.freeze(nativeCall("evaluation.report", { ...input, generatedAt: generatedAt.toISOString() }));
}
export async function runCommandBehaviorEvaluation(input, { generatedAt = new Date() } = {}) {
  const { executor, taskSet, runtimeDependencies, repetitions } = nativeCall("evaluation.prepare", { ...input, cwd: process.cwd() });
  const run = await runBehaviorTaskSet({
    taskSet, repetitions, budget: input?.budget ?? null,
    execute: async (task, context) => JSON.parse(await nativeBinding().executeBehaviorCommand(JSON.stringify({
      executor, environment: isolatedEnvironment(executor.environment),
      payload: { contract: "agent-engine-behavior-executor-v1", task, repetition: context.repetition,
        taskSet: { id: taskSet.taskSetId, version: taskSet.taskSetVersion, digest: digest(taskSet) } },
    }))),
  });
  return defineBehaviorEvaluationReport({ mode: "execute", taskSet, samples: run.samples, policy: input.policy ?? {},
    executor, runtimeDependencies, budget: run.budget, generatedAt });
}
export function parseBehaviorEvaluationInput(input, { generatedAt = new Date() } = {}) {
  const mode = nativeCall("evaluation.mode", input);
  if (mode === "execute") return runCommandBehaviorEvaluation(input, { generatedAt });
  return defineBehaviorEvaluationReport({ ...input, mode, generatedAt });
}
