import assert from "node:assert/strict";
import test from "node:test";
import { spawn } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { DatabaseSync } from "node:sqlite";
import { PersistentTaskBudget } from "@eternallight/agent-engine";
import { nativeTestBinary } from "./helpers/native-binaries.mjs";
import { mainHandlers, rendererEvent } from "./main-handler-harness.mjs";
import { createInitialTaskBudget } from "../task-budget-grant.mjs";
import { taskProviderBudgetOptions } from "../task-provider-budget.mjs";
import { readFixtureAuthorRequirements, updateFixtureDeliveryPlan } from "./fixtures/model-plan-current-contract.mjs";

// Actual production Host consumer/coordinator + Native HTTP and durable DB.
// Controlled AppServer/model decisions exercise the mechanism, not literary
// quality or certification of an external provider. No SQL edits seed events.
for (const sourceOutcome of ["completed", "failed", "generationRetryable", "generationInterrupted", "generationBlocked"]) test(`durable ${sourceOutcome} result returns to the original Work after Native/Host restart`, { timeout: 180_000 }, async t => {
  const generation = sourceOutcome.startsWith("generation");
  const directory = await mkdtemp(join(tmpdir(), "novel-durable-wake-"));
  const budgetDb = join(directory, "engine.sqlite3"), database = join(directory, "story.sqlite3");
  const authorToken = "durable-author-token-123456789", candidateToken = "durable-candidate-token-123456789";
  const capability = "durable-task-capability-123456789", originalIntent = generation
    ? "生成港城小说第1章，方法由模型选择，保存候选，不采用。" : "生成港城创作方向并保存候选，不采用。";
  const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;
  let runtime, origin, main;
  const children = [], hostCalls = [], attempts = [], turns = [], failures = [];
  const readAuthorMessages = workId => {
    const db = new DatabaseSync(database, { readOnly: true });
    try { db.exec("PRAGMA busy_timeout=5000"); return db.prepare("SELECT message_id,intent_revision,author_text FROM story_author_messages_v1 WHERE work_task_id=? ORDER BY intent_revision,message_id").all(workId); }
    finally { db.close(); }
  };
  const stop = async child => {
    if (!child || child.exitCode !== null || child.signalCode !== null) return;
    const stopped = new Promise(resolve => child.once("exit", resolve)); child.kill(); await stopped;
  };
  t.after(async () => {
    await main?.workExecutionConsumer.stop(); await main?.workExecutionOwners.stop(); main?.adapter.close();
    for (const child of children) await stop(child);
    await rm(directory, { recursive: true, force: true });
  });
  const launch = async () => {
    const child = spawn(nativeTestBinary("novel-codex-story-runtime"), ["--port", "0", "--db", database,
      "--engine-budget-db", budgetDb, "--token-stdin"], { stdio: ["pipe", "pipe", "pipe"] });
    children.push(child);
    let output = "", diagnostic = "";
    child.stderr.on("data", data => { diagnostic += data; });
    const address = await new Promise((resolve, reject) => {
      const timeout = setTimeout(() => reject(new Error(`Native not ready: ${diagnostic.slice(-1000)}`)), 60_000);
      child.stdout.on("data", data => {
        output += data;
        const value = /Story Runtime listening on (http:\/\/127\.0\.0\.1:\d+)/.exec(output)?.[1];
        if (value) { clearTimeout(timeout); resolve(value); }
      });
      child.once("error", error => { clearTimeout(timeout); reject(error); });
      child.once("exit", code => { clearTimeout(timeout); reject(new Error(`Native exit ${code}: ${diagnostic.slice(-1000)}`)); });
      child.stdin.end(`${JSON.stringify({ authorToken, candidateToken })}\n`);
    });
    runtime = child; origin = address;
  };
  const request = async (path, body = null, method = body === null ? "GET" : "POST", secret = null) => {
    const response = await fetch(`${origin}${path}`, { method, headers: { "content-type": "application/json",
      authorization: `Bearer ${secret ? candidateToken : authorToken}`,
      ...(secret ? { "x-novel-task-capability": secret } : {}) }, ...(body === null ? {} : { body: JSON.stringify(body) }) });
    const text = await response.text();
    assert.match(response.headers.get("content-type") ?? "", /^application\/json/);
    const value = JSON.parse(text); assert.ok(response.ok, `${method} ${path}: HTTP ${response.status} ${text}`); return value;
  };
  const transport = async (path, body, options, url) => {
    hostCalls.push({ path, method: options.method ?? "GET", body });
    const value = await request(path + url.search, body, options.method ?? "GET", options.headers?.["x-novel-task-capability"] ?? null);
    if (path.endsWith("/tasks") && body) attempts.push(value);
    return value;
  };
  await launch();
  const { project } = await request("/api/projects", { title: "持久接续验证", logline: "隔离 Runtime 的真实持久结果与原任务消费", medium: "novel", idempotencyKey: `wake-${sourceOutcome}` });
  const base = `/api/projects/${project.id}`;
  const state = { nativeRuntime: true, runtimeUrl: origin, runtimeProcess: runtime,
    rendererWebContents: rendererEvent().sender, agentStorage: { budgetDb } };
  main = await mainHandlers(transport, command, state);
  const { thread } = await main.adapter.createThread();
  const workspace = await request(`${base}/creative-workspace`);
  let run = null, next = null;
  if (generation) {
    run = await request(`${base}/generation-runs`, { expectedProjectRevision: workspace.revision,
      expectedBaseSnapshotId: workspace.headSnapshotId, expectedSourceDigest: workspace.sourceDigest, authorIntent: originalIntent,
      targetChapters: 1, maxTaskAttempts: 10, maxProviderRequests: 16, maxProviderTokens: 200_000, deadlineAt: null });
    next = await request(`${base}/generation-runs/${run.id}/next`, {});
    assert.equal(next.action.kind, "agentTask"); assert.equal(next.action.operation, "advisory");
  }
  const source = await request(`${base}/tasks`, { operation: generation ? next.action.operation : sourceOutcome === "failed" ? "generateBrief" : "advisory",
    target: generation ? next.action.target : { type: sourceOutcome === "failed" ? "artifact" : "project" }, inputRevision: workspace.revision,
    threadId: thread.id, authorIntent: originalIntent, ...(!generation ? { initialBudget: createInitialTaskBudget({ workspace }) } : {}),
    capabilityHash: createHash("sha256").update(capability).digest("hex"), capabilityExpiresAt: Math.floor(Date.now() / 1000) + 600 });
  const work = await request(`${base}/tasks/${source.id}/work-task`), workPath = `${base}/work-tasks/${work.id}`;
  const grant = await request(`${base}/tasks/${source.id}/provider-budget`);
  const producerBudget = new PersistentTaskBudget({ databasePath: budgetDb, runId: source.id,
    ...taskProviderBudgetOptions(grant, { taskId: source.id, workTaskId: work.id, deadlineAt: grant.deadlineAt }) });
  if (sourceOutcome === "completed") {
    await updateFixtureDeliveryPlan((method, path, body) => request(path, body, method, capability), base, source);
    const action = await request(`${base}/tasks/${source.id}/actions`, { taskId: source.id, threadId: source.threadId,
      expectedRevision: source.inputRevision, expectedIntentRevision: 0, idempotencyKey: "direction-after-read",
      operation: "generateBrief", target: { type: "artifact" }, instruction: originalIntent }, "POST", capability);
    await request(`${base}/tasks/${source.id}/actions/${action.id}/approve`, { expectedIntentRevision: 0 });
    await request(`${base}/tasks/${source.id}/complete`, { threadId: source.threadId, summary: "已读取并安排原授权创作方向候选，尚未交付。" }, "POST", capability);
  } else {
    await request(`${base}/tasks/${source.id}`, { threadId: source.threadId, status: "failed",
      error: sourceOutcome === "generationBlocked" ? "provider_transport_failed" : sourceOutcome === "generationInterrupted" ? "execution_interrupted; executor stopped" : "invalid_tool_arguments" }, "PATCH");
  }
  producerBudget.complete(sourceOutcome === "completed" ? "completed" : sourceOutcome === "generationInterrupted" ? "cancelled" : "failed");
  await request(generation ? `${base}/generation-runs/${run.id}/provider-usage` : `${workPath}/provider-usage`, { taskId: source.id });
  if (sourceOutcome === "generationBlocked") {
    const stopped = await request(`${base}/generation-runs/${run.id}/next`, {});
    assert.equal(stopped.run.status, "blocked");
  }
  const prior = await request(`${base}/work-execution-events?limit=20`);
  assert.equal(prior.items.length, 1); const event = prior.items[0];
  assert.equal(event.sourceAttemptId, source.id); assert.equal(event.rootThreadId, thread.id); assert.equal(event.state, "pending");
  const before = await request(workPath);
  assert.equal(before.status, sourceOutcome === "completed" ? "running" : "blocked");
  if (generation) assert.equal(before.steps.find(step => step.currentAttemptId === source.id).status, "blocked");
  assert.equal(before.goal, originalIntent);
  const originalAuthorMessages = readAuthorMessages(work.id);
  assert.equal(originalAuthorMessages.length, 1); assert.equal(originalAuthorMessages[0].intent_revision, 0);
  assert.equal(originalAuthorMessages[0].author_text, originalIntent);
  const originalAuthorization = generation ? null : await request(`${workPath}/budget-authorization`);
  const originalRun = generation ? await request(`${base}/generation-runs/${run.id}`) : null;
  if (generation) {
    assert.equal(originalRun.status, "blocked");
    assert.equal(originalRun.providerBudget.maxRequests, 16); assert.equal(originalRun.providerBudget.maxTokens, 200_000);
    assert.equal(originalRun.maxTaskAttempts, 10); assert.equal(originalRun.deadlineAt, null);
    assert.ok(originalRun.providerBudget.charges.some(charge => charge.taskId === source.id));
    assert.equal(originalRun.intentRevisions[0].instruction, originalIntent);
    assert.equal(typeof originalRun.authorMessageId, "string"); assert.ok(originalRun.authorMessageId);
  }
  await main.workExecutionConsumer.stop(); await main.workExecutionOwners.stop(); main.adapter.close(); await stop(runtime);

  // Reopen the same current schema DB and original Engine thread storage.
  await launch(); state.runtimeUrl = origin; state.runtimeProcess = runtime;
  main = await mainHandlers(transport, command, state);
  const afterRestart = await request(`${base}/work-execution-events?limit=20`);
  assert.equal(afterRestart.items[0].eventId, event.eventId); assert.equal(afterRestart.items[0].sourceAttemptId, source.id);
  const candidateRead = await fetch(`${origin}${base}/work-execution-events?limit=20`, { headers: { authorization: `Bearer ${candidateToken}` } });
  assert.equal(candidateRead.status, 401, "candidate credential cannot claim Host delivery authority");
  let saved, savedTask, holdWake = false, releaseWake, wakeStarted;
  const wakeHold = new Promise(resolve => { releaseWake = resolve; });
  const wakeBeginning = new Promise(resolve => { wakeStarted = resolve; });
  t.after(() => releaseWake());
  main.workExecutionConsumer.failed = (...args) => failures.push(args);
  state.onPrepareDomain = domain => {
  const rpc = domain.codexAdapter.request;
  domain.codexAdapter.request = async function(method, params) {
    if (method === "turn/start") {
      const task = attempts.at(-1), secret = this.env.NOVEL_CODEX_TASK_CAPABILITY;
      turns.push({ operation: task.operation, threadId: params.threadId, taskId: task.id });
      if (holdWake) { holdWake = false; wakeStarted(); await wakeHold; }
      const modelRequest = (method, path, body) => request(path, body, method, secret);
      if (generation) {
        assert.ok(["generationRetryable", "generationInterrupted"].includes(sourceOutcome)); assert.equal(task.operation, "advisory");
        assert.equal(task.target.runId, run.id); assert.equal(params.threadId, thread.id);
        const currentRun = await request(`${base}/generation-runs/${run.id}`);
        assert.equal(currentRun.status, "running");
        // A real author pause after the new model Attempt attaches must stop
        // execution. It is not a model decision, recovery command or receipt.
        await request(`${base}/generation-runs/${run.id}/control`, { expectedVersion: currentRun.version, command: "pause" });
      } else if (task.operation === "generateBrief") {
        saved = await request(`${base}/artifacts/brief`, { taskId: task.id, threadId: task.threadId, expectedRevision: task.inputRevision,
          expectedCandidateRevisionId: null, idempotencyKey: "wake-saved-direction", payload: { premise: "港城抄写员追查失忆档案" } }, "POST", secret);
        savedTask = task;
      } else if (!saved) {
        assert.equal(sourceOutcome, "failed"); assert.equal(task.operation, "advisory");
        await updateFixtureDeliveryPlan(modelRequest, base, task);
        const status = await request(`${base}/tasks/${task.id}/work-decision?threadId=${encodeURIComponent(task.threadId)}&expectedRevision=${task.inputRevision}`, null, "GET", secret);
        const action = status.readyActions.find(item => item.operation === "generateBrief"); assert.ok(action, JSON.stringify(status));
        await request(`${base}/tasks/${task.id}/work-decision/actions`, { taskId: task.id, threadId: task.threadId,
          expectedRevision: task.inputRevision, expectedWorkVersion: status.version, expectedSourceDigest: status.sourceDigest,
          actionKey: action.actionKey, instruction: originalIntent, idempotencyKey: `wake-repair-${task.id}` }, "POST", secret);
      } else {
        assert.equal(task.operation, "advisory"); assert.equal(params.threadId, thread.id);
        await readFixtureAuthorRequirements(modelRequest, base, task, originalIntent);
        await updateFixtureDeliveryPlan(modelRequest, base, task, { status: "satisfied", evidence: [{ taskId: savedTask.id, revisionId: saved.revisionId }] });
        const status = await request(`${base}/tasks/${task.id}/work-decision?threadId=${encodeURIComponent(task.threadId)}&expectedRevision=${task.inputRevision}`, null, "GET", secret);
        await request(`${base}/tasks/${task.id}/complete`, { threadId: task.threadId, expectedRevision: task.inputRevision,
          summary: "创作方向候选已保存，尚未采用。", deliveryReview: { expectedWorkVersion: status.version, expectedSourceDigest: status.sourceDigest,
            goalStatus: "fulfilled", goalAssessment: "完成作者授权的港城创作方向候选。", checkedScope: "已核对当前创作方向候选保存回执和完整原作者要求；未检查或发布正文。",
            unresolved: [], issueResolutions: [], authorDecisions: [], evidence: [{ taskId: savedTask.id, revisionId: saved.revisionId }] } }, "POST", secret);
      }
    }
    return rpc.call(this, method, params);
  };
  };
  // Cross-Host lease and author pause are actual Native controls, never mock
  // availability flags. They leave the persisted result and original budget.
  let current = await request(workPath);
  if (!generation) {
    const foreignLease = await request(`${workPath}/execution-owner`, { hostSessionId: "foreign-active-host", expectedWorkVersion: current.version });
    await main.workExecutionConsumer.tick();await main.workExecutionConsumer.settled(); assert.equal(turns.length, 0, "live foreign Host must not be taken over");
    await request(`${workPath}/execution-owner/release`, { hostSessionId: foreignLease.hostSessionId, leaseId: foreignLease.leaseId });
    const waiting = main.workExecutionConsumer.health().find(item => item.eventId === event.eventId);
    assert.equal(waiting.reason, "activeOwner"); assert.equal(waiting.failures, 0);
    // Advance only the Host polling clock to the promised check. The real
    // Native lease was explicitly released above; no authorization is mocked.
    main.workExecutionConsumer.now = () => waiting.nextCheckAt;
  }
  if (sourceOutcome === "completed") {
    const claimCount = hostCalls.filter(call => call.path.endsWith(`/work-execution-events/${event.eventId}/claim`)).length;
    main.handlers.get("agent:set-workspace-context")(rendererEvent(), { projectId: project.id, dirty:true,chapterScope:null,mediaScope:null });
    await main.workExecutionConsumer.tick();await main.workExecutionConsumer.settled(); assert.equal(turns.length, 0, "unsaved editing must hold idle wake");
    assert.equal(hostCalls.filter(call => call.path.endsWith(`/work-execution-events/${event.eventId}/claim`)).length,
      claimCount, "dirty workspace must not claim execution authority");
    main.handlers.get("agent:set-workspace-context")(rendererEvent(), { projectId: project.id, dirty:false,chapterScope:null,mediaScope:null });
    current = await request(workPath);
    await request(`${workPath}/control`, { commandId: randomUUID(), threadId: current.threadId, expectedControlRevision: current.controlRevision, command: "pause" });
    await main.workExecutionConsumer.tick();await main.workExecutionConsumer.settled(); assert.equal(turns.length, 0, "author pause must survive idle event scan");
    current = await request(workPath);
    await request(`${workPath}/control`, { commandId: randomUUID(), threadId: current.threadId, expectedControlRevision: current.controlRevision, command: "resume" });
  }
  if (sourceOutcome === "completed") {
    holdWake = true;
    await main.workExecutionConsumer.tick(); await wakeBeginning;
    try {
      const scans = hostCalls.filter(call => call.path === "/api/projects").length;
      const claims = hostCalls.filter(call => call.path.endsWith(`/work-execution-events/${event.eventId}/claim`)).length;
      const attemptCount = attempts.length;
      await main.workExecutionConsumer.tick();
      assert.ok(hostCalls.filter(call => call.path === "/api/projects").length > scans,
        "another scan must run while the real original-Work Adapter is held");
      assert.equal(hostCalls.filter(call => call.path.endsWith(`/work-execution-events/${event.eventId}/claim`)).length, claims);
      assert.equal(attempts.length, attemptCount, "repeated scans must not duplicate the active Work");
    } finally { releaseWake(); }
    await main.workExecutionConsumer.settled();
  } else {
    await main.workExecutionConsumer.tick(); await main.workExecutionConsumer.settled();
  }
  if (generation) {
    const result = await request(workPath), currentRun = await request(`${base}/generation-runs/${run.id}`);
    assert.equal(result.id, work.id); assert.equal(result.threadId, thread.id); assert.equal(result.goal, originalIntent);
    assert.equal(currentRun.id, run.id); assert.deepEqual(currentRun.intentRevisions, originalRun.intentRevisions);
    assert.equal(currentRun.authorMessageId, originalRun.authorMessageId);
    assert.deepEqual(readAuthorMessages(work.id), originalAuthorMessages);
    for (const field of ["maxRequests", "maxTokens"]) assert.equal(currentRun.providerBudget[field], originalRun.providerBudget[field]);
    assert.equal(currentRun.maxTaskAttempts, originalRun.maxTaskAttempts); assert.equal(currentRun.deadlineAt, originalRun.deadlineAt);
    assert.deepEqual(currentRun.providerBudget.charges.find(charge => charge.taskId === source.id),
      originalRun.providerBudget.charges.find(charge => charge.taskId === source.id), "the original settled generation charge must remain unchanged");
    assert.equal(hostCalls.some(call => call.body?.initialBudget || /\/control$|\/rebind$|\/budget-authorization$/.test(call.path) && call.method !== "GET"), false);
    assert.ok(hostCalls.some(call => call.path.endsWith(`/work-execution-events/${event.eventId}/claim`)), "Native, not Host status guessing, decides generation admission");
    if (sourceOutcome === "generationBlocked") {
      assert.equal(failures.length, 0); assert.deepEqual(turns, []); assert.equal(currentRun.status, "blocked");
      assert.equal(hostCalls.some(call => call.path.endsWith(`/generation-runs/${run.id}/next`)), false);
    } else {
      assert.equal(turns.length, 1); assert.equal(turns[0].operation, "advisory"); assert.equal(turns[0].threadId, thread.id);
      assert.ok(hostCalls.some(call => call.path.endsWith(`/generation-runs/${run.id}/next`)));
      assert.ok(result.steps.some(step => step.attempts.some(attempt => attempt.id === turns[0].taskId)));
      assert.equal(currentRun.status, "paused", "the actual subsequent author pause must remain stopped");
    }
    assert.equal(hostCalls.some(call => call.path.endsWith(`/work-execution-events/${event.eventId}/ack`)), false,
      "blocked or author-stopped dispatch must not manufacture a consumed event");
    const pending = (await request(`${base}/work-execution-events?limit=20`)).items.find(item => item.eventId === event.eventId);
    assert.equal(pending?.state, "pending", "Native must preserve the original durable event after a blocked or author-stopped wake");
    const count = turns.length; await main.workExecutionConsumer.tick();await main.workExecutionConsumer.settled(); assert.equal(turns.length, count);
    return;
  }
  assert.equal(failures.length, 0, failures.map(args => args[2]?.stack).join("\n"));
  assert.deepEqual(turns.map(turn => turn.operation), sourceOutcome === "failed" ? ["advisory", "generateBrief", "advisory"] : ["generateBrief", "advisory"]);
  const result = await request(workPath); assert.equal(result.status, "completed"); assert.equal(result.threadId, thread.id);
  assert.equal(result.goal, originalIntent); assert.deepEqual(await request(`${workPath}/budget-authorization`), originalAuthorization);
  assert.deepEqual(readAuthorMessages(work.id), originalAuthorMessages,
    "persistent author message identity, intent version and original text must not be duplicated by wake");
  assert.equal((await request(`${base}/creative-workspace`)).artifacts.brief.candidate.id, saved.revisionId);
  assert.ok(hostCalls.some(call => call.path.endsWith(`/work-execution-events/${event.eventId}/claim`)));
  const ack = hostCalls.find(call => call.path.endsWith(`/work-execution-events/${event.eventId}/ack`));
  assert.ok(ack); assert.ok(result.steps.find(step => step.id === ack.body.consumerStepId)?.currentAttemptId);
  assert.equal(hostCalls.some(call => call.body?.initialBudget), false, "wake must never submit a second author budget or root");
  assert.equal(hostCalls.some(call => call.path.endsWith("/control") && call.body?.command === "resume"), false,
    "durable correction is not an author resume transition");
  const count = turns.length; await main.workExecutionConsumer.tick();await main.workExecutionConsumer.settled(); assert.equal(turns.length, count,
    "already consumed/final events must not open another model turn");
});
