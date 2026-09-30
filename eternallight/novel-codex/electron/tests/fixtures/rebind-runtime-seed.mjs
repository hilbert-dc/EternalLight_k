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
    authorIntent: "写一章港城故事", targetChapters: 1, maxTaskAttempts: 12, maxProviderRequests: 256, maxProviderTokens: 4_000_000, deadlineAt });
  const uri = `${base}/generation-runs/${initial.id}`;
  const createTask = async (next, threadId, inputRevision) => request(`${base}/tasks`, {
    operation: next.action.operation, target: next.action.target, inputRevision, threadId, authorIntent: "写港城故事",
    capabilityHash: createHash("sha256").update(capability).digest("hex"), capabilityExpiresAt: deadlineAt,
  });
  const plan = { chapters: [{ chapterPlanId: "plan-1", chapterNumber: 1, goal: "发现信件",
    publicTitle: "焚信", writerGuidance: "主角焚烧密函。", povCharacterId: "linxi", storyTime: 10, scenes: [{ id: "scene-1", goal: "找到密函", beats: [],
      expectedEffects: [{ id: "effect-1", dimension: "knowledge", description: "发现密函", commitmentRefs: [] }] }] }] };
  for (const [kind, payload] of [
    ["brief", { premise: "抄写员保留密函" }],
    ["outline", { nodes: [{ id: "book", kind: "book", position: 0, title: "港城", purpose: "保留记忆", summary: "见证者" }] }],
    ["plan", plan],
  ]) {
    const task = await createTask(await request(`${uri}/next`, null), kind, 0);
    await candidateRequest(`${base}/artifacts/${kind}`, { payload, expectedRevision: 0,
      expectedCandidateRevisionId: null, idempotencyKey: kind, taskId: task.id, threadId: kind });
    await candidateRequest(`${base}/tasks/${task.id}/complete`, { threadId: kind, summary: "已保存" });
    await recordUsage(projectId, initial.id, task.id);
    const claimed = await request(uri);
    await request(`${uri}/advance-task`, { expectedVersion: claimed.version, taskId: task.id });
  }
  const ready = await request(uri);
  const materialized = await request(`${uri}/materialize`, { expectedVersion: ready.version });
  const workspace = await request(`${base}/creative-workspace`);
  const task = await createTask(await request(`${uri}/next`, null), "draft", workspace.revision);
  const original = await candidateRequest(`${base}/drafts/checkpoints`, { expectedRevision: workspace.revision,
    expectedCandidateRevisionId: null, idempotencyKey: "draft", taskId: task.id, threadId: "draft", payload: {
      chapterId: materialized.currentChapterId, chapterNumber: 1, title: "信", body: "他烧掉密函。",
      sceneGroups: [{ id: "group-1", sceneId: "scene-1", body: "他烧掉密函。", locked: false }],
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
    oldTaskId: task.id, draft, newPlan, run, createTask, payload };
}
