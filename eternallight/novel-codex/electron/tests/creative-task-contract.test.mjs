import assert from "node:assert/strict";
import { test } from "node:test";

import { chapterGenerationTask, planningGenerationTask } from "../creative-task-contract.mjs";
import { authorIntentError, modelInputTokenBudget } from "../../shared/author-input-budget.mjs";

test("chapter generation binds an explicit writer target", () => {
  const task = chapterGenerationTask("project-1", {
    chapterId: "chapter-3",
    chapterNumber: 3,
    povCharacterId: "daughter",
    intent: "父女在饭桌上互相试探",
  });
  assert.deepEqual(task, {
    prompt: "为当前选中的第 3 章生成完整候选草稿。\n\n作者意图：父女在饭桌上互相试探",
    taskType: { operation: "generateChapter" },
    target: { type: "chapter", chapterId: "chapter-3", chapterNumber: 3, readingChapter: 3, scope: { type: "wholeChapter" }, povCharacterId: "daughter" },
  });
});

test("chapter generation rejects ambiguous or mismatched targets before model execution", () => {
  assert.throws(() => chapterGenerationTask("project-1", { chapterId: "", chapterNumber: 3, intent: "写作" }), /目标章节无效/);
  assert.throws(() => chapterGenerationTask("project-1", { chapterId: "chapter-3", chapterNumber: 0, intent: "写作" }), /目标章节位置无效/);
  assert.throws(() => chapterGenerationTask("project-1", { chapterId: "chapter-3", chapterNumber: 3, intent: "" }), /创作意图不能为空/);
});

test("planning generation uses an explicit operation instead of prompt classification", () => {
  assert.deepEqual(planningGenerationTask("generatePlan", "规划接下来三章"), {
    prompt: "规划接下来三章",
    taskType: { operation: "generatePlan" },
    target: { type: "artifact" },
  });
  assert.throws(() => planningGenerationTask("generateChapter", "写一章"), /operation 无效/);
});

test("long unbroken Unicode input is budgeted promptly without truncation", () => {
  modelInputTokenBudget("warm encoders");
  const started = performance.now();
  assert.match(authorIntentError("🌙".repeat(5000)), /文本预算/);
  assert.ok(performance.now() - started < 2000, "unbroken emoji input must not freeze the editor");
  assert.equal(authorIntentError("父女在饭桌上互相试探".repeat(400)), null);
  assert.equal(authorIntentError("原意🌙保留\n".repeat(500)), null);
  assert.match(authorIntentError("界".repeat(16001)), /UTF-8 字节/);
});
