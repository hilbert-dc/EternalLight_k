import assert from "node:assert/strict";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { PersistentTaskBudget } from "@eternallight/agent-engine";

import { mainHandlers, rendererEvent, runtimeFixture } from "./main-handler-harness.mjs";

const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));
const command = `${process.execPath} ${fixture}`;

test("generation leaf sends its unique operation after Runtime freezes the whole author goal", async () => {
  const authorGoal = "[要求版本 0] 实际创作并保存第1–3章，每章1500–2000个中文字符；保存必要规划，不正式发布。";
  const operations = {
    generateBrief: { kind: "brief", tool: "save_brief", responsibility: "只建立本作品创作简报候选" },
    generateOutline: { kind: "outline", tool: "save_outline", responsibility: "只生成全书层级大纲候选" },
    generatePlan: { kind: "plan", tool: "save_chapter_plans", responsibility: "只生成近章章纲候选" },
    generateChapter: { kind: "draft", tool: "save_draft_checkpoint", responsibility: "只生成或修改冻结 target" },
  };
  for (const [operation, contract] of Object.entries(operations)) {
    const runtime = runtimeFixture();
    const runId = `generation-${"e".repeat(32)}`, workId = `work-${runId}`;
    const owner = { id: workId, status: "running", version: 0, kind: "generation", steps: [],
      initialBudget: { maxRequests: null, maxTokens: null, deadlineAt: null } };
    runtime.workTasks.set(workId, owner);
    const runPath = `/api/projects/project/generation-runs/${runId}`;
    let run = { id: runId, projectId: "project", version: 7, status: "running", stage: "coordinate",
      activeTaskId: null, deadlineAt: null, providerBudget: { charges: [] } };
    const target = { type: operation === "generateChapter" ? "chapter" : "artifact", runId,
      runVersion: 7, stepToken: "step", intentGeneration: 0, resumeStepId: "admitted-step",
      ...(operation === "generateChapter" ? { chapterId: "chapter-one", chapterNumber: 1, sceneOrder: 1 } : {}),
      ...(operation === "generatePlan" ? { planFromChapter: 1 } : {}) };
    owner.steps.push({ id: "admitted-step", operation, status: "queued", target });
    const sent = [], frozen = [];
    const { handlers, adapter } = await mainHandlers(async (path, body, options) => {
      if (path === runPath) return structuredClone(run);
      if (path.endsWith("/creative-workspace")) return { revision: 0, tasks: [], chapters: [{ id: "chapter-one", number: 1 }] };
      if (path.endsWith("/next")) return { run: structuredClone(run), action: run.status === "completed" ? null
        : { kind: "agentTask", operation, authorIntent: authorGoal, target } };
      if (path.endsWith("/provider-budget")) return { taskId: run.activeTaskId, workTaskId: workId,
        budgetId: `budget-${run.activeTaskId}`, ...owner.initialBudget };
      if (path.endsWith("/provider-usage")) { run.version = 9; run.providerBudget.charges.push({ taskId: body.taskId }); return structuredClone(run); }
      if (path.endsWith("/advance-task")) { run = { ...run, version: 10, status: "completed", activeTaskId: null }; return run; }
      const result = await runtime.request(path, body, options);
      if (path.endsWith("/tasks") && body.operation === operation) {
        // Match the real generation_author_source::freeze contract: source is the
        // original author goal, not the Host's transient stage-prefixed prompt.
        result.inputManifest = { ...result.inputManifest, authorIntent: authorGoal, authorMessageId: "author-one",
          generationAuthorIntentBinding: { runId, authorMessageId: "author-one", intentGeneration: 0, chapterNumber: 1 } };
        result.purpose = operation === "generateChapter" ? "writer" : "planner";
        result.requiredOutputKind = contract.kind;
        frozen.push(structuredClone(result));
        run = { ...run, version: 8, activeTaskId: result.id };
      }
      return result;
    }, command);
    const send = adapter.sendTurn.bind(adapter);
    adapter.sendTurn = async (input, options) => { sent.push(input); return send(input, options); };
    try {
      assert.equal((await handlers.get("agent:continue-work")(rendererEvent(), "project", runId)).status, "completed");
      assert.equal(sent.length, 1);
      const text = sent[0].map(item => item.text).join("");
      assert.ok(text.includes(contract.responsibility) && text.includes(contract.tool), operation);
      assert.ok(text.includes("read_task_source"), "the worker reads frozen source pages on demand");
      assert.ok(text.includes(authorGoal), "complete author source remains present");
      assert.equal(frozen[0].inputManifest.authorIntent, authorGoal);
      assert.deepEqual(frozen[0].target, target);
      assert.equal(runtime.completions.length, 1);
      assert.equal(run.providerBudget.charges.length, 1);
    } finally { adapter.close(); }
  }
});

test("automatic generation executes commitment extraction through the existing Agent task path", async () => {
  const directory = await mkdtemp(join(tmpdir(), "novel-generation-feedback-"));
  const budgetDb = join(directory, "engine-budget.sqlite3");
  const runtime = runtimeFixture();
  const runId = `generation-${"c".repeat(32)}`;
  runtime.workTasks.set(`work-${runId}`, { id: `work-${runId}`, status: "running", version: 0, kind: "generation", steps: [{ id: "admitted-step", operation: "extractCommitments", status: "queued" }] });
  const runPath = `/api/projects/project/generation-runs/${runId}`;
  let run = { id: runId, projectId: "project", version: 7, status: "running", stage: "coordinate", activeTaskId: null,
    deadlineAt: Math.floor(Date.now() / 1000) + 600,
    providerBudget: { maxRequests: 64, maxTokens: 1_000_000, usedRequests: 0, usedTokens: 0, unknownTokens: 0, overrun: false, charges: [] } };
  const targets = [];
  const { handlers, adapter } = await mainHandlers(async (path, body, options) => {
    if (path === runPath) return { ...run };
    if (path.endsWith("/provider-budget")) return { taskId: run.activeTaskId, workTaskId: `work-${runId}`,
      budgetId: `budget-${run.activeTaskId}`, maxRequests: 64, maxTokens: 1_000_000, deadlineAt: run.deadlineAt };
    if (path.endsWith("/next")) return run.status === "completed" ? { run, action: null } : {
      run, action: { kind: "agentTask", operation: "extractCommitments", authorIntent: "留下灯光线索",
        target: { type: "commitmentExtraction", runId, runVersion: 7, stepToken: "step", intentGeneration: 0, resumeStepId: "admitted-step",
          chapterId: "chapter-1", chapterNumber: 1, sourceRevisionId: "draft-1" } },
    };
    if (path.endsWith("/advance-task")) {
      assert.deepEqual(body, { expectedVersion: 9, taskId: run.activeTaskId });
      run = { ...run, version: 10, status: "completed", stage: "finished", activeTaskId: null };
      return run;
    }
    if (path.endsWith("/provider-usage")) {
      const receipt = PersistentTaskBudget.receipt(budgetDb, body.taskId);
      assert.equal(receipt.runId, run.activeTaskId);
      run = { ...run, version: 9, providerBudget: { ...run.providerBudget, usedRequests: receipt.usage.requests,
        usedTokens: receipt.usage.tokens, unknownTokens: receipt.usage.unknownTokens,
        charges: [{ taskId: body.taskId, budgetId: receipt.budgetId, status: receipt.status }] } };
      return run;
    }
    const result = await runtime.request(path, body, options);
    if (path.endsWith("/tasks") && body.operation === "extractCommitments") {
      targets.push(body);
      result.purpose = "critic";
      result.requiredOutputKind = "commitmentExtraction";
      run = { ...run, version: 8, activeTaskId: result.id };
    }
    return result;
  }, command, { agentStorage: { budgetDb } });
  try {
    const result = await handlers.get("agent:continue-work")(rendererEvent(), "project", runId);
    assert.equal(result.status, "completed");
    assert.equal(targets.length, 1);
    assert.equal(targets[0].operation, "extractCommitments");
    assert.equal(targets[0].target.sourceRevisionId, "draft-1");
    assert.equal(targets[0].authorIntent, "留下灯光线索");
    assert.equal(targets[0].target.resumeStepId, "admitted-step");
    assert.equal(runtime.completions.length, 1);
    assert.equal(run.providerBudget.charges.length, 1);
  } finally { adapter.close(); await rm(directory, { recursive: true, force: true }); }
});

test("desktop binds feedback decisions to the author assessment and resumes the same run", async () => {
  const requests = [];
  const runId = `generation-${"a".repeat(32)}`;
  const chapterId = `chapter-${"b".repeat(32)}`;
  const runPath = `/api/projects/project/generation-runs/${runId}`;
  const { handlers, adapter } = await mainHandlers(async (path, body, options) => {
    requests.push({ path, body, method: options?.method ?? "GET" });
    if (path.endsWith("/scene-assessments")) return { revisionId: "assessment-rev", state: "current", assessment: { assessments: [] } };
    if (path === runPath) return { version: 7 };
    if (path.endsWith("/resolve-feedback")) return { status: "running" };
    if (path.endsWith(`/work-tasks/work-${runId}`)) return { status: "running" };
    if (path.endsWith("/next")) return { run: { id: runId, status: "completed" }, action: null };
    throw new Error(`Unexpected Runtime request: ${path}`);
  }, command);
  try {
    const assessment = await handlers.get("runtime:get-scene-assessment")(rendererEvent(), "project", chapterId);
    assert.equal(assessment.revisionId, "assessment-rev");
    const finished = await handlers.get("agent:resolve-generation-feedback")(
      rendererEvent(), "project", runId, assessment.revisionId, "repairDraft", "补上第二场事件",
    );
    assert.equal(finished.status, "completed");
    assert.deepEqual(requests.slice(1), [
      { path: runPath, body: null, method: "GET" },
      { path: `${runPath}/resolve-feedback`, body: { expectedVersion: 7, assessmentRevisionId: "assessment-rev", choice: "repairDraft", instruction: "补上第二场事件" }, method: "POST" },
      { path: `/api/projects/project/work-tasks/work-${runId}`, body: null, method: "GET" },
      { path: `${runPath}/next`, body: {}, method: "POST" },
    ]);
    await assert.rejects(
      handlers.get("agent:resolve-generation-feedback")(rendererEvent(), "project", runId, assessment.revisionId, "overwriteCanon", ""),
      /参数无效/,
    );
  } finally {
    adapter.close();
  }
});

test("opening a work reconciles terminal generation usage without hiding an unsettled run", async () => {
  const runId = `generation-${"d".repeat(32)}`;
  const taskId = `task-${"e".repeat(32)}`;
  const root = "/api/projects/project";
  let canCharge = false;
  let chargeCalls = 0;
  let run = { id: runId, version: 4, status: "blocked", activeTaskId: taskId,
    providerBudget: { maxRequests: 8, maxTokens: 100, usedRequests: 0, usedTokens: 0, unknownTokens: 0, overrun: false, charges: [] } };
  const { handlers, adapter } = await mainHandlers(async (path, body) => {
    if (path === `${root}/generation-runs`) return { runs: [run] };
    if (path === `${root}/tasks/${taskId}`) return { status: "failed" };
    if (path === `${root}/generation-runs/${runId}/provider-usage`) {
      assert.deepEqual(body, { taskId });
      chargeCalls += 1;
      if (!canCharge) throw new Error("terminal receipt unavailable");
      run = { ...run, version: 5, providerBudget: { ...run.providerBudget,
        usedRequests: 1, unknownTokens: 20,
        charges: [{ taskId, budgetId: "budget-recovered", status: "recovered_unknown", requests: 1, tokens: 0, unknownTokens: 20, overrun: false }] } };
      return run;
    }
    throw new Error(`Unexpected Runtime request: ${path}`);
  }, command);
  try {
    const list = () => handlers.get("runtime:list-generation-runs")(rendererEvent(), "project");
    const unsettled = await list();
    assert.equal(unsettled.runs[0].usageUnsettled, true);
    assert.equal(unsettled.runs[0].providerBudget.usedRequests, 0);
    canCharge = true;
    const settled = await list();
    assert.equal(settled.runs[0].usageUnsettled, false);
    assert.equal(settled.runs[0].providerBudget.unknownTokens, 20);
    assert.deepEqual(await list(), settled);
    assert.equal(chargeCalls, 2);
  } finally { adapter.close(); }
});

test("generation follows a newly admitted decision after failure without replaying the failed step", async () => {
  const runtime = runtimeFixture();
  const runId = `generation-${"f".repeat(32)}`, workId = `work-${runId}`;
  const base = `/api/projects/project/generation-runs/${runId}`;
  const target = { type: "artifact", runId, runVersion: 1, stepToken: "first", intentGeneration: 0, resumeStepId: "writer-step" };
  const work = { id: workId, projectId: "project", threadId: "thr-test", status: "running", kind: "generation", version: 1,
    initialBudget: { maxRequests: null, maxTokens: null, deadlineAt: null },
    steps: [{ id: "writer-step", operation: "generateBrief", status: "queued", target }] };
  runtime.workTasks.set(workId, work);
  let run = { id: runId, projectId: "project", version: 1, status: "running", stage: "coordinate", activeTaskId: null, deadlineAt: null };
  let correction = false, starts = 0;
  const attemptedSteps = [];
  const { handlers, adapter } = await mainHandlers(async (path, body, options) => {
    if (path === base) return structuredClone(run);
    if (path.endsWith("/next")) {
      const step = work.steps.at(-1);
      return { run: structuredClone(run), action: run.status === "completed" ? null : {
        kind: "agentTask", operation: step.operation, instruction: "完成原作者目标", target: step.target,
      } };
    }
    if (path.endsWith("/provider-budget")) return { taskId: run.activeTaskId, workTaskId: workId, budgetId: `budget-${run.activeTaskId}`, ...work.initialBudget };
    if (path.endsWith("/provider-usage")) { run.version++; return structuredClone(run); }
    if (path.endsWith("/advance-task")) { run = { ...run, status: "completed", activeTaskId: null, version: run.version + 1 }; return structuredClone(run); }
    const result = await runtime.request(path, body, options);
    if (path.endsWith("/tasks") && body) {
      attemptedSteps.push(body.target.resumeStepId);
      assert.equal(body.initialBudget, undefined, "a successor must not mint a root permit");
      run.activeTaskId = result.id; run.version = body.target.runVersion + 1;
    }
    if (options?.method === "PATCH" && body.status === "failed") {
      correction = true;
      work.steps[0].status = "superseded";
      run.activeTaskId = null;
      work.steps.push({ id: "correction-step", operation: "advisory", status: "queued", target: {
        runId, runVersion: run.version + 1, stepToken: "correction", intentGeneration: 0, resumeStepId: "correction-step",
        workDecision: { workTaskId: workId, originTaskId: null, sourceTaskId: result.id ?? null },
      } });
    }
    return result;
  }, command);
  const send = adapter.sendTurn.bind(adapter);
  adapter.sendTurn = async (...args) => { starts++; if (starts === 1) throw new Error("invalid_tool_arguments"); return send(...args); };
  try {
    const result = await handlers.get("agent:continue-work")(rendererEvent(), "project", runId);
    assert.equal(result.status, "completed");
    assert.equal(correction, true);
    assert.equal(starts, 2);
    assert.deepEqual(attemptedSteps, ["writer-step", "correction-step"]);
    assert.equal(runtime.workTasks.size, 1);
  } finally { adapter.close(); }
});
