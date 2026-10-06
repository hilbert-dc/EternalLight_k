import assert from "node:assert/strict";
import { createHash, randomUUID } from "node:crypto";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { PersistentTaskBudget } from "@eternallight/agent-engine";
import { taskProviderBudgetOptions } from "../../electron/task-provider-budget.mjs";
import { finalizeProbeStage } from "../probe-live-extraction.mjs";
import { startRuntime, request } from "../probe-live-story.mjs";

test("conversation delegates an authoritative target and completes only after an anchored candidate is saved", { timeout: 120000 }, async t => {
  const root = await mkdtemp(join(tmpdir(), "novel-probe-lifecycle-"));
  const author = "fixture-author-token-123456789", candidate = "fixture-candidate-token-123456789";
  const capability = "fixture-lifecycle-capability-123456789";
  const database = join(root, "story.sqlite3");
  let runtime;
  t.after(async () => {
    if (runtime?.child.exitCode === null && runtime.child.signalCode === null) {
      const stopped = new Promise(resolve => runtime.child.once("exit", resolve)); runtime.child.kill(); await stopped;
    }
    await rm(root, { recursive: true, force: true });
  });
  runtime = await startRuntime(database, author, candidate);
  const call = (method, path, payload) => request(runtime.origin, author, method, path, payload);
  const runtimeRequest = (path, options = {}) => call(options.method ?? "GET", path, options.body ? JSON.parse(options.body) : undefined);
  const { project } = await call("POST", "/api/projects", { title: "探针生命周期隔离夹具" });
  const base = `/api/projects/${project.id}`;
  await call("POST", `${base}/drafts/checkpoints`, { expectedRevision: 0, expectedCandidateRevisionId: null,
    idempotencyKey: "original", payload: { chapterId: "chapter-one", chapterNumber: 1, title: "来信", body: "她藏信。", completeness: "chapterComplete" } });
  const original = await call("GET", `${base}/creative-workspace`);
  const deadlineAt = Math.floor(Date.now() / 1000) + 600;
  const intent = "请提取本章伏笔并保存候选报告，不改正文，不建立或确认伏笔。";
  const common = { inputRevision: original.revision, authorIntent: intent,
    capabilityHash: createHash("sha256").update(capability).digest("hex"), capabilityExpiresAt: deadlineAt };
  const task = await call("POST", `${base}/tasks`, { ...common, operation: "advisory", target: { type: "project" }, threadId: "advisory-thread",
    initialBudget: { authorizationId: randomUUID(), authorMessageId: randomUUID(), expectedBaseSnapshotId: original.headSnapshotId,
      expectedSourceDigest: original.sourceDigest, maxRequests: 12, maxTokens: 100000, deadlineAt } });
  const grant = await call("GET", `${base}/tasks/${task.id}/provider-budget`);
  const contextResponse = await fetch(`${runtime.origin}${base}/context?taskId=${task.id}&threadId=${task.threadId}&projectRevision=${task.inputRevision}&purpose=planner&budget=24&byteBudget=60000`, {
    signal: AbortSignal.timeout(10000), headers: { authorization: `Bearer ${candidate}`, "x-novel-task-capability": capability },
  });
  assert.equal(contextResponse.status, 200); const context = await contextResponse.json();
  assert.equal(context.target.projectId, project.id); assert.equal(context.snapshotId, original.headSnapshotId);
  assert.equal(context.relevantText.length, 1);
  const delivered = context.relevantText[0];
  assert.equal(delivered.objectId, "draft-chapter-one");
  const targets = context.operationContext.delegationTargets;
  assert.deepEqual(targets.coverage, { total: 1, included: 1, truncated: false });
  const { target, operations } = targets.entries[0];
  assert.ok(operations.includes("extractCommitments"));
  assert.deepEqual(target, { chapterId: "chapter-one", chapterNumber: 1, sourceRevisionId: original.chapters[0].candidate.id });
  assert.equal(context.textRepresentation, undefined, "Text representation belongs to each delivered draft, not the whole context");
  const deliveredBody = delivered.textRepresentation === "sceneGroups"
    ? delivered.content.sceneGroups.map(group => group.body.trim()).join("\n\n---\n\n") : delivered.content.body;
  assert.equal(deliveredBody, "她藏信。");
  const actionResponse = await fetch(`${runtime.origin}${base}/tasks/${task.id}/actions`, { method: "POST", signal: AbortSignal.timeout(10000),
    headers: { authorization: `Bearer ${candidate}`, "content-type": "application/json", "x-novel-task-capability": capability },
    body: JSON.stringify({ taskId: task.id, threadId: task.threadId, expectedRevision: original.revision,
      expectedIntentRevision: 0, idempotencyKey: "one-extraction", operation: "extractCommitments", target, instruction: intent }) });
  assert.equal(actionResponse.status, 201); const action = await actionResponse.json();
  assert.equal((await call("GET", `${base}/tasks/${task.id}`)).status, "queued", "Registration is not model completion");
  const result = { turn: { id: "actual-scripted-turn", status: "completed" }, message: "已登记提取请求。",
    finalReply: { itemId: "final-public-item", text: "已登记提取请求。", truncated: false } };
  const observation = {};
  await finalizeProbeStage({ task, result, runtimeRequest, observation });
  assert.deepEqual(observation, { modelTurnStatus: "completed", runtimeTaskStatus: "completed", finalReplyRecorded: true });
  const savedReply = await call("GET", `${base}/tasks/${task.id}/final-reply`);
  assert.deepEqual(savedReply.binding, { threadId: task.threadId, turnId: result.turn.id, intentRevision: 0,
    itemId: result.finalReply.itemId, text: result.finalReply.text, truncated: false });
  const budget = new PersistentTaskBudget({ databasePath: `${database}.budget`, runId: task.id,
    ...taskProviderBudgetOptions(grant, { taskId: task.id, workTaskId: grant.workTaskId, deadlineAt: grant.deadlineAt }) });
  budget.complete("completed"); budget.close();
  await call("POST", `${base}/work-tasks/${grant.workTaskId}/provider-usage`, { taskId: task.id });
  const approved = await call("POST", `${base}/tasks/${task.id}/actions/${action.id}/approve`, { expectedIntentRevision: 0 });
  const child = await call("POST", `${base}/tasks`, { ...common, operation: "extractCommitments", threadId: "child-thread",
    target: { ...approved.target, requestedByTaskId: task.id, requestedActionId: action.id } });
  assert.equal((await call("GET", `${base}/tasks/${child.id}/provider-budget`)).workTaskId, grant.workTaskId);
  const childObservation = {};
  await assert.rejects(finalizeProbeStage({ task: child, result: { ...result, turn: { id: "child-turn", status: "completed" } },
    runtimeRequest, observation: childObservation }), { probeCode: "CHILD_TASK_NOT_COMPLETED" });
  assert.deepEqual(childObservation, { modelTurnStatus: "completed", runtimeTaskStatus: "queued" });
  assert.equal((await call("GET", `${base}/tasks/${child.id}`)).status, "queued");
  const missing = await fetch(`${runtime.origin}${base}/tasks/${child.id}/complete`, { method: "POST", signal: AbortSignal.timeout(10000),
    headers: { authorization: `Bearer ${author}`, "content-type": "application/json" }, body: JSON.stringify({ threadId: child.threadId, summary: "不能替工具完成" }) });
  assert.equal(missing.status, 409); const missingBody = await missing.json();
  assert.equal(missingBody.error.code, "conflict");
  assert.match(missingBody.error.message, /output/i);
  // Exercise the same authenticated Runtime endpoints as the worker tools. This
  // is deterministic domain integration, not a model or MCP transport fixture.
  const workerCall = async (method, path, payload, tool) => {
    const body = payload === undefined ? undefined : JSON.stringify(payload);
    const response = await fetch(`${runtime.origin}${path}`, { method, body, signal: AbortSignal.timeout(10000), headers: {
      authorization: `Bearer ${candidate}`, "content-type": "application/json", "x-novel-task-capability": capability,
      ...(tool ? { "x-novel-tool-name": tool, "x-novel-tool-call": `fixture-${tool}`, "x-novel-turn": "child-turn",
        "x-novel-root-thread": child.threadId, "x-novel-caller-thread": child.threadId,
        "x-novel-request-hash": createHash("sha256").update(body).digest("hex") } : {}),
    } });
    const value = await response.json();
    assert.ok(response.ok, JSON.stringify(value)); return value;
  };
  const workerContext = await workerCall("GET", `${base}/context?taskId=${child.id}&threadId=${child.threadId}&projectRevision=${child.inputRevision}&purpose=critic&budget=24&byteBudget=60000`);
  assert.equal(workerContext.relevantText[0].sourceRevisionId, target.sourceRevisionId);
  const source = workerContext.relevantText[0];
  const body = source.textRepresentation === "sceneGroups"
    ? source.content.sceneGroups.map(group => group.body.trim()).join("\n\n---\n\n") : source.content.body;
  assert.equal(body, "她藏信。");
  const evidence = await workerCall("POST", `${base}/tasks/${child.id}/source-evidence`, {
    threadId: child.threadId, expectedRevision: child.inputRevision, sourceObjectId: source.objectId,
    sourceRevisionId: target.sourceRevisionId, quote: body,
  }, "resolve_story_evidence");
  assert.equal(evidence.status, "unique");
  const payload = { chapterId: target.chapterId, sourceRevisionId: target.sourceRevisionId,
    examinedRanges: [{ start: 0, end: evidence.source.unicodeLength }], candidates: [{
      id: "hidden-letter", kind: "suspense", summary: "信中内容尚未揭示", relatedEntityIds: [], plotThreadId: null, characterArcId: null,
      setupEvidenceRefs: [{ objectId: evidence.source.objectId, revisionId: evidence.source.revisionId,
        quote: evidence.quote, ...evidence.matches[0] }], fulfillmentCriteria: "后续揭示信的内容", targetWindow: { kind: "open" },
      status: "open", fulfillmentEvidenceRefs: [], decisionReason: null,
    }] };
  const reportPath = `${base}/chapters/${target.chapterId}/commitment-extractions`;
  const receipt = await workerCall("POST", reportPath, { taskId: child.id, threadId: child.threadId,
    expectedRevision: child.inputRevision, expectedCandidateRevisionId: null, idempotencyKey: "anchored-report", payload,
  }, "submit_commitment_extraction");
  assert.equal(receipt.candidateCount, 1);
  const completed = await workerCall("POST", `${base}/tasks/${child.id}/complete`, {
    threadId: child.threadId, expectedRevision: child.inputRevision, summary: "已保存一项悬念候选，等待作者决定。",
  }, "complete_task");
  assert.equal(completed.status, "completed");
  await finalizeProbeStage({ task: child, result: { ...result, turn: { id: "child-turn", status: "completed" } },
    runtimeRequest, observation: childObservation });
  assert.equal(childObservation.runtimeTaskStatus, "completed");
  const report = await call("GET", reportPath);
  assert.equal(report.revisionId, receipt.revisionId);
  assert.deepEqual(report.payload, payload);
  assert.equal(report.authority, "candidateProposalBatch");
  const denied = {};
  await assert.rejects(finalizeProbeStage({ task: { ...task, threadId: "foreign-thread" }, result, runtimeRequest, observation: denied }),
    { probeCode: "ADVISORY_HOST_FINALIZE_FAILED" });
  assert.deepEqual(denied, { modelTurnStatus: "completed", runtimeTaskStatus: "completed" });
  const after = await call("GET", `${base}/creative-workspace`);
  assert.deepEqual(after.chapters, original.chapters);
  assert.equal(after.headSnapshotId, report.snapshotId, "The workspace contains the saved candidate report's snapshot");
  assert.equal(report.sourceCurrent, true);
  assert.deepEqual(report.established, [], "Extraction does not establish or confirm commitments");
});
