import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import test from "node:test";
import { build } from "esbuild";

test("public activity excludes reasoning and keeps author commentary distinct from tool parameters", async () => {
  const directory = await mkdtemp(join(tmpdir(), "work-conversation-"));
  try {
    const output = join(directory, "logic.mjs");
    await build({ entryPoints: [new URL("../../src/work-conversation.ts", import.meta.url).pathname],
      outfile: output, bundle: true, format: "esm", platform: "node" });
    const { currentWorkError, conversationActivity, conversationWorkSteps, sameConversationFailure, taskContainsTurn, conversationItems, attemptContainsRun, primaryReply } = await import(pathToFileURL(output).href);
    const failedAttempt = { id: "old", status: "failed", error: "old failure" };
    const failedWork = { status: "blocked", error: null, steps: [{ status: "blocked", currentAttemptId: "old", attempts: [failedAttempt] }] };
    assert.equal(currentWorkError(failedWork), "old failure");
    assert.equal(currentWorkError({ ...failedWork, status: "completed" }), null);
    assert.equal(currentWorkError({ ...failedWork, steps: [{ ...failedWork.steps[0], status: "superseded" }] }), null);
    assert.equal(currentWorkError({ ...failedWork, steps: [{ ...failedWork.steps[0], currentAttemptId: "new", attempts: [failedAttempt, { id: "new", status: "completed", error: null }] }] }), null);
    assert.equal(currentWorkError({ ...failedWork, error: "current task failure" }), "current task failure");
    const group = (id, starts) => ({ id, steps: [{ attempts: starts.map((startedAt, i) => ({ id: `${id}-${i}`, startedAt } )) }] });
    const resumed = group("original", [1, 30]);
    const oldDiscussion = group("old-discussion", [20]);
    assert.deepEqual(conversationItems([oldDiscussion, resumed], null).map(item => item.task.id), ["old-discussion", "original"],
      "following the conversation tail must show the resumed Work, not an older discussion");
    const commentary = { id: "public", kind: "system", title: "Agent 进度", text: "我正在检查第二章的时间线。", messagePhase: "commentary", status: "completed" };
    const tool = { id: "tool", kind: "tool", title: "读取本章正文", detail: '{"secretFixtureArgument":true}', status: "running" };
    const reasoning = { id: "reason", kind: "system", sourceType: "reasoning", title: "正在分析", status: "running" };
    const run = { steps: [reasoning, commentary, tool], answer: "", state: "running" };
    assert.deepEqual(conversationWorkSteps(run), [commentary, tool]);
    assert.deepEqual(conversationActivity(run), { title: "读取本章正文", commentary: commentary.text });
    assert.deepEqual(conversationActivity({ ...run, steps: [reasoning] }), { title: "正在分析", commentary: "" });
    assert.deepEqual(conversationActivity({ ...run, steps: [] }), { title: "正在处理", commentary: "" });
    const maintenance = { id: "context-compaction", kind: "system", sourceType: "contextCompaction", title: "压缩上下文", status: "running" };
    const failedMaintenance = { ...maintenance, id: "failed-maintenance", status: "failed" };
    assert.deepEqual(conversationWorkSteps({ steps: [maintenance, failedMaintenance, commentary, tool] }), [maintenance, failedMaintenance, commentary, tool]);
    const untypedMaintenance = { ...maintenance, sourceType: undefined };
    assert.deepEqual(conversationWorkSteps({ steps: [untypedMaintenance] }), []);
    assert.equal(conversationActivity({ ...run, steps: [untypedMaintenance] }).title, "正在处理");
    assert.equal(conversationActivity({ ...run, steps: [maintenance] }).title, "正在整理对话，稍后继续");
    assert.equal(conversationActivity({ ...run, state: "completed", steps: [maintenance] }).title, "正在处理");
    assert.equal(conversationActivity({ ...run, steps: [{ ...maintenance, id: "retry:turn", title: "正在重试当前请求" }] }).title, "正在重试请求，稍后继续");
    assert.equal(conversationActivity({ ...run, steps: [{ ...maintenance, id: "retry:turn", title: "正在重试当前请求" }, tool] }).title, tool.title);
    const steer = { id: "steer:1", kind: "system", title: "已追加创作指令", detail: "补充要求", status: "completed" };
    assert.deepEqual(conversationWorkSteps({ steps: [maintenance, steer] }), [maintenance, steer]);
    assert.deepEqual(conversationActivity({ ...run, answer: "已有部分公开回复", steps: [commentary, { ...tool, status: "completed" }, reasoning] }),
      { title: "正在分析", commentary: commentary.text });
    assert.equal(sameConversationFailure("Error invoking remote method 'agent:resume': Error: precise failure", "precise failure"), true);
    assert.equal(sameConversationFailure("provider_output_item_limit; outputItem={\"utf8Bytes\":1}", "provider_output_item_limit; outputItem={\"utf8Bytes\":2}"), false);
    assert.equal(sameConversationFailure(null, "failure"), false);
    const task = { steps: [{ attempts: [{ threadId: "thread", turnId: "turn" }] }] };
    assert.equal(taskContainsTurn(task, { threadId: "thread", turnId: "turn" }), true);
    assert.equal(taskContainsTurn(task, { threadId: "other", turnId: "turn" }), false);
    const unboundTask = { id: "work", steps: [{ attempts: [{ id: "attempt", threadId: "thread", turnId: null, startedAt: 1 }] }] };
    const cancelled = { state: "cancelled", taskId: "attempt", threadId: "thread", turnId: "turn", history: [], startedAt: 1000 };
    const conversation = { run: cancelled };
    assert.equal(taskContainsTurn(unboundTask, cancelled), true);
    assert.equal(attemptContainsRun(unboundTask.steps[0].attempts[0], cancelled), true);
    assert.deepEqual(conversationItems([unboundTask], conversation).map(item => ({ kind: item.kind, current: item.current })),
      [{ kind: "task", current: true }]);
    const originReply = { ...cancelled, id: "initial", taskId: "parent-first", threadId: "root", turnId: "initial-turn", createdAt: 4000 };
    const savedReply = { ...cancelled, id: "saved", taskId: "writer-first", threadId: "writer", turnId: "writer-turn", createdAt: 2000 };
    const decisionRun = { ...cancelled, taskId: "decision-next", threadId: "root", turnId: "decision-turn", state: "running", history: [savedReply, originReply] };
    const orderedWork = { ...unboundTask, steps: [
      { attempts: [{ id: originReply.taskId, threadId: "root", turnId: originReply.turnId, startedAt: 1 }] },
      { attempts: [{ id: savedReply.taskId, threadId: "writer", turnId: savedReply.turnId, startedAt: 2 }] },
      { attempts: [{ id: decisionRun.taskId, threadId: "root", turnId: decisionRun.turnId, startedAt: 3 }] },
    ] };
    assert.deepEqual(conversationItems([orderedWork], { run: decisionRun }).map(item => ({ kind: item.kind, current: item.current, history: item.history.map(entry => entry.id) })),
      [{ kind: "task", current: true, history: ["initial", "saved"] }], "initial registration must precede saved writer and current decision, regardless of archive/completion time");
    const savedTask = { ...unboundTask, steps: [{ attempts: [{ ...unboundTask.steps[0].attempts[0], turnId: "turn" }] }] };
    for (const foreign of [{ ...cancelled, taskId: "other" }, { ...cancelled, threadId: "other" }, { ...cancelled, turnId: "other" }]) {
      assert.equal(taskContainsTurn(savedTask, foreign), false);
      assert.deepEqual(conversationItems([savedTask], { run: foreign }).map(item => item.kind), ["task", "current"]);
    }
    const history = { ...cancelled, id: "summary", createdAt: 1000 };
    assert.deepEqual(conversationItems([unboundTask], { run: { state: "idle", history: [history] } }).map(item => ({ kind: item.kind, history: item.history?.map(entry => entry.id) })),
      [{ kind: "task", history: ["summary"] }]);
    assert.equal(taskContainsTurn(savedTask, { threadId: "thread", turnId: "turn" }), true);
    assert.equal(taskContainsTurn(unboundTask, { threadId: "thread", turnId: "turn" }), false);
    const writer = { id: "writer", threadId: "thread", turnId: "writer-turn", status: "completed" };
    const knowledge = { id: "knowledge", threadId: "thread", turnId: null, status: "cancelled" };
    const workflow = { steps: [{ operation: "generateChapter", attempts: [writer] }, { operation: "modelKnowledge", attempts: [knowledge] }] };
    const analysisRun = { ...cancelled, taskId: "knowledge", turnId: "analysis-turn" };
    assert.equal(taskContainsTurn(workflow, analysisRun), true);
    assert.deepEqual(primaryReply(workflow), { attempt: writer, operation: "generateChapter" });
    const decision = { id: "decision-last", threadId: "root", turnId: "decision-turn", status: "completed" };
    const coordinated = { id: "work", threadId: "root", steps: [{ operation: "advisory", target: {}, attempts: [{ ...writer, id: "initial" }] },
      { operation: "generateChapter", target: {}, attempts: [writer] },
      { operation: "advisory", currentAttemptId: decision.id, target: { workDecision: { workTaskId: "work" } }, attempts: [decision] }] };
    assert.deepEqual(primaryReply(coordinated), { attempt: decision, operation: "advisory" }, "The last persisted coordinator explains the actual outcome after a saved writer");
    assert.deepEqual(primaryReply({ ...coordinated, steps: [...coordinated.steps.slice(0, 2), { ...coordinated.steps[2], attempts: [{ ...decision, status: "failed" }] }] }),
      { attempt: { ...decision, status: "failed" }, operation: "advisory" }, "A final coordination failure remains the primary reply instead of an earlier saved draft");
    const analysis = { ...knowledge, status: "completed" };
    assert.deepEqual(primaryReply({ steps: [{ operation: "advisory", attempts: [writer] },
      { operation: "modelKnowledge", attempts: [analysis] }] }), { attempt: analysis, operation: "modelKnowledge" });
    assert.equal(attemptContainsRun(primaryReply(workflow).attempt, analysisRun), false);
    assert.equal(attemptContainsRun(primaryReply(workflow).attempt, { ...analysisRun, taskId: "writer", turnId: "writer-turn" }), true);
    assert.equal(attemptContainsRun(writer, { ...analysisRun, taskId: "writer" }), false);
    assert.equal(attemptContainsRun(writer, { ...analysisRun, taskId: "writer", threadId: "foreign", turnId: "writer-turn" }), false);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test("generation descriptions fold only for the exact clean primary saved result", async () => {
  const directory = await mkdtemp(join(tmpdir(), "work-generation-description-"));
  try {
    const output = join(directory, "logic.mjs");
    await build({ entryPoints: [new URL("../../src/work-conversation.ts", import.meta.url).pathname],
      outfile: output, bundle: true, format: "esm", platform: "node" });
    const { generatedReplyDescription, completedDeliveryPresentation } = await import(pathToFileURL(output).href);
    const attempt = { id: "saved-attempt", threadId: "writer-thread", turnId: "writer-turn", status: "completed", error: null };
    const task = { id: "work", projectId: "project", status: "completed", deliveryStatus: "ready_for_review", error: null,
      deliveryReport: { pendingActionIds: [], unsettledAttemptIds: [], evidence: [
        { stepId: "writer", attemptId: attempt.id, kind: "outline", referenceId: "saved-outline", state: "recorded" }] },
      steps: [{ id: "writer", operation: "generateOutline", expectedOutputKind: "outline", status: "completed", currentAttemptId: attempt.id,
        target: { type: "artifact" }, attempts: [attempt] }] };
    const result = { projectId: "project", workTaskId: "work", stepId: "writer", attemptId: attempt.id, referenceId: "saved-outline", kind: "outline" };
    const binding = { stepId: "writer", attemptId: attempt.id };
    assert.deepEqual(generatedReplyDescription(task, [result]), binding);
    assert.equal(completedDeliveryPresentation(task), "current");
    assert.equal(generatedReplyDescription(task, []), null);
    const historical = structuredClone(task);
    historical.deliveryStatus = "partial";
    historical.deliveryReport.evidence[0].state = "superseded";
    assert.deepEqual(generatedReplyDescription(historical, []), binding);
    assert.equal(completedDeliveryPresentation(historical), "updated");
    assert.equal(historical.deliveryReport.evidence[0].referenceId, "saved-outline");
    historical.steps.push({ id: "knowledge", operation: "modelKnowledge", expectedOutputKind: "knowledgeBatch", status: "completed", currentAttemptId: "knowledge-attempt",
      target: {}, attempts: [{ ...attempt, id: "knowledge-attempt" }] });
    historical.deliveryReport.evidence.push({ stepId: "knowledge", attemptId: "knowledge-attempt", kind: "knowledgeBatch", referenceId: "original-batch", state: "recorded" });
    assert.deepEqual(generatedReplyDescription(historical, []), binding);
    const withAdvisory = structuredClone(historical);
    withAdvisory.steps.unshift({ id: "discussion", operation: "advisory", expectedOutputKind: "completionReport", status: "completed",
      currentAttemptId: "discussion-attempt", target: {}, attempts: [{ ...attempt, id: "discussion-attempt" }] });
    withAdvisory.deliveryReport.evidence.push({ stepId: "discussion", attemptId: "discussion-attempt", kind: "completionReport", referenceId: "discussion-attempt", state: "recorded" });
    assert.equal(completedDeliveryPresentation(withAdvisory), "updated");
    assert.deepEqual(generatedReplyDescription(withAdvisory, []), binding);
    withAdvisory.deliveryReport.evidence.at(-1).referenceId = "other-attempt";
    assert.equal(completedDeliveryPresentation(withAdvisory), null);
    for (const mutate of [
      candidate => { candidate.deliveryReport.evidence[1].state = "missing"; },
      candidate => { candidate.deliveryReport.evidence[1].state = "pending"; },
      candidate => { candidate.deliveryReport.evidence[0].referenceId = null; },
      candidate => { candidate.deliveryReport.pendingActionIds.push("action"); },
      candidate => { candidate.deliveryReport.unsettledAttemptIds.push(attempt.id); },
      candidate => { candidate.steps[1].status = "queued"; },
      candidate => { candidate.steps[0].attempts[0].error = "actual failure"; },
      candidate => { candidate.steps[0].attempts[0].status = "failed"; },
      candidate => { candidate.steps[0].attempts[0].status = "cancelled"; },
      candidate => { candidate.steps[0].target.revisionRepairSource = { chapterId: "chapter", draftRevisionId: "source" }; },
    ]) {
      const candidate = structuredClone(historical); mutate(candidate);
      assert.equal(completedDeliveryPresentation(candidate), null);
      assert.equal(generatedReplyDescription(candidate, []), null);
    }
    for (const mutate of [
      candidate => { candidate.steps[0].currentAttemptId = "other-attempt"; },
      candidate => { candidate.deliveryReport.evidence[0].attemptId = "other-attempt"; },
      candidate => { candidate.deliveryReport.evidence[0].kind = "brief"; },
      candidate => { candidate.deliveryReport.evidence.push(structuredClone(candidate.deliveryReport.evidence[0])); },
      candidate => { candidate.deliveryReport.evidence[1].attemptId = "other-attempt"; },
      candidate => { candidate.deliveryReport.evidence[1].stepId = "missing-step"; },
      candidate => { candidate.deliveryReport.evidence.splice(1, 1); },
      candidate => { candidate.deliveryReport.evidence[1].kind = "brief"; },
      candidate => { candidate.steps[1].currentAttemptId = "other-attempt"; },
      candidate => { candidate.deliveryReport.evidence[1].referenceId = "x".repeat(161); },
      candidate => { candidate.deliveryReport.evidence[1].referenceId = "unsafe\nref"; },
    ]) {
      const candidate = structuredClone(historical); mutate(candidate);
      assert.equal(completedDeliveryPresentation(candidate), null);
      assert.equal(generatedReplyDescription(candidate, []), null);
    }
    for (const [operation, kind] of [["generateBrief", "brief"], ["generateOutline", "outline"], ["generatePlan", "plan"],
      ["generateChapter", "chapter"], ["generatePropertyRules", "propertyRules"]]) {
      const candidate = structuredClone(task); candidate.steps[0].operation = operation;
      candidate.steps[0].expectedOutputKind = kind === "chapter" ? "draft" : kind;
      candidate.deliveryReport.evidence[0].kind = kind === "chapter" ? "draft" : kind;
      assert.deepEqual(generatedReplyDescription(candidate, [{ ...result, kind }]), binding);
    }
    for (const [key, value] of [["projectId", "other-project"], ["workTaskId", "other-work"], ["stepId", "other-step"],
      ["attemptId", "other-attempt"], ["referenceId", "other-revision"], ["kind", "knowledge"], ["kind", "chapter"]]) {
      assert.equal(generatedReplyDescription(task, [{ ...result, [key]: value }]), null);
    }
    for (const mutate of [
      candidate => { candidate.status = "running"; },
      candidate => { candidate.deliveryStatus = "partial"; },
      candidate => { candidate.deliveryReport.evidence[0].state = "missing"; },
      candidate => { candidate.deliveryReport.evidence[0].state = "pending"; },
      candidate => { candidate.deliveryReport.pendingActionIds.push("action"); },
      candidate => { candidate.deliveryReport.unsettledAttemptIds.push(attempt.id); },
      candidate => { candidate.error = "failed"; },
      candidate => { candidate.steps[0].target.revisionRepairSource = { chapterId: "chapter", draftRevisionId: "source" }; },
      candidate => { candidate.steps[0].attempts[0].error = "failed"; },
      candidate => { candidate.steps[0].attempts[0].status = "cancelled"; },
      candidate => { candidate.steps[0].attempts.unshift({ ...attempt, id: "previous-failure", status: "failed" }); },
      candidate => { candidate.steps[0].currentAttemptId = "other-attempt"; },
      candidate => { candidate.steps[0].attempts.push(structuredClone(attempt)); },
      candidate => { candidate.steps[0].operation = "advisory"; },
      candidate => { candidate.steps[0].operation = "modelKnowledge"; },
      candidate => { candidate.steps[0].operation = "extractCommitments"; },
    ]) {
      const candidate = structuredClone(task); mutate(candidate);
      assert.equal(generatedReplyDescription(candidate, [result]), null);
    }
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test("generation continuation reuses only its exact original Work author attempt", async () => {
  const directory = await mkdtemp(join(tmpdir(), "work-author-origin-"));
  try {
    const output = join(directory, "logic.mjs");
    await build({ entryPoints: [new URL("../../src/work-conversation.ts", import.meta.url).pathname], outfile: output, bundle: true, format: "esm", platform: "node" });
    const { hasOriginalAuthorMessage } = await import(pathToFileURL(output).href);
    const parent = { id: "author-work", projectId: "project", threadId: "root-thread", goal: "原始目标", kind: "conversation",
      steps: [{ operation: "advisory", attempts: [{ id: "proposal-creator", threadId: "root-thread" }] }] };
    const generation = { id: "generation-work", projectId: "project", threadId: "root-thread", goal: "原始目标", kind: "generation",
      steps: [{ target: { workDecision: { workTaskId: "generation-work", originTaskId: "proposal-creator" } }, attempts: [] }] };
    assert.equal(hasOriginalAuthorMessage(generation, [parent, generation]), true);
    assert.equal(hasOriginalAuthorMessage({ ...generation, goal: "相同身份，不靠文字推断" }, [parent]), true);
    for (const alter of [value => { value.steps[0].target.workDecision.originTaskId = null; },
      value => { value.steps[0].target.workDecision.originTaskId = "unrelated"; },
      value => { value.steps[0].target.workDecision.workTaskId = "other"; },
      value => { value.threadId = "other"; }, value => { value.projectId = "other"; }]) {
      const value = structuredClone(generation); alter(value); assert.equal(hasOriginalAuthorMessage(value, [parent]), false);
    }
    assert.equal(hasOriginalAuthorMessage(generation, []), false);
    assert.equal(hasOriginalAuthorMessage(generation, [parent, { ...parent, id: "duplicate" }]), false);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
