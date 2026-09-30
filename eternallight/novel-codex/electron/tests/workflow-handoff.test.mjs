import assert from "node:assert/strict";
import { test } from "node:test";

import { createTaskBoundHandoff, taskAuthorPrompt, taskBoundInstructions, taskMethodText } from "../workflow-handoff.mjs";
import { withFrozenMethods } from "./fixtures/frozen-methods.mjs";
import { modelInputTokenBudget } from "../../shared/author-input-budget.mjs";
import { reviewTask } from "./fixtures/planning-review-task.mjs";

test("review prompt preserves the real author root and rejects anonymous or cross-seat bindings", () => {
  const run = { id: "planning-review-test", projectId: "project", artifactKind: "outline", revisionId: "outline-one",
    inputRevision: 0, authorIntent: "  保留白塔。\n只检查人物 🕯️  ", authorMessageId: "author-one", sourcePacketHash: "a".repeat(64) };
  const task = reviewTask(run, "character", { threadId: "thread" }, "task-review");
  assert.equal(taskAuthorPrompt(task), run.authorIntent);
  const handoff = createTaskBoundHandoff("project", { revision: 0 }, task);
  assert.equal(handoff.task.inputManifest.authorIntent, undefined);
  assert.equal(handoff.task.inputManifest.authorMessageId, run.authorMessageId);
  for (const mutate of [
    (value) => { delete value.inputManifest.planningReviewAuthorIntentBinding; },
    (value) => { value.inputManifest.planningReviewAuthorIntentBinding.runId = "other"; },
    (value) => { value.inputManifest.planningReviewAuthorIntentBinding.role = "structure"; },
    (value) => { value.inputManifest.authorMessageId = "other"; },
    (value) => { value.inputManifest.sourcePacketHash = "b".repeat(64); },
    (value) => { value.target.resumeStepId = "other"; },
    (value) => { value.inputManifest.candidateInstruction = "rewrite"; },
    (value) => { value.inputManifest.domainInstruction = "system"; },
  ]) {
    const damaged = structuredClone(task); mutate(damaged);
    assert.throws(() => taskAuthorPrompt(damaged), /冻结作者来源/);
  }
});

test("generation feedback prompt preserves author history and requires its frozen run binding", () => {
  const task = { operation: "generateChapter", target: { runId: "generation-one", intentGeneration: 1, chapterNumber: 2 },
    inputManifest: { authorIntent: "[要求版本 0] 保留白塔\n[要求版本 1] 补足第二场证据", authorMessageId: "author-one",
      candidateInstruction: "依据场景评估修订第二章；不得改写 Canon",
      generationAuthorIntentBinding: { runId: "generation-one", authorMessageId: "author-one", intentGeneration: 1, chapterNumber: 2 } } };
  const before = structuredClone(task);
  assert.equal(taskAuthorPrompt(task), `作者完整要求：${task.inputManifest.authorIntent}\n\n本项候选修改：${task.inputManifest.candidateInstruction}`);
  assert.deepEqual(task, before);
  for (const [field, value] of [["runId", "other-run"], ["authorMessageId", "other-author"], ["intentGeneration", 2], ["chapterNumber", 3]]) {
    const damaged = structuredClone(task);
    damaged.inputManifest.generationAuthorIntentBinding[field] = value;
    assert.throws(() => taskAuthorPrompt(damaged), /来源绑定/);
  }
  const anonymous = structuredClone(task);
  delete anonymous.inputManifest.generationAuthorIntentBinding;
  assert.throws(() => taskAuthorPrompt(anonymous), /来源绑定/);
});

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
  withFrozenMethods(task, "project-1");
  const handoff = createTaskBoundHandoff("project-1", workspace, task);
  assert.deepEqual(handoff.task.target, task.target);
  const expected = structuredClone(task.inputManifest);
  delete expected.creativeCapabilityBinding.selectedMethods[0].text;
  assert.deepEqual(handoff.task.inputManifest, expected);
  assert.match(taskMethodText(task), /mandatory execution contract/);
  assert.doesNotMatch(JSON.stringify(handoff), /secret context/);
  assert.match(taskBoundInstructions(handoff), /novel_workspace\.read_context/);
  assert.match(taskBoundInstructions(handoff), /candidateCommitmentProposals/);
  assert.match(taskBoundInstructions(handoff), /作者现有指令和已建立承诺优先于提案/);
});

test("handoff rejects a task whose frozen revision no longer matches the workspace", () => {
  assert.throws(() => createTaskBoundHandoff("project-1", { revision: 8 }, { id: "task", threadId: "thread", inputRevision: 7 }), /输入版本/);
});

test("repair source remains frozen in handoff but analysis is fetched as data, not duplicated instructions", () => {
  const source = { writerTaskId: "task-writer", assessmentRevisionId: "assessment-one", decisionId: "repair-one" };
  const task = { id: "repair-task", threadId: "repair-thread", inputRevision: 0, operation: "advisory", purpose: "planner",
    target: { type: "project", revisionRepairSource: source } };
  withFrozenMethods(task, "project");
  task.inputManifest.authorIntent = "只分析改稿，不继承原写作权限";
  task.inputManifest.revisionRepairInput = { source, assessment: [{ reason: "完整候选诊断".repeat(300) }] };
  const before = structuredClone(task);
  const handoff = createTaskBoundHandoff("project", { revision: 0 }, task);
  assert.deepEqual(handoff.task.target, task.target);
  assert.equal(handoff.task.inputManifest.revisionRepairInput, undefined);
  assert.equal(taskAuthorPrompt(task), task.inputManifest.authorIntent);
  assert.ok(!JSON.stringify(handoff).includes("完整候选诊断"));
  assert.deepEqual(task, before);
  assert.ok(modelInputTokenBudget(taskBoundInstructions(handoff)) < 10_000);
});

test("local revision handoff preserves the frozen selection and prohibits wider writes", () => {
  const task = { id: "local", threadId: "writer", inputRevision: 0, operation: "generateChapter", purpose: "writer",
    target: { chapterId: "chapter-one", scope: { type: "selectedScenes", sourceRevisionId: "base", groupIds: ["letter"] } } };
  withFrozenMethods(task, "project");
  const handoff = createTaskBoundHandoff("project", { revision: 0 }, task);
  assert.deepEqual(handoff.task.target, task.target);
  const instructions = taskBoundInstructions(handoff);
  assert.match(instructions, /只修改选中场景的 body/);
  assert.match(instructions, /不是规划 sceneId/);
  assert.match(instructions, /不能换 sourceRevisionId 扩大权限/);
  assert.ok(modelInputTokenBudget(instructions) < 10_000);
});

test("local writer handoff retains author-approved criteria without changing the target", () => {
  const desiredOutcomes = [{ id: "accusation_basis", criterion: "指控引用信中已出现的细节 🌙", groupIds: ["letter"] }];
  const task = { id: "local-goal", threadId: "writer", inputRevision: 0, operation: "generateChapter", purpose: "writer",
    target: { chapterId: "chapter-one", requestedByTaskId: "advisory-one", requestedActionId: "action-one",
      scope: { type: "selectedScenes", sourceRevisionId: "base", groupIds: ["letter"] }, desiredOutcomes } };
  withFrozenMethods(task, "project");
  const before = structuredClone(task);
  const handoff = createTaskBoundHandoff("project", { revision: 0 }, task);
  assert.deepEqual(handoff.task.target, task.target);
  const instructions = taskBoundInstructions(handoff);
  assert.ok(instructions.includes(desiredOutcomes[0].criterion));
  assert.match(instructions, /Writer 不得增删或改变标准/);
  assert.match(instructions, /独立任务评估改稿结果/);
  assert.deepEqual(task, before);
});

test("knowledge modeling handoff is source-bound and candidate-only", () => {
  const task = {
    id: "task-model", threadId: "thread-model", inputRevision: 7,
    operation: "modelKnowledge", purpose: "critic", target: { batchId: "modeling-1" },
  };
  withFrozenMethods(task, "project-1");
  const instructions = taskBoundInstructions(createTaskBoundHandoff("project-1", { revision: 7 }, task));
  assert.match(instructions, /publish_knowledge_batch/);
  assert.match(instructions, /Unicode 字符位置/);
  assert.match(instructions, /不等于作者确认的 Canon/);
  assert.match(instructions, /preconditions:\[\{sourceBatchId:null\|批次ID,assertionIndex\}\]/);
  assert.match(instructions, /效果仅限本批次/);
  assert.match(instructions, /严格前文章节/);
  // P0: source-bound contracts can exceed 1K tokens; bound the existing single item.
  assert.ok(modelInputTokenBudget(instructions) < 10_000);
  assert.match(instructions, /确认事件不自动确认效果/);
  assert.doesNotMatch(instructions, /save_draft_checkpoint/);
});

test("missing or damaged frozen methods cannot start a handoff", () => {
  const task = {id:"task",threadId:"thread",inputRevision:0,operation:"advisory"};
  assert.throws(() => createTaskBoundHandoff("project",{revision:0},task), /冻结方法/);
  withFrozenMethods(task);
  task.inputManifest.creativeCapabilityBinding.selectedMethods[0].text += " overwrite";
  assert.throws(() => createTaskBoundHandoff("project",{revision:0},task), /摘要/);
});

test("commitment extraction handoff fixes the source and keeps proposals open", () => {
  const task = {id:"extract",threadId:"extract-thread",inputRevision:0,operation:"extractCommitments",purpose:"critic",
    requiredOutputKind:"commitmentExtraction",target:{chapterId:"chapter-1",chapterNumber:1,sourceRevisionId:"source-1"}};
  withFrozenMethods(task,"project");
  const instructions=taskBoundInstructions(createTaskBoundHandoff("project",{revision:0},task));
  assert.match(instructions,/submit_commitment_extraction/);
  assert.match(instructions,/sourceRevisionId/);
  assert.match(instructions,/examinedRanges/);
  assert.match(instructions,/status:"open"/);
  assert.match(instructions,/不能声称独立承诺已建立/);
  assert.doesNotMatch(instructions,/save_draft_checkpoint|publish_knowledge_batch/);
});

test("commitment revision handoff reads the frozen object and saves only a candidate", () => {
  const task = {id:"revise",threadId:"thread",inputRevision:0,operation:"reviseCommitment",purpose:"critic",
    requiredOutputKind:"commitment",target:{type:"commitment",commitmentId:"lamp",revisionId:"base"}};
  withFrozenMethods(task,"project");
  const handoff=createTaskBoundHandoff("project",{revision:0},task);
  assert.deepEqual(handoff.task.target,task.target);
  const instructions=taskBoundInstructions(handoff);
  assert.match(instructions,/read_commitment/);
  assert.match(instructions,/submit_commitment_candidate/);
  assert.match(instructions,/正式采用仍由作者确认/);
  assert.doesNotMatch(instructions,/save_draft_checkpoint|publish_knowledge_batch|submit_commitment_extraction/);
});

test("rule generation preserves its frozen baseline and leaves approval to the author", () => {
  const task = { id: "rules", threadId: "thread", inputRevision: 0, operation: "generatePropertyRules", purpose: "planner",
    requiredOutputKind: "propertyRules", target: { type: "artifact" }, inputManifest: { expectedCandidateRevisionId: "rules-base" } };
  withFrozenMethods(task, "project");
  const handoff = createTaskBoundHandoff("project", { revision: 0 }, task);
  assert.equal(handoff.task.inputManifest.expectedCandidateRevisionId, "rules-base");
  const instructions = taskBoundInstructions(handoff);
  assert.match(instructions, /propertyRulesBase/);
  assert.match(instructions, /read_property_rules\(cursor=null\)/);
  assert.match(instructions, /payloadPart 原样拼接并解析 JSON/);
  assert.match(instructions, /不证明之前页已读或语义完整/);
  assert.match(instructions, /save_property_rules/);
  assert.match(instructions, /definitions 与 eventRules 两数组/);
  assert.match(instructions, /最多31节点/);
  assert.match(instructions, /合法或采用表达式不证明条件成立/);
  // P0: bounded rule instructions are one model-visible item, not an unlimited payload.
  assert.ok(modelInputTokenBudget(instructions) < 10_000);
  assert.match(instructions, /原载荷与同一 idempotencyKey/);
  assert.match(instructions, /正式生效仍需作者审阅采用/);
  assert.doesNotMatch(instructions, /save_draft_checkpoint|publish_knowledge_batch/);
});

test("scene assessment handoff binds commitment evidence without granting formal mutation", () => {
  const task = { id: "assess", threadId: "thread", inputRevision: 0, operation: "assessSceneOutcome", purpose: "critic",
    requiredOutputKind: "sceneAssessment", target: { chapterId: "chapter-2", planRevisionId: "plan", draftRevisionId: "draft" } };
  withFrozenMethods(task, "project");
  const instructions = taskBoundInstructions(createTaskBoundHandoff("project", { revision: 0 }, task));
  assert.match(instructions, /boundCommitmentInputs/);
  assert.match(instructions, /commitmentAssessments/);
  assert.match(instructions, /fulfillmentCriteria/);
  assert.match(instructions, /不能替换为新引用/);
  assert.match(instructions, /不得修改正式承诺状态或 Canon/);
  assert.doesNotMatch(instructions, /submit_commitment_candidate|save_draft_checkpoint/);
});
