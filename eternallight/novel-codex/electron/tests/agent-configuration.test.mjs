import assert from "node:assert/strict";
import { test } from "node:test";
import { createTaskExecution, bindExecutionTask } from "@eternallight/agent-engine";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { applyAgentConfiguration } from "../agent-configuration.mjs";
import { composeAdvisoryPrompt } from "../advisory-prompt.mjs";
import { createTaskBoundHandoff, taskExecutionPrompt } from "../workflow-handoff.mjs";
import { CodexAppServerAdapter } from "../codex-adapter.mjs";
import { modelInputTokenBudget } from "../../shared/author-input-budget.mjs";

const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));
const command = `${process.execPath} ${join("", fixture)}`;

test("recovery quietly closes finished reads but preserves writes, mixed requests and active reads", async () => {
  const request = (operation) => ({operation,recovery:"manual-reconciliation"});
  const reports = [
    {runId:"reads",runtimeStatus:"completed",requests:[request("read_commitment"),request("read_source_content"),request("read_work_status"),request("read_revision_outcome"),request("search_story"),request("query_story"),request("search_relations")]},
    {runId:"write",runtimeStatus:"completed",requests:[request("submit_commitment_candidate")]},
    {runId:"mixed",runtimeStatus:"failed",requests:[request("read_commitment"),request("submit_commitment_candidate")]},
    {runId:"active",runtimeStatus:"running",requests:[request("read_commitment")]},
    {runId:"unknown",runtimeStatus:"completed",requests:[request("unknown_tool")]},
    {runId:"pause",runtimeStatus:"cancelled",requests:[request("pause_generation_run")]},
  ];
  const resolved=[];
  const {handlers,adapter}=await mainHandlers(async()=>{throw new Error("Recovery report must not call Runtime");},command,
    {recoveryIndex:{unfinishedRuns:async()=>[],recoveryReport:async()=>reports,resolveRun:async(...args)=>resolved.push(args)}});
  try {
    const result=await handlers.get("agent:recovery-report")(rendererEvent());
    assert.deepEqual(resolved,[["reads","manual-reviewed"]]);
    assert.deepEqual(Array.from(result,(run)=>run.runId),["write","mixed","active","unknown","pause"]);
  } finally {adapter.close();}
});

test("main forwards author export and metric requests with their methods and selected payloads", async () => {
  const requests = [];
  const { handlers, adapter } = await mainHandlers(async (path, body, options) => {
    requests.push({ path, body, method: options.method ?? "GET" });
    return { content: "正式正文", projectRevision: 7 };
  }, command);
  try {
    const selected = { scopes: ["manuscript"], format: "markdown" };
    const result = await handlers.get("runtime:export-project")(rendererEvent(), "project", selected);
    assert.equal(result.content, "正式正文");
    await handlers.get("runtime:metrics")(rendererEvent());
    await handlers.get("runtime:metrics-abandon")(rendererEvent(), "brief", "00000000-0000-4000-8000-000000000001");
    assert.deepEqual(requests, [
      { path: "/api/projects/project/export", body: selected, method: "POST" },
      { path: "/api/metrics", body: null, method: "GET" },
      { path: "/api/metrics/abandon", body: { stage: "brief", idempotencyKey: "00000000-0000-4000-8000-000000000001" }, method: "POST" },
    ]);
  } finally { adapter.close(); }
});

test("author issue pages stay read-only and invalid identities or page bounds never dispatch", async () => {
  const requests = [];
  const page = { entries: [], totalCount: 0, nextCursor: null };
  const { handlers, adapter } = await mainHandlers(async (path, body, options, url) => {
    requests.push({ path: path + url.search, body, method: options.method ?? "GET" });
    return page;
  }, command);
  try {
    const read = handlers.get("runtime:get-work-issues");
    const request = { issueId: null, cursor: 0, limit: 8 };
    assert.equal(await read(rendererEvent(), "project", "work-task", request), page);
    await read(rendererEvent(), "project", "work-task", { issueId: "issue:1", cursor: 8, limit: 16 });
    assert.deepEqual(requests, [
      { path: "/api/projects/project/work-tasks/work-task/issues?cursor=0&limit=8", body: null, method: "GET" },
      { path: "/api/projects/project/work-tasks/work-task/issues?cursor=8&limit=16&issueId=issue%3A1", body: null, method: "GET" },
    ]);
    for (const args of [
      ["", "work-task", request], ["project\nother", "work-task", request],
      ["project", "foreign", request], ["project", "work-task", { ...request, cursor: -1 }],
      ["project", "work-task", { ...request, cursor: 0.5 }], ["project", "work-task", { ...request, cursor: Infinity }],
      ["project", "work-task", { ...request, limit: 0 }], ["project", "work-task", { ...request, limit: 17 }],
      ["project", "work-task", { ...request, issueId: "other/issue" }],
      ["project", "work-task", { ...request, authority: "writer" }], ["project", "work-task", null],
    ]) assert.throws(() => read(rendererEvent(), ...args), /问题读取参数无效/);
    assert.equal(requests.length, 2);
  } finally { adapter.close(); }
});

test("main exposes entity audit only through validated author IPC routes", async () => {
  const requests = [];
  const { handlers, adapter } = await mainHandlers(async (path, body, options) => {
    requests.push({ path, body, method: options.method ?? "GET" });
    return { status: "queued" };
  }, command);
  const jobId = `entity-audit-${"a".repeat(32)}`;
  try {
    await handlers.get("runtime:get-entity-audit")(rendererEvent(), "project");
    await handlers.get("runtime:start-entity-audit")(rendererEvent(), "project");
    await handlers.get("runtime:control-entity-audit")(rendererEvent(), "project", jobId, "pause", 2);
    assert.deepEqual(requests, [
      { path: "/api/projects/project/entity-audits", body: null, method: "GET" },
      { path: "/api/projects/project/entity-audits", body: {}, method: "POST" },
      { path: `/api/projects/project/entity-audits/${jobId}/control`, body: { command: "pause", expectedVersion: 2 }, method: "POST" },
    ]);
    assert.throws(() => handlers.get("runtime:control-entity-audit")(rendererEvent(), "project", "invalid", "pause", 2), /参数无效/);
    assert.throws(() => handlers.get("runtime:control-entity-audit")(rendererEvent(), "project", jobId, "delete", 2), /参数无效/);
    assert.equal(requests.length, 3);
  } finally { adapter.close(); }
});

test("main exposes relation audit only through validated author IPC routes", async () => {
  const requests = [];
  const { handlers, adapter } = await mainHandlers(async (path, body, options) => {
    requests.push({ path, body, method: options.method ?? "GET" });
    return { status: "queued" };
  }, command);
  const jobId = `relation-audit-${"a".repeat(32)}`;
  try {
    await handlers.get("runtime:get-relation-audit")(rendererEvent(), "project");
    await handlers.get("runtime:start-relation-audit")(rendererEvent(), "project");
    await handlers.get("runtime:control-relation-audit")(rendererEvent(), "project", jobId, "pause", 2);
    assert.deepEqual(requests, [
      { path: "/api/projects/project/relation-audits", body: null, method: "GET" },
      { path: "/api/projects/project/relation-audits", body: {}, method: "POST" },
      { path: `/api/projects/project/relation-audits/${jobId}/control`, body: { command: "pause", expectedVersion: 2 }, method: "POST" },
    ]);
    assert.throws(() => handlers.get("runtime:control-relation-audit")(rendererEvent(), "project", "invalid", "pause", 2), /参数无效/);
    assert.throws(() => handlers.get("runtime:control-relation-audit")(rendererEvent(), "project", jobId, "delete", 2), /参数无效/);
    assert.equal(requests.length, 3);
  } finally { adapter.close(); }
});

test("main exposes knowledge audit only through validated author IPC routes", async () => {
  const requests = [];
  const { handlers, adapter } = await mainHandlers(async (path, body, options) => {
    requests.push({ path, body, method: options.method ?? "GET" });
    return { status: "queued" };
  }, command);
  const jobId = `knowledge-audit-${"a".repeat(32)}`;
  try {
    await handlers.get("runtime:get-knowledge-audit")(rendererEvent(), "project");
    await handlers.get("runtime:start-knowledge-audit")(rendererEvent(), "project");
    await handlers.get("runtime:control-knowledge-audit")(rendererEvent(), "project", jobId, "pause", 2);
    assert.deepEqual(requests, [
      { path: "/api/projects/project/knowledge-audits", body: null, method: "GET" },
      { path: "/api/projects/project/knowledge-audits", body: {}, method: "POST" },
      { path: `/api/projects/project/knowledge-audits/${jobId}/control`, body: { command: "pause", expectedVersion: 2 }, method: "POST" },
    ]);
    assert.throws(() => handlers.get("runtime:control-knowledge-audit")(rendererEvent(), "project", "invalid", "pause", 2), /参数无效/);
    assert.throws(() => handlers.get("runtime:control-knowledge-audit")(rendererEvent(), "project", jobId, "delete", 2), /参数无效/);
    assert.equal(requests.length, 3);
  } finally { adapter.close(); }
});

test("main exposes historical knowledge audit only through validated author IPC", async () => {
  const requests = [];
  const { handlers, adapter } = await mainHandlers(async (path, body, options) => {
    requests.push({ path, body, method: options.method ?? "GET" });
    return { status: "queued" };
  }, command);
  const jobId = `snapshot-knowledge-audit-${"a".repeat(32)}`;
  try {
    await handlers.get("runtime:get-snapshot-knowledge-audit")(rendererEvent(), "project");
    await handlers.get("runtime:start-snapshot-knowledge-audit")(rendererEvent(), "project");
    await handlers.get("runtime:control-snapshot-knowledge-audit")(rendererEvent(), "project", jobId, "pause", 2);
    assert.deepEqual(requests, [
      { path: "/api/projects/project/snapshot-knowledge-audits", body: null, method: "GET" },
      { path: "/api/projects/project/snapshot-knowledge-audits", body: {}, method: "POST" },
      { path: `/api/projects/project/snapshot-knowledge-audits/${jobId}/control`, body: { command: "pause", expectedVersion: 2 }, method: "POST" },
    ]);
    assert.throws(() => handlers.get("runtime:control-snapshot-knowledge-audit")(rendererEvent(), "project", "invalid", "pause", 2), /参数无效/);
    assert.throws(() => handlers.get("runtime:control-snapshot-knowledge-audit")(rendererEvent(), "project", jobId, "delete", 2), /参数无效/);
    assert.equal(requests.length, 3);
  } finally { adapter.close(); }
});

test("main exposes confirmed fact audit only through validated author IPC", async () => {
  const requests = [];
  const { handlers, adapter } = await mainHandlers(async (path, body, options) => {
    requests.push({ path, body, method: options.method ?? "GET" });
    return { status: "queued" };
  }, command);
  const jobId = `fact-audit-${"a".repeat(32)}`;
  try {
    await handlers.get("runtime:get-fact-audit")(rendererEvent(), "project");
    await handlers.get("runtime:start-fact-audit")(rendererEvent(), "project");
    await handlers.get("runtime:control-fact-audit")(rendererEvent(), "project", jobId, "pause", 2);
    assert.deepEqual(requests, [
      { path: "/api/projects/project/fact-audits", body: null, method: "GET" },
      { path: "/api/projects/project/fact-audits", body: {}, method: "POST" },
      { path: `/api/projects/project/fact-audits/${jobId}/control`, body: { command: "pause", expectedVersion: 2 }, method: "POST" },
    ]);
    assert.throws(() => handlers.get("runtime:control-fact-audit")(rendererEvent(), "project", "invalid", "pause", 2), /参数无效/);
    assert.throws(() => handlers.get("runtime:control-fact-audit")(rendererEvent(), "project", jobId, "delete", 2), /参数无效/);
    assert.equal(requests.length, 3);
  } finally { adapter.close(); }
});

test("running generation accepts next-chapter intent only after its current task is bound", async () => {
  const runId = `generation-${"a".repeat(32)}`;
  let activeTaskId = "task-current";
  const requests = [];
  const { handlers, adapter } = await mainHandlers(async (path, body, options) => {
    requests.push({ path, body, method: options.method ?? "GET" });
    if (path.endsWith(`/generation-runs/${runId}`)) return { version: 7, activeTaskId };
    if (path.endsWith("/future-intents")) return { intentGeneration: 1 };
    if (path.endsWith("/current-chapter-revision")) return { intentGeneration: 2, pendingCurrentIntentGeneration: 2 };
    throw new Error(`Unexpected Runtime path: ${path}`);
  }, command, { activeGenerationRunId: runId, activeAgentExecution: { runId } });
  try {
    const apply = handlers.get("agent:add-future-intent");
    const saved = await apply(rendererEvent(), "project", runId, "第二章调查白塔");
    assert.equal(saved.intentGeneration, 1);
    assert.deepEqual(requests.at(-1), {
      path: `/api/projects/project/generation-runs/${runId}/future-intents`,
      body: { expectedVersion: 7, instruction: "第二章调查白塔" },
      method: "POST",
    });
    const revised = await handlers.get("agent:revise-current-chapter")(rendererEvent(), "project", runId, "第一章补写白塔证据");
    assert.equal(revised.pendingCurrentIntentGeneration, 2);
    assert.deepEqual(requests.at(-1), {
      path: `/api/projects/project/generation-runs/${runId}/current-chapter-revision`,
      body: { expectedVersion: 7, instruction: "第一章补写白塔证据" },
      method: "POST",
    });
    activeTaskId = null;
    await assert.rejects(apply(rendererEvent(), "project", runId, "第三章回到港口"), /正在切换/);
    await assert.rejects(handlers.get("agent:revise-current-chapter")(rendererEvent(), "project", runId, "第一章改写"), /正在切换/);
    assert.equal(requests.filter((request) => request.path.endsWith("/future-intents")).length, 1);
  } finally { adapter.close(); }
});

function deepSeekConfiguration() {
  return {
    command,
    cwd: process.cwd(),
    env: { PATH: process.env.PATH },
    model: "deepseek-flash",
    provider: "deepseek",
  };
}

test("reconfigures an idle app-server that was started by listing threads", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-deepseek-key", command });
  await adapter.listThreads();
  const child = adapter.process;
  let persisted = false;
  const status = applyAgentConfiguration({
    adapter,
    configuration: deepSeekConfiguration(),
    persist: () => { persisted = true; },
  });
  assert.equal(persisted, true);
  assert.equal(child.killed, true);
  assert.equal(adapter.process, null);
  assert.equal(status.provider, "deepseek");
  assert.equal(status.model, "deepseek-flash");
});

test("rejects an active-turn reconfiguration before persistence", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-deepseek-key", command });
  let started;
  const turnStarted = new Promise((resolve) => { started = resolve; });
  adapter.setCallbacks({ onEvent: (event) => { if (event.method === "turn/started") started(); } });
  const turn = adapter.sendTurn([{ type: "text", text: "interrupt" }]);
  await turnStarted;
  let persisted = false;
  assert.throws(() => applyAgentConfiguration({
    adapter,
    configuration: deepSeekConfiguration(),
    persist: () => { persisted = true; },
  }), /正在处理上一轮请求/);
  assert.equal(persisted, false);
  assert.equal(adapter.provider, "deepseek");
  await adapter.interruptTurn();
  await turn;
  adapter.close();
});

test("keeps the running configuration when persistence fails", async () => {
  const adapter = new CodexAppServerAdapter({ apiKey: "test-deepseek-key", command });
  await adapter.listThreads();
  const child = adapter.process;
  assert.throws(() => applyAgentConfiguration({
    adapter,
    configuration: deepSeekConfiguration(),
    persist: () => { throw new Error("disk full"); },
  }), /disk full/);
  assert.equal(adapter.provider, "deepseek");
  assert.equal(adapter.process, child);
  assert.equal(child.killed, false);
  adapter.close();
});

import { mainHandlers, rendererEvent, runtimeFixture } from "./main-handler-harness.mjs";

test("desktop commitment bridge binds explicit author choices and preserves immutable origin", async () => {
  const requests = [];
  const { handlers, adapter } = await mainHandlers(async (path, body, options, url) => {
    if (path.endsWith("/commitments")) assert.deepEqual(Object.fromEntries(url.searchParams), { limit: "5", cursor: "frozen-cursor" });
    requests.push({ path, body, method: options?.method ?? "GET" });
    return { confirmationState: "candidate" };
  }, command);
  try {
    const choice = { reportRevisionId: "report-one", proposalId: "lamp", baseSnapshotId: "snapshot-one", expectedRevision: 4, idempotencyKey: "choose-lamp" };
    await handlers.get("runtime:get-commitment-extraction")(rendererEvent(), "project", "chapter-one");
    await handlers.get("runtime:establish-commitment")(rendererEvent(), "project", "chapter-one", choice);
    const edit = { payload: { id: "stable-lamp", summary: "作者调整" }, extractionOrigin: { chapterId: "chapter-one", reportRevisionId: "report-one", proposalId: "lamp" },
      expectedRevision: 4, baseSnapshotId: "snapshot-two", expectedCandidateRevisionId: "established", idempotencyKey: "edit-lamp" };
    await handlers.get("runtime:save-commitment")(rendererEvent(), "project", "stable-lamp", edit);
    await handlers.get("runtime:get-commitment")(rendererEvent(), "project", "stable-lamp", "snapshot-one");
    await handlers.get("runtime:create-commitment-review")(rendererEvent(), "project", "stable-lamp", "revision-one", "review-lamp");
    await handlers.get("runtime:list-commitments")(rendererEvent(), "project", "frozen-cursor", 5);
    assert.deepEqual(requests, [
      { path: "/api/projects/project/chapters/chapter-one/commitment-extractions", body: null, method: "GET" },
      { path: "/api/projects/project/chapters/chapter-one/commitment-extractions/establish", body: choice, method: "POST" },
      { path: "/api/projects/project/commitments/stable-lamp", body: edit, method: "POST" },
      { path: "/api/projects/project/commitments/stable-lamp", body: null, method: "GET" },
      { path: "/api/projects/project/commitments/stable-lamp/reviews", body: { revisionId: "revision-one", idempotencyKey: "review-lamp" }, method: "POST" },
      { path: "/api/projects/project/commitments", body: null, method: "GET" },
    ]);
    assert.throws(() => handlers.get("runtime:establish-commitment")(rendererEvent(), "project", "..", choice), /无效/);
    assert.throws(() => handlers.get("runtime:establish-commitment")(rendererEvent(), "project", "chapter-one", { ...choice, expectedRevision: -1 }), /无效/);
    assert.throws(() => handlers.get("runtime:get-commitment")(rendererEvent(), "project", "../other"), /无效/);
    assert.throws(() => handlers.get("runtime:list-commitments")(rendererEvent(), "project", null, 21), /无效/);
    assert.throws(() => handlers.get("runtime:list-commitments")(rendererEvent(), "project", "x".repeat(1025), 20), /无效/);
    assert.equal(requests.length, 6);
  } finally { adapter.close(); }
});

test("running domain retains frozen model defaults throughout Runtime preparation", async () => {
  const runtime = runtimeFixture();
  let release;
  let entered;
  const preparing = new Promise((resolve) => { entered = resolve; });
  const gate = new Promise((resolve) => { release = resolve; });
  const host = await mainHandlers(async (path, body) => {
    if (path.endsWith('/creative-workspace')) { entered(); await gate; }
    return runtime.request(path, body);
  }, command);
  const { handlers } = host;
  try {
    const turn = handlers.get('agent:send-prompt')(rendererEvent(),'project','hello');
    await preparing;
    const domain = host.executionDomains.active()[0];
    assert.notEqual(domain.agentProviderConfig, host.providerConfig);
    const frozen = structuredClone(domain.agentProviderConfig);
    host.providerConfig.model = 'changed-default';
    assert.deepEqual(domain.agentProviderConfig, frozen);
    release();
    await turn;
  } finally { release(); await host.executionDomains.closeAll(); host.adapter.close(); }
});

test("main persists a bounded Unicode summary and returns the complete discussion", async () => {
  const runtime = runtimeFixture();
  const {handlers,adapter} = await mainHandlers(runtime.request.bind(runtime),command);
  try {
    const result = await handlers.get('agent:send-prompt')(rendererEvent(),'project','integration-long-reply');
    assert.equal(result.message,'🌙'.repeat(5000));
    assert.equal(runtime.completions[0].summary,'🌙'.repeat(4000));
  } finally {adapter.close();}
});

for (const operation of ["generateBrief", "generatePropertyRules"]) test(`an explicit conversation executes ${operation} in the same send without duplicate approval`, async () => {
  const runtime = runtimeFixture();
  const calls = [];
  const actionId = `action-${"1".repeat(32)}`;
  const sourceTaskId = `task-${"0".repeat(32)}`;
  const proposal = { id: actionId, sourceTaskId, status: "proposed", inputRevision: 0, intentRevision: 0,
    operation, target: { type: "artifact" }, instruction: "突出主角为何不再信预言" };
  const { handlers, adapter } = await mainHandlers(async (path, body, options = {}) => {
    calls.push({ path, body, method: options.method ?? "GET" });
    if (path.endsWith("/actions")) return { actions: [proposal] };
    if (path.endsWith(`/${actionId}/approve`)) { proposal.status = "queued"; return { ...proposal }; }
    if (path.endsWith(`/${actionId}/finish`)) { proposal.status = "completed"; return { status: "completed" }; }
    return runtime.request(path, body);
  }, command);
  try {
    const discussion = await handlers.get("agent:send-prompt")(
      rendererEvent(), "project", "请修改故事简介，突出主角为何不再信预言",
    );
    assert.deepEqual(discussion.proposedActions, []);
    assert.equal(calls.filter((call) => call.path.endsWith(`/${actionId}/approve`)).length, 1);
    const result = discussion.candidateResults[0];
    assert.equal(result.action.operation, operation);
    assert.ok(result.taskId);
    assert.equal(calls.filter((call) => call.path.endsWith("/tasks") && call.method === "POST").length, 2);
    const candidateTask = calls.find((call) => call.path.endsWith("/tasks") && call.body?.operation === operation);
    assert.equal(candidateTask.body.target.requestedActionId, actionId);
    assert.equal(candidateTask.body.target.requestedByTaskId, sourceTaskId);
    const approval = calls.find((call) => call.path.endsWith(`/${actionId}/approve`));
    assert.deepEqual(approval.body, { expectedIntentRevision: 0 });
    const finish = calls.find((call) => call.path.endsWith(`/${actionId}/finish`));
    assert.equal(finish.body.resultTaskId, result.taskId);
  } finally { adapter.close(); }
});

test("the desktop declines the exact proposal without starting candidate work", async () => {
  const runtime = runtimeFixture();
  const calls = [];
  const actionId = `action-${"d".repeat(32)}`;
  const sourceTaskId = `task-${"0".repeat(32)}`;
  const proposal = { id: actionId, sourceTaskId, status: "proposed", inputRevision: 0, intentRevision: 2,
    operation: "generateBrief", target: { type: "artifact" }, instruction: "不需要的简报" };
  const { handlers, adapter } = await mainHandlers(async (path, body, options = {}) => {
    calls.push({ path, body, method: options.method ?? "GET" });
    if (path.endsWith("/actions")) return { actions: [proposal] };
    if (path.endsWith(`/${actionId}/decline`)) return { ...proposal, status: "declined" };
    return runtime.request(path, body);
  }, command);
  try {
    const result = await handlers.get("agent:decline-action")(rendererEvent(), "project", sourceTaskId, actionId);
    assert.equal(result.status, "declined");
    assert.deepEqual(calls.map(({ path, method }) => [path, method]), [
      [`/api/projects/project/tasks/${sourceTaskId}/actions`, "GET"],
      [`/api/projects/project/tasks/${sourceTaskId}/actions/${actionId}/decline`, "POST"],
    ]);
    assert.deepEqual(calls[1].body, { expectedIntentRevision: 2 });
    await assert.rejects(handlers.get("agent:decline-action")(rendererEvent(), "project", sourceTaskId, "bad-id"), /身份无效/);
    assert.equal(calls.length, 2);
  } finally { adapter.close(); }
});

test("an unavailable model does not consume author approval or queue candidate work", async () => {
  const runtime = runtimeFixture();
  const calls = [];
  const actionId = `action-${"e".repeat(32)}`;
  const sourceTaskId = `task-${"0".repeat(32)}`;
  const proposal = { id: actionId, sourceTaskId, status: "proposed", inputRevision: 0, intentRevision: 0,
    operation: "generateBrief", target: { type: "artifact" }, instruction: "生成简报" };
  const { handlers, adapter } = await mainHandlers(async (path, body, options = {}) => {
    calls.push({ path, body, method: options.method ?? "GET" });
    if (path.endsWith("/actions")) return { actions: [proposal] };
    return runtime.request(path, body);
  }, command);
  try {
    adapter.configure({ command: null });
    await assert.rejects(handlers.get("agent:approve-action")(rendererEvent(), "project", sourceTaskId, actionId), /未配置/);
    assert.deepEqual(calls, []);
  } finally { adapter.close(); }
});

test("stopping after a discussion cannot turn a proposal into a candidate step", async () => {
  const runtime = runtimeFixture();
  let announceActions;
  let releaseActions;
  const reachedActions = new Promise((resolve) => { announceActions = resolve; });
  const actionsGate = new Promise((resolve) => { releaseActions = resolve; });
  const { handlers, adapter, executionTargets } = await mainHandlers(async (path, body, options) => {
    if (path.endsWith("/actions") && (options.method ?? "GET") === "GET") {
      announceActions();
      await actionsGate;
      return { actions: [{ id: `action-${"a".repeat(32)}`, sourceTaskId: `task-${"0".repeat(32)}`, status: "proposed", inputRevision: 0, intentRevision: 0,
        operation: "generateBrief", target: { type: "artifact" }, instruction: "生成简报" }] };
    }
    return runtime.request(path, body);
  }, command);
  const event = rendererEvent();
  try {
    const pending = handlers.get("agent:send-prompt")(event, "project", "请生成创作简报");
    await reachedActions;
    const stopped = await handlers.get("agent:interrupt")(event, executionTargets.get(event));
    assert.equal(stopped.interrupted, true);
    releaseActions();
    await assert.rejects(pending, error => error.code === "TASK_CANCELLED");
    assert.equal([...runtime.tasks.values()].filter((task) => task.operation === "generateBrief").length, 0);
    assert.equal([...runtime.workTasks.values()][0].status, "paused");
  } finally { releaseActions(); adapter.close(); }
});

test("cancelling a different work task does not interrupt the active Agent turn", async () => {
  const runtime = runtimeFixture();
  runtime.workTasks.set("work-unrelated", { id: "work-unrelated", projectId: "project", threadId: "unrelated-thread", kind: "direct", status: "running", version: 0, controlRevision: 0, steps: [] });
  let workspaceReads = 0;
  let announceBound;
  let releaseBound;
  const bound = new Promise((resolve) => { announceBound = resolve; });
  const gate = new Promise((resolve) => { releaseBound = resolve; });
  const { handlers, adapter } = await mainHandlers(async (path, body) => {
    if (path.endsWith("/creative-workspace") && ++workspaceReads === 2) {
      announceBound();
      await gate;
    }
    return runtime.request(path, body);
  }, command);
  try {
    const running = handlers.get("agent:generate-planning")(rendererEvent(), "project", "generateBrief", "hello");
    await bound;
    const cancelled = await handlers.get("agent:control-work-task")(rendererEvent(), "project", "work-unrelated", "cancel", 0, "unrelated-thread");
    assert.equal(cancelled.status, "cancelled");
    releaseBound();
    const reply = await running;
    assert.ok(reply.message);
    assert.equal(runtime.completions.length, 1);
  } finally { releaseBound(); adapter.close(); }
});

test("cancelling the active work task stops only its bound execution", async () => {
  const runtime = runtimeFixture();
  let workspaceReads = 0;
  let announceBound;
  let releaseBound;
  const bound = new Promise((resolve) => { announceBound = resolve; });
  const gate = new Promise((resolve) => { releaseBound = resolve; });
  const { handlers, adapter } = await mainHandlers(async (path, body) => {
    if (path.endsWith("/creative-workspace") && ++workspaceReads === 2) {
      announceBound();
      await gate;
    }
    return runtime.request(path, body);
  }, command);
  try {
    const running = handlers.get("agent:generate-planning")(rendererEvent(), "project", "generateBrief", "hello");
    await bound;
    const workId = [...runtime.workTasks.keys()][0];
    const cancelled = await handlers.get("agent:control-work-task")(rendererEvent(), "project", workId, "cancel", runtime.workTasks.get(workId).controlRevision, runtime.workTasks.get(workId).threadId);
    assert.equal(cancelled.status, "cancelled");
    releaseBound();
    await assert.rejects(running, /生成已停止/);
    assert.equal(runtime.completions.length, 0);
  } finally { releaseBound(); adapter.close(); }
});

for (const entry of ["button", "conversation", "localRevision"]) {
  test(`${entry} chapter generation executes the admitted same-Work knowledge step`, async () => {
    const runtime = runtimeFixture();
    const calls = [];
    const { handlers, adapter } = await mainHandlers(async (path, body, options = {}) => {
      calls.push({ path, body, method: options.method ?? "GET" });
      if (path.endsWith("/actions")) return { actions: [{
        id: `action-${"c".repeat(32)}`, sourceTaskId: `task-${"0".repeat(32)}`, status: "proposed", inputRevision: 0, intentRevision: 0,
        operation: "generateChapter", target: { type: "chapter", chapterId: "chapter-one", chapterNumber: 1,
          ...(entry === "localRevision" ? { scope: { type: "selectedScenes", sourceRevisionId: "original", groupIds: ["letter"] } } : {}) },
        instruction: "修改白塔章节",
      }] };
      if (path.endsWith("/creative-workspace")) return {
        project: {id:"project"}, chapters:[{id:"chapter-one",number:1}], revision: 0, headSnapshotId: "snapshot-fixture", sourceDigest: "source-fixture",
        modeling: [{ objectId: "draft-chapter-one", sourceRevisionId: "revision-one", status: "model_pending" }],
      };
      if (path.endsWith("/modeling-batches")) return { id: "batch-one" };
      if (path.endsWith("/approve")) return { id: `action-${"c".repeat(32)}`, status: "queued", inputRevision: 0, intentRevision: 0,
        operation: "generateChapter", target: { type: "chapter", chapterId: "chapter-one", chapterNumber: 1,
          ...(entry === "localRevision" ? { scope: { type: "selectedScenes", sourceRevisionId: "original", groupIds: ["letter"] } } : {}) },
        instruction: "修改白塔章节" };
      if (path.endsWith("/finish")) return { status: "completed" };
      const result = await runtime.request(path, body);
      if (path.endsWith("/complete")) {
        const writer = [...runtime.tasks.values()].find(task => task.operation === "generateChapter" && path.includes(task.id));
        if (writer) {
          const work = await runtime.request(`/api/projects/project/tasks/${writer.id}/work-task`);
          work.steps.push({ id: "admitted-knowledge", operation: "modelKnowledge", status: "queued", instruction: "分析本章已保存的正文",
            dependsOnStepId: work.steps[0].id, target: { type: "knowledge", chapterId: "chapter-one", batchId: "batch-one", sourceRevisionId: "revision-one" } });
        }
      }
      return result;
    }, command);
    try {
      const discussion = entry === "button" ? null : await handlers.get("agent:send-prompt")(rendererEvent(), "project", "请修改白塔章节");
      if (discussion) assert.equal(discussion.executionFailure, undefined);
      const result = entry === "button"
        ? await handlers.get("agent:generate-chapter")(rendererEvent(), "project", {
          chapterId: "chapter-one", chapterNumber: 1, intent: "修改白塔章节",
        })
        : discussion.candidateResults[0];
      assert.ok(result);
      assert.equal(calls.filter((call) => call.path.endsWith("/tasks") && call.body?.operation === "modelKnowledge").length, 1);
      const knowledge = calls.find(call => call.path.endsWith("/tasks") && call.body?.operation === "modelKnowledge");
      assert.equal(knowledge.body.target.resumeStepId, "admitted-knowledge");
      assert.equal(knowledge.body.target.sourceRevisionId, "revision-one");
      assert.equal(runtime.workTasks.size, 1);
      if (entry === "localRevision") {
        const candidate = calls.find((call) => call.path.endsWith("/tasks") && call.body?.operation === "generateChapter");
        assert.deepEqual(candidate.body.target.scope, { type: "selectedScenes", sourceRevisionId: "original", groupIds: ["letter"] });
        assert.deepEqual(result.action.target.scope, candidate.body.target.scope);
      }
    } finally { adapter.close(); }
  });
}

test("author starts pending knowledge without a source task or regenerating the chapter", async () => {
  const runtime = runtimeFixture();
  const calls = [];
  const { handlers, adapter } = await mainHandlers(async (path, body, options = {}) => {
    calls.push({ path, body, method: options.method ?? "GET" });
    if (path.endsWith("/chapters/chapter-one/next-step")) return { action: {
      kind: "modelKnowledge", chapterId: "chapter-one", sourceRevisionId: "revision-one",
    } };
    if (path.endsWith("/creative-workspace")) return {
      revision: 0, headSnapshotId: "snapshot-fixture", sourceDigest: "source-fixture",
      modeling: [{ objectId: "draft-chapter-one", sourceRevisionId: "revision-one", status: "model_pending" }],
    };
    if (path.endsWith("/modeling-batches")) return { id: "batch-one" };
    return runtime.request(path, body);
  }, command);
  try {
    const result = await handlers.get("agent:resume-knowledge")(rendererEvent(), "project", "chapter-one");
    assert.equal(result.status, "ready");
    assert.equal(calls.filter((call) => call.path.endsWith("/tasks") && call.method === "POST").length, 1);
    assert.equal(calls.find((call) => call.path.endsWith("/tasks") && call.method === "POST").body.operation, "modelKnowledge");
    const grant = calls.find((call) => call.path.endsWith("/tasks") && call.method === "POST").body.initialBudget;
    assert.deepEqual([grant.maxRequests, grant.maxTokens, grant.deadlineAt], [null, null, null]);
  } finally { adapter.close(); }
});

test("main hands a long Chinese discussion to planning in one bounded turn without losing text", async () => {
  const runtime = runtimeFixture();
  const { handlers, adapter } = await mainHandlers(runtime.request.bind(runtime), command);
  const intent = "作者原始意图：" + "父女在饭桌上互相试探，最后一起看月亮。".repeat(120) + "🌙终点";
  try {
    for (const [name, args] of [
      ["agent:send-prompt", ["project", intent]],
      ["agent:generate-planning", ["project", "generateBrief", intent]],
    ]) {
      const result = await handlers.get(name)(rendererEvent(), ...args);
      const input = result.turn.receivedParams.input;
      assert.ok(Array.isArray(input));
      assert.ok(input.length >= 1);
      assert.ok(input.every((item) => item.type === "text" && Buffer.byteLength(item.text, "utf8") <= 9500));
      assert.ok(input.every((item) => Buffer.from(item.text, "utf8").toString("utf8") === item.text));
      const full = input.map((item) => item.text).join("") + "\n" + Object.values(result.turn.receivedParams.additionalContext).map(item => item.value).join("\n");
      if (name === "agent:send-prompt") assert.match(full, /operation=extractCommitments/);
      if (name === "agent:send-prompt") assert.equal(input.map(item => item.text).join(""), intent);
      else assert.equal(JSON.parse(Object.entries(result.turn.receivedParams.additionalContext).filter(([key]) => key === "novelBackground" || key.startsWith("novelBackground_part_")).map(([,item]) => item.value).join("")).authorRequirements.sourceField, "authorRequirementsSource");
      assert.equal(full.split("作者可见表达：").length - 1, 1);
      assert.match(full, /novel-workflow/);
      assert.match(full, /taskId/);
      assert.ok(Buffer.byteLength(full) <= 256 * 1024);
      assert.equal(full.includes("\ufffd"), false);
      assert.deepEqual(result.turn.receivedParams.environments, []);
    }
  } finally { adapter.close(); }
});

test("main rejects an oversized Unicode intent before creating a task", async () => {
  const runtime = runtimeFixture();
  const { handlers, adapter } = await mainHandlers(runtime.request.bind(runtime), command);
  try {
    await assert.rejects(handlers.get("agent:send-prompt")(rendererEvent(), "project", "🌙 ".repeat(16001)), /UTF-8 字节/);
    assert.equal(runtime.tasks.size, 0);
    assert.equal(adapter.process, null);
  } finally { adapter.close(); }
});

test("main steers an existing turn with all long Unicode instructions", async () => {
  const runtime = runtimeFixture();
  const host = await mainHandlers(runtime.request.bind(runtime), command);
  const intent = "请保留人物的克制和犹豫。".repeat(330) + "🌙结束";
  const handlers = host.handlers;
  try {
    await assert.rejects(handlers.get("agent:steer")({}, "project", intent), /没有可追加指令/);
    const turn = handlers.get("agent:send-prompt")(rendererEvent(), "project", "fixture-wait-for-steer");
    for (let attempt = 0; attempt < 100 && !host.adapter.turnId; attempt += 1) await new Promise((resolve) => setTimeout(resolve, 10));
    assert.equal(host.adapter.turnId, "turn-test");
    await assert.rejects(handlers.get("agent:steer")({}, "other-project", intent), /没有可追加指令/);
    await handlers.get("agent:steer")({}, "project", intent, host.adapter.threadId);
    const result = await turn;
    const input = result.turn.receivedParams.input;
    assert.ok(input.length > 1);
    assert.ok(input.every((item) => Buffer.byteLength(item.text, "utf8") <= 9500));
    assert.ok(input.map((item) => item.text).join("").endsWith(intent));
    const steerBinding = JSON.parse(result.turn.receivedParams.additionalContext.novelTask.value);
    assert.equal(steerBinding.expectedIntentRevision, 1);
    assert.equal(steerBinding.authorMessageId, runtime.intents.get(result.workflow.task.id)[0].authorMessageId);
    assert.equal(runtime.intents.get(result.workflow.task.id)[0].status, "delivered");
    assert.equal(result.turn.receivedParams.expectedTurnId, "turn-test");
  } finally { host.adapter.close(); }
});

test("an uncertain steer never delivers author permission and cancels the active task", async () => {
  const runtime = runtimeFixture();
  const requests = [];
  const host = await mainHandlers(async (path, body, options) => {
    requests.push({ path, body, method: options?.method ?? "GET" });
    return runtime.request(path, body);
  }, command);
  const handlers = host.handlers;
  try {
    const turn = handlers.get("agent:send-prompt")(rendererEvent(), "project", "fixture-wait-for-steer");
    for (let attempt = 0; attempt < 100 && !host.adapter.turnId; attempt += 1) await new Promise((resolve) => setTimeout(resolve, 10));
    assert.equal(host.adapter.turnId, "turn-test");
    host.adapter.steerTurn = async () => { throw new Error("transport status unknown"); };
    await assert.rejects(handlers.get("agent:steer")({}, "project", "请先不要生成简报", host.adapter.threadId), /transport status unknown/);
    await assert.rejects(turn);
    assert.equal(runtime.intents.get(`task-${"0".repeat(32)}`)[0].status, "pending");
    assert.ok(requests.some((request) => request.path === `/api/projects/project/tasks/task-${"0".repeat(32)}/cancel` && request.method === "POST"));
    assert.ok(!requests.some((request) => request.path.endsWith("/intents/1/delivered")));
  } finally { host.adapter.close(); }
});

test("damaged author message receipts cancel steering rather than creating anonymous permission", async () => {
  for (const phase of ["pending", "delivered"]) {
    const runtime = runtimeFixture();
    const requests = [];
    const host = await mainHandlers(async (path, body, options) => {
      requests.push(path);
      const result = await runtime.request(path, body, options);
      if (phase === "pending" && path.endsWith("/intents") && body) return { ...result, authorMessageId: undefined };
      if (phase === "delivered" && path.endsWith("/delivered")) return { ...result, authorMessageId: "wrong-message" };
      return result;
    }, command);
    const handlers = host.handlers;
    try {
      const outcome = handlers.get("agent:send-prompt")(rendererEvent(), "project", "fixture-wait-for-steer")
        .then(value => ({ value }), error => ({ error }));
      for (let attempt = 0; attempt < 100 && !host.adapter.turnId; attempt += 1) await new Promise(resolve => setTimeout(resolve, 10));
      assert.equal(host.adapter.turnId, "turn-test");
      await assert.rejects(handlers.get("agent:steer")({}, "project", "仅讨论人物动机", host.adapter.threadId), phase === "pending" ? /来源无效/ : /交付回执不匹配/);
      assert.equal((await outcome).error?.code, "TASK_CANCELLED");
      const task = [...runtime.tasks.values()][0];
      assert.ok(requests.includes(`/api/projects/project/tasks/${task.id}/cancel`));
      assert.equal(runtime.completions.length, 0);
      const intent = runtime.intents.get(task.id)[0];
      assert.match(intent.authorMessageId, /^author-message-/);
      assert.equal(intent.status, phase === "pending" ? "pending" : "delivered");
    } finally { host.adapter.close(); }
  }
});

test("resuming a candidate references Runtime frozen author history instead of the old work goal", async () => {
  const runtime = runtimeFixture();
  const deadlineAt = Math.floor(Date.now() / 1000) + 120;
  const sourceId = `task-${"a".repeat(32)}`, actionId = `action-${"b".repeat(32)}`, childId = `task-${"c".repeat(32)}`;
  const frozenAuthorHistory = "原始要求\n\n作者追加要求：\n只改动机，保留结局";
  const instruction = "生成动机调整候选";
  const source = { id: sourceId, projectId: "project", threadId: "thr-test", operation: "advisory", status: "completed", inputRevision: 0 };
  const step = { id: actionId, operation: "generateBrief", status: "queued", inputRevision: 0,
    sourceTaskId: sourceId, instruction, target: { type: "artifact" }, dependsOnStepId: "source-step", currentAttemptId: null, attempts: [] };
  const work = { id: "work-source", projectId: "project", threadId: "thr-test", kind: "conversation", status: "running", version: 1,
    goal: "过期工作目标", steps: [{ id: "source-step", operation: "advisory", status: "completed", currentAttemptId: sourceId,
      attempts: [{ id: sourceId, threadId: source.threadId, status: "completed" }] }, step] };
  const action = { id: actionId, sourceTaskId: sourceId, operation: step.operation, target: step.target,
    instruction, inputRevision: 0, intentRevision: 1, status: "queued", resultTaskId: null };
  const permission = { kind: "exactAction", actionId, workTaskId: work.id, scopePermitId: null, actionStatus: "queued", evidence: {
    projectId: "project", sourceTaskId: sourceId, inputRevision: 0, intentRevision: 1, operation: action.operation,
    target: action.target, instruction, authorIntent: frozenAuthorHistory } };
  const inputs = [];
  const onPrepareDomain = domain => {
    const send = domain.codexAdapter.sendTurn.bind(domain.codexAdapter);
    domain.codexAdapter.sendTurn = async (input, options) => { inputs.push({input, options}); return send(input, options); };
  };
  const { handlers, adapter } = await mainHandlers(async (path, body) => {
    if (path.endsWith(`/tasks/${sourceId}`)) return source;
    if (path.endsWith(`/tasks/${sourceId}/actions`)) return { actions: [action] };
    if (path.endsWith(`/actions/${actionId}/permission`)) return { permission };
    if (path.endsWith("/budget-authorization")) return { authorization: { deadlineAt } };
    if (path.endsWith("/provider-budget")) return { taskId: childId, workTaskId: work.id,
      budgetId: `budget-${childId}`, maxRequests: 3, maxTokens: 700, deadlineAt };
    if (path.endsWith("/provider-usage") && !body) return { pending: null };
    if (path.endsWith("/decision")) return { workTaskId: work.id, threadId: work.threadId, step: null, stopReason: "completed" };
    if (path.includes("/work-tasks/") || path.endsWith("/work-task")) return work;
    if (path.endsWith("/tasks")) {
      assert.deepEqual(body.target, { ...action.target, requestedByTaskId: sourceId, requestedActionId: actionId });
      Object.assign(step, { currentAttemptId: childId, status: "running", attempts: [{ id: childId, threadId: body.threadId, status: "queued" }] });
      Object.assign(action, { status: "running", resultTaskId: childId });
      const child = { ...body, id: childId, projectId: "project", purpose: "planner",
      inputManifest: { authorIntent: frozenAuthorHistory,
        candidateInstruction: instruction, authorIntentBinding: { sourceTaskId: sourceId, intentRevision: 1, actionId } } };
      runtime.tasks.set(child.threadId, child);
      return child;
    }
    if (path.endsWith("/complete")) { step.status = "completed"; step.attempts[0].status = "completed"; action.status = "completed"; work.status = "completed"; }
    return runtime.request(path, body);
  }, command, { onPrepareDomain });

  try {
    const result = await handlers.get("agent:resume-work-task")(rendererEvent(), "project", "work-source");
    assert.equal(result.status, "completed");
    const text = inputs[0].input.map(item => item.text).join("") + "\n" + Object.values(inputs[0].options.additionalContext).map(item => item.value).join("\n");
    assert.match(text, /authorRequirementsSource/);
    assert.equal([...runtime.tasks.values()].find(task => task.id === childId).inputManifest.authorIntent, frozenAuthorHistory);
    assert.match(text, /生成动机调整候选/);
    assert.doesNotMatch(text, /过期工作目标/);
    assert.equal(text.includes("只改动机，保留结局"), false);
    assert.equal(inputs[0].input[0].text.includes("只改动机"), false);
  } finally { adapter.close(); }
});

for (const restore of ['agent:resume-thread','agent:reconnect-thread']) {
  test(`main ${restore} keeps the selected historical advisory on the next turn`, async () => {
    const runtime = runtimeFixture();
    runtime.tasks.set('thr-history',{projectId:'project',threadId:'thr-history',purpose:'advisory',target:{type:'project'}});
  runtime.conversations.set('thr-history', {id:'thr-history',projectId:'project',title:'历史对话',isArchived:false,isPinned:false,createdAt:1,updatedAt:1,version:0});
    const {handlers,adapter} = await mainHandlers(runtime.request.bind(runtime),command);
    try {
      await handlers.get('agent:send-prompt')(rendererEvent(),'project','first discussion');
      await handlers.get(restore)({},'project','thr-history',null);
      const result = await handlers.get('agent:send-prompt')(rendererEvent(),'project','continue selected',null,null,'thr-history');
      assert.equal(result.workflow.task.threadId,'thr-history');
      assert.equal(result.message,'hello');
      const page = await handlers.get('agent:list-threads')({},'project');
      assert.ok(page.threads.some(thread => thread.id === 'thr-history'));
      await assert.rejects(handlers.get('agent:resume-thread')({},'other-project','foreign-thread'),/不属于当前作品/);
    } finally {adapter.close();}
  });
}

test("main restores an advisory after application restart without an in-memory preference", async () => {
  const runtime = runtimeFixture();
  runtime.tasks.set('thr-history',{projectId:'project',threadId:'thr-history',purpose:'advisory',target:{type:'project'}});
  runtime.conversations.set('thr-history', {id:'thr-history',projectId:'project',title:'历史对话',isArchived:false,isPinned:false,createdAt:1,updatedAt:1,version:0});
  const {handlers,adapter} = await mainHandlers(runtime.request.bind(runtime),command);
  try {
    await handlers.get('agent:resume-thread')({},'project','thr-history');
    const result = await handlers.get('agent:send-prompt')(rendererEvent(),'project','continue after restart',null,null,'thr-history');
    assert.equal(result.workflow.task.threadId,'thr-history');
  } finally {adapter.close();}
});


test("main returns an empty thread page without an invalid ownership lookup", async () => {
  const runtime = runtimeFixture();
  const {handlers,adapter} = await mainHandlers(runtime.request.bind(runtime),command);
  try {
    const page = await handlers.get('agent:list-threads')({},'project',false,null);
    assert.equal(page.threads.length,0);
    assert.equal(runtime.lookup.length,0);
  } finally {adapter.close();}
});


test("production advisory sends the shared builder output with exact author, source and task binding", async () => {
  const runtime = runtimeFixture();
  const workspace = { project: { id: "project" }, revision: 0, headSnapshotId: "snapshot-fixture", sourceDigest: "source-fixture",
    chapters: [{ id: "chapter-one", number: 1, candidate: { id: "draft-one", sceneGroups: [{ id: "scene-one", body: "门后传来钟声。" }] }, accepted: null }] };
  const reference = { kind: "scene", projectId: "project", snapshotId: workspace.headSnapshotId,
    objectId: "draft-chapter-one", revisionId: "draft-one", sceneGroupId: "scene-one" };
  const requests = [];
  const sent = [];
  const onPrepareDomain = domain => {
    const send = domain.codexAdapter.sendTurn.bind(domain.codexAdapter);
    domain.codexAdapter.sendTurn = async (input, options) => { sent.push({input, options}); return send(input, options); };
  };
  const { handlers, adapter } = await mainHandlers(async (path, body, options) => {
    requests.push({ path, body });
    if (path.endsWith("/creative-workspace")) return structuredClone(workspace);
    return runtime.request(path, body, options);
  }, command, { onPrepareDomain });

  try {
    for (const focus of [null, { view: "chapters", chapterId: "chapter-one", chapterNumber: 1, reference }]) {
      const author = "请讨论这个场景的悬念节奏，保留原文，不生成或确认任何内容。";
      const result = await handlers.get("agent:send-prompt")(rendererEvent(), "project", author, null, focus);
      const task = result.workflow.task;
      const expected = composeAdvisoryPrompt(task, taskExecutionPrompt(task), createTaskBoundHandoff("project", workspace, task), focus);
      assert.deepEqual(sent.at(-1).input, expected.input);
      assert.deepEqual(sent.at(-1).options.additionalContext.novelTask, expected.additionalContext.novelTask);
      assert.equal(sent.at(-1).options.disableEnvironment, true);
      const text = expected.input.map(item => item.text).join("") + "\n" + Object.values(expected.additionalContext).map(item => item.value).join("\n");
      assert.ok(text.includes(author) && text.includes(task.id) && text.includes(task.threadId));
      assert.equal(text.split("作者可见表达：").length - 1, 1);
      assert.match(text, /query_story/);
      assert.match(text, /selectedScenes/);
      assert.match(text, /request_candidate_task/);
      assert.match(text, /不重复已授权事项/);
      assert.match(text, /有真实回执才称保存/);
      assert.match(text, /保存|保存了什么/);
      if (focus) assert.ok(text.includes(JSON.stringify(focus).replaceAll('"', '\\"')));
      assert.ok(modelInputTokenBudget(text) <= 9000);
    }
    assert.equal(sent.length, 2);
    assert.ok(!requests.some(request => /accept|establish|save-candidate/.test(request.path)));
  } finally { adapter.close(); }
});

for (const sameWork of [true, false]) {
  test(`pausing generation interrupts only its current lease (same Work: ${sameWork})`, async () => {
    const cancellations = [], requests = [];
    const workId = "work-generation-paused";
    const flow = { id: sameWork ? workId : "work-generation-other", projectId: "project", cancelled: false };
    const execution = createTaskExecution();
    bindExecutionTask(execution, { id: "task-current", projectId: "project" });
    execution.engineRunId = "lease-current";
    execution.engineHost = { cancelRun: async (...args) => { cancellations.push(args); return true; } };
    let paused = false;
    const { handlers, adapter } = await mainHandlers(async (path, body) => {
      requests.push({ path, body });
      if (path.endsWith("/control")) { assert.equal(body.command, "pause"); assert.equal(body.expectedControlRevision,2); paused = true; return {id:workId,projectId:"project",threadId:"generation-thread",kind:"generation",status:"paused",version:3,controlRevision:3}; }
      if (path.includes("/generation-runs/")) return { version: 4 };
      return { id: workId, projectId: "project", threadId: "generation-thread", kind: "generation", status: paused ? "paused" : "running", version: 20, controlRevision: 2 };
    }, command, { activeWorkFlow: flow, activeAgentExecution: execution });
    try {
      const result = await handlers.get("agent:control-work-task")(rendererEvent(), "project", workId, "pause", 2, "generation-thread");
      assert.equal(result.status, "paused");
      assert.deepEqual(cancellations, sameWork ? [["lease-current", "author-interrupt"]] : []);
      assert.equal(flow.cancelled, false, "pause must preserve the parent Work for continuation");
      assert.equal(requests.some(({ path }) => path.endsWith("/cancel")), false,
        "Native pause owns the author_paused marker; ordinary cancellation must not overwrite it");
    } finally { adapter.close(); }
  });
}

test("generation pause uses one Native Work CAS endpoint rather than a separate Run ledger", async () => {
  const calls=[];
  const {handlers,adapter}=await mainHandlers(async(path,body)=>{
    calls.push({path,body});
    if(path.endsWith('/control')) {assert.deepEqual({expectedControlRevision:body.expectedControlRevision,command:body.command},{expectedControlRevision:3,command:'pause'});return {id:'work-generation-target',projectId:'project',threadId:'selected-thread',kind:'generation',status:'paused',version:40,controlRevision:4};}
    return {id:'work-generation-target',projectId:'project',threadId:'selected-thread',kind:'generation',status:'running',version:30,controlRevision:3};
  },command);
  try {
    const result=await handlers.get('agent:control-work-task')(rendererEvent(),'project','work-generation-target','pause',3,'selected-thread');
    assert.equal(result.status,'paused');assert.equal(calls.length,2);assert.equal(calls.some(call=>call.path.includes('/generation-runs/')),false);
  } finally {adapter.close();}
});

test("a delayed applied pause never interrupts a later local execution of the same Work", async () => {
  const work = { id: "work-generation-late", projectId: "project", threadId: "thread", status: "running", version: 5, controlRevision: 0 };
  const firstFlow = { id: work.id, projectId: work.projectId }, laterFlow = { ...firstFlow };
  const cancelled = [];
  const makeExecution = id => {
    const execution = createTaskExecution();
    bindExecutionTask(execution, { id, projectId: "project" });
    execution.engineRunId = id;
    execution.engineHost = { cancelRun: async (...args) => { cancelled.push(args); return true; } };
    return execution;
  };
  let release, entered;
  const blocked = new Promise(resolve => { release = resolve; });
  const received = new Promise(resolve => { entered = resolve; });
  const first = makeExecution("first-attempt"), later = makeExecution("later-attempt");
  const main = await mainHandlers(async path => {
    if (path.endsWith("/control")) { entered(); await blocked; return { ...work, status: "paused", version: 6, controlRevision: 1 }; }
    return structuredClone(work);
  }, command, { activeWorkFlow: firstFlow, activeAgentExecution: first });
  try {
    const pending = main.handlers.get("agent:control-work-task")(rendererEvent(), "project", work.id, "pause", 0, "thread");
    await received;
    main.replaceActiveExecution(laterFlow, later);
    release();
    await assert.rejects(pending, /没有停止后来的执行/);
    assert.deepEqual(cancelled, []);
    assert.equal(later.cancellationRequested, false);
    assert.equal(laterFlow.stopRequested, undefined);
  } finally { main.adapter.close(); main.chapterStructureConsumer.stop(); }
});

test("pause after step preserves the current execution and validates the Native barrier", async () => {
  for (const valid of [true, false]) {
    const cancellations = [], flow = { id: "work-draining", projectId: "project", cancelled: false };
    const execution = createTaskExecution();
    bindExecutionTask(execution, { id: "current-task", projectId: "project" });
    execution.engineRunId = "current-lease";
    execution.engineHost = { cancelRun: async (...args) => { cancellations.push(args); return true; } };
    const work = { id: flow.id, projectId: "project", threadId: "thread", status: "running", version: 4, controlRevision: 1 };
    const { handlers, adapter } = await mainHandlers(async (path, body) => {
      if (path.endsWith("/control")) {
        assert.equal(body.command, "pauseAfterStep");
        return { ...work, version: 5, controlRevision: 2, pauseAfterStep: { controlRevision: valid ? 2 : 1, remainingAttempts: 1 } };
      }
      return work;
    }, command, { activeWorkFlow: flow, activeAgentExecution: execution });
    try {
      const promise = handlers.get("agent:control-work-task")(rendererEvent(), "project", work.id, "pauseAfterStep", 1, "thread");
      if (valid) { const result = await promise; assert.equal(result.status, "running"); assert.equal(result.pauseAfterStep.remainingAttempts, 1); }
      else await assert.rejects(promise, /暂停收尾回执无法核对/);
      assert.deepEqual(cancellations, []); assert.equal(flow.cancelled, false); assert.equal(flow.stopRequested, undefined);
    } finally { adapter.close(); }
  }
});
