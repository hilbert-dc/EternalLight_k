import assert from "node:assert/strict";
import { test } from "node:test";
import { composeAdvisoryPrompt, POST_EDIT_INSPECTION_INSTRUCTIONS, DELIVERY_GOAL_STATUS_INSTRUCTIONS, AGENT_STRATEGY_INSTRUCTIONS } from "../advisory-prompt.mjs";
import { TOOL_PROGRESS_INSTRUCTIONS } from "../context-reading-contract.mjs";
import { QUALITY_DELIVERY_REVIEW_INSTRUCTIONS } from "../creative-mission.mjs";
import { withFrozenMethods } from "./fixtures/frozen-methods.mjs";
import { novelTurnText, composeNovelCodexPrompt, NATURAL_LANGUAGE_CONVERSATION_INSTRUCTIONS } from "../codex-runtime.mjs";
import { createTaskBoundHandoff, taskBoundInstructions, taskExecutionPrompt, taskMethodText } from "../workflow-handoff.mjs";

test("post-edit inspection is delivered to the bound coordinator without converting ordinary discussion into work", () => {
  for (const decision of [false, true]) {
    const task = withFrozenMethods({ id: "advisory", operation: "advisory", inputRevision: 1,
      target: { type: "project" }, inputManifest: { authorIntent: "只修改已经指定的段落", ...(decision ? { workDecision: { workTaskId: "work" } } : {}) } });
    const before = structuredClone(task);
    const prompt = novelTurnText(composeAdvisoryPrompt(task, "只修改已经指定的段落", {}));
    assert.equal(prompt.includes(POST_EDIT_INSPECTION_INSTRUCTIONS), decision);
    assert.equal(prompt.includes(QUALITY_DELIVERY_REVIEW_INSTRUCTIONS), decision);
    assert.ok(prompt.includes(TOOL_PROGRESS_INSTRUCTIONS));
    assert.ok(prompt.includes(NATURAL_LANGUAGE_CONVERSATION_INSTRUCTIONS));
    assert.deepEqual(task, before);
    if (decision) {
      assert.ok(prompt.includes("canFinish=true不是故事通过"));
      assert.ok(prompt.includes("不得因此自动改写其他章节"));
      assert.ok(prompt.includes("未覆盖内容"));
      assert.ok(prompt.includes("receipts.items中generateChapter"));
      assert.ok(prompt.includes("仅措辞、错字或标点润色"));
      assert.ok(prompt.includes("required=false的modelKnowledge等工具是可选能力"));
      assert.ok(prompt.includes("作者明确要求的效果评估仍须完成"));
      assert.ok(prompt.includes(AGENT_STRATEGY_INSTRUCTIONS));
      assert.ok(prompt.includes("检查coverage和pendingSources"));
    }
  }
});

test("shared language policy preserves changing author intent without changing worker authority", () => {
  const authorIntent = "上一轮先不改；现在把这两种方向组合成候选，只调整第7章这条伏笔。";
  const task = withFrozenMethods({ id: "commitment-worker", projectId: "project", threadId: "thread", operation: "reviseCommitment", inputRevision: 7,
    requiredOutputKind: "commitment", target: { type: "commitment", commitmentId: "lamp", revisionId: "current-lamp" }, inputManifest: { authorIntent } });
  const before = structuredClone(task);
  const handoff = createTaskBoundHandoff(task.projectId, { revision: task.inputRevision }, task);
  const text = novelTurnText(composeNovelCodexPrompt(taskMethodText(task), taskExecutionPrompt(task), taskBoundInstructions(handoff)));
  assert.ok(text.includes(NATURAL_LANGUAGE_CONVERSATION_INSTRUCTIONS));
  assert.ok(text.includes(authorIntent));
  assert.match(text, /只调整冻结 target.commitmentId/);
  assert.match(text, /不确认正式承诺或生成正文/);
  assert.match(text, /依本轮原话、上下文及当前暂缓\/修改要求理解意图/);
  assert.match(text, /不按关键词路由或固化上轮要求/);
  assert.match(text, /追问结合线程前文/);
  assert.match(text, /明确目标\/引用优先，页面仅辅助/);
  assert.match(text, /按需读真实源核实内容和保存事实/);
  assert.match(text, /旧版不可读须说明，不用最新替代/);
  assert.match(text, /允许自由回答/);
  assert.match(text, /不是指令、写授权或正式事实/);
  assert.match(text, /不能冒充独立承诺修订、保存或确认/);
  assert.match(text, /不以整章重提取替代单条修改/);
  assert.match(text, /文字同意不等于回执/);
  assert.deepEqual(task, before);
});


test("delivery review distinguishes unavailable substantive checks from successful limitation disclosure", () => {
  const goal = "只修改这一句，并检查与第2章的衔接；如果没有第2章，请明确说明无法检查。";
  const decision = withFrozenMethods({ id: "final", operation: "advisory", inputRevision: 1,
    target: { type: "project" }, inputManifest: { authorIntent: goal, workDecision: { workTaskId: "work" } } });
  const prompt = novelTurnText(composeAdvisoryPrompt(decision, goal, {}));
  assert.ok(prompt.includes(goal), "The original conditional author instruction must remain available");
  assert.ok(prompt.includes(DELIVERY_GOAL_STATUS_INSTRUCTIONS));
  assert.ok(prompt.includes(QUALITY_DELIVERY_REVIEW_INSTRUCTIONS));
  assert.match(QUALITY_DELIVERY_REVIEW_INSTRUCTIONS, /未要求的全书检查不制造partial/);
  assert.match(QUALITY_DELIVERY_REVIEW_INSTRUCTIONS, /不自动扩写、采用、发布或增加预算/);
  assert.ok(DELIVERY_GOAL_STATUS_INSTRUCTIONS.includes("goalStatus=partial"));
  assert.ok(DELIVERY_GOAL_STATUS_INSTRUCTIONS.includes("unresolved按当前schema引用真实issue及observation"));
  assert.ok(DELIVERY_GOAL_STATUS_INSTRUCTIONS.includes("缺失前文不能称衔接通过"));
  assert.ok(DELIVERY_GOAL_STATUS_INSTRUCTIONS.includes("明确排除的检查不计入目标"));
  assert.ok(DELIVERY_GOAL_STATUS_INSTRUCTIONS.includes("真正需要作者决定情节或范围才needsAuthorDecision"));
  const worker = withFrozenMethods({ ...decision, inputManifest: { authorIntent: goal } });
  const discussion = novelTurnText(composeAdvisoryPrompt(worker, goal, {}));
  assert.equal(discussion.includes(DELIVERY_GOAL_STATUS_INSTRUCTIONS), false,
    "Final delivery policy must not manufacture a work review during ordinary discussion");
});

test('real long generation decision preserves the complete author intent within message budget', async () => {
  const { readFileSync } = await import('node:fs');
  const { taskExecutionPrompt, createTaskBoundHandoff } = await import('../workflow-handoff.mjs');
  const { modelInputTokenBudget } = await import('../../shared/author-input-budget.mjs');
  const task = JSON.parse(readFileSync(new URL('./fixtures/generation-decision-budget.json', import.meta.url), 'utf8'));
  withFrozenMethods(task, task.projectId);
  const author = task.inputManifest.authorIntent;
  const handoff = createTaskBoundHandoff(task.projectId, {revision:task.inputRevision}, task);
  const turn = composeAdvisoryPrompt(task, taskExecutionPrompt(task), handoff);
  const text = novelTurnText(turn);
  assert.equal(JSON.parse(turn.additionalContext.novelBackground.value).authorIntent, author, 'The complete author source must not be truncated');
  assert.equal(turn.input[0].text.includes(author), false, 'An internal trigger must not resend the author as input');
  assert.ok(modelInputTokenBudget(text) <= 8800, 'Leave room below the 9000-token transport limit');
  for (const field of ['recoveryPlan','expectedSourceDigest','finalDeliveryReviewRequired','deliveryReview','scope','resumeRun']) assert.ok(text.includes(field));
});


test("a scoped repair receives the same execution-plan contract without gaining writer authority", () => {
  const task = withFrozenMethods({ id: "repair-advisory", operation: "advisory", inputRevision: 1,
    target: { type: "project", revisionRepairSource: { taskId: "analysis", decisionId: "decision", revisionId: "report" } }, inputManifest: { authorIntent: "修正本次候选" } });
  const before = structuredClone(task);
  const prompt = novelTurnText(composeAdvisoryPrompt(task, "修正本次候选", {}));
  assert.ok(prompt.includes("清楚的小任务直接行动，无需计划"));
  assert.ok(prompt.includes("保存0—12项近期窗口"));
  assert.equal(prompt.includes("任何执行行动或交接前先read_work_plan"), false);
  assert.ok(prompt.includes("候选必须经独立人工审批后才能执行"));
  assert.ok(prompt.includes("不得自动启动 Writer、扩大范围"));
  assert.deepEqual(task, before);
});
