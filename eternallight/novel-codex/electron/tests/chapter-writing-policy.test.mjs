import assert from "node:assert/strict";
import { test } from "node:test";
import { taskBoundInstructions, createTaskBoundHandoff, taskAuthorPrompt } from "../workflow-handoff.mjs";
import { composeNovelCodexPrompt } from "../codex-runtime.mjs";
import { CHAPTER_LENGTH_INSTRUCTIONS } from "../../shared/chapter-writing-policy.mjs";
import { CONTEXT_READING_INSTRUCTIONS } from "../context-reading-contract.mjs";
import { modelInputTokenBudget } from "../../shared/author-input-budget.mjs";
import { withFrozenMethods } from "./fixtures/frozen-methods.mjs";

function bound(operation, intent, target = {}) {
  const task = { id: `task-${operation}`, threadId: "thread", inputRevision: 1, operation,
    purpose: operation === "generateChapter" ? "writer" : "critic", requiredOutputKind: operation === "generateChapter" ? "draft" : "knowledge",
    target: { type: "chapter", chapterId: "chapter-one", chapterNumber: 1, ...target },
    inputManifest: { authorIntent: intent, expectedCandidateRevisionId: null } };
  withFrozenMethods(task, "project");
  return task;
}

// Check individual obligations independently of the imported production constant.
// These are prompt-contract checks, not proof of model or Runtime behavior.
function assertClauses(instructions, clauses) {
  for (const [obligation, pattern] of clauses) assert.match(instructions, pattern, obligation);
}

function assertFrozenSourceReading(instructions) {
  assertClauses(instructions, [
    ["read the frozen directory before requesting source contents", /先read_context核对冻结target、coverage和taskSources/],
    ["the directory is scoped metadata, not delivered source text", /taskSources是已通过同一任务范围\/POV\/时间\/保护规则的冻结资料目录，未交付正文/],
    ["read frozen source pages without guessing paths or switching to latest", /read_task_source\(sourceId,cursor\)，不猜路径或切换latest/],
    ["fully read required sources without automatically reading the whole book", /原稿修改、保护场景及明确评估\/抽取范围必须实际完整读取，普通问答不默认遍历全书/],
    ["verify contiguous ranges and hashes before reconstructing source JSON", /连续byteRange与contentHash核对后拼接payloadPart并解析JSON/],
    ["the last page does not certify earlier reading or semantic completeness", /最后一页不证明先前已读或语义完整/],
    ["disclose unread, truncated or stale sources instead of claiming completion", /未读\/截断\/来源过期须说明，不能当不存在或伪造完成/],
    ["reconstruct scene groups in canonical order", /textRepresentation=sceneGroups时，按原顺序body\.trim\(\)用双换行、---、双换行重建全文/],
    ["do not silently replace an inconsistent body representation", /body表示则读原body，二者不一致不得擅自替换/],
    ["paging and queries cannot expand evidence or writing authority", /query_story仍只查本任务授权证据，资料分页不扩事实视角或写权限/],
    ["advance only via the returned cursor", /分页只沿返回的 nextCursor 前进，不从首段反复开始/],
  ]);
}

function assertScopedWriter(instructions) {
  assertClauses(instructions, [
    ["read the frozen draft, bound plan and protected scope", /完整读取本章冻结原稿、绑定场景计划和保护范围/],
    ["do not change author-approved goals", /desiredOutcomes是作者已批准标准，不得增删或改变/],
    ["submit all original groups but change only selected text", /selectedScenes 必须提交全部原组，只改选中组正文/],
    ["preserve unselected and locked groups and never invent unread content", /未选与锁定组、身份、顺序、标题和sourcePlanRefs不变，不重造未读内容/],
    ["save only one complete representation with protected identity and text", /保存正文仅body或完整sceneGroups之一，未选\/锁定场景身份与原文不能改变/],
    ["validation repair cannot switch the target or CAS", /同任务预算内纠正完整payload，不换目标\/CAS/],
  ]);
}

test("writer soft-length contract preserves explicit short/English/scoped author requests and frozen bindings", () => {
  for (const [intent, target] of [
    ["首次起草这一章", {}], ["只写150字中文", {}], ["改写为约100英文单词，对话用中文", {}],
    ["只改第二场结尾，其他正文不动", { scope: { type: "selectedScenes", sourceRevisionId: "draft-before", groupIds: ["second"] } }],
  ]) {
    const task = bound("generateChapter", intent, target), original = structuredClone(task);
    const handoff = createTaskBoundHandoff("project", { revision: 1 }, task);
    const instructions = taskBoundInstructions(handoff);
    const wire = composeNovelCodexPrompt("", taskAuthorPrompt(task), instructions).map(item => item.text).join("");
    assert.equal(instructions.split(CHAPTER_LENGTH_INSTRUCTIONS).length - 1, 1);
    assert.equal(instructions.split(CONTEXT_READING_INSTRUCTIONS).length - 1, 1);
    assert.ok(wire.endsWith(`用户当前指令：\n${intent}`));
    assert.deepEqual(handoff.task.target, original.target);
    assert.deepEqual(task, original);
    assert.ok(modelInputTokenBudget(wire) <= 9000);
    assert.ok(Buffer.byteLength(wire) <= 38000);
    if (target.scope) assertScopedWriter(instructions);
  }
});

test("knowledge analysis retains its independent bounded extraction contract without writer length targets", () => {
  const task = bound("modelKnowledge", "只分析已保存正文", { batchId: "batch" });
  const instructions = taskBoundInstructions(createTaskBoundHandoff("project", { revision: 1 }, task));
  assert.equal(instructions.includes(CHAPTER_LENGTH_INSTRUCTIONS), false);
  assert.equal(instructions.split(CONTEXT_READING_INSTRUCTIONS).length - 1, 1);
  assertFrozenSourceReading(instructions);
  assertClauses(instructions, [
    ["read this batch's frozen source", /完整读取本批次冻结正文与必要实体身份/],
    ["extract only source-backed candidates", /只提取有精确原文依据的候选实体、事实、事件、状态、关系和认知/],
    ["retain evidence for every assertion kind", /每条assertions都必须包含kind、subjectId、predicate、value、evidence，包括relation和epistemic/],
    ["bind quotes to actually read text and let Runtime resolve offsets", /已读原文中的唯一短句 \{quote\}，由Runtime解析位置/],
    ["do not guess character offsets", /不得自行估算Unicode字符位置/],
    ["bound the entire request including identity fields to 8192 UTF-8 bytes", /参数含绑定身份总量不超过8192UTF8B/],
    ["do not truncate required data or republish when over budget", /超限停止说明，不截断必填数据或重复发布/],
    ["finish after a committed publication", /发布committed后立即complete_task，不再查询或重发/],
  ]);
  assert.ok(modelInputTokenBudget(instructions) < 9000);
});

test("critic and planner handoffs share exact text representations while retaining their own frozen source authority", () => {
  for (const operation of ["extractCommitments", "advisory", "generatePlan", "assessSceneOutcome"]) {
    const task = bound(operation, "只分析当前授权来源", { sourceRevisionId: "source-frozen" });
    const original = structuredClone(task);
    const handoff = createTaskBoundHandoff("project", { revision: 1 }, task);
    const instructions = taskBoundInstructions(handoff);
    assert.equal(instructions.split(CONTEXT_READING_INSTRUCTIONS).length - 1, 1);
    assert.deepEqual(handoff.task.target, original.target);
    assert.deepEqual(task, original);
    assert.equal(instructions.includes(CHAPTER_LENGTH_INSTRUCTIONS), false);
    assertFrozenSourceReading(instructions);
    if (operation === "extractCommitments") {
      assertClauses(instructions, [
        ["extract only from the frozen revision", /只读冻结sourceRevisionId原文/],
        ["declare only actually read Unicode ranges", /examinedRanges按整章Unicode位置只声明实际阅读/],
        ["missing source cannot justify invented coverage", /原文不足不能虚构范围/],
        ["resolve a unique source anchor for excerpts", /候选摘录经resolve_story_evidence核对唯一锚点/],
        ["empty extraction does not prove absence", /空结果不证明没有伏笔/],
        ["use the atomic save receipt, not accumulated counts", /完整报告一次原子替换，最后回执candidateCount才是保存数量，不累加/],
      ]);
    }
    assert.ok(modelInputTokenBudget(instructions) < 9000);
  }
});

test("writer frozen source paging instructions keep protected scenes and author scope intact", () => {
  const sourceRevisionId = "draft-frozen";
  const task = bound("generateChapter", "只改这一场，不动锁定正文", {
    scope: { type: "selectedScenes", sourceRevisionId, groupIds: ["selected"] },
    povCharacterId: "authorized-pov", readingChapter: 2, sceneOrder: 3,
  });
  const original = structuredClone(task);
  const handoff = createTaskBoundHandoff("project", { revision: 1 }, task);
  const instructions = taskBoundInstructions(handoff);
  const wire = composeNovelCodexPrompt("", taskAuthorPrompt(task), instructions).map(item => item.text).join("");
  assert.deepEqual(handoff.task.target, original.target);
  assert.deepEqual(task, original);
  assertFrozenSourceReading(instructions);
  assertScopedWriter(instructions);
  assert.ok(wire.endsWith(`用户当前指令：\n${original.inputManifest.authorIntent}`));
  assert.ok(modelInputTokenBudget(wire) <= 9000);
  assert.ok(Buffer.byteLength(wire) <= 38000);
});
