import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import { runInNewContext } from "node:vm";
import { fileURLToPath } from "node:url";
import { registerExecutionScopeHandlers } from "../execution-scope-handlers.mjs";
import { advanceWorkSteps } from "../work-step-continuation.mjs";
import { mainHandlers, rendererEvent, runtimeFixture } from "./main-handler-harness.mjs";

const source = `task-${"b".repeat(32)}`, action = `action-${"a".repeat(32)}`, digest = "d".repeat(64);
const request = { actionIds: [action], maxActions: 2, maxTaskAttempts: 8, expectedIntentRevision: 0 };
const scope = { projectId: "project", sourceTaskId: source, workTaskId: "work-scope", previewDigest: digest,
  targets: [{ operation: "generateBrief", target: { type: "artifact" } }], request };
const permit = { id: `scope-${digest}`, scope, status: "active" };

function fixture({ existing = null, workStatus = "running", currentDigest = digest, failAvailable = false } = {}) {
  const handlers = new Map(), calls = [], runs = [], preflights = [];
  registerExecutionScopeHandlers({ handle: (name, handler) => handlers.set(name, handler) }, {
    runtimeRequest: async (path, options) => {
      calls.push([path, options?.body ? JSON.parse(options.body) : null]);
      if (path.endsWith("/preview")) return { ...scope, previewDigest: currentDigest };
      if (path.includes("/work-tasks/")) return { id: "work-scope", status: workStatus };
      return options?.method === "POST" ? permit : { scope: existing };
    },
    withWorkFlow: async (_project, run) => run({ id: null }),
    assertOperationsAvailable: operations => { preflights.push(operations); if (failAvailable) throw new Error("模型不可用"); },
    advance: async (...args) => { runs.push(args); return { status: "completed" }; },
  });
  return { handlers, calls, runs, preflights };
}

test("scope IPC rejects malformed or authority-bearing input before any request", () => {
  const { handlers, calls } = fixture();
  const preview = handlers.get("agent:preview-execution-scope");
  for (const value of [{ ...request, authorized: true }, { ...request, actionIds: [action, action] }, { ...request, maxActions: 0 },
    { ...request, maxTaskAttempts: 17 }, { ...request, actionIds: ["../other"] }, { ...request, expectedIntentRevision: -1 }]) {
    assert.throws(() => preview({}, "project", source, value), /参数无效/);
  }
  assert.throws(() => preview({}, "a\0b", source, request), /身份无效/);
  assert.throws(() => handlers.get("agent:confirm-execution-scope")({}, "project", source, request, "invalid"), /摘要无效/);
  assert.deepEqual(calls, []);
});

test("confirmation preflights current scope and model before saving and advances the same owner", async () => {
  const { handlers, calls, runs, preflights } = fixture();
  const event = {};
  assert.deepEqual(await handlers.get("agent:confirm-execution-scope")(event, "project", source, request, digest), { permit, work: { status: "completed" } });
  assert.deepEqual(calls.map(([path]) => path), [`/api/projects/project/tasks/${source}/execution-scope`, `/api/projects/project/tasks/${source}/execution-scope/preview`,
    `/api/projects/project/tasks/${source}/execution-scope`, "/api/projects/project/work-tasks/work-scope"]);
  assert.deepEqual(calls[2][1], { request, previewDigest: digest });
  assert.deepEqual(runs, [[event, "project", "work-scope"]]);
  assert.deepEqual(preflights, [["generateBrief", "advisory"], ["generateBrief", "advisory"]]);
  for (const options of [{ currentDigest: "e".repeat(64) }, { failAvailable: true }]) {
    const denied = fixture(options);
    await assert.rejects(denied.handlers.get("agent:confirm-execution-scope")({}, "project", source, request, digest));
    assert.equal(denied.calls.some(([path, body]) => body && path.endsWith("/execution-scope")), false);
    assert.deepEqual(denied.runs, []);
  }
});

test("confirmation replay reads history without re-previewing or resuming stopped work", async () => {
  for (const workStatus of ["paused", "cancelled", "blocked", "completed"]) {
    const f = fixture({ existing: permit, workStatus });
    const result = await f.handlers.get("agent:confirm-execution-scope")({}, "project", source, request, digest);
    assert.equal(result.work.status, workStatus);
    assert.deepEqual(f.preflights, []);
    assert.deepEqual(f.runs, []);
    assert.equal(f.calls.some(([path]) => path.endsWith("/preview") || path.endsWith("/control")), false);
  }
});

test("shared work advancement respects current domain state, dependencies and persisted action identity", async () => {
  const steps = [{ id: "root", status: "completed", attempts: [{ id: source, threadId: "root-thread", status: "completed" }] },
    { id: action, status: "queued", operation: "generateBrief", instruction: "候选方法", inputRevision: 0,
      currentAttemptId: null, attempts: [], sourceTaskId: source, target: { type: "artifact" }, dependsOnStepId: "root" },
    { id: "outside", status: "proposed", operation: "generateOutline" }];
  const executions = [];
  const work = { id: "work-scope", projectId: "project", threadId: "root-thread", status: "running", goal: "原作者要求", steps };
  const deps = { currentWorkFlowState: async () => "running", runtimeRequest: async path => {
    if (path.endsWith("/decision")) { work.status = "completed"; return { workTaskId: work.id, threadId: work.threadId, step: null, stopReason: "completed" }; }
    if (path.endsWith(`/tasks/${source}`)) return { id: source, projectId: "project", threadId: work.threadId,
      operation: "advisory", status: "completed", inputRevision: 0 };
    if (path.endsWith("/actions")) return { actions: [{ ...steps[1], intentRevision: 0, resultTaskId: null }] };
    if (path.endsWith("/permission")) return { permission: { kind: "finiteScope", actionId: action, workTaskId: work.id,
      scopePermitId: permit.id, actionStatus: "queued", evidence: { projectId: "project", sourceTaskId: source, inputRevision: 0,
        intentRevision: 0, operation: steps[1].operation, target: steps[1].target, instruction: steps[1].instruction, authorIntent: work.goal } } };
    return work;
  }, runCreativeAgentTask: async (...args) => { executions.push(args); steps[1].status = "completed"; } };
  const event = {};
  await advanceWorkSteps(event, "project", "work-scope", deps);
  assert.deepEqual(executions, [[event, "project", "候选方法", { operation: "generateBrief" }, {
    type: "artifact", requestedByTaskId: source, requestedActionId: action }]]);
  steps[1].status = "queued"; work.status = "running";
  await advanceWorkSteps(event, "project", "work-scope", { ...deps, currentWorkFlowState: async () => "paused" });
  work.status = "paused"; await advanceWorkSteps(event, "project", "work-scope", deps);
  assert.equal(executions.length, 1);

});

test("production confirmation locks its Native thread while independent conversation proceeds", async () => {
  let unblock, entered;
  const held = new Promise(resolve => { unblock = resolve; });
  const reading = new Promise(resolve => { entered = resolve; });
  const calls = [], runtime = runtimeFixture();
  const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;
  const { handlers, adapter } = await mainHandlers(async (path, body, options) => {
    calls.push([path, body]);
    if (path === `/api/projects/project/tasks/${source}/work-task`) return {
      id: "work-scope", projectId: "project", threadId: "thread-scope", status: "paused", steps: [] };
    if (path.endsWith("/execution-scope") && !body) { entered(); await held; return { scope: permit }; }
    if (path.endsWith("/execution-scope")) return permit;
    if (path.endsWith("/work-tasks/work-scope")) return { status: "paused", id: "work-scope" };
    return runtime.request(path, body, options);
  }, command);
  try {
    const event = rendererEvent();
    const pending = handlers.get("agent:confirm-execution-scope")(event, "project", source, request, digest);
    await reading;
    await assert.rejects(handlers.get("agent:confirm-execution-scope")(event, "project", source, request, digest), /该对话已有任务正在运行/);
    await assert.rejects(handlers.get("agent:send-prompt")(event, "project", "不能并行创建根任务", null, null, "thread-scope"), /该对话已有任务正在运行/);
    assert.equal(calls.some(([path]) => path.endsWith("/conversations") || path.endsWith("/tasks")), false,
      "same-thread conflict is rejected before conversation or task mutation");
    await handlers.get("agent:send-prompt")(event, "project", "另一对话可以独立执行");
    assert.equal(runtime.workTasks.size, 1, "independent thread can finish before confirmation is released");
    unblock();
    assert.equal((await pending).work.status, "paused");
    assert.equal(calls.filter(([path, body]) => path.endsWith("/execution-scope") && body).length, 1);
    assert.equal(calls.some(([path]) => path.endsWith("/control")), false);
  } finally { unblock(); adapter.close(); }
});

test("stopping the original confirmation request during read prevents permit mutation", async () => {
  let unblock, entered;
  const held = new Promise(resolve => { unblock = resolve; });
  const reading = new Promise(resolve => { entered = resolve; });
  const calls = [];
  const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;
  const main = await mainHandlers(async (path, body) => {
    calls.push([path, body]);
    if (path.endsWith("/work-task") || path.endsWith("/work-tasks/work-scope")) return { id: "work-scope", projectId: "project", threadId: "thread-scope", status: "paused", steps: [] };
    if (path.endsWith("/execution-scope") && !body) { entered(); await held; return { scope: permit }; }
    throw Error(`Unexpected request ${path}`);
  }, command);
  try {
    const event = rendererEvent();
    const pending = main.handlers.get("agent:confirm-execution-scope")(event, "project", source, request, digest);
    await reading;
    const target = main.executionTargets.get(event);
    assert.ok(target.requestId);
    await assert.rejects(main.handlers.get("agent:interrupt")(rendererEvent(), target), /不属于原执行/);
    await main.handlers.get("agent:interrupt")(event, target);
    unblock();
    await assert.rejects(pending, error => error.code === "TASK_CANCELLED");
    assert.equal(calls.some(([, body]) => body), false, "cancelled read cannot grant authority or create a task");
  } finally { unblock(); main.adapter.close(); }
});

for (const followup of [false, true]) test(`production confirmation executes candidate and ${followup ? "Agent-selected followup" : "no followup"} through the original workflow without new budget consent`, async () => {
  const runtime = runtimeFixture();
  const state = { confirmations: [], budgets: [] };
  let currentPermit = null, currentPreview, sourceTask, work, approvedAction;
  const created = [], requests = [];
  const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;
  const { handlers, adapter } = await mainHandlers(async (path, body) => {
    requests.push([path, body]);
    if (currentPermit && path.endsWith(`/tasks/${sourceTask.id}/actions`)) return { actions: [approvedAction] };
    if (currentPermit && path.endsWith(`/actions/${action}/permission`)) return { permission: { kind: "finiteScope", actionId: action,
      workTaskId: work.id, scopePermitId: currentPermit.id, actionStatus: approvedAction.status, evidence: {
        projectId: "project", sourceTaskId: sourceTask.id, inputRevision: sourceTask.inputRevision, intentRevision: 0,
        operation: approvedAction.operation, target: approvedAction.target, instruction: approvedAction.instruction, authorIntent: work.goal } } };
    if (path.endsWith("/decision") && currentPermit) {
      if (followup && work.steps.every(step => step.status === "completed") && !work.steps.some(step => step.id === `followup-${action}`)) {
        work.steps.push({ id: `followup-${action}`, operation: "advisory", status: "queued", dependsOnStepId: action,
          target: { workDecision: { workTaskId: work.id, originTaskId: sourceTask.id, sourceTaskId: created.find(task => task.operation === "generateBrief").id } } });
        work.status = "running";
      }
      const step = work.steps.find(step => step.status === "queued");
      if (!step) work.status = "completed";
      return { workTaskId: work.id, threadId: work.threadId, decisionStepId: step?.id ?? null, step: step ?? null, stopReason: step ? null : "completed" };
    }
    if (path.endsWith("/execution-scope/preview")) return currentPreview;
    if (path.endsWith("/execution-scope")) {
      if (!body) return { scope: currentPermit };
      if (!currentPermit) {
        currentPermit = { ...permit, scope: currentPreview };
        work.status = "running";
        work.steps.push({ id: action, status: "queued", operation: "generateBrief", instruction: "已许可的候选简报",
          sourceTaskId: sourceTask.id, dependsOnStepId: work.steps[0].id, target: { type: "artifact" },
          inputRevision: sourceTask.inputRevision, currentAttemptId: null, attempts: [] });
        approvedAction = { ...work.steps.at(-1), intentRevision: 0, resultTaskId: null };
      }
      return currentPermit;
    }
    const result = await runtime.request(path, body);
    if (path.endsWith("/complete")) {
      for (const owner of runtime.workTasks.values()) for (const step of owner.steps) {
        for (const attempt of step.attempts ?? []) if (path.includes(`/tasks/${attempt.id}/`)) attempt.status = "completed";
      }
    }
    if (path.endsWith("/tasks")) {
      created.push(result);
      const owner = [...runtime.workTasks.values()].find(item => item.steps.some(step => step.currentAttemptId === result.id));
      let step = owner.steps.find(item => item.currentAttemptId === result.id);
      if (approvedAction && result.target.requestedActionId === approvedAction.id) {
        owner.steps.splice(owner.steps.indexOf(step), 1);
        step = owner.steps.find(item => item.id === approvedAction.id);
        Object.assign(step, { currentAttemptId: result.id, status: "running" });
        Object.assign(approvedAction, { resultTaskId: result.id, status: "running" });
      }
      Object.assign(step, { operation: result.operation, attempts: [{ id: result.id, threadId: result.threadId }] });
      if (result.target.workDecision) result.inputManifest = { workDecision: { ...result.target.workDecision,
        rootThreadId: owner.threadId, decisionStepId: step.id, decisionTaskId: result.id } };
    }
    if (path.endsWith("/complete") && work && path.includes(created.at(-1)?.id) && created.at(-1)?.operation === "generateBrief") {
      work.steps.find(step => step.id === action).status = "completed";
      approvedAction.status = "completed";
      work.status = "completed";
    }
    if (path.endsWith("/complete") && created.at(-1)?.target?.workDecision) {
      work.steps.find(step => step.id === `followup-${action}`).status = "completed";
      work.status = "completed";
    }
    return result;
  }, command, state);
  try {
    const event = rendererEvent();
    await handlers.get("agent:send-prompt")(event, "project", "分析定位，然后在许可内生成候选");
    sourceTask = created[0]; work = [...runtime.workTasks.values()][0];
    work.status = "completed"; work.goal = sourceTask.authorIntent; work.steps[0].status = "completed";
    const continuedRequest = request;
    currentPreview = { ...scope, request: continuedRequest, sourceTaskId: sourceTask.id, workTaskId: work.id };
    const result = await handlers.get("agent:confirm-execution-scope")(event, "project", sourceTask.id, continuedRequest, digest);
    assert.equal(result.work.status, "completed");
    assert.equal(created.length, 2 + Number(followup));
    assert.deepEqual(created[1].target, { type: "artifact", requestedByTaskId: sourceTask.id, requestedActionId: action });
    assert.equal(created[1].initialBudget, undefined);
    assert.deepEqual(state.confirmations, []);
    assert.equal(state.budgets.length, 2 + Number(followup));
    if (followup) {
      assert.deepEqual(created[2].target, { resumeStepId: `followup-${action}`, workDecision: { workTaskId: work.id, originTaskId: sourceTask.id, sourceTaskId: created[1].id } });
      assert.equal(created[2].initialBudget, undefined);
    }
    assert.equal(requests.some(([path]) => path.endsWith("/approve") || path.endsWith("/control")), false);
    await handlers.get("agent:confirm-execution-scope")(event, "project", sourceTask.id, continuedRequest, digest);
    assert.equal(created.length, 2 + Number(followup), "completed confirmation replay must not execute the candidate or followup again");
  } finally { adapter.close(); }
});

test("preload confirms scope with a stoppable request identity and exact arguments", async () => {
  const calls = [], events = [];
  let api;
  runInNewContext(await readFile(new URL("../preload.cjs", import.meta.url), "utf8"), {
    crypto: { randomUUID: () => "scope-request" }, process,
    require: () => ({ contextBridge: { exposeInMainWorld: (_name, value) => { api = value; } },
      ipcRenderer: { invoke: (...args) => { calls.push(args); return Promise.resolve({}); }, on() {}, removeListener() {} } }),
  });
  const unsubscribe = api.agent.onEvent(event => events.push(event));
  await api.agent.confirmExecutionScope("project", source, request, digest);
  unsubscribe();
  assert.deepEqual(JSON.parse(JSON.stringify(calls)), [["agent:confirm-execution-scope", {
    requestId: "scope-request", args: ["project", source, request, digest],
  }]]);
  assert.deepEqual(JSON.parse(JSON.stringify(events)), [{ method: "agent/execution-request", params: {
    requestId: "scope-request", projectId: "project",
  } }]);
});

test("confirmation rejects a foreign Native owner before reading or writing permission", async () => {
  const calls = [];
  const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;
  const main = await mainHandlers(async path => {
    calls.push(path);
    return { id: "work-foreign", projectId: "foreign", threadId: "foreign-thread" };
  }, command);
  try {
    await assert.rejects(main.handlers.get("agent:confirm-execution-scope")(rendererEvent(), "project", source, request, digest), /执行归属不匹配/);
    assert.deepEqual(calls, [`/api/projects/project/tasks/${source}/work-task`]);
  } finally { main.adapter.close(); }
});

test("stop during committed scope confirmation pauses the original Work and never advances", async () => {
  let unblock, entered;
  const held = new Promise(resolve => { unblock = resolve; });
  const writing = new Promise(resolve => { entered = resolve; });
  const calls = [];
  const work = { id: "work-scope", projectId: "project", threadId: "thread-scope", status: "running", controlRevision: 0, version: 0, steps: [] };
  const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;
  const main = await mainHandlers(async (path, body) => {
    calls.push([path, body]);
    if (path.endsWith("/work-task") || path.endsWith("/work-tasks/work-scope")) return work;
    if (path.endsWith("/execution-scope")) {
      if (!body) return { scope: permit };
      entered(); await held; return permit;
    }
    if (path.endsWith("/control")) {
      assert.equal(body.command, "pause");
      work.status = "paused"; work.controlRevision++; work.version++; return work;
    }
    throw Error(`Unexpected request ${path}`);
  }, command);
  try {
    const event = rendererEvent();
    const pending = main.handlers.get("agent:confirm-execution-scope")(event, "project", source, request, digest);
    await writing;
    await main.handlers.get("agent:interrupt")(event, main.executionTargets.get(event));
    unblock();
    await assert.rejects(pending, error => error.code === "TASK_CANCELLED");
    assert.equal(work.status, "paused");
    assert.equal(calls.filter(([path, body]) => path.endsWith("/execution-scope") && body).length, 1,
      "already committed permission is not resent or undone");
    assert.equal(calls.filter(([path]) => path.endsWith("/control")).length, 1);
    assert.equal(calls.some(([path]) => path.endsWith("/tasks") || path.endsWith("/decision")), false);
  } finally { unblock(); main.adapter.close(); }
});
