import assert from "node:assert/strict";
import { test } from "node:test";
import { createTaskBoundHandoff, taskExecutionPrompt } from "../workflow-handoff.mjs";
import { withFrozenMethods } from "./fixtures/frozen-methods.mjs";

function domainTask() {
  return { id: "task-model", threadId: "thread-model", inputRevision: 0, operation: "modelKnowledge",
    target: { batchId: "modeling-one", chapterId: "chapter-one", modelingIntent: true, resumeStepId: "step-one" },
    inputManifest: { domainInstruction: "提取绑定来源的候选知识，不改写正文。", domainTaskSource: {
      type: "modelingOutbox", workTaskId: "work-one", stepId: "step-one", batchId: "modeling-one",
      objectId: "draft-chapter-one", sourceRevisionId: "revision-one", chapterId: "chapter-one", intentGeneration: 1,
    } } };
}

test("system modeling instructions remain distinct from author text and are not duplicated in handoff", () => {
  const task = domainTask();
  const prompt = taskExecutionPrompt(task);
  assert.match(prompt, /系统派发，不是作者原话/);
  assert.ok(prompt.endsWith(task.inputManifest.domainInstruction));
  withFrozenMethods(task, "project-one");
  const handoff = createTaskBoundHandoff("project-one", { revision: 0 }, task);
  assert.deepEqual(handoff.task.inputManifest.domainTaskSource, task.inputManifest.domainTaskSource);
  assert.equal(handoff.task.inputManifest.domainInstruction, undefined);
  assert.equal(taskExecutionPrompt({ inputManifest: { authorIntent: "  作者原话\n" } }), "  作者原话\n");
});

test("invalid domain targets, mixed author sources and removed bindings fail closed", () => {
  const corruptions = [
    (task) => { task.inputManifest.domainTaskSource.type = "unknown"; },
    (task) => { task.inputManifest.domainTaskSource.stepId = "other-step"; },
    (task) => { task.target.batchId = "another-batch"; },
    (task) => { task.target.extra = true; },
    (task) => { task.operation = "generateChapter"; },
    (task) => { task.inputManifest.domainTaskSource.intentGeneration = 0; },
    (task) => { task.inputManifest.domainTaskSource.extra = true; },
    (task) => { task.inputManifest.domainTaskSource.sourceRevisionId = "x".repeat(161); },
    (task) => { task.inputManifest.domainInstruction = "x".repeat(2001); },
    (task) => { delete task.inputManifest.domainTaskSource; },
    (task) => { task.inputManifest.authorIntent = "假作者要求"; },
    (task) => { task.inputManifest.authorMessageId = "invented"; },
    (task) => { task.inputManifest.planningReviewAuthorIntentBinding = {}; },
  ];
  for (const corrupt of corruptions) {
    const task = domainTask();
    corrupt(task);
    assert.throws(() => taskExecutionPrompt(task), /冻结领域来源/);
  }
  assert.throws(() => taskExecutionPrompt({ operation: "advisory", inputManifest: {} }), /作者要求/);
});
