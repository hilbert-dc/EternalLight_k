import assert from "node:assert/strict";
import { createHash } from "node:crypto";

// Controlled model outputs; tasks, budgets, source capture and storage use the real Runtime.
export async function seedRebindRuntime(request, candidateRequest, capability, recordUsage) {
  const created = await request("/api/projects", { title: "港城关联接续" });
  const projectId = created.project.id;
  const base = `/api/projects/${projectId}`;
  const deadlineAt = Math.floor(Date.now() / 1000) + 600;
  const initialWorkspace = await request(`${base}/creative-workspace`);
  const initial = await request(`${base}/generation-runs`, { expectedProjectRevision: 0,
    expectedBaseSnapshotId: initialWorkspace.headSnapshotId, expectedSourceDigest: initialWorkspace.sourceDigest,
    authorIntent: "写一章港城故事", targetChapters: 1, maxTaskAttempts: 64, maxProviderRequests: 256, maxProviderTokens: 4_000_000, deadlineAt });
  const uri = `${base}/generation-runs/${initial.id}`;
  const createTask = async (next, threadId, inputRevision) => request(`${base}/tasks`, {
    operation: next.action.operation, target: next.action.target, inputRevision, threadId, authorIntent: "写港城故事",
    capabilityHash: createHash("sha256").update(capability).digest("hex"), capabilityExpiresAt: deadlineAt,
  });
  const complete = async (task, deliveryReview = null) => {
    await candidateRequest(`${base}/tasks/${task.id}/complete`, { threadId: task.threadId, summary: "保存当前来源输出", ...(deliveryReview ? { deliveryReview } : {}) });
    await recordUsage(projectId, task.target.runId, task.id);
    const runPath = `${base}/generation-runs/${task.target.runId}`;
    const claimed = await request(runPath);
    return request(`${runPath}/advance-task`, { expectedVersion: claimed.version, taskId: task.id });
  };
  const readDecision = async parent => {
    const pages = []; let cursor = null;
    do {
      const query = new URLSearchParams({ threadId: parent.threadId, expectedRevision: String(parent.inputRevision) });
      if (cursor !== null) query.set("cursor", cursor);
      const page = await candidateRequest(`${base}/tasks/${parent.id}/work-decision?${query}`);
      if (pages.length) {
        assert.equal(page.version, pages[0].version);
        assert.equal(page.sourceDigest, pages[0].sourceDigest);
      }
      pages.push(page); cursor = page.pagination.nextCursor;
    } while (cursor !== null);
    return { ...pages[0], readyActions: pages.flatMap(page => page.readyActions),
      receipts: { items: pages.flatMap(page => page.receipts.items) } };
  };
  const choose = async (operation, workerThread, runPath = uri) => {
    let next = await request(`${runPath}/next`, null);
    assert.equal(next.action.operation, "advisory", JSON.stringify(next));
    const workspace = await request(`${base}/creative-workspace`);
    const parent = await createTask(next, "rebind-root", workspace.revision);
    const status = await readDecision(parent);
    const action = status.readyActions.find(item => item.operation === operation
      || (operation === "generateChapter" && item.operation === "writeNextChapter"));
    assert.ok(action, `${operation}: ${JSON.stringify(status)}`);
    const selected = await candidateRequest(`${base}/tasks/${parent.id}/work-decision/actions`, {
      taskId: parent.id, threadId: parent.threadId, expectedRevision: parent.inputRevision,
      expectedWorkVersion: status.version, expectedSourceDigest: status.sourceDigest,
      actionKey: action.actionKey, instruction: "按当前授权目标处理当前冻结来源", idempotencyKey: `choose-${parent.id}`,
    });
    let deliveryReview = null;
    if (operation === "finishChapter") {
      if (status.finalDeliveryReviewRequired) {
        // The model rereads the current decision after selecting completion:
        // selection changes the Work version and no stale review may be reused.
        const finalStatus = await readDecision(parent);
        assert.equal(finalStatus.deliveryReviewRequired, true);
        const references = finalStatus.receipts.items.filter(item => item.status === "completed")
          .flatMap(item => item.outputs.filter(output => typeof output.revisionId === "string")
            .map(output => ({ taskId: item.taskId, revisionId: output.revisionId })));
        deliveryReview = {
          expectedWorkVersion: finalStatus.version, expectedSourceDigest: finalStatus.sourceDigest,
          goalStatus: "fulfilled", goalAssessment: "已按作者修改后的藏信方向完成本章候选，未正式发布。",
          checkedScope: "核对本轮保存的正文、当前规划、设定与场景评估回执；评估仅覆盖本章。",
          unresolved: [], authorDecisions: [], evidence: [...new Map(references.map(ref => [`${ref.taskId}:${ref.revisionId}`, ref])).values()],
        };
      }
    }
    await complete(parent, deliveryReview);
    if (operation === "finishChapter") return { parent, selected };
    next = await request(`${runPath}/next`, null);
    assert.equal(next.action.operation, operation, JSON.stringify(next));
    return createTask(next, workerThread, (await request(`${base}/creative-workspace`)).revision);
  };
  const readContext = async (task, purpose) => {
    const value = await candidateRequest(`${base}/context?${new URLSearchParams({taskId:task.id,threadId:task.threadId,projectRevision:String(task.inputRevision),purpose,byteBudget:"60000"})}`);
    for (const source of value.taskSources.entries) {
      let cursor = null, payload = "";
      do {
        const query = new URLSearchParams({threadId:task.threadId,expectedRevision:String(task.inputRevision),sourceId:source.sourceId});
        if (cursor !== null) query.set("cursor",cursor);
        const page = await candidateRequest(`${base}/tasks/${task.id}/task-source?${query}`);
        assert.equal(page.byteRange.start,Buffer.byteLength(payload));
        assert.equal(page.contentHash,source.contentHash);
        payload += page.payloadPart; assert.equal(page.byteRange.end,Buffer.byteLength(payload));
        cursor = page.nextCursor;
        if (cursor === null) { assert.equal(page.endOfInputFromCursor,true); assert.equal(page.byteRange.total,Buffer.byteLength(payload)); }
      } while (cursor !== null);
      assert.equal(createHash("sha256").update(payload).digest("hex"),source.contentHash);
      const keys = source.sectionPath.slice(1).split("/");
      const field = keys.pop(); let parent = value;
      for (const key of keys) parent = parent[key];
      parent[field] = JSON.parse(payload);
    }
    return value;
  };
  const plan = { chapters: [{ chapterPlanId: "plan-1", chapterNumber: 1, goal: "发现信件",
    publicTitle: "焚信", writerGuidance: "主角焚烧密函。", povCharacterId: "linxi", storyTime: 10, scenes: [{ id: "scene-1", goal: "找到密函", beats: [],
      expectedEffects: [{ id: "effect-1", dimension: "knowledge", description: "发现密函", commitmentRefs: [] }] }] }] };
  for (const [kind, payload] of [
    ["brief", { premise: "抄写员保留密函" }],
    ["outline", { nodes: [{ id: "book", kind: "book", position: 0, title: "港城", purpose: "保留记忆", summary: "见证者" }] }],
    ["plan", plan],
  ]) {
    const operation = { brief:"generateBrief",outline:"generateOutline",plan:"generatePlan" }[kind];
    const task = await choose(operation,kind);
    await candidateRequest(`${base}/artifacts/${kind}`, { payload, expectedRevision: 0,
      expectedCandidateRevisionId: null, idempotencyKey: kind, taskId: task.id, threadId: task.threadId });
    await complete(task);
  }
  const task = await choose("generateChapter","draft");
  const materialized = await request(uri);
  const workspace = await request(`${base}/creative-workspace`);
  const original = await candidateRequest(`${base}/drafts/checkpoints`, { expectedRevision: workspace.revision,
    expectedCandidateRevisionId: null, idempotencyKey: "draft", taskId: task.id, threadId: "draft", payload: {
      chapterId: materialized.currentChapterId, chapterNumber: 1, title: "信", body: "他烧掉密函。",
      completeness: "chapterComplete", sceneGroups: [{ id: "group-1", sceneId: "scene-1", body: "他烧掉密函。", locked: false }],
    } });
  const editable = await request(`${base}/creative-workspace`);
  const payload = structuredClone(editable.chapters[0].candidate);
  payload.body = "他藏好密函。"; payload.sceneGroups[0].body = payload.body;
  const draft = await request(`${base}/drafts/checkpoints`, { expectedRevision: workspace.revision,
    expectedCandidateRevisionId: original.revisionId, idempotencyKey: "author-edit", payload });
  const changedPlan = structuredClone(plan);
  changedPlan.chapters[0].goal = "保留密函";
  changedPlan.chapters[0].publicTitle = "藏信";
  changedPlan.chapters[0].writerGuidance = "保留作者藏信稿，记录证词。";
  changedPlan.chapters[0].povCharacterId = "zhangheng";
  changedPlan.chapters[0].storyTime = 20;
  const newPlan = await request(`${base}/artifacts/plan`, { payload: changedPlan, expectedRevision: workspace.revision,
    expectedCandidateRevisionId: editable.artifacts.plan.candidate.id, idempotencyKey: "new-plan" });
  const run = await request(uri);
  assert.equal(run.status, "running");
  return { projectId, chapterId: materialized.currentChapterId, runId: initial.id, base, uri,
    oldTaskId: task.id, draft, newPlan, run, createTask, choose, complete, readContext, payload };
}
