import { nativeBinding } from "../native.mjs";

import { digest } from "../contracts/capabilities.mjs";
import { defineBehaviorTaskSet, evaluateAutonomousBehavior, runBehaviorTaskSet } from "../capabilities/behavior-evaluation.mjs";

const MAX_EXECUTOR_OUTPUT_BYTES = 1024 * 1024;
const EXECUTOR_ENVIRONMENT_KEYS = ["PATH", "PATHEXT", "SystemRoot", "WINDIR", "ComSpec", "TEMP", "TMP", "TMPDIR", "LANG", "LC_ALL"];

function isolatedEnvironment(extra) {
  return Object.fromEntries([
    ...EXECUTOR_ENVIRONMENT_KEYS.filter((key) => typeof process.env[key] === "string").map((key) => [key, process.env[key]]),
    ...Object.entries(extra),
  ]);
}

function positiveInteger(value, label, fallback) {
  const resolved = value ?? fallback;
  if (!Number.isInteger(resolved) || resolved < 1) throw new Error(`${label} must be a positive integer`);
  return resolved;
}

function defineExecutor(input) {
  if (!input || typeof input !== "object" || Array.isArray(input)) throw new Error("behavior command executor is required");
  if (typeof input.command !== "string" || !input.command.trim()) throw new Error("behavior executor command is required");
  if (!Array.isArray(input.args ?? []) || (input.args ?? []).some((argument) => typeof argument !== "string")) throw new Error("behavior executor args are invalid");
  if (input.cwd !== undefined && (typeof input.cwd !== "string" || !input.cwd)) throw new Error("behavior executor cwd is invalid");
  const environment = input.environment ?? {};
  if (!environment || typeof environment !== "object" || Array.isArray(environment)
    || Object.entries(environment).some(([key, value]) => !/^[A-Za-z_][A-Za-z0-9_]*$/.test(key) || typeof value !== "string")) {
    throw new Error("behavior executor environment is invalid");
  }
  return Object.freeze({
    command: input.command,
    args: Object.freeze([...(input.args ?? [])]),
    cwd: input.cwd ?? process.cwd(),
    environment: Object.freeze({ ...environment }),
    timeoutMs: positiveInteger(input.timeoutMs, "behavior executor timeoutMs", 120_000),
    maxOutputBytes: positiveInteger(input.maxOutputBytes, "behavior executor maxOutputBytes", MAX_EXECUTOR_OUTPUT_BYTES),
  });
}

function defineRuntimeDependencies(input, { required = [] } = {}) {
  if (!input || typeof input !== "object" || Array.isArray(input)) throw new Error("behavior runtimeDependencies must be an object");
  const entries = Object.entries(input);
  if (entries.length > 32) throw new Error("behavior runtimeDependencies exceed the entry limit");
  const dependencies = Object.freeze(Object.fromEntries(entries.sort(([left], [right]) => left.localeCompare(right)).map(([name, dependency]) => {
    if (!/^[a-z][a-z0-9._-]{0,79}$/i.test(name)) throw new Error(`behavior runtime dependency name is invalid: ${name}`);
    if (!dependency || typeof dependency !== "object" || Array.isArray(dependency) || !/^[a-f0-9]{64}$/.test(dependency.digest ?? "")) {
      throw new Error(`behavior runtime dependency digest is invalid: ${name}`);
    }
    if (dependency.ref !== undefined && (typeof dependency.ref !== "string" || !dependency.ref || dependency.ref.length > 500)) {
      throw new Error(`behavior runtime dependency ref is invalid: ${name}`);
    }
    return [name, Object.freeze({ digest: dependency.digest, ...(dependency.ref ? { ref: dependency.ref } : {}) })];
  })));
  for (const name of required) if (!dependencies[name]) throw new Error(`behavior runtime dependency is required: ${name}`);
  return dependencies;
}

async function executeCommand(executor, payload) {
  return JSON.parse(await nativeBinding().executeBehaviorCommand(JSON.stringify({
    executor, payload, environment: isolatedEnvironment(executor.environment),
  })));
}

export function defineBehaviorEvaluationReport({ mode, taskSet, samples, policy = {}, executor = null, runtimeDependencies = {}, budget = null, generatedAt = new Date() }) {
  if (!['execute', 'samples'].includes(mode)) throw new Error("behavior evaluation mode is invalid");
  const frozenTaskSet = taskSet ? defineBehaviorTaskSet(taskSet) : null;
  const evaluation = evaluateAutonomousBehavior(samples, policy);
  const body = {
    version: 2,
    mode,
    generatedAt: generatedAt.toISOString(),
    authorizationEffect: "none",
    taskSet: frozenTaskSet ? { id: frozenTaskSet.taskSetId, version: frozenTaskSet.taskSetVersion, digest: digest(frozenTaskSet) } : null,
    scorer: { id: "execution-facts-v2", digest: digest({ id: "execution-facts-v2", semantics: "content-assertions-plus-authoritative-tool-and-lineage-facts" }) },
    executor: executor ? { digest: digest({ command: executor.command, args: executor.args, cwd: executor.cwd, timeoutMs: executor.timeoutMs, maxOutputBytes: executor.maxOutputBytes, environment: executor.environment }), command: executor.command, args: executor.args } : null,
    runtimeDependencies: defineRuntimeDependencies(runtimeDependencies),
    samples,
    budget,
    policy,
    evaluation,
  };
  return Object.freeze({ reportId: digest(body), ...body });
}

export async function runCommandBehaviorEvaluation(input, { generatedAt = new Date() } = {}) {
  const runtimeDependencies = defineRuntimeDependencies(input?.runtimeDependencies ?? {}, { required: ["engineBuild"] });
  const taskSet = defineBehaviorTaskSet(input?.taskSet);
  const executor = defineExecutor(input?.executor);
  const repetitions = positiveInteger(input?.repetitions, "behavior repetitions", 1);
  const run = await runBehaviorTaskSet({
    taskSet,
    repetitions,
    budget: input?.budget ?? null,
    execute: (task, context) => executeCommand(executor, {
      contract: "agent-engine-behavior-executor-v1",
      task,
      repetition: context.repetition,
      taskSet: { id: taskSet.taskSetId, version: taskSet.taskSetVersion, digest: digest(taskSet) },
    }),
  });
  return defineBehaviorEvaluationReport({
    mode: "execute",
    taskSet,
    samples: run.samples,
    policy: input.policy ?? {},
    executor,
    runtimeDependencies,
    budget: run.budget,
    generatedAt,
  });
}

export function parseBehaviorEvaluationInput(input, { generatedAt = new Date() } = {}) {
  if (input?.mode === "execute") return runCommandBehaviorEvaluation(input, { generatedAt });
  if (input?.mode !== undefined && input.mode !== "samples") throw new Error("behavior evaluation mode must be execute or samples");
  if (!Array.isArray(input?.samples)) throw new Error("sample report mode requires samples[]");
  return defineBehaviorEvaluationReport({ mode: "samples", taskSet: input.taskSet ?? null, samples: input.samples, policy: input.policy ?? {}, runtimeDependencies: input.runtimeDependencies ?? {}, generatedAt });
}
