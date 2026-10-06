import assert from "node:assert/strict";
import { test } from "node:test";

import { createTaskBoundHandoff, taskAuthorPrompt, taskBoundInstructions, taskExecutionPrompt, taskMethodText } from "../workflow-handoff.mjs";
import { withFrozenMethods } from "./fixtures/frozen-methods.mjs";
import { modelInputTokenBudget } from "../../shared/author-input-budget.mjs";
import { reviewTask } from "./fixtures/planning-review-task.mjs";

test("outline handoff preserves whole-book and local author scopes while separating their narrative roles", () => {
  for (const [authorIntent, target, candidateInstruction] of [
    ["起草整部故事及三卷安排，结局尚未决定。", { type: "artifact" }, null],
    ["保留全书主线，只调整第二卷的中段转折。", { type: "artifact", targetNodeId: "volume-two" }, "只调整 volume-two，结局保持待定。"],
  ]) {
    const task = { id: "outline", threadId: "thread", inputRevision: 2, operation: "generateOutline", purpose: "planner",
      requiredOutputKind: "outline", target, inputManifest: { expectedCandidateRevisionId: "outline-source", authorIntent,
        ...(candidateInstruction ? { candidateInstruction, authorIntentBinding: { actionId: "action-two" } } : {}) } };
    withFrozenMethods(task, "project");
    const before = structuredClone(task);
    const handoff = createTaskBoundHandoff("project", { revision: 2 }, task);
    const bound = taskBoundInstructions(handoff);
    const semantics = bound.slice(bound.indexOf("本任务职责："), bound.indexOf("所有保存遵守"));
    assert.ok(semantics && modelInputTokenBudget(semantics) < 1_000);
    assert.ok(bound.includes("book.purpose 写全书主线冲突与目标"));
    assert.ok(bound.includes("summary 写起因、推进、转折、高潮、收束的因果脉络"));
    assert.ok(bound.includes("unit 写具体安排"));
    assert.ok(bound.includes("局部范围不强写全书"));
    assert.ok(bound.includes("未定结局不虚构定案"));
    assert.deepEqual(handoff.task.target, target);
    assert.equal(handoff.task.inputManifest.expectedCandidateRevisionId, "outline-source");
    assert.ok(taskExecutionPrompt(task).includes(authorIntent));
    if (candidateInstruction) assert.ok(taskExecutionPrompt(task).includes(candidateInstruction));
    assert.deepEqual(task, before);
  }
});

test("brief worker keeps frozen source reading and candidate CAS distinct", () => {
  const task = { id: "brief", threadId: "thread", inputRevision: 2, operation: "generateBrief", purpose: "planner",
    target: { type: "artifact", requestedByTaskId: "parent", requestedActionId: "action" },
    inputManifest: { expectedCandidateRevisionId: "newer-candidate", authorIntent: "只调整简报结局方向", candidateInstruction: "完整读取原简报，保留其他内容" } };
  withFrozenMethods(task, "project");
  const before = structuredClone(task);
  const handoff = createTaskBoundHandoff("project", { revision: 2 }, task);
  const instructions = taskBoundInstructions(handoff);
  assert.ok(instructions.includes('read_task_source'));
  assert.ok(instructions.includes("完整读取冻结原简报"));
  assert.ok(instructions.includes("冻结资料目录"));
  assert.ok(instructions.includes("普通问答不默认遍历全书"));
  assert.equal(handoff.task.inputManifest.expectedCandidateRevisionId, "newer-candidate");
  assert.deepEqual(task, before);
});

test("first plan handoff uses current chapters shape and preserves the actual nullable CAS and optional starting chapter", () => {
  for (const target of [{ type: "artifact" }, { type: "artifact", planFromChapter: 30 }]) {
    const task = { id: "plan", threadId: "thread", inputRevision: 0, operation: "generatePlan", purpose: "planner",
      requiredOutputKind: "plan", target, inputManifest: { expectedCandidateRevisionId: null, authorIntent: "只规划接下来三章，不正式采用。" } };
    withFrozenMethods(task, "project");
    const before = structuredClone(task);
    const handoff = createTaskBoundHandoff("project", { revision: 0 }, task);
    const bound = taskBoundInstructions(handoff), prompt = taskExecutionPrompt(task);
    assert.deepEqual(task, before);
    assert.deepEqual(handoff.task.target, target);
    assert.equal(handoff.task.inputManifest.expectedCandidateRevisionId, null);
    assert.ok(bound.includes("JSON null不得改成字符串"));
    assert.ok(bound.includes("候选不等于作者采用"));
    assert.ok(prompt.includes("target.planFromChapter 存在时"));
    assert.ok(prompt.includes("未提供时按作者要求和实际章节核对起章号"));
    assert.ok(prompt.includes("全计划承诺引用总数最多64项"));
    assert.ok(prompt.includes(task.inputManifest.authorIntent));
  }
});

test("review prompt preserves the real author root and rejects anonymous or cross-seat bindings", () => {
  const run = { sourceTaskId: null, sourceIntentRevision: null, id: "planning-review-test", workTaskId: "work-persisted-review-owner", projectId: "project", artifactKind: "outline", revisionId: "outline-one",
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
  ]) {
    const damaged = structuredClone(task); mutate(damaged);
    assert.throws(() => taskAuthorPrompt(damaged), /冻结作者来源/);
  }
});

test("conversation review preserves bounded author history and requires a paired source revision", () => {
  const run = { sourceTaskId: "discussion-one", sourceIntentRevision: 2, id: "review-history", workTaskId: "conversation-work", projectId: "project", artifactKind: "outline", revisionId: "outline-one",
    inputRevision: 3, authorIntent: "[要求版本 0] 保留白塔。\n" + "补充人物动机。".repeat(650), authorMessageId: "author-one", sourcePacketHash: "a".repeat(64) };
  const task = reviewTask(run, "character", { threadId: "thread" }, "task-review");
  assert.equal(taskAuthorPrompt(task), run.authorIntent);
  for (const patch of [{ sourceTaskId: null }, { sourceIntentRevision: null }, { sourceIntentRevision: -1 }]) {
    const damaged = structuredClone(task);
    Object.assign(damaged.inputManifest.planningReviewAuthorIntentBinding, patch);
    assert.throws(() => taskAuthorPrompt(damaged), /冻结作者来源/);
  }
  const oversized = structuredClone(task);
  oversized.inputManifest.authorIntent = "字".repeat(8001);
  assert.throws(() => taskAuthorPrompt(oversized), /冻结作者来源/);
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
  assert.match(taskBoundInstructions(handoff), /read_context/);
  assert.match(taskBoundInstructions(handoff), /候选/);
  assert.match(taskBoundInstructions(handoff), /冻结target/);
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
  assert.match(instructions, /只改选中组正文/);
  assert.match(instructions, /body或完整sceneGroups之一/);
  assert.match(instructions, /必须提交全部原组/);
  assert.ok(modelInputTokenBudget(instructions) < 10_000);
});

test("whole chapter handoff preserves frozen source selection independently of nullable save CAS", () => {
  for (const expectedCandidateRevisionId of [null, "candidate-original"]) {
    const task = { id: "whole", threadId: "writer", inputRevision: 3, operation: "generateChapter", purpose: "writer",
      target: { chapterId: "chapter-one", chapterNumber: 1, readingChapter: 1, sceneOrder: 1,
        povCharacterId: "person-one", scope: { type: "wholeChapter" } },
      inputManifest: { authorIntent: "重新写好第一章，保留线索，把场景归属写清。", expectedCandidateRevisionId } };
    withFrozenMethods(task, "project");
    const before = structuredClone(task);
    const handoff = createTaskBoundHandoff("project", { revision: 3 }, task);
    const bound = taskBoundInstructions(handoff);
    assert.deepEqual(handoff.task.target, task.target);
    assert.equal(handoff.task.inputManifest.expectedCandidateRevisionId, expectedCandidateRevisionId);
    assert.match(bound, /本章合法sceneId/);
    assert.match(bound, /body或完整sceneGroups之一/);
    assert.match(bound, /完整sceneGroups/);
    assert.match(bound, /同任务预算内纠正完整payload/);
    assert.ok(taskExecutionPrompt(task).includes(task.inputManifest.authorIntent));
    assert.ok(modelInputTokenBudget(bound) < 9_000);
    assert.deepEqual(task, before);
    task.target.scope = { type: "throughScene", endSceneOrder: 1 };
    assert.doesNotMatch(taskBoundInstructions(createTaskBoundHandoff("project", { revision: 3 }, task)), /writerBaselinePassages/);
  }
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
  assert.match(instructions, /不得增删或改变/);
  assert.match(instructions, /明确评估/);
  assert.deepEqual(task, before);
});

test("knowledge modeling handoff is source-bound and candidate-only", () => {
  const task = {
    id: "task-model", threadId: "thread-model", inputRevision: 7,
    operation: "modelKnowledge", purpose: "critic", target: { batchId: "modeling-1" },
  };
  withFrozenMethods(task, "project-1");
  const before = structuredClone(task);
  const instructions = taskBoundInstructions(createTaskBoundHandoff("project-1", { revision: 7 }, task));
  assert.match(instructions, /完整读取本批次冻结正文/);
  assert.match(instructions, /Unicode字符位置/);
  assert.match(instructions, /不等于作者确认的Canon/);
  assert.match(instructions, /前提只来自本批次或严格前文章节/);
  assert.match(instructions, /确认事件不自动确认效果/);
  assert.match(instructions, /已有目录ID与跨源引用原样保留/);
  assert.match(instructions, /发布committed后立即complete_task，不再查询或重发/);
  assert.match(instructions, /参数含绑定身份总量不超过8192UTF8B/);
  assert.match(instructions, /超限停止说明，不截断必填数据或重复发布/);
  assert.match(instructions, /报告实际范围/);
  assert.doesNotMatch(instructions, /save_draft_checkpoint/);
  assert.ok(modelInputTokenBudget(instructions) < 10_000);
  assert.deepEqual(task, before);
  const writer = structuredClone(task); writer.operation = "generateChapter"; writer.purpose = "writer";
  withFrozenMethods(writer, "project-1");
  assert.doesNotMatch(taskBoundInstructions(createTaskBoundHandoff("project-1", { revision: 7 }, writer)), /参数含绑定身份总量不超过8192UTF8B/);
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
  assert.match(instructions,/完整报告一次原子替换/);
  assert.match(instructions,/sourceRevisionId/);
  assert.match(instructions,/examinedRanges/);
  assert.match(instructions,/完整报告一次原子替换/);
  assert.match(instructions,/完整报告/);
  assert.match(instructions,/最后回执candidateCount/);
  assert.match(instructions,/不累加/);
  assert.match(instructions,/resolve_story_evidence/);
  assert.match(instructions,/原文不足不能虚构范围/);
  assert.match(instructions,/整章Unicode位置/);
  assert.match(instructions,/空结果不证明没有伏笔/);
  assert.doesNotMatch(instructions,/save_draft_checkpoint|publish_knowledge_batch/);
});

test("commitment revision handoff reads the frozen object and saves only a candidate", () => {
  const task = {id:"revise",threadId:"thread",inputRevision:0,operation:"reviseCommitment",purpose:"critic",
    requiredOutputKind:"commitment",target:{type:"commitment",commitmentId:"lamp",revisionId:"base"}};
  withFrozenMethods(task,"project");
  const handoff=createTaskBoundHandoff("project",{revision:0},task);
  assert.deepEqual(handoff.task.target,task.target);
  const instructions=taskBoundInstructions(handoff);
  assert.match(instructions,/本承诺冻结基准/);
  assert.match(instructions,/resolve_story_evidence/);
  assert.match(instructions,/只保存规则候选|不确认正式承诺/);
  assert.match(instructions,/候选不等于作者采用/);
  assert.doesNotMatch(instructions,/save_draft_checkpoint|publish_knowledge_batch|submit_commitment_extraction/);
});

test("rule generation preserves its frozen baseline and leaves approval to the author", () => {
  const task = { id: "rules", threadId: "thread", inputRevision: 0, operation: "generatePropertyRules", purpose: "planner",
    requiredOutputKind: "propertyRules", target: { type: "artifact" }, inputManifest: { expectedCandidateRevisionId: "rules-base" } };
  withFrozenMethods(task, "project");
  const handoff = createTaskBoundHandoff("project", { revision: 0 }, task);
  assert.equal(handoff.task.inputManifest.expectedCandidateRevisionId, "rules-base");
  const instructions = taskBoundInstructions(handoff);
  assert.match(instructions, /冻结规则基准/);
  assert.match(instructions, /read_task_source/);
  assert.match(instructions, /拼接payloadPart并解析JSON/);
  assert.match(instructions, /不证明先前已读或语义完整/);
  assert.match(instructions, /只保存规则候选/);
  assert.match(instructions, /不确认属性值或事实/);
  // P0: bounded rule instructions are one model-visible item, not an unlimited payload.
  assert.ok(modelInputTokenBudget(instructions) < 10_000);
  assert.match(instructions, /候选不等于作者采用/);
  assert.doesNotMatch(instructions, /save_draft_checkpoint|publish_knowledge_batch/);
});

test("scene assessment handoff binds commitment evidence without granting formal mutation", () => {
  const task = { id: "assess", threadId: "thread", inputRevision: 0, operation: "assessSceneOutcome", purpose: "critic",
    requiredOutputKind: "sceneAssessment", target: { chapterId: "chapter-2", planRevisionId: "plan", draftRevisionId: "draft" } };
  withFrozenMethods(task, "project");
  const instructions = taskBoundInstructions(createTaskBoundHandoff("project", { revision: 0 }, task));
  assert.match(instructions, /taskSources.*未交付正文/);
  assert.match(instructions, /不是文学质量或正式承诺确认/);
  assert.doesNotMatch(instructions, /submit_commitment_candidate|save_draft_checkpoint/);
});

test("Work decision author instructions require exact frozen Native counterparts", () => {
  const parent = { id: "decision", threadId: "root", operation: "advisory",
    target: { type: "project", resumeStepId: "decision-step", workDecision: { workTaskId: "work", originTaskId: "author-root", sourceTaskId: "saved-child", fingerprint: "a".repeat(64) } },
    inputManifest: { authorIntent: "作者原始要求", candidateInstruction: "根据真实成果选择行动",
      authorIntentBinding: { sourceTaskId: "author-root", intentRevision: 1, actionId: null },
      workDecision: { workTaskId: "work", originTaskId: "author-root", sourceTaskId: "saved-child", rootThreadId: "root", decisionStepId: "decision-step", decisionTaskId: "decision" } } };
  const child = structuredClone(parent);
  child.id = "writer"; child.threadId = "writer-thread"; child.operation = "generateChapter";
  child.target = { type: "chapter", resumeStepId: "writer-step", chapterId: "chapter",
    workDecisionAction: { workTaskId: "work", decisionStepId: "decision-step", decisionTaskId: "decision", actionKey: "action" } };
  delete child.inputManifest.workDecision;
  child.inputManifest.workDecisionAction = { ...parent.inputManifest.workDecision, actionKey: "action", operation: "generateChapter", sourceRevisionId: "original" };
  const generation = structuredClone(parent);
  generation.target.runId = "run"; generation.target.intentGeneration = 2;
  delete generation.inputManifest.authorIntentBinding;
  generation.inputManifest.authorMessageId = "message";
  generation.inputManifest.generationAuthorIntentBinding = { runId: "run", intentGeneration: 2, authorMessageId: "message", chapterNumber: 3 };
  for (const task of [parent, child, generation]) {
    const before = structuredClone(task);
    assert.equal(taskAuthorPrompt(task), "作者完整要求：作者原始要求\n\n本项候选修改：根据真实成果选择行动");
    assert.deepEqual(task, before);
    for (const mutate of [
      value => { (value.inputManifest.workDecisionAction ?? value.inputManifest.workDecision).workTaskId = "foreign"; },
      value => { value.target.resumeStepId = null; },
      value => { (value.inputManifest.workDecisionAction ?? value.inputManifest.workDecision).rootThreadId = ""; },
      value => { value.inputManifest.workDecision = {}; value.inputManifest.workDecisionAction = {}; },
      value => { if (value.inputManifest.authorIntentBinding) value.inputManifest.authorIntentBinding.sourceTaskId = "foreign";
        else value.inputManifest.generationAuthorIntentBinding.runId = "foreign"; },
    ]) { const invalid = structuredClone(task); mutate(invalid); assert.throws(() => taskAuthorPrompt(invalid), /来源绑定/); }
  }
});
