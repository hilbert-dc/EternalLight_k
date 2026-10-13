import { digest } from "../contracts/capabilities.mjs";
import { nativeBinding, nativeCall } from "../native.mjs";

export const QUALIFICATION_SUITE_KINDS = Object.freeze(["protocol", "core-integration", "execution-conformance"]);
function freezeDeep(value) {
  if (value && typeof value === "object") { for (const child of Object.values(value)) freezeDeep(child); Object.freeze(value); }
  return value;
}
export const defineBehaviorTaskSet = input => freezeDeep(nativeCall("behavior.tasks", input));
export const scoreBehaviorOutput = (task, execution) => freezeDeep(nativeCall("behavior.score", { task, execution }));
/** Runs a frozen task set; Rust owns ordering, accounting and stop conditions. */
export async function runBehaviorTaskSet({ taskSet, execute, repetitions = 1, scorer = scoreBehaviorOutput, budget = null }) {
  const frozenTaskSet = defineBehaviorTaskSet(taskSet);
  if (typeof execute !== "function" || typeof scorer !== "function") throw new Error("behavior executor and scorer are required");
  const now = () => Math.floor(performance.now());
  const run = new (nativeBinding().NativeBehaviorRun)(JSON.stringify({ taskSet: frozenTaskSet, repetitions, budget, now: now() }));
  const apply = (method, input = {}) => JSON.parse(run.apply(method, JSON.stringify({ ...input, now: now() })));
  for (let item = apply("next"); item; item = apply("next")) {
    const task = freezeDeep(item.task);
    let execution;
    try {
      execution = await execute(task, { repetition: item.repetition, taskSet: frozenTaskSet });
    } catch (error) {
      execution = { status: "failed", failureCategory: "executor-error", error: String(error?.message ?? error).slice(0, 500) };
    }
    const score = await scorer(task, execution);
    apply("complete", { sampleId: item.sampleId, execution, score });
  }
  return freezeDeep(apply("result"));
}

export const wilsonLowerBound = (successes, total, z = 1.959963984540054) => nativeCall("behavior.wilson", { successes, total, z });
export const defineQualificationSuiteResult = input => freezeDeep(nativeCall("behavior.suite", input));
export function evaluateAutonomousBehavior(samples, options = {}) {
  const body = nativeCall("behavior.evaluate", { samples, options });
  return freezeDeep({ evaluationId: digest(body), ...body });
}
