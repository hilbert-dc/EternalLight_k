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
    prompt: "为当前选中的第 3 章生成完整候选草稿。限定视角人物为 daughter。先按作者要求确定验收标准，再起草、独立检查并修订未满足项。\n\n作者意图：父女在饭桌上互相试探",
    taskType: { operation: "generateChapter" },
    target: { type: "chapter", chapterId: "chapter-3", chapterNumber: 3, readingChapter: 3, sceneOrder: 1, scope: { type: "wholeChapter" }, povCharacterId: "daughter" },
  });
});

test("chapter generation rejects ambiguous or mismatched targets before model execution", () => {
  assert.throws(() => chapterGenerationTask("project-1", { chapterId: "", chapterNumber: 3, intent: "写作" }), /目标章节无效/);
  assert.throws(() => chapterGenerationTask("project-1", { chapterId: "chapter-3", chapterNumber: 0, intent: "写作" }), /目标章节位置无效/);
  assert.throws(() => chapterGenerationTask("project-1", { chapterId: "chapter-3", chapterNumber: 3, intent: "" }), /创作意图不能为空/);
});

test("planning generation uses an explicit operation instead of prompt classification", () => {
  assert.deepEqual(planningGenerationTask("generatePlan", "规划接下来三章"), {
    prompt: "规划接下来三章\n\n每个规划章节需要稳定 chapterPlanId；每个场景需要稳定 id、目标、节拍与非空 expectedEffects。每个预期效果包含稳定 id、dimension(world/knowledge/relationship/narrative) 和可在正文中核对的 description，以及 commitmentRefs 数组；未关联承诺时为空。引用独立承诺用 {type:commitment,commitmentId,revisionId}，引用提案用 {type:proposal,reportRevisionId,proposalId,sourceRevisionId}，身份从 Context Pack 原样复制。每项最多4条，整份计划最多64条，不引用未来、过期或已建立的旧提案。规划效果不是已发生事实。",
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
