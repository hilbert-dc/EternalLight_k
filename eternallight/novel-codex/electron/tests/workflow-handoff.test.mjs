import assert from "node:assert/strict";
import { test } from "node:test";

import { createTaskBoundHandoff, taskBoundInstructions } from "../workflow-handoff.mjs";

test("handoff contains the immutable task target and manifest without embedding Context", () => {
  const workspace = { revision: 7, artifacts: { brief: { accepted: { payload: { premise: "secret context" } } } } };
  const task = {
    id: "task-7",
    threadId: "thread-7",
    inputRevision: 7,
    operation: "generateChapter",
    purpose: "writer",
    requiredOutputKind: "draft",
    target: { type: "chapter", chapterId: "chapter-2", chapterNumber: 2, sceneOrder: 3, scope: { type: "throughScene", endSceneOrder: 3 } },
    inputManifest: { expectedCandidateRevisionId: "draft-rev-1", planningBaseline: { planRevisionId: "plan-rev-4" } },
  };
  const handoff = createTaskBoundHandoff("project-1", workspace, task);
  assert.deepEqual(handoff.task.target, task.target);
  assert.deepEqual(handoff.task.inputManifest, task.inputManifest);
  assert.doesNotMatch(JSON.stringify(handoff), /secret context/);
  assert.match(taskBoundInstructions(handoff), /novel_workspace\.read_context/);
});

test("handoff rejects a task whose frozen revision no longer matches the workspace", () => {
  assert.throws(() => createTaskBoundHandoff("project-1", { revision: 8 }, { id: "task", threadId: "thread", inputRevision: 7 }), /输入版本/);
});
