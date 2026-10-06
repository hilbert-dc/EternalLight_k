import assert from "node:assert/strict";

// Continue the exact rebound run. Only model responses are controlled, not domain decisions.
export async function completeReboundChapter({ request, candidateRequest, fixture, writer, revision, recordUsage }) {
  const { uri, base, chapterId } = fixture;
  const complete = fixture.complete;
  const startAttempts = (await request(uri)).usedTaskAttempts;
  const startCharges = (await request(uri)).providerBudget.charges.length;
  const extracted = await complete(writer);
  assert.equal(extracted.stage,"coordinate");
  let draft = (await request(`${base}/creative-workspace`)).chapters[0].candidate;
  assert.equal(draft.body,"他藏好密函。");
  const evidence = { quote: "藏好密函", start: 1, end: 5 };
  const modeler = await fixture.choose("modelKnowledge","modeler");
  const batch = {id:modeler.target.batchId};
  assert.equal(modeler.target.sourceRevisionId,draft.id);
  await assert.rejects(() => candidateRequest(`${base}/tasks/${modeler.id}/complete`, { threadId: "modeler", summary: "没有发布" }), /TaskOutputMissing/);
  await candidateRequest(`${base}/modeling-batches/${batch.id}/publish`, {
    taskId: modeler.id, threadId: "modeler", expectedRevision: revision,
    entities: [{ id: "letter", kind: "item", displayName: "密函", aliases: [], evidence: { quote: "密函", start: 3, end: 5 } },
      { id: "hide-letter", kind: "event", displayName: "藏信", aliases: [], evidence }],
    assertions: [{ kind: "event", subjectId: "hide-letter", predicate: "hideLetter", storyTime: 1, evidence,
      value: { participants: [{ role: "item", entityId: "letter" }], preconditions: [], effects: [], untilExclusive: null } }],
  });
  await complete(modeler);
  const critic = await fixture.choose("extractCommitments","commitments");
  assert.equal(critic.target.sourceRevisionId,draft.id);
  const report = await candidateRequest(`${base}/chapters/${chapterId}/commitment-extractions`, {
    taskId: critic.id, threadId: "commitments", expectedRevision: revision, expectedCandidateRevisionId: null,
    idempotencyKey: "current-commitments", payload: { chapterId, sourceRevisionId: draft.id,
      examinedRanges: [{ start: 0, end: [...draft.body].length }], candidates: [] },
  });
  assert.ok(report.revisionId);
  await complete(critic);
  const assessor = await fixture.choose("assessSceneOutcome","assessor");
  assert.equal(assessor.target.draftRevisionId,draft.id);
  assert.equal(assessor.target.planRevisionId,fixture.newPlan.revisionId);
  assert.equal(assessor.target.knowledgeBatchId,batch.id);
  await candidateRequest(`${base}/chapters/${chapterId}/scene-assessments`, { taskId: assessor.id, threadId: "assessor",
    expectedRevision: revision, expectedCandidateRevisionId: null, idempotencyKey: "current-assessment", payload: {
      chapterId, planRevisionId: assessor.target.planRevisionId, draftRevisionId: draft.id, knowledgeBatchId: batch.id,
      inputSnapshotId: assessor.target.inputSnapshotId, intentGeneration: assessor.target.intentGeneration,
      assessments: [{ sceneId: "scene-1", verdict: "realized", rationale: "当前稿明确藏信", recommendation: "continue",
        effectAssessments: [{ effectId: "effect-1", verdict: "realized", rationale: "密函得以保留",
          actualEventRefs: [{ modelingBatchId: batch.id, assertionIndex: 0 }], commitmentAssessments: [] }] }],
    } });
  await complete(assessor);
  await fixture.choose("finishChapter","finish");
  const finalNext = await request(`${uri}/next`,null);
  assert.equal(finalNext.action,null);
  const done = await request(uri);
  assert.equal(done.status,"completed");
  assert.equal(done.completedChapters,1);
  assert.equal(done.usedTaskAttempts,startAttempts+7);
  assert.equal(done.providerBudget.charges.length,startCharges+8);
  assert.equal(done.deadlineAt, fixture.run.deadlineAt);
  assert.equal((await request(`${base}/work-tasks/work-${fixture.runId}`)).status, "completed");
  const continuationWorkspace = await request(`${base}/creative-workspace`);
  draft = continuationWorkspace.chapters[0].candidate;
  const nextRun = await request(`${base}/generation-runs`, { expectedProjectRevision: revision,
    expectedBaseSnapshotId: continuationWorkspace.headSnapshotId, expectedSourceDigest: continuationWorkspace.sourceDigest,
    authorIntent: "基于藏信后的当前稿，规划下一章", targetChapters: 1, maxTaskAttempts: 32, maxProviderRequests: 256, maxProviderTokens: 4_000_000, deadlineAt: fixture.run.deadlineAt });
  assert.equal(nextRun.stage,"coordinate");
  assert.equal(nextRun.startChapterNumber, 2);
  assert.equal(nextRun.completedChapters, 1);
  const nextUri = `${base}/generation-runs/${nextRun.id}`;
  const planner = await fixture.choose("generatePlan","next-planner",nextUri);
  assert.equal(planner.target.planFromChapter,2);
  const context = await fixture.readContext(planner,"planner");
  assert.equal(context.operationContext.workingKnowledge.coverage.complete, true);
  const knowledge = context.operationContext.workingKnowledge.entries;
  assert.ok(knowledge.length > 0);
  assert.ok(knowledge.every(entry => entry.sourceRevisionId === draft.id), JSON.stringify(knowledge));
  assert.ok(JSON.stringify(knowledge).includes("hideLetter"));
  assert.equal(JSON.stringify(knowledge).includes("他烧掉密函"), false);
  assert.equal((await request(`${base}/creative-workspace`)).chapters[0].accepted, null);
  return { done, nextRun };
}
