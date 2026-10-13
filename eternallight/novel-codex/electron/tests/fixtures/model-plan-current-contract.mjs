import assert from "node:assert/strict";
import { createHash } from "node:crypto";

// Synthetic model reads still use the real Candidate source endpoint. The
// complete original author source, not its catalogue reference, permits final
// delivery; a Host helper must never fabricate that proof for production.
export async function readFixtureAuthorRequirements(request, base, task, expectedAuthorText) {
  const query = new URLSearchParams({ taskId: task.id, threadId: task.threadId,
    projectRevision: String(task.inputRevision), purpose: "planner", budget: "24", byteBudget: "60000" });
  const context = await request("GET", `${base}/context?${query}`);
  const source = context.taskSources.entries.find(entry => entry.sectionPath === "/authorRequirementsSource");
  assert.ok(source, "final model must discover its actual frozen original author source");
  assert.equal(context.authorRequirementsSource.sourceId, source.sourceId);
  assert.equal(context.authorRequirementsSource.textDelivered, false);
  let payload = "", cursor = null;
  const cursors = new Set();
  do {
    const pageQuery = new URLSearchParams({ threadId: task.threadId, expectedRevision: String(task.inputRevision), sourceId: source.sourceId });
    if (cursor !== null) pageQuery.set("cursor", cursor);
    const page = await request("GET", `${base}/tasks/${task.id}/task-source?${pageQuery}`);
    assert.equal(page.sourceId, source.sourceId); assert.equal(page.contentHash, source.contentHash);
    assert.equal(page.byteRange.start, Buffer.byteLength(payload));
    payload += page.payloadPart;
    assert.equal(page.byteRange.end, Buffer.byteLength(payload));
    cursor = page.nextCursor;
    if (cursor !== null) { assert.ok(!cursors.has(cursor)); cursors.add(cursor); }
    else {
      assert.equal(page.endOfInputFromCursor, true);
      assert.equal(page.byteRange.total, Buffer.byteLength(payload));
    }
  } while (cursor !== null);
  assert.equal(createHash("sha256").update(payload).digest("hex"), source.contentHash);
  const requirements = JSON.parse(payload);
  assert.ok(JSON.stringify(requirements).includes(expectedAuthorText), "the final model must read the complete human author input");
  return requirements;
}

// Controlled model decisions for synthetic integration fixtures. This is not a
// production Host default: real agents author their plans through MCP tools.
export async function updateFixtureDeliveryPlan(request, base, task, { status = "inProgress", evidence = [] } = {}) {
  const path = `${base}/tasks/${task.id}/work-plan`;
  const query = new URLSearchParams({ threadId: task.threadId, expectedRevision: String(task.inputRevision) });
  const current = await request("GET", `${path}?${query}`);
  assert.ok(Number.isSafeInteger(current.version));
  if (status === "satisfied") assert.ok(evidence.length, "completed fixture goals require current saved outputs from this Work");
  const goal = { id: "authorized-delivery", title: "完成当前授权的创作范围", acceptance: "保存限定范围内的候选成果，并核对当前保存回执与规定的检查。",
    kind: "deliverable", status, dependsOn: [], readingQuestion: "哪些当前来源和保存结果支撑本次限定交付？",
    checkScope: "本合成测试明确授权的章节和规划", approach: "读取当前来源，分派限定创作，核对实际保存与检查回执。",
    assessment: status === "satisfied" ? "当前测试范围的成果已保存，以下证据属于本 Work 的当前修订。" : "本次授权工作尚在执行，不宣称已完成。",
    evidence, sourceIds: [], sourceChecks: [], continuation: null };
  const saved = await request("POST", path, { taskId: task.id, threadId: task.threadId, expectedRevision: task.inputRevision,
    expectedPlanVersion: current.version, idempotencyKey: `model-plan-${task.id}-${status}`, plan: { intentMode: "executeGoals", goals: [goal] } });
  assert.equal(saved.intentMode, "executeGoals");
  assert.equal(saved.activeGoalId, status === "inProgress" ? goal.id : null);
  return saved;
}

export function currentFixtureEvidence(status, workspace) {
  const revisions = new Set([
    ...workspace.chapters.map(chapter => (chapter.candidate ?? chapter.accepted)?.id),
    ...Object.values(workspace.artifacts).map(artifact => (artifact.candidate ?? artifact.accepted)?.id),
  ].filter(Boolean));
  const references = status.receipts.items.filter(receipt => receipt.status === "completed")
    .flatMap(receipt => receipt.outputs.filter(output => revisions.has(output.revisionId))
      .map(output => ({ taskId: receipt.taskId, revisionId: output.revisionId })));
  return [...new Map(references.map(ref => [`${ref.taskId}:${ref.revisionId}`, ref])).values()].slice(0, 8);
}
