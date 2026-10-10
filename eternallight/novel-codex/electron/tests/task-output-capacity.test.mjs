import assert from "node:assert/strict";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { createTaskExecution, bindExecutionTask, TaskBudget } from "@eternallight/agent-engine";
import { assertNovelModelMessageBudget, novelTurnText, composeNovelCodexPrompt } from "../codex-runtime.mjs";
import { taskOutputCapacityInstructions, withTaskOutputCapacity } from "../task-output-capacity.mjs";
import { composeAdvisoryPrompt } from "../advisory-prompt.mjs";
import { createTaskBoundHandoff, taskBoundInstructions, taskExecutionPrompt } from "../workflow-handoff.mjs";
import { modelInputTokenBudget } from "../../shared/author-input-budget.mjs";
import { withFrozenMethods } from "./fixtures/frozen-methods.mjs";
import { mainHandlers } from "./main-handler-harness.mjs";

const current = () => ({ task: { id: "bound-current" }, modelRoute: "current",
  sessionPlan: { defaultModel: "other", routes: [
    { logicalModel: "other", outputBudget: 25000 }, { logicalModel: "current", outputBudget: 8192 },
  ] } });

test("only the exact unique current route supplies a single-call capacity", () => {
  const binding = current();
  const instructions = taskOutputCapacityInstructions(binding);
  assert.match(instructions, /8192 token/);
  assert.equal(instructions.includes("25000"), false);
  assert.match(instructions, /实际请求可更低/);
  assert.match(instructions, /作者总预算、任务范围和写入授权仍以原契约为准/);
  assert.match(instructions, /不得截断正文或必填参数/);
  assert.equal(taskOutputCapacityInstructions({ ...binding, modelRoute: "unmatched" }), null);
  assert.equal(taskOutputCapacityInstructions({ ...binding, modelRoute: null }), null);
  assert.equal(taskOutputCapacityInstructions({ ...binding, sessionPlan: { defaultModel: "current" } }), null);
  binding.sessionPlan.routes.push({ logicalModel: "current", outputBudget: 8192 });
  assert.equal(taskOutputCapacityInstructions(binding), null, "ambiguous routes are never guessed");
});

test("missing or invalid capacity does not fabricate a model limit", () => {
  const input = { input: [{ type: "text", text: "完整作者要求" }], additionalContext: {} };
  for (const value of [undefined, null, "8192", 0, -1, 1.5, NaN, Infinity, Number.MAX_SAFE_INTEGER + 1]) {
    const binding = current();
    binding.sessionPlan.routes[1].outputBudget = value;
    assert.equal(taskOutputCapacityInstructions(binding), null);
    assert.equal(withTaskOutputCapacity(input, binding), input);
  }
});

test("capacity is separate application context and changes no frozen input, authority or credentials", () => {
  const input = { input: [{ type: "text", text: "保持原章原文。" }, { type: "text", text: "作者当前要求：只改选中场景。" }], additionalContext: {} };
  const binding = current();
  Object.assign(binding.sessionPlan.routes[1], { credentialRef: { value: "private-credential" },
    endpoint: "https://private-route.invalid/private-path", provider: "private-provider", model: "private-model" });
  binding.task = { id: "bound-current", inputRevision: 7,
    target: { scope: { type: "selectedScenes", groupIds: ["one"], sourceRevisionId: "frozen" } },
    inputManifest: { authorIntent: "只改选中場景。" } };
  binding.providerBudget = { maxRequests: 2, maxTokens: 16000, deadlineAt: null };
  const original = structuredClone({ input, binding });
  const output = withTaskOutputCapacity(input, binding);
  assert.notEqual(output, input);
  assert.equal(output.input, input.input);
  assert.equal(output.additionalContext.novelOutputCapacity.kind, "application");
  for (const privateValue of ["private-credential", "private-route", "private-path", "private-provider", "private-model", "bound-current", "frozen"]) {
    assert.equal(output.additionalContext.novelOutputCapacity.value.includes(privateValue), false);
  }
  assert.deepEqual({ input, binding }, original);
  assert.equal(withTaskOutputCapacity(input, { ...binding, task: null }), input,
    "unbound Core turns do not receive a domain task hint");
});

test("the added capacity hint participates in the complete byte and token admission checks", () => {
  for (const message of ["x".repeat(37990), "🌙 ".repeat(2990)]) {
    assertNovelModelMessageBudget(message);
    const input = { input: [{ type: "text", text: message.slice(0, Math.floor(message.length / 2)) },
      { type: "text", text: message.slice(Math.floor(message.length / 2)) }], additionalContext: {} };
    const original = structuredClone(input);
    assert.throws(() => withTaskOutputCapacity(input, current()), /超过安全上限/);
    assert.deepEqual(input, original, "capacity pressure never clips the original message");
  }
  assert.throws(() => withTaskOutputCapacity({ input: [{ type: "image", imageUrl: "private-file" }], additionalContext: {} }, current()), /完整文本/);
});

function frozenTask(operation) {
  return withFrozenMethods({ id: `task-${operation}`, projectId: "project", threadId: "thread", inputRevision: 1,
    operation, purpose: operation === "generateChapter" ? "writer" : "critic",
    requiredOutputKind: operation === "generateChapter" ? "draft" : "knowledge",
    target: { type: "chapter", chapterId: "chapter-one", chapterNumber: 1,
      ...(operation === "generateChapter" ? { scope: { type: "selectedScenes", sourceRevisionId: "draft-before", groupIds: ["one"] } } : {}) },
    inputManifest: { authorIntent: "保留已授权目标，完成局部修订并核对。", expectedCandidateRevisionId: null,
      ...(operation === "advisory" ? { workDecision: { workTaskId: "work-current" } } : {}) } }, "project");
}

test("actual frozen writer and sameWork decision prompt composition admits the added capacity", () => {
  for (const operation of ["generateChapter", "advisory"]) {
    const task = frozenTask(operation), original = structuredClone(task);
    const handoff = createTaskBoundHandoff("project", { revision: 1 }, task);
    const input = operation === "advisory"
      ? composeAdvisoryPrompt(task, taskExecutionPrompt(task), handoff)
      : composeNovelCodexPrompt("", taskExecutionPrompt(task), taskBoundInstructions(handoff));
    const wire = novelTurnText(withTaskOutputCapacity(input, { ...current(), task }));
    assert.ok(modelInputTokenBudget(wire) <= 9000);
    assert.ok(Buffer.byteLength(wire) <= 38000);
    assert.equal(wire.includes(input.input[0].text), true);
    assert.deepEqual(task, original);
  }
});

test("production sendAgentTurn forwards the trusted route hint once without changing the task budget", async () => {
  const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));
  const { sendAgentTurn, adapter, providerConfig, modelBridge } = await mainHandlers(
    async () => { throw new Error("the synthetic turn must not query domain data"); }, `${process.execPath} ${fixture}`);
  adapter.modelRoute = providerConfig.sessionPlan.routes[0].logicalModel;
  const task = frozenTask("generateChapter");
  const execution = createTaskExecution();
  bindExecutionTask(execution, task);
  Object.assign(execution, { adapter, providerConfig, modelBridge, sessionPlan: providerConfig.sessionPlan,
    operation: "generateChapter", taskBudget: new TaskBudget({ id: `budget-${task.id}`, maxRequests: 2, maxTokens: 16000 }) });
  const before = structuredClone({ task, routes: execution.sessionPlan.routes });
  const budgetBefore = execution.taskBudget.snapshot();
  const input = composeNovelCodexPrompt("", taskExecutionPrompt(task));
  try {
    const result = await sendAgentTurn(execution, input, { disableEnvironment: true });
    const received = result.turn.receivedParams;
    assert.equal(received.additionalContext.novelOutputCapacity.value, taskOutputCapacityInstructions({ sessionPlan: execution.sessionPlan, modelRoute: adapter.modelRoute }));
    assert.equal(Object.keys(received.additionalContext).filter(key => key === "novelOutputCapacity").length, 1);
    assert.deepEqual(received.input, input.input);
    assert.deepEqual(received.additionalContext.novelRules, input.additionalContext.novelRules);
    assert.deepEqual({ task, routes: execution.sessionPlan.routes }, before);
    const after = execution.taskBudget.snapshot();
    for (const key of ["maxRequests", "maxTokens", "maxDurationMs"]) assert.equal(after[key], budgetBefore[key]);
  } finally { adapter.close(); }
});
