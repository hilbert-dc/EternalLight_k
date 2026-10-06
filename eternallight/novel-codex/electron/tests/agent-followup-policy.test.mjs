import assert from "node:assert/strict";
import { test } from "node:test";
import { composeAdvisoryPrompt, POST_EDIT_INSPECTION_INSTRUCTIONS, DELIVERY_GOAL_STATUS_INSTRUCTIONS, AGENT_STRATEGY_INSTRUCTIONS } from "../advisory-prompt.mjs";
import { TOOL_PROGRESS_INSTRUCTIONS } from "../context-reading-contract.mjs";
import { QUALITY_DELIVERY_REVIEW_INSTRUCTIONS } from "../creative-mission.mjs";
import { withFrozenMethods } from "./fixtures/frozen-methods.mjs";

test("post-edit inspection is delivered to the bound coordinator without converting ordinary discussion into work", () => {
  for (const decision of [false, true]) {
    const task = withFrozenMethods({ id: "advisory", operation: "advisory", inputRevision: 1,
      target: { type: "project" }, inputManifest: decision ? { workDecision: { workTaskId: "work" } } : {} });
    const before = structuredClone(task);
    const prompt = composeAdvisoryPrompt(task, "只修改已经指定的段落", {}).map(item => item.text).join("");
    assert.equal(prompt.includes(POST_EDIT_INSPECTION_INSTRUCTIONS), decision);
    assert.equal(prompt.includes(QUALITY_DELIVERY_REVIEW_INSTRUCTIONS), decision);
    assert.ok(prompt.includes(TOOL_PROGRESS_INSTRUCTIONS));
    assert.deepEqual(task, before);
    if (decision) {
      assert.ok(prompt.includes("作者未明确排除检查且必需行动已完成、canFinish=true"));
      assert.ok(prompt.includes("不得因此自动改写其他章节"));
      assert.ok(prompt.includes("未覆盖内容"));
      assert.ok(prompt.includes("receipts.items中generateChapter"));
      assert.ok(prompt.includes("仅措辞、错字或标点润色"));
      assert.ok(prompt.includes("required=false的modelKnowledge等工具是可选能力"));
      assert.ok(prompt.includes("作者明确要求的效果评估仍须完成，可直接对照冻结的修改前后正文"));
      assert.ok(prompt.includes(AGENT_STRATEGY_INSTRUCTIONS));
      assert.ok(prompt.includes("检查coverage和pendingSources"));
    }
  }
});


test("delivery review distinguishes unavailable substantive checks from successful limitation disclosure", () => {
  const goal = "只修改这一句，并检查与第2章的衔接；如果没有第2章，请明确说明无法检查。";
  const decision = withFrozenMethods({ id: "final", operation: "advisory", inputRevision: 1,
    target: { type: "project" }, inputManifest: { workDecision: { workTaskId: "work" } } });
  const prompt = composeAdvisoryPrompt(decision, goal, {}).map(item => item.text).join("");
  assert.ok(prompt.includes(goal), "The original conditional author instruction must remain available");
  assert.ok(prompt.includes(DELIVERY_GOAL_STATUS_INSTRUCTIONS));
  assert.ok(prompt.includes(QUALITY_DELIVERY_REVIEW_INSTRUCTIONS));
  assert.match(QUALITY_DELIVERY_REVIEW_INSTRUCTIONS, /未要求的全书检查不制造partial/);
  assert.match(QUALITY_DELIVERY_REVIEW_INSTRUCTIONS, /不自动扩写、采用、发布或增加预算/);
  assert.ok(DELIVERY_GOAL_STATUS_INSTRUCTIONS.includes("goalStatus必须为partial，unresolved写明具体未完成目标及原因"));
  assert.ok(DELIVERY_GOAL_STATUS_INSTRUCTIONS.includes("如实说明满足沟通要求，但第2章衔接检查仍未完成"));
  assert.ok(DELIVERY_GOAL_STATUS_INSTRUCTIONS.includes("作者明确排除的检查不属于本轮目标"));
  assert.ok(DELIVERY_GOAL_STATUS_INSTRUCTIONS.includes("仅当下一步确实需要作者作出情节或范围选择时用needsAuthorDecision"));
  const worker = withFrozenMethods({ ...decision, inputManifest: {} });
  const discussion = composeAdvisoryPrompt(worker, goal, {}).map(item => item.text).join("");
  assert.equal(discussion.includes(DELIVERY_GOAL_STATUS_INSTRUCTIONS), false,
    "Final delivery policy must not manufacture a work review during ordinary discussion");
});
