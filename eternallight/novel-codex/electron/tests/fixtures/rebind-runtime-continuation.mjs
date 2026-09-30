import assert from "node:assert/strict";

// Continue the exact rebound run. Only model responses are controlled, not domain decisions.
export async function completeReboundChapter({ request, candidateRequest, fixture, writer, revision, recordUsage }) {
  const { uri, base, chapterId } = fixture;
  const complete = async (task, threadId) => {
    await candidateRequest(`${base}/tasks/${task.id}/complete`, { threadId, summary: "保存当前来源输出" });
    await recordUsage(fixture.projectId, fixture.runId, task.id);
    const claimed = await request(uri);
    return request(`${uri}/advance-task`, { expectedVersion: claimed.version, taskId: task.id });
  };
  const extracted = await complete(writer, "renewed");
  assert.equal(extracted.stage, "extract");
  await assert.rejects(() => request(`${uri}/skip-ready-extract`, { expectedVersion: extracted.version }), /chapter knowledge is not ready/);
  await assert.rejects(() => request(`${uri}/check`, { expectedVersion: extracted.version }), /AuthoringRunStale/);
  assert.deepEqual(await request(uri), extracted);
  let draft = (await request(`${base}/creative-workspace`)).chapters[0].candidate;
  const evidence = { quote: "藏好密函", start: 1, end: 5 };
  const batch = await request(`${base}/modeling-batches`, { objectId: `draft-${chapterId}`, sourceRevisionId: draft.id,
    expectedRevision: revision, extractorVersion: "rebind-journey-fixture" });
  let action = await request(`${uri}/next`, null);
  assert.equal(action.action.operation, "modelKnowledge");
  action.action.target.batchId = batch.id;
  const modeler = await fixture.createTask(action, "modeler", revision);
  await assert.rejects(() => candidateRequest(`${base}/tasks/${modeler.id}/complete`, { threadId: "modeler", summary: "没有发布" }), /TaskOutputMissing/);
  await candidateRequest(`${base}/modeling-batches/${batch.id}/publish`, {
    taskId: modeler.id, threadId: "modeler", expectedRevision: revision,
    entities: [{ id: "letter", kind: "item", displayName: "密函", aliases: [], evidence: { quote: "密函", start: 3, end: 5 } },
      { id: "hide-letter", kind: "event", displayName: "藏信", aliases: [], evidence }],
    assertions: [{ kind: "event", subjectId: "hide-letter", predicate: "hideLetter", storyTime: 1, evidence,
      value: { participants: [{ role: "item", entityId: "letter" }], preconditions: [], effects: [], untilExclusive: null } }],
  });
  const commitments = await complete(modeler, "modeler");
  assert.equal(commitments.stage, "commitments");
  action = await request(`${uri}/next`, null);
  assert.equal(action.action.operation, "extractCommitments");
  assert.equal(action.action.target.sourceRevisionId, draft.id);
  const critic = await fixture.createTask(action, "commitments", revision);
  const report = await candidateRequest(`${base}/chapters/${chapterId}/commitment-extractions`, {
    taskId: critic.id, threadId: "commitments", expectedRevision: revision, expectedCandidateRevisionId: null,
    idempotencyKey: "current-commitments", payload: { chapterId, sourceRevisionId: draft.id,
      examinedRanges: [{ start: 0, end: [...draft.body].length }], candidates: [] },
  });
  assert.ok(report.revisionId);
  const assessed = await complete(critic, "commitments");
  assert.equal(assessed.stage, "assess");
  action = await request(`${uri}/next`, null);
  assert.equal(action.action.operation, "assessSceneOutcome");
  assert.equal(action.action.target.draftRevisionId, draft.id);
  assert.equal(action.action.target.planRevisionId, fixture.newPlan.revisionId);
  assert.equal(action.action.target.knowledgeBatchId, batch.id);
  const assessor = await fixture.createTask(action, "assessor", revision);
  await candidateRequest(`${base}/chapters/${chapterId}/scene-assessments`, { taskId: assessor.id, threadId: "assessor",
    expectedRevision: revision, expectedCandidateRevisionId: null, idempotencyKey: "current-assessment", payload: {
      chapterId, planRevisionId: action.action.target.planRevisionId, draftRevisionId: draft.id, knowledgeBatchId: batch.id,
      inputSnapshotId: action.action.target.inputSnapshotId, intentGeneration: action.action.target.intentGeneration,
      assessments: [{ sceneId: "scene-1", verdict: "realized", rationale: "当前稿明确藏信", recommendation: "continue",
        effectAssessments: [{ effectId: "effect-1", verdict: "realized", rationale: "密函得以保留",
          actualEventRefs: [{ modelingBatchId: batch.id, assertionIndex: 0 }], commitmentAssessments: [] }] }],
    } });
  const checked = await complete(assessor, "assessor");
  assert.equal(checked.stage, "check");
  const done = await request(`${uri}/check`, { expectedVersion: checked.version });
  assert.equal(done.status, "completed");
  assert.equal(done.completedChapters, 1);
  assert.equal(done.usedTaskAttempts, fixture.run.usedTaskAttempts + 4);
  assert.equal(done.deadlineAt, fixture.run.deadlineAt);
  assert.equal((await request(`${base}/work-tasks/work-${fixture.runId}`)).status, "completed");
  const continuationWorkspace = await request(`${base}/creative-workspace`);
  draft = continuationWorkspace.chapters[0].candidate;
  const nextRun = await request(`${base}/generation-runs`, { expectedProjectRevision: revision,
    expectedBaseSnapshotId: continuationWorkspace.headSnapshotId, expectedSourceDigest: continuationWorkspace.sourceDigest,
    authorIntent: "基于藏信后的当前稿，规划下一章", targetChapters: 1, maxTaskAttempts: 7, maxProviderRequests: 256, maxProviderTokens: 4_000_000, deadlineAt: fixture.run.deadlineAt });
  assert.equal(nextRun.stage, "rollingPlan");
  assert.equal(nextRun.startChapterNumber, 2);
  assert.equal(nextRun.completedChapters, 1);
  const nextUri = `${base}/generation-runs/${nextRun.id}`;
  const next = await request(`${nextUri}/next`, null);
  assert.equal(next.action.operation, "generatePlan");
  assert.equal(next.action.target.planFromChapter, 2);
  const planner = await fixture.createTask(next, "next-planner", revision);
  const context = await candidateRequest(`${base}/context?taskId=${planner.id}&threadId=next-planner&projectRevision=${revision}&purpose=planner&budget=24&byteBudget=60000`);
  assert.equal(context.operationContext.workingKnowledge.coverage.complete, true);
  const knowledge = context.operationContext.workingKnowledge.entries;
  assert.ok(knowledge.length > 0);
  assert.ok(knowledge.every(entry => entry.sourceRevisionId === draft.id), JSON.stringify(knowledge));
  assert.ok(JSON.stringify(knowledge).includes("hideLetter"));
  assert.equal(JSON.stringify(knowledge).includes("他烧掉密函"), false);
  assert.equal((await request(`${base}/creative-workspace`)).chapters[0].accepted, null);
  return { done, nextRun };
}
