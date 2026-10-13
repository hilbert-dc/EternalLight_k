import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { currentFixtureEvidence, readFixtureAuthorRequirements, updateFixtureDeliveryPlan } from "./model-plan-current-contract.mjs";

// Controlled model outputs; tasks, budgets, source capture and storage use the real Runtime.
export async function seedRebindRuntime(request, candidateRequest, capability, recordUsage, bindBudget) {
  const created = await request("/api/projects", { title: "港城关联接续", logline: "Runtime 重启后关联建模与近章规划验收", medium: "novel", idempotencyKey: "rebind-runtime-project" });
  const projectId = created.project.id;
  const base = `/api/projects/${projectId}`;
  const deadlineAt = Math.floor(Date.now() / 1000) + 600;
  const initialWorkspace = await request(`${base}/creative-workspace`);
  const initialAuthorIntent = "写一章港城故事";
  const initial = await request(`${base}/generation-runs`, { expectedProjectRevision: 0,
    expectedBaseSnapshotId: initialWorkspace.headSnapshotId, expectedSourceDigest: initialWorkspace.sourceDigest,
    authorIntent: initialAuthorIntent, targetChapters: 1, maxTaskAttempts: 32, maxProviderRequests: 256, maxProviderTokens: 4_000_000, deadlineAt });
  const uri = `${base}/generation-runs/${initial.id}`;
  const modelRequest = (method, path, body) => candidateRequest(path, method === "GET" ? undefined : body);
  const createTask = async (next, threadId, inputRevision) => {
    const task = await request(`${base}/tasks`, {
      operation: next.action.operation, target: next.action.target, inputRevision, threadId, authorIntent: "写港城故事",
      capabilityHash: createHash("sha256").update(capability).digest("hex"), capabilityExpiresAt: deadlineAt,
    });
    await bindBudget(projectId, task);
    return task;
  };
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
    return { ...pages[0], requirements: pages.flatMap(page => page.requirements), readyActions: pages.flatMap(page => page.readyActions),
      receipts: { items: pages.flatMap(page => page.receipts.items) } };
  };
  const readRawSource = async (task, source) => {
    let cursor = null, payload = "";
    do {
      const query = new URLSearchParams({ threadId: task.threadId, expectedRevision: String(task.inputRevision), sourceId: source.sourceId });
      if (cursor !== null) query.set("cursor", cursor);
      const page = await candidateRequest(`${base}/tasks/${task.id}/task-source?${query}`);
      assert.equal(page.byteRange.start, Buffer.byteLength(payload));
      assert.equal(page.contentHash, source.contentHash);
      payload += page.payloadPart;
      assert.equal(page.byteRange.end, Buffer.byteLength(payload));
      cursor = page.nextCursor;
      if (cursor === null) {
        assert.equal(page.endOfInputFromCursor, true);
        assert.equal(page.byteRange.total, Buffer.byteLength(payload));
      }
    } while (cursor !== null);
    assert.equal(createHash("sha256").update(payload).digest("hex"), source.contentHash);
    return JSON.parse(payload);
  };
  const choose = async (operation, workerThread, runPath = uri) => {
    let next = await request(`${runPath}/next`, null);
    assert.equal(next.action.operation, "advisory", JSON.stringify(next));
    const workspace = await request(`${base}/creative-workspace`);
    const parent = await createTask(next, "rebind-root", workspace.revision);
    await updateFixtureDeliveryPlan(modelRequest, base, parent);
    const status = await readDecision(parent);
    const action = status.readyActions.find(item => item.operation === operation
      || (operation === "generateChapter" && item.operation === "writeNextChapter"));
    assert.ok(action, `${operation}: ${JSON.stringify(status)}`);
    const acceptance = status.chapterAcceptanceSource ? await readRawSource(parent, status.chapterAcceptanceSource) : null;
    const sourceChecks = [];
    let chapterReview = null;
    if (operation === "finishChapter") {
      const context = await readContext(parent,"planner");
      const find = (record, entry) => {
        if (!record || typeof record !== "object") return;
        const revisionId=record.sourceRevisionId ?? record.revisionId;
        const content=record.content ?? record;
        const prose=content.body ?? content.sceneGroups?.map(group=>group.body.trim()).join("\n\n---\n\n");
        if (revisionId===action.sourceRevisionId && typeof prose==="string" && prose) {
          sourceChecks.push({sourceId:entry.sourceId,revisionId,contentHash:entry.contentHash,evidence:{quote:Array.from(prose).slice(0,48).join("")}});
        }
        for(const child of Object.values(record))find(child,entry);
      };
      for(const entry of context.taskSources.entries) {
        const record=entry.sectionPath.slice(1).split("/").reduce((value,key)=>value[key],context);
        find(record,entry);
      }
      assert.ok(sourceChecks.length,"finishChapter must read its current saved draft");
      const assessment = status.requirements.find(item => item.kind === "revisionOutcome" && item.sourceRevisionId === action.sourceRevisionId)?.assessment;
      assert.ok(assessment?.current && assessment.verdict === "passed", "finishChapter requires a current independent assessment");
      const reported = await candidateRequest(`${base}/tasks/${parent.id}/revision-outcome?${new URLSearchParams({
        threadId: parent.threadId, expectedRevision: String(parent.inputRevision), writerTaskId: assessment.writerTaskId,
      })}`);
      assert.equal(reported.revisionId, assessment.reportRevisionId);
      const report = await readRawSource(parent, reported.source);
      assert.ok(acceptance?.requirements?.length, "finishChapter must read its frozen acceptance source");
      chapterReview = { sourceRevisionId: action.sourceRevisionId, assessmentRevisionId: reported.revisionId,
        checks: acceptance.requirements.map(criterion => {
          const judgment = report.outcomes.find(item => item.id === criterion.id);
          assert.ok(judgment && Object.hasOwn(judgment, "issue"));
          assert.equal(judgment.status, "supported");
          return { criterionId: criterion.id, issueId: judgment.issue === null ? null : judgment.issue.issueId };
        }), length: acceptance.length };
    }
    const selected = await candidateRequest(`${base}/tasks/${parent.id}/work-decision/actions`, {
      taskId: parent.id, threadId: parent.threadId, expectedRevision: parent.inputRevision,
      expectedWorkVersion: status.version, expectedSourceDigest: status.sourceDigest,
      actionKey: action.actionKey, instruction: "按当前授权目标处理当前冻结来源", idempotencyKey: `choose-${parent.id}`,
      ...(["generateChapter","writeNextChapter"].includes(action.operation) ? {chapterAcceptance: acceptance ?? {requirements:[{id:"authorized-chapter",text:"按作者要求完成章节"}],length:{requirement:"无篇幅限制",minCharacters:null,maxCharacters:null}}} : {}),
    });
    let deliveryReview = null;
    if (operation === "finishChapter") {
      const finishedStatus = await readDecision(parent);
      await updateFixtureDeliveryPlan(modelRequest, base, parent, { status: "satisfied",
        evidence: currentFixtureEvidence(finishedStatus, await request(`${base}/creative-workspace`)) });
      if (finishedStatus.finalDeliveryReviewRequired) {
        assert.equal(runPath, uri, "this final delivery belongs to the original rebound author run");
        const authorRequirements = await readFixtureAuthorRequirements(modelRequest, base, parent, initialAuthorIntent);
        assert.ok(authorRequirements.authorText.includes("保留作者藏信稿，按新章纲补全"),
          "the final model must also read the author's actual UI replan instruction");
        // The model rereads the current decision after selecting completion:
        // selection changes the Work version and no stale review may be reused.
        const finalStatus = await readDecision(parent);
        assert.equal(finalStatus.finalDeliveryReviewRequired, true);
        const references = finalStatus.receipts.items.filter(item => item.status === "completed")
          .flatMap(item => item.outputs.filter(output => typeof output.revisionId === "string")
            .map(output => ({ taskId: item.taskId, revisionId: output.revisionId })));
        deliveryReview = {
          expectedWorkVersion: finalStatus.version, expectedSourceDigest: finalStatus.sourceDigest,
          goalStatus: "fulfilled", goalAssessment: "已按作者修改后的藏信方向完成本章候选，未正式发布。",
          checkedScope: "核对本轮保存的正文、当前规划、设定与场景评估回执；评估仅覆盖本章。",
          unresolved: [], issueResolutions: [], authorDecisions: [], evidence: [...new Map(references.map(ref => [`${ref.taskId}:${ref.revisionId}`, ref])).values()],
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
      const record = await readRawSource(task, source);
      const keys = source.sectionPath.slice(1).split("/");
      const field = keys.pop(); let parent = value;
      for (const key of keys) parent = parent[key];
      parent[field] = record;
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
  // Stop the actual old attempt before the author's UI mutation. The replan
  // reconciles its already-bound budget, without authoring a replacement receipt.
  const running = await request(uri);
  assert.equal(running.status, "running");
  assert.equal(running.activeTaskId, task.id);
  const cancelled = await request(`${base}/tasks/${task.id}/cancel`, {});
  assert.equal(cancelled.status, "cancelled");
  const run = await request(uri);
  // Terminal observation is now atomic with cancellation: no live writer is
  // projected as running while the author's rebind awaits budget reconciliation.
  assert.deepEqual(run, { ...running, status: "blocked", blockedReason: "executionBlocked", version: running.version + 1 });
  return { projectId, chapterId: materialized.currentChapterId, runId: initial.id, base, uri,
    oldTaskId: task.id, draft, newPlan, run, createTask, choose, complete, readContext, payload };
}
