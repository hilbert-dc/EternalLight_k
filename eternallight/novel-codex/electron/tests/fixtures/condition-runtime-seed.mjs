import assert from "node:assert/strict";
import { createHash, randomUUID } from "node:crypto";
import { setTimeout as delay } from "node:timers/promises";

export async function seedConditionRuntime(request, candidateRequest, capability) {
  const { project } = await request("/api/projects", { title: "真实条件检查旅程" });
  const base = `/api/projects/${project.id}`;
  const body = "甲持有信。" + "潮🌊".repeat(90);
  const evidence = { quote: body, start: 0, end: [...body].length };
  const entities = [{ id: "a", kind: "character", displayName: "甲" }, { id: "letter", kind: "item", displayName: "信" }]
    .map(value => ({ ...value, aliases: [], evidence }));
  let eventBatch, eventDraft;
  for (const number of [1, 2, 3]) {
    const draft = await request(`${base}/drafts/checkpoints`, {
      payload: { chapterNumber: number, title: "持信", body }, expectedRevision: 0,
      expectedCandidateRevisionId: null, idempotencyKey: `chapter-${number}`,
    });
    const batch = await request(`${base}/modeling-batches`, {
      objectId: `draft-${draft.chapterId}`, sourceRevisionId: draft.revisionId,
      expectedRevision: 0, extractorVersion: "condition-desktop-fixture",
    });
    const workspace = await request(`${base}/creative-workspace`);
    const task = await request(`${base}/tasks`, {
      operation: "modelKnowledge", inputRevision: 0, threadId: "condition-desktop",
      target: { batchId: batch.id }, authorIntent: "提取测试原文",
      capabilityExpiresAt: Math.floor(Date.now() / 1000) + 600,
      capabilityHash: createHash("sha256").update(capability).digest("hex"),
      initialBudget: { authorizationId: randomUUID(), authorMessageId: randomUUID(), expectedBaseSnapshotId: workspace.headSnapshotId,
        expectedSourceDigest: workspace.sourceDigest, maxRequests: 16, maxTokens: 200_000,
        deadlineAt: Math.floor(Date.now()/1000)+600 },
    });
    const isEvent = number === 2;
    await candidateRequest(`${base}/modeling-batches/${batch.id}/publish`, {
      taskId: task.id, threadId: "condition-desktop", expectedRevision: 0,
      entities: isEvent ? [...entities, { id: "delivery", kind: "event", displayName: "交信", aliases: [], evidence }] : entities,
      assertions: isEvent ? [{ kind: "event", subjectId: "delivery", predicate: "transfer", storyTime: 5,
        value: { participants: [{ role: "item", entityId: "letter" }], preconditions: [], effects: [], untilExclusive: null }, evidence }]
        : [{ kind: "state", subjectId: "letter", predicate: "held", value: number === 1,
          stateSchema: { cardinality: "single", valueType: { type: "boolean" } }, validFrom: 1, evidence }],
    });
    if (isEvent) { eventBatch = batch; eventDraft = draft; }
  }
  const saved = await request(`${base}/artifacts/propertyRules`, {
    expectedRevision: 0, expectedCandidateRevisionId: null, idempotencyKey: "rules",
    payload: {
      definitions: [{ predicate: "held", subjectKind: "item", schema: { cardinality: "single", valueType: { type: "boolean" } }, description: "持有", exceptions: [] }],
      eventRules: [{ eventType: "transfer", description: "持有方可交出", exceptions: [], condition: {
        type: "state", role: "item", subjectKind: "item", predicate: "held", quantifier: "all", operator: "eq", operand: { type: "boolean", value: true },
      } }],
    },
  });
  const review = await request(`${base}/reviews`, { artifactKind: "propertyRules", revisionId: saved.revisionId, proposals: [], selections: {}, idempotencyKey: "rules-review" });
  await request(`${base}/reviews/${review.id}/accept`, { expectedRevision: 0, idempotencyKey: "rules-accept", selections: {} });
  const audit = await request(`${base}/knowledge-audits`, {});
  let verified = false;
  for (let attempt = 0; attempt < 100; attempt++) {
    const state = await request(`${base}/knowledge-audits/${audit.id}`);
    if (state.status === "completed") { assert.equal(state.verifiedConsistent, true); verified = true; break; }
    assert.ok(!["failed", "cancelled"].includes(state.status), JSON.stringify(state));
    await delay(100);
  }
  assert.ok(verified, "Real source audit did not finish");
  return { projectId: project.id, target: { snapshotId: null, sourceBatchId: eventBatch.id, assertionIndex: 0 }, eventDraft, body };
}
