import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { PersistentTaskBudget } from "@eternallight/agent-engine";
import { taskProviderBudgetOptions } from "../../electron/task-provider-budget.mjs";
import { assertFeedbackIsolation, createProbeRevisionScope, summarizeFeedback } from "../probe-live-revision.mjs";

test("probe uses approved production goals and canonical report receipts, not final reply JSON", { timeout: 120000 }, async t => {
  // Scripted author/model payloads through real HTTP/SQLite; no vendor calls.
  const root = await mkdtemp(join(tmpdir(), "novel-probe-scope-"));
  const budgetPath = join(root, "budget.sqlite3");
  const token = "fixture-author-token-123456789";
  const candidate = "fixture-candidate-token-123456789";
  const suffix = process.platform === "win32" ? ".exe" : "";
  const runtime = spawn(fileURLToPath(new URL(`../../native/target/debug/novel-codex-story-runtime${suffix}`, import.meta.url)),
    ["--port", "0", "--db", join(root, "story.sqlite3"), "--engine-budget-db", budgetPath, "--token-stdin"],
    { stdio: ["pipe", "pipe", "ignore"] });
  t.after(async () => {
    if (runtime.exitCode === null && runtime.signalCode === null) {
      const stopped = new Promise(resolve => runtime.once("exit", resolve)); runtime.kill(); await stopped;
    }
    await rm(root, { recursive: true, force: true });
  });
  const origin = await new Promise((resolve, reject) => {
    const lines = createInterface({ input: runtime.stdout });
    lines.on("line", line => { if (line.startsWith("Story Runtime listening on ")) resolve(line.slice(27)); });
    runtime.once("error", reject); runtime.once("exit", () => reject(new Error("Runtime startup failed")));
    runtime.stdin.end(`${JSON.stringify({ authorToken: token, candidateToken: candidate })}\n`);
  });
  const call = async (method, path, payload, capability = null, status = null) => {
    const response = await fetch(`${origin}${path}`, { method, signal: AbortSignal.timeout(10000),
      headers: { authorization: `Bearer ${capability ? candidate : token}`, "content-type": "application/json",
        ...(capability ? { "x-novel-task-capability": capability } : {}) },
      ...(payload === undefined ? {} : { body: JSON.stringify(payload) }) });
    const value = await response.json();
    if (status) assert.equal(response.status, status, JSON.stringify(value));
    else assert.ok(response.ok, `${method} ${path}: ${response.status} ${JSON.stringify(value)}`);
    return value;
  };
  const { project } = await call("POST", "/api/projects", { title: "结构化反馈" });
  const base = `/api/projects/${project.id}`;
  const original = { chapterId: "chapter-one", chapterNumber: 1, title: "密函", body: "她烧信。",
    sceneGroups: [{ id: "letter", sceneId: null, body: "她烧信。", locked: false }] };
  await call("POST", `${base}/drafts/checkpoints`, { expectedRevision: 0, expectedCandidateRevisionId: null,
    idempotencyKey: "source", payload: original });
  const before = await call("GET", `${base}/creative-workspace`);
  const approval = await createProbeRevisionScope(call, base, before, before.chapters[0].candidate,
    { maxRequests: 5, maxTokens: 140000, timeoutSeconds: 180, budgetPath });
  const capability = "fixture-phase-capability-123456789";
  const claim = async (operation, target, threadId) => call("POST", `${base}/tasks`, {
    operation, target, inputRevision: 0, threadId, authorIntent: approval.intent,
    capabilityHash: createHash("sha256").update(capability).digest("hex"),
    capabilityExpiresAt: Math.floor(Date.now() / 1000) + 600,
  });
  const complete = async task => {
    const grant = await call("GET", `${base}/tasks/${task.id}/provider-budget`);
    await call("POST", `${base}/tasks/${task.id}/complete`, { threadId: task.threadId, summary: "脚本候选夹具完成" }, capability);
    const budget = new PersistentTaskBudget({ databasePath: budgetPath, runId: task.id,
      ...taskProviderBudgetOptions(grant, { taskId: task.id, workTaskId: grant.workTaskId, deadlineAt: grant.deadlineAt }) });
    budget.complete("completed"); budget.close();
    await call("POST", `${base}/work-tasks/${grant.workTaskId}/provider-usage`, { taskId: task.id });
  };
  const writer = await claim("generateChapter", approval.target, "writer");
  const body = "🌙她藏信，为未来留下证词。";
  const saved = await call("POST", `${base}/drafts/checkpoints`, { taskId: writer.id, threadId: writer.threadId,
    expectedRevision: 0, expectedCandidateRevisionId: before.chapters[0].candidate.id, idempotencyKey: "new-draft",
    completeness: "chapterComplete", payload: { ...original, body, sceneGroups: [{ ...original.sceneGroups[0], body }] } }, capability);
  await complete(writer);
  const revised = await call("GET", `${base}/creative-workspace`);
  const batch = await call("POST", `${base}/modeling-batches`, { objectId: "draft-chapter-one",
    sourceRevisionId: saved.revisionId, expectedRevision: 0, extractorVersion: "probe-scope-fixture" });
  const model = await claim("modelKnowledge", { batchId: batch.id, sourceTaskId: writer.id }, "model");
  const published = await call("POST", `${base}/modeling-batches/${batch.id}/publish`, { taskId: model.id,
    threadId: model.threadId, expectedRevision: 0,
    entities: [{ id: "letter", kind: "item", displayName: "信", evidence: { quote: "她藏信" } }], assertions: [] }, capability);
  assert.equal(published.publicationState, "committed");
  assert.equal(published.nextAction, "complete_task");
  assert.equal(published.entities, undefined, "publication returns a receipt, not the entire input again");
  const conflict = await call("POST", `${base}/modeling-batches/${batch.id}/publish`, { taskId: model.id,
    threadId: model.threadId, expectedRevision: 0,
    entities: [{ id: "letter", kind: "item", displayName: "信", aliases: ["密函"], evidence: { quote: "她藏信" } }],
    assertions: [] }, capability, 409);
  assert.deepEqual(conflict.error.recovery, { nextAction: "complete_task", retryPublication: false });
  assert.equal(conflict.error.details.publicationState, "committed");
  const durable = await call("GET", `${base}/knowledge`);
  assert.deepEqual(durable.sources[0].entities[0], { id: "letter", kind: "item", displayName: "信", aliases: [],
    evidence: { quote: "她藏信", start: 1, end: 4 } });
  await complete(model);
  const baseline = await call("GET", `${base}/creative-workspace`);
  assert.notEqual(baseline.sourceDigest, revised.sourceDigest, "knowledge publication legitimately changes aggregate source digest");
  const knowledge = await call("GET", `${base}/knowledge`);
  const work = await call("GET", `${base}/tasks/${writer.id}/work-task`);
  const step = work.steps.find(item => item.operation === "assessRevisionOutcome" && item.status === "queued");
  assert.ok(step);
  const assessment = await claim("assessRevisionOutcome", { ...step.target, resumeStepId: step.id }, "critic");
  const payload = Object.fromEntries(["writerTaskId", "chapterId", "originalRevisionId", "draftRevisionId", "knowledgeBatchId"]
    .map(key => [key, step.target[key]]));
  payload.outcomes = [
    { id: "retain-letter", status: "supported", reason: "正文写明藏信", evidence: [{ groupId: "letter", quote: "她藏信" }] },
    { id: "testify-motive", status: "supported", reason: "正文写明留下证词", evidence: [{ groupId: "letter", quote: "为未来留下证词" }] },
  ];
  const binding = { taskId: assessment.id, threadId: assessment.threadId, expectedRevision: 0,
    expectedCandidateRevisionId: null, idempotencyKey: "outcome", payload };
  const route = `${base}/revision-outcomes/${writer.id}`;
  const invalid = structuredClone(binding);
  invalid.idempotencyKey = "bad-anchor";
  Object.assign(invalid.payload.outcomes[0].evidence[0], { start: 2, end: 5 });
  const rejected = await call("POST", route, invalid, capability, 400);
  assert.match(rejected.error.message, /evidence_anchor_mismatch/);
  assert.equal(await call("GET", route), null);
  const receipt = await call("POST", route, binding, capability);
  assert.deepEqual(await call("POST", route, binding, capability), receipt);
  await complete(assessment);
  const report = await call("GET", route);
  assert.equal(report.revisionId, receipt.revisionId);
  assert.equal(summarizeFeedback(report, revised.chapters[0].candidate).goals.length, 2);
  const after = await call("GET", `${base}/creative-workspace`);
  assert.notEqual(after.sourceDigest, baseline.sourceDigest, "saving the analysis artifact legitimately changes aggregate source digest");
  assertFeedbackIsolation(baseline, after, knowledge, await call("GET", `${base}/knowledge`));
});
