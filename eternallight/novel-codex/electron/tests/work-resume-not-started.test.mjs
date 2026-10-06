import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { mainHandlers, rendererEvent } from "./main-handler-harness.mjs";

const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;
for (const boundary of ["writerStart", "advisoryStart", "workspace", "lostCreate", "foreign", "claimed", "cancelled", "generation", "generationFirst", "generationContinue", "generationRetry", "generationStart", "generationLostCreate"]) {
  test(`resume preserves original Work after ${boundary}`, async () => {
    const generation = boundary.startsWith("generation"), advisory = ["advisoryStart", "generationStart"].includes(boundary);
    const runId = `generation-${"a".repeat(32)}`;
    const work = { id: generation ? `work-${runId}` : "work-original", projectId: "project", threadId: "original-thread",
      kind: generation ? "generation" : "conversation", status: "paused", version: 7, goal: "修订已有正文",
      steps: [{ id: "step-original", operation: advisory ? "advisory" : "generateChapter", status: "queued",
        currentAttemptId: null, target: { chapterId: "chapter-original" } }] };
    if (advisory) work.steps[0].target = { type: "project", workDecision: { workTaskId: work.id }, ...(generation ? { runId } : {}) };
    if (["generationFirst", "generationStart"].includes(boundary)) work.threadId = "";
    const run = { id: runId, projectId: "project", status: "paused", version: 3, activeTaskId: null, authorIntent: work.goal };
    const original = new Error("Core thread/start timed out");
    const requests = [], state = { budgets: [], confirmations: [] };
    const directGeneration = ["generationContinue", "generationRetry", "generationStart", "generationLostCreate"].includes(boundary);
    if (["generationContinue", "generationLostCreate"].includes(boundary)) { work.status = "running"; run.status = "running"; }
    let failed = false, turns = 0;
    const main = await mainHandlers(async (path, body, options) => {
      requests.push({ path, body, method: options?.method ?? "GET" });
      if (path.endsWith("/control")) {
        const owner = path.includes("/generation-runs/") ? run : work;
        assert.equal(body.expectedVersion, owner.version);
        owner.status = body.command === "resume" ? "running" : "paused"; owner.version++;
        if (generation) { work.status = owner.status; work.version++; }
        return structuredClone(owner);
      }
      if (path.endsWith(`/work-tasks/${work.id}`)) return { ...structuredClone(work),
        ...(failed && boundary === "foreign" ? { threadId: "foreign-thread" } : {}) };
      if (path.endsWith("/provider-usage")) return { pending: null };
      if (path.endsWith("/budget-authorization")) return { authorization: { deadlineAt: null } };
      if (path.endsWith("/retry-failed-task") || (path.endsWith("/generation-runs") && body)) {
        work.status = "running"; run.status = "running"; return structuredClone(run);
      }
      if (path.endsWith(`/generation-runs/${runId}`)) return structuredClone(run);
      if (path.endsWith("/next")) return { run: structuredClone(run), action: { kind: "agentTask", operation: work.steps[0].operation,
        instruction: work.goal, target: { ...work.steps[0].target, runId, resumeStepId: "step-original" } } };
      if (path.endsWith("/decision")) return { workTaskId: work.id, threadId: work.threadId, step: null, stopReason: "workNotReady" };
      if (path.endsWith("/creative-workspace")) {
        if (boundary === "workspace") { failed = true; throw original; }
        return { project: { id: "project" }, revision: 0, headSnapshotId: "snapshot", sourceDigest: "a".repeat(64) };
      }
      if (path.endsWith("/tasks")) {
        failed = true;
        if (["lostCreate", "generationLostCreate"].includes(boundary)) { work.steps[0].currentAttemptId = "persisted-lost-task"; work.steps[0].status = "running"; }
        throw original;
      }
      assert.fail(`Unexpected Runtime request: ${path}`);
    }, command, state);
    main.adapter.ensureSession = async () => {
      if (["workspace", "lostCreate", "generationLostCreate"].includes(boundary)) return "original-thread";
      failed = true;
      if (boundary === "claimed") { work.steps[0].currentAttemptId = "other-task"; work.steps[0].status = "running"; }
      if (boundary === "cancelled") await main.handlers.get("agent:interrupt")();
      throw original;
    };
    main.adapter.resumeThread = async () => {};
    const adapterRequest = main.adapter.request;
    main.adapter.request = async function(method, params) {
      if (method === "turn/start") turns++;
      return adapterRequest.call(this, method, params);
    };
    try {
      const promise = boundary === "generationStart"
        ? main.handlers.get("agent:generate-work")(rendererEvent(), "project", work.goal, 1, 0, "snapshot", "a".repeat(64), null)
        : directGeneration ? main.handlers.get(boundary === "generationRetry" ? "agent:retry-work" : "agent:continue-work")(rendererEvent(), "project", runId)
          : main.handlers.get("agent:resume-work-task")(rendererEvent(), "project", work.id);
      if (["lostCreate", "generationLostCreate", "foreign", "claimed", "cancelled"].includes(boundary)) {
        await assert.rejects(promise, error => boundary === "cancelled" ? error.code === "TASK_CANCELLED" : error.message === original.message);
        assert.equal(requests.filter(item => item.body?.command === "pause").length, 0);
      } else {
        const result = await promise;
        assert.equal(result.id, directGeneration ? run.id : work.id); assert.equal(result.status, "paused");
        assert.deepEqual(JSON.parse(JSON.stringify(result.resumeFailure)), {
          code: "WORK_RESUME_NOT_STARTED", message: original.message, workTaskId: work.id,
        });
        if (directGeneration) assert.deepEqual(JSON.parse(JSON.stringify(result.pausedWork)), {
          id: work.id, projectId: "project", threadId: work.threadId, status: "paused", version: work.version,
        });
        else assert.deepEqual(result.steps.map(item => item.currentAttemptId), [null]);
        assert.equal(requests.filter(item => item.body?.command === "pause").length, 1);
      }
      assert.equal(turns, 0);
      assert.equal(requests.filter(item => item.path.endsWith("/tasks")).length, ["lostCreate", "generationLostCreate"].includes(boundary) ? 1 : 0);
      assert.equal(requests.filter(item => item.method === "POST" && /provider-usage|budget-authorization/.test(item.path)).length, 0);
      assert.deepEqual(state.budgets, []); assert.deepEqual(state.confirmations, []);
    } finally { main.adapter.close(); main.chapterStructureConsumer.stop(); }
  });
}
