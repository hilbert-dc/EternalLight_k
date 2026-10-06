import assert from "node:assert/strict";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { existsSync } from "node:fs";
import { PersistentTaskBudget } from "@eternallight/agent-engine";
import { mainHandlers, rendererEvent, runtimeFixture } from "./main-handler-harness.mjs";
import { CHAPTER_STRUCTURE_ANALYSIS_PROMPT } from "../chapter-structure-analysis.mjs";

const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;
const workspace = { revision: 9, headSnapshotId: "snapshot-now", sourceDigest: "source-now", tasks: [] };
const deletion = () => ({ id: "change-one", projectId: "project", kind: "chapterDeleted", chapterId: "chapter-one",
  chapterNumber: 2, title: "删除前章节", createdAt: 100, baseRevision: 2, resultRevision: 3,
  baseSnapshotId: "snapshot-before", resultSnapshotId: "snapshot-after",
  source: { objectId: "draft-chapter-one", savedRevisionId: "saved-old", acceptedRevisionId: null, workingRevisionId: null, knowledgeBatchId: null },
  previousChapter: { id: "chapter-first", number: 1 }, nextChapter: { id: "chapter-last", number: 4 },
  assessment: { state: "pending", skipReason: null, taskId: null, workTaskId: null } });
const deferred = () => { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; };

async function setup({ enabled = true, lostCreate = false, holdWorkspace = null, holdCreate = null, createReady = null, readTask = task => task, existingBudget = false } = {}) {
  const event = rendererEvent(); const events = [];
  event.sender.send = (_channel, payload) => events.push(payload);
  const state = { rendererWebContents: event.sender, confirmations: [], budgets: [], leases: [], charges: [] };
  const ledger = runtimeFixture(); const requests = []; let item = deletion();
  const runtime = async (path, body, options, url) => {
    requests.push({ path, body, method: options.method ?? "GET" });
    if (path.endsWith("/chapter-structure-changes")) {
      assert.equal(url.searchParams.get("assessmentState"), "pending");
      return { projectId: "project", items: item.assessment.state === "pending" ? [item] : [], nextCursor: null };
    }
    if (path.endsWith("/chapter-structure-changes/change-one")) return item;
    if (path.endsWith("/creative-workspace")) { if (holdWorkspace) await holdWorkspace.promise; return workspace; }
    const task = [...ledger.tasks.values()].find(task => path === `/api/projects/project/tasks/${task.id}`);
    if (task) {
      if (options.method === "PATCH") {
        assert.equal(body.threadId, task.threadId); Object.assign(task, body);
        const work = ledger.workTasks.get(item.assessment.workTaskId);
        work.status = body.status === "cancelled" ? "cancelled" : "blocked";
        work.resumeMode = "continueExecution"; work.version += 1;
        work.steps.find(step => step.currentAttemptId === task.id).status = body.status === "cancelled" ? "cancelled" : "blocked";
      }
      return readTask(task);
    }
    if (path.endsWith("/provider-usage") && body) {
      const charged = [...ledger.tasks.values()].find(task => task.id === body.taskId);
      assert.ok(["completed", "failed", "cancelled"].includes(charged.status), "Native usage settlement requires a terminal task");
      const receipt = PersistentTaskBudget.receipt(state.agentStorage.budgetDb, body.taskId);
      if (!state.charges.some(charge => charge.runId === body.taskId)) state.charges.push(receipt);
    }
    if (path.endsWith("/control") && body?.command === "resume") {
      const work = ledger.workTasks.get(path.split("/work-tasks/")[1].split("/")[0]);
      work.steps.filter(step => step.status === "blocked").forEach(step => { step.status = "queued"; });
    }
    const result = await ledger.request(path, body);
    if (path.endsWith("/tasks") && body) {
      result.purpose = "critic";
      result.requiredOutputKind = null; result.status = "queued"; result.turnId = null; result.outputRefs = [];
      result.inputManifest = { ...result.inputManifest, authorIntent: body.authorIntent, authorMessageId: body.initialBudget?.authorMessageId };
      const work = [...ledger.workTasks.values()].find(work => work.steps.some(step => step.currentAttemptId === result.id)); work.kind = "direct";
      const step = work.steps.find(step => step.currentAttemptId === result.id);
      Object.assign(step, { operation: "assessChapterStructure", target: { type: "project", structureChangeId: item.id }, status: "running", inputRevision: body.inputRevision });
      if (!body.target.resumeStepId) {
        item = { ...item, assessment: { state: "bound", skipReason: null, taskId: result.id, workTaskId: work.id } };
        if (existingBudget) {
          const budget = new PersistentTaskBudget({ databasePath: state.agentStorage.budgetDb, runId: result.id, id: `budget-${result.id}` });
          budget.complete("failed"); budget.close();
        }
        createReady?.resolve();
        if (holdCreate) await holdCreate.promise;
        if (lostCreate) throw new Error("create response lost");
      }
    }
    if (path.endsWith("/complete")) {
      const taskId = path.split("/").at(-2); const work = [...ledger.workTasks.values()].find(work => work.steps.some(step => step.currentAttemptId === taskId));
      [...ledger.tasks.values()].find(task => task.id === taskId).status = "completed";
      work.status = "completed"; work.steps.find(step => step.currentAttemptId === taskId).status = "completed";
    }
    return result;
  };
  const main = await mainHandlers(runtime, enabled ? command : null, state);
  main.handlers.get("agent:set-workspace-context")(event, { projectId: "project", dirty: false });
  const close = () => { main.chapterStructureConsumer.stop(); main.adapter.close(); };
  return { ...main, event, events, state, ledger, requests, close, readChange: () => item };
}

test("production consumer binds dedicated root budget, actual Adapter turn/final reply and completion without candidate dispatch", async () => {
  const fixture = await setup();
  try {
    await fixture.chapterStructureConsumer.tick();
    assert.deepEqual(fixture.events.filter(message => message?.method === "structure/error"), []);
    const create = fixture.requests.find(request => request.path.endsWith("/tasks") && request.body);
    assert.deepEqual({ operation: create.body.operation, target: create.body.target, intent: create.body.authorIntent, inputRevision: create.body.inputRevision },
      { operation: "assessChapterStructure", target: { type: "project", structureChangeId: "change-one" }, intent: CHAPTER_STRUCTURE_ANALYSIS_PROMPT, inputRevision: 9 });
    assert.deepEqual({ ...create.body.initialBudget, authorizationId: "present", authorMessageId: "present" }, {
      authorizationId: "present", authorMessageId: "present", expectedBaseSnapshotId: "snapshot-now", expectedSourceDigest: "source-now", maxRequests: null, maxTokens: null, deadlineAt: null });
    assert.match(create.body.initialBudget.authorizationId, /^[a-f0-9-]{36}$/); assert.match(create.body.initialBudget.authorMessageId, /^[a-f0-9-]{36}$/);
    const task = [...fixture.ledger.tasks.values()][0];
    const turn = fixture.requests.find(request => request.path.endsWith("/turn"));
    const reply = fixture.requests.find(request => request.path.endsWith("/final-reply"));
    assert.deepEqual(turn.body, { threadId: "thr-test", turnId: "turn-test" });
    assert.equal(reply.body.text, "hello"); assert.equal(reply.body.turnId, "turn-test");
    assert.equal(fixture.ledger.completions.length, 1); assert.equal(fixture.state.budgets.length, 1);
    assert.equal(fixture.state.budgets[0].id, `budget-${task.id}`); assert.equal(fixture.state.confirmations.length, 0);
    assert.ok(fixture.events.some(message => message?.method === "agent/structure-started" && message.params.taskId === task.id));
    const finished = fixture.events.find(message => message?.method === "turn/completed" && message.params.turn?.receivedParams);
    const submitted = finished.params.turn.receivedParams.input.map(item => item.text ?? "").join("\n");
    assert.match(submitted, /read_chapter_structure_source/); assert.match(submitted, /"operation": "assessChapterStructure"/);
    assert.match(submitted, /"structureChangeId": "change-one"/); assert.match(submitted, /不得生成候选/);
    assert.equal(fixture.requests.filter(request => /\/(actions|planning-reviews|approve|delete)$/.test(request.path)).length, 0);
    assert.equal(fixture.requests.filter(request => request.path.includes("/draft-checkpoints") || request.path.endsWith("/candidates")).length, 0);
    await fixture.chapterStructureConsumer.tick();
    assert.equal(fixture.requests.filter(request => request.path.endsWith("/tasks") && request.body).length, 1);
  } finally { fixture.close(); }
});

test("production evidence preflight fails before task/budget/provider when previously verified capabilities are gone", async () => {
  const fixture = await setup();
  try {
    fixture.state.fixtureEvidence = [];
    await fixture.chapterStructureConsumer.tick();
    assert.equal(fixture.requests.filter(request => request.path.endsWith("/tasks") && request.body).length, 0);
    assert.deepEqual(fixture.state.budgets, []); assert.deepEqual(fixture.state.leases, []);
    assert.equal(fixture.adapter.process, null);
    assert.ok(fixture.events.some(message => message?.method === "structure/error"));
    assert.equal(fixture.readChange().assessment.state, "pending");
  } finally { fixture.close(); }
});

for (const condition of ["offline", "dirty", "untrusted"]) test(`production automatic consumer does not create a task while ${condition}`, async () => {
  const fixture = await setup({ enabled: condition !== "offline" });
  try {
    if (condition === "dirty") fixture.handlers.get("agent:set-workspace-context")(fixture.event, { projectId: "project", dirty: true });
    if (condition === "untrusted") fixture.event.sender.getURL = () => "https://foreign.example/";
    await fixture.chapterStructureConsumer.tick();
    assert.deepEqual(fixture.requests, []); assert.deepEqual(fixture.state.budgets, []); assert.deepEqual(fixture.state.leases, []);
    assert.equal(fixture.adapter.process, null);
  } finally { fixture.close(); }
});

test("lost production task-create response reads the unique binding and never grants or creates another automatic root", async () => {
  const fixture = await setup({ lostCreate: true });
  try {
    await fixture.chapterStructureConsumer.tick();
    fixture.handlers.get("agent:set-workspace-context")(fixture.event, { projectId: "project", dirty: false });
    await fixture.chapterStructureConsumer.tick();
    assert.equal(fixture.readChange().assessment.state, "bound");
    assert.equal(fixture.requests.filter(request => request.path.endsWith("/tasks") && request.body).length, 1);
    assert.equal(fixture.requests.filter(request => request.path.endsWith("/chapter-structure-changes/change-one")).length, 3);
    const work = fixture.ledger.workTasks.get(fixture.readChange().assessment.workTaskId);
    assert.equal(work.status, "blocked"); assert.equal(work.steps[0].status, "blocked");
    assert.equal([...fixture.ledger.tasks.values()][0].status, "failed");
    assert.deepEqual(fixture.state.charges.map(charge => ({ status: charge.status, usage: charge.usage })),
      [{ status: "failed", usage: { requests: 0, tokens: 0, unknownTokens: 0, overrun: false } }]);
    const pauses = fixture.requests.filter(request => request.path.endsWith("/control") && request.body?.command === "pause");
    assert.deepEqual(pauses, []);
    assert.deepEqual(fixture.state.budgets, []); assert.deepEqual(fixture.state.leases, []);
    assert.ok(fixture.events.some(message => message?.method === "structure/error" && message.params.message.includes("create response lost")));
    await assert.rejects(fixture.handlers.get("agent:retry-chapter-structure-check")(fixture.event, "project", "change-one"), /原任务恢复/);
    assert.equal(fixture.requests.filter(request => request.path.endsWith("/tasks") && request.body).length, 1);
    await fixture.handlers.get("agent:resume-work-task")(fixture.event, "project", work.id);
    const claims = fixture.requests.filter(request => request.path.endsWith("/tasks") && request.body);
    assert.equal(claims.length, 2); assert.equal(claims[1].body.initialBudget, undefined);
    assert.equal(claims[1].body.target.resumeStepId, work.steps[0].id);
    assert.equal(fixture.ledger.workTasks.size, 1); assert.equal(work.status, "completed");
  } finally { fixture.close(); }
});

test("lost creation after a scope change settles only the original bound Work without a new project task or budget", async () => {
  const hold = deferred(); const ready = deferred();
  const fixture = await setup({ lostCreate: true, holdCreate: hold, createReady: ready });
  try {
    const pending = fixture.chapterStructureConsumer.tick(); await ready.promise;
    fixture.handlers.get("agent:set-workspace-context")(fixture.event, { projectId: "other", dirty: false });
    hold.resolve(); await pending;
    const binding = fixture.readChange().assessment;
    assert.equal(fixture.ledger.workTasks.get(binding.workTaskId).status, "blocked");
    const pauses = fixture.requests.filter(request => request.path.endsWith("/control"));
    assert.deepEqual(pauses, []);
    assert.deepEqual(fixture.state.charges.map(charge => charge.usage), [{ requests: 0, tokens: 0, unknownTokens: 0, overrun: false }]);
    assert.equal(fixture.requests.filter(request => request.path.endsWith("/tasks") && request.body).length, 1);
    assert.equal(fixture.requests.some(request => request.path.startsWith("/api/projects/other/")), false);
    assert.deepEqual(fixture.state.budgets, []); assert.deepEqual(fixture.state.leases, []);
    assert.equal(fixture.ledger.completions.length, 0);
    const error = fixture.events.find(message => message?.method === "structure/error");
    assert.equal(error.params.projectId, "project"); assert.equal(error.params.taskId, binding.taskId);
    assert.equal(fixture.events.some(message => message?.method === "agent/structure-updated"), false);
    fixture.handlers.get("agent:set-workspace-context")(fixture.event, { projectId: "project", dirty: false });
    await fixture.chapterStructureConsumer.tick();
    assert.equal(fixture.requests.filter(request => request.path.endsWith("/tasks") && request.body).length, 1);
    assert.equal(fixture.ledger.workTasks.get(binding.workTaskId).status, "blocked");
  } finally { hold.resolve(); fixture.close(); }
});

test("unknown creation never marks a mismatched task failed or manufactures its zero-usage receipt", async () => {
  for (const readTask of [task => ({ ...task, threadId: "foreign-thread" }),
    task => ({ ...task, target: { type: "project", structureChangeId: "foreign-change" } }),
    task => ({ ...task, inputRevision: 8 }),
    task => ({ ...task, inputManifest: { ...task.inputManifest, authorMessageId: "foreign-author" } })]) {
    const fixture = await setup({ lostCreate: true, readTask });
    try {
      await fixture.chapterStructureConsumer.tick();
      const binding = fixture.readChange().assessment;
      assert.equal(fixture.ledger.workTasks.get(binding.workTaskId).status, "paused");
      assert.equal([...fixture.ledger.tasks.values()][0].status, "queued");
      assert.equal(fixture.requests.some(request => request.method === "PATCH"), false);
      assert.deepEqual(fixture.state.charges, []); assert.deepEqual(fixture.state.budgets, []);
      assert.equal(existsSync(fixture.state.agentStorage.budgetDb), false);
      assert.ok(fixture.events.some(message => message?.method === "structure/error" && message.params.message.includes("create response lost")));
    } finally { fixture.close(); }
  }
});

test("unknown creation preserves an existing engine receipt rather than overwriting it with inferred zero usage", async () => {
  const hold = deferred(); const ready = deferred();
  const fixture = await setup({ lostCreate: true, existingBudget: true, holdCreate: hold, createReady: ready });
  try {
    const pending = fixture.chapterStructureConsumer.tick(); await ready.promise;
    const taskId = fixture.readChange().assessment.taskId;
    const receipt = PersistentTaskBudget.receipt(fixture.state.agentStorage.budgetDb, taskId);
    hold.resolve(); await pending;
    assert.deepEqual(PersistentTaskBudget.receipt(fixture.state.agentStorage.budgetDb, taskId), receipt);
    assert.equal([...fixture.ledger.tasks.values()][0].status, "queued");
    assert.equal(fixture.requests.some(request => request.method === "PATCH"), false);
    assert.deepEqual(fixture.state.charges, []); assert.deepEqual(fixture.state.leases, []);
    assert.equal(fixture.ledger.workTasks.get(fixture.readChange().assessment.workTaskId).status, "paused");
    assert.equal(fixture.requests.filter(request => request.path.endsWith("/tasks") && request.body).length, 1);
  } finally { hold.resolve(); fixture.close(); }
});

test("author Stop during the locally unsent lost POST settles the exact task as cancelled with no provider request", async () => {
  const hold = deferred(); const ready = deferred();
  const fixture = await setup({ lostCreate: true, holdCreate: hold, createReady: ready });
  try {
    const pending = fixture.chapterStructureConsumer.tick(); await ready.promise;
    await fixture.handlers.get("agent:interrupt")(fixture.event);
    hold.resolve(); await pending;
    const binding = fixture.readChange().assessment;
    assert.equal([...fixture.ledger.tasks.values()][0].status, "cancelled");
    assert.equal(fixture.ledger.workTasks.get(binding.workTaskId).status, "cancelled");
    assert.deepEqual(fixture.state.charges.map(charge => ({ status: charge.status, usage: charge.usage })),
      [{ status: "cancelled", usage: { requests: 0, tokens: 0, unknownTokens: 0, overrun: false } }]);
    assert.deepEqual(fixture.state.budgets, []); assert.deepEqual(fixture.state.leases, []);
    assert.equal(fixture.requests.filter(request => request.path.endsWith("/tasks") && request.body).length, 1);
    assert.equal(fixture.events.some(message => message?.method === "structure/error"), false);
  } finally { hold.resolve(); fixture.close(); }
});

for (const boundary of ["session", "workspace"]) test(`production prebind guard rejects a project switch after ${boundary} without a task or provider turn`, async () => {
  const hold = deferred(); const fixture = await setup({ holdWorkspace: boundary === "workspace" ? hold : null });
  const ready = deferred();
  try {
    if (boundary === "session") {
      const ensure = fixture.adapter.ensureSession.bind(fixture.adapter);
      fixture.adapter.ensureSession = async () => { const thread = await ensure(); ready.resolve(); await hold.promise; return thread; };
    } else {
      // Resolve only when production has reached its actual workspace fetch.
      const until = setInterval(() => { if (fixture.requests.some(request => request.path.endsWith("/creative-workspace"))) { clearInterval(until); ready.resolve(); } }, 5);
    }
    const pending = fixture.chapterStructureConsumer.tick(); await ready.promise;
    fixture.handlers.get("agent:set-workspace-context")(fixture.event, { projectId: "other", dirty: false });
    hold.resolve(); await pending;
    assert.equal(fixture.requests.filter(request => request.path.endsWith("/tasks") && request.body).length, 0);
    assert.deepEqual(fixture.state.budgets, []); assert.deepEqual(fixture.state.leases, []);
    assert.equal(fixture.ledger.completions.length, 0);
  } finally { hold.resolve(); fixture.close(); }
});

test("production Work recovery selects the dedicated runner with inherited budget rather than a candidate workflow", async () => {
  const fixture = await setup();
  try {
    await fixture.chapterStructureConsumer.tick();
    const work = [...fixture.ledger.workTasks.values()][0]; const step = work.steps[0];
    work.status = "blocked"; work.resumeMode = "continueExecution"; step.status = "queued";
    const restored = await fixture.handlers.get("agent:resume-work-task")(fixture.event, "project", work.id);
    assert.equal(restored.status, "completed");
    const claims = fixture.requests.filter(request => request.path.endsWith("/tasks") && request.body);
    assert.equal(claims.length, 2);
    assert.deepEqual(claims[1].body.target, { type: "project", structureChangeId: "change-one", resumeStepId: step.id });
    assert.equal(claims[1].body.operation, "assessChapterStructure"); assert.equal(claims[1].body.initialBudget, undefined);
    assert.equal(fixture.state.budgets.length, 2); assert.equal(fixture.state.confirmations.length, 0);
    assert.equal(fixture.ledger.completions.length, 2);
    assert.equal(fixture.requests.filter(request => /\/(actions|approve|planning-reviews|delete)$/.test(request.path)).length, 0);
    assert.equal(fixture.requests.filter(request => request.path.endsWith("/budget-authorization") && request.body).length, 0);
  } finally { fixture.close(); }
});
