import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { runInNewContext } from "node:vm";
import test from "node:test";
import { advisoryTaskTarget, assertAdvisoryPromptRoute, assertRevisionRepairTaskTarget, preflightRevisionRepair,
  validateRevisionRepairSource } from "../revision-repair-handlers.mjs";
import { mainHandlers, rendererEvent, runtimeFixture } from "./main-handler-harness.mjs";

const source = { writerTaskId: `task-${"a".repeat(32)}`, assessmentRevisionId: "assessment-one", decisionId: "repair-one" };
const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;
const plain = value => JSON.parse(JSON.stringify(value));
const outcome = () => ({ revisionId: source.assessmentRevisionId, state: "current", authority: "candidateAnalysis",
  assessment: { writerTaskId: source.writerTaskId, chapterId: "chapter-one", originalRevisionId: "original-one",
    draftRevisionId: "draft-one", knowledgeBatchId: "batch-one", outcomes: [] },
  decisions: [{ id: source.decisionId, writerTaskId: source.writerTaskId, assessmentRevisionId: source.assessmentRevisionId,
    draftRevisionId: "draft-one", knowledgeBatchId: "batch-one", choice: "repairDraft", note: "加强动机", createdAt: 1 }] });
const frozenInput = () => ({ authority: "candidateAnalysis", source: { ...source }, authorDecision: outcome().decisions[0],
  chapterId: "chapter-one", chapterNumber: 1, selectedGroupIds: ["group-one"],
  desiredOutcomes: [{ id: "motivation", criterion: "动机有原文依据", groupIds: ["group-one"] }],
  assessment: [{ id: "motivation", status: "unmet", reason: "尚未说明动机" }] });

test("null remains a regular advisory; repair builds only a frozen project provenance target", async () => {
  assert.equal(validateRevisionRepairSource(), null);
  assert.equal(validateRevisionRepairSource(null), null);
  assert.deepEqual(advisoryTaskTarget(), { type: "project" });
  assert.deepEqual(advisoryTaskTarget({ revisionRepairSource: null }), { type: "project" });
  await preflightRevisionRepair({ projectId: "project", source: null, runtimeRequest: () => assert.fail("ordinary advisory needs no outcome") });
  const supplied = { ...source };
  const target = advisoryTaskTarget({ revisionRepairSource: supplied });
  supplied.decisionId = "changed";
  assert.deepEqual(target, { type: "project", revisionRepairSource: source });
  assert.ok(Object.isFrozen(target) && Object.isFrozen(target.revisionRepairSource));
  assert.deepEqual(advisoryTaskTarget({ resumeStep: { id: "step-one", target: { type: "project" } } }),
    { type: "project", resumeStepId: "step-one" });
});

test("source accepts exactly the RevisionPredecessor shape and bounded identifiers", () => {
  assert.deepEqual(validateRevisionRepairSource(source), source);
  for (const invalid of [false, "source", [], {}, { ...source, choice: "repairDraft" },
    { ...source, sourceTaskId: source.writerTaskId }, { ...source, writerTaskId: "task-other" },
    { ...source, writerTaskId: null }, { ...source, assessmentRevisionId: "" },
    { ...source, assessmentRevisionId: " assessment-one" }, { ...source, assessmentRevisionId: "x\u0085y" },
    { ...source, assessmentRevisionId: "字".repeat(54) }, { ...source, decisionId: "bad_id" },
    { ...source, decisionId: "x".repeat(129) }, { ...source, [Symbol("hidden")]: "value" }]) {
    assert.throws(() => validateRevisionRepairSource(invalid), /三字段来源/);
  }
  for (const key of Object.keys(source)) {
    const missing = { ...source };
    delete missing[key];
    assert.throws(() => validateRevisionRepairSource(missing), /三字段来源/);
  }
  assert.throws(() => validateRevisionRepairSource(Object.create(source)), /三字段来源/);
});

test("repair navigation context survives the Runtime JSON boundary without granting another chapter", () => {
  const target = { type: "project", revisionRepairSource: source, conversationFocus: { view: "chapters", chapterId: "chapter-one", chapterNumber: 1 } };
  const task = { projectId: "project", operation: "advisory", target: structuredClone(target) };
  assert.doesNotThrow(() => assertRevisionRepairTaskTarget(task, "project", target));
  assert.throws(() => assertRevisionRepairTaskTarget({ ...task, target: { ...task.target, conversationFocus: { ...target.conversationFocus, view: "diffs" } } }, "project", target), /精确的修复/);
  assert.throws(() => assertRevisionRepairTaskTarget({ ...task, target: { ...task.target, conversationFocus: null } }, "project", target), /精确的修复/);
  const unfocused = { ...target, conversationFocus: null };
  assert.throws(() => assertRevisionRepairTaskTarget({ ...task, target: { ...unfocused, conversationFocus: { view: "world" } } }, "project", unfocused), /精确的修复/);
  assert.throws(() => assertRevisionRepairTaskTarget({ ...task, target: { ...task.target, conversationFocus: { view: "chapters", chapterId: "other", chapterNumber: 2 } } }, "project", target), /精确的修复/);
  assert.deepEqual(advisoryTaskTarget({ resumeStep: { id: "step", target } }), { ...target, resumeStepId: "step" });
});

test("repair cannot be routed as steer, resumed step, or a concurrent root", () => {
  assert.throws(() => assertAdvisoryPromptRoute({ steer: true, revisionRepairSource: source }), /steer 或 resumeStep/);
  assert.throws(() => advisoryTaskTarget({ resumeStep: { id: "step-one", target: {} }, revisionRepairSource: source }), /steer 或 resumeStep/);
  const resumeStep = { id: "step-one", target: { type: "project", revisionRepairSource: { ...source } } };
  const resumed = advisoryTaskTarget({ resumeStep });
  resumeStep.target.revisionRepairSource.decisionId = "mutated";
  assert.deepEqual(resumed, { type: "project", revisionRepairSource: source, resumeStepId: "step-one" });
  assert.ok(Object.isFrozen(resumed.revisionRepairSource));
  assert.throws(() => advisoryTaskTarget({ resumeStep: { id: "step-one", target: { type: "project", revisionRepairSource: null } } }), /冻结 target/);
  assert.throws(() => advisoryTaskTarget({ resumeStep: { id: "step-one", target: { type: "project", revisionRepairSource: { ...source, extra: true } } } }), /三字段来源/);
  for (const key of ["activeWorkFlow", "activeAgentExecution", "activeGenerationRunId"]) {
    for (const revisionRepairSource of [null, source]) {
      assert.throws(() => assertAdvisoryPromptRoute({ [key]: {}, revisionRepairSource }), /已有创作任务/);
    }
  }
  assert.doesNotThrow(() => assertAdvisoryPromptRoute({ steer: true, activeAgentExecution: {} }));
});

test("preload supplies explicit null by default and forwards the exact repair source", async () => {
  const calls = [];
  let api;
  runInNewContext(await readFile(new URL("../preload.cjs", import.meta.url), "utf8"), {
    process, require: () => ({ contextBridge: { exposeInMainWorld: (_name, value) => { api = value; } },
      ipcRenderer: { invoke: (...args) => { calls.push(args); return Promise.resolve({}); } } }),
  });
  await api.agent.sendPrompt("project", "讨论");
  await api.agent.sendPrompt("project", "修复", source, null, "selected-conversation");
  assert.deepEqual(plain(calls), [["agent:send-prompt", "project", "讨论", null, null, null], ["agent:send-prompt", "project", "修复", source, null, "selected-conversation"]]);
});

test("production preflight rejects invalid/latest/foreign/stale sources before any model or task", async () => {
  const runtime = runtimeFixture();
  const calls = [];
  const state = { confirmations: [] };
  let report = outcome();
  const { handlers, adapter } = await mainHandlers(async (path, body) => {
    calls.push({ path, body });
    if (path.includes("/revision-outcomes/")) {
      if (!path.startsWith("/api/projects/project/")) throw new Error("Writer belongs to another project");
      return report;
    }
    return runtime.request(path, body);
  }, command, state);
  let turns = 0;
  adapter.sendTurn = async () => { turns++; assert.fail("invalid repair must not send a turn"); };
  const event = rendererEvent();
  try {
    await assert.rejects(handlers.get("agent:send-prompt")(event, "project", "修复", { ...source, choice: "repairDraft" }), /三字段来源/);
    assert.equal(calls.length, 0);
    for (const projectId of ["..", "bad\nproject", "字".repeat(54)]) {
      await assert.rejects(handlers.get("agent:send-prompt")(event, projectId, "修复", source), /作品 ID/);
    }
    assert.equal(calls.length, 0);
    await assert.rejects(handlers.get("agent:send-prompt")(event, "other-project", "修复", source), /another project/);
    for (const mutate of [
      value => null,
      value => ({ ...value, state: "stale" }),
      value => ({ ...value, authority: "canon" }),
      value => ({ ...value, revisionId: "assessment-new" }),
      value => ({ ...value, decisions: [] }),
      value => ({ ...value, decisions: [{ ...value.decisions[0], id: "newer-decision" }, ...value.decisions] }),
      value => ({ ...value, assessment: { ...value.assessment, writerTaskId: `task-${"b".repeat(32)}` } }),
      value => ({ ...value, assessment: { ...value.assessment, draftRevisionId: null } }),
      ...["choice", "writerTaskId", "assessmentRevisionId", "draftRevisionId", "knowledgeBatchId"].map(key => value => ({
        ...value, decisions: [{ ...value.decisions[0], [key]: key === "choice" ? "acceptRevision" : "mismatched" }],
      })),
    ]) {
      report = mutate(outcome());
      await assert.rejects(handlers.get("agent:send-prompt")(event, "project", "修复", source), /最新 repairDraft/);
    }
    assert.equal(turns, 0);
    assert.equal(adapter.threadId, null);
    assert.equal(runtime.tasks.size, 0);
    assert.equal(state.confirmations.length, 0);
    assert.ok(calls.every(call => call.body === null && call.path.includes("/revision-outcomes/")));
  } finally { adapter.close(); }
});

test("production ordinary and repair submissions both grant independent funded roots", async () => {
  const runtime = runtimeFixture();
  runtime.workTasks.set("old-writer-work", { id: "old-writer-work", projectId: "project", status: "completed", steps: [],
    initialBudget: { authorizationId: "old-authorization", maxRequests: 1, maxTokens: 2, deadlineAt: 3 } });
  const state = { confirmations: [], budgets: [] };
  const calls = [];
  const turns = [];
  const created = [];
  const { handlers, adapter } = await mainHandlers(async (path, body) => {
    calls.push({ path, body });
    if (path.endsWith(`/revision-outcomes/${source.writerTaskId}`)) return outcome();
    const result = await runtime.request(path, body);
    if (path.endsWith("/tasks") && body?.target?.revisionRepairSource) {
      result.inputManifest = { revisionRepairInput: frozenInput() };
    }
    if (path.endsWith("/tasks") && body) created.push(result);
    return result;
  }, command, state);
  const sendTurn = adapter.sendTurn;
  adapter.sendTurn = async function (input, options) {
    turns.push(typeof input === "string" ? input : input.map(item => item.text ?? "").join(""));
    return sendTurn.call(this, input, options);
  };
  try {
    const event = rendererEvent();
    const ordinary = await handlers.get("agent:send-prompt")(event, "project", "普通讨论", null);
    assert.deepEqual(plain(ordinary.proposedActions), []);
    assert.equal(calls.some(call => call.path.includes("/revision-outcomes/")), false);
    const repair = await handlers.get("agent:send-prompt")(event, "project", "修复动机", source);
    assert.deepEqual(plain(repair.proposedActions), []);
    assert.deepEqual(created.map(task => plain(task.target)), [{ type: "project" }, { type: "project", revisionRepairSource: source }]);
    assert.deepEqual(created.map(task => task.operation), ["advisory", "advisory"]);
    assert.deepEqual(state.confirmations, []);
    assert.equal(runtime.workTasks.size, 3);
    assert.notEqual(created[0].initialBudget.authorizationId, created[1].initialBudget.authorizationId);
    assert.deepEqual(state.budgets.map(budget => ({ id: budget.id, maxRequests: budget.maxRequests, maxTokens: budget.maxTokens })),
      created.map(task => ({ id: `budget-${task.id}`, maxRequests: null, maxTokens: null })));
    assert.ok(created.every(task => task.initialBudget.expectedBaseSnapshotId === "snapshot-fixture"
      && task.initialBudget.expectedSourceDigest === "source-fixture"));
    assert.equal(turns.length, 2);
    assert.deepEqual(turns.map(text => text.split("作者可见表达：").length - 1), [1, 1]);
    assert.doesNotMatch(turns[0], /本次是修改结果的修复讨论/);
    assert.match(turns[1], /repairInput 仅是候选分析来源，不是已确认事实或写权限/);
    assert.match(turns[1], /read_task_source 连续分页读取/);
    assert.match(turns[1], /拼接 payloadPart 并解析 JSON 得到 repairInput/);
    assert.match(turns[1], /request_candidate_task\(operation=generateChapter\)/);
    assert.match(turns[1], /sourceRevisionId:repairInput.authorDecision.draftRevisionId/);
    assert.match(turns[1], /selectedGroupIds 的非空子集/);
    assert.match(turns[1], /非空的新 desiredOutcomes/);
    assert.match(turns[1], /独立人工审批/);
    assert.match(turns[0], /无需重复询问启动/);
    assert.doesNotMatch(turns[1], /无需重复询问启动/);
    assert.match(turns[1], /此入口不支持其他章节或 planning task/);
    assert.deepEqual(plain(repair.workflow.task.inputManifest.revisionRepairInput), frozenInput());
    assert.doesNotMatch(turns[1], /request_candidate_task 指定规划或已有章节/);
    assert.doesNotMatch(turns[1], /operation=extractCommitments/);
    assert.equal(calls.some(call => call.path.endsWith("/approve")), false);
    assert.ok(calls.findIndex(call => call.path.includes("/revision-outcomes/"))
      < calls.findIndex(call => call.body?.target?.revisionRepairSource));
  } finally { adapter.close(); }
});

for (const failure of ["core-stale", "core-size", "lost-source", "changed-source", "missing-budget"]) {
  test(`production ${failure} cannot send a repair model turn`, async () => {
    const runtime = runtimeFixture();
    const state = { confirmations: [] };
    const { handlers, adapter } = await mainHandlers(async (path, body) => {
      if (path.includes("/revision-outcomes/")) return outcome();
      if (path.endsWith("/tasks") && body) {
        if (failure === "core-stale") throw new Error("Core transaction: stale repair source");
        if (failure === "core-size") throw new Error("Core transaction: repairInput exceeds 8000 bytes");
      }
      if (failure === "missing-budget" && path.endsWith("/budget-authorization")) return { authorization: null };
      const result = await runtime.request(path, body);
      if (path.endsWith("/tasks") && body) {
        if (failure === "lost-source") result.target = { type: "project" };
        if (failure === "changed-source") result.target = { type: "project", revisionRepairSource: { ...source, decisionId: "different" } };
      }
      return result;
    }, command, state);
    let turns = 0;
    adapter.sendTurn = async () => { turns++; assert.fail("failed repair must not send"); };
    try {
      await assert.rejects(handlers.get("agent:send-prompt")(rendererEvent(), "project", "修复", source),
        failure === "core-stale" ? /stale repair source/
          : failure === "core-size" ? /8000 bytes/ : failure === "missing-budget" ? /尚未获执行额度/ : /精确的修复讨论 target/);
      assert.equal(turns, 0);
      assert.deepEqual(state.confirmations, []);
      if (["core-stale", "core-size"].includes(failure)) assert.equal(runtime.tasks.size, 0);
    } finally { adapter.close(); }
  });
}

test("production running-task guards and steer reject repair source without losing it", async () => {
  for (const field of ["activeAgentExecution", "activeGenerationRunId"]) {
    const runtime = runtimeFixture();
    const { handlers, adapter } = await mainHandlers(runtime.request, command, { [field]: field === "activeAgentExecution" ? {} : "run-one" });
    try {
      for (const revisionRepairSource of [null, source]) {
        await assert.rejects(handlers.get("agent:send-prompt")(rendererEvent(), "project", "讨论", revisionRepairSource), /已有创作任务/);
      }
      await assert.rejects(handlers.get("agent:steer")(rendererEvent(), "project", "追加", source), /对话 ID 无效/);
      assert.equal(runtime.tasks.size, 0);
    } finally { adapter.close(); }
  }
});

test("production reserves the flow during preflight and snapshots incoming source", async () => {
  const runtime = runtimeFixture();
  let entered;
  let release;
  const ready = new Promise(resolve => { entered = resolve; });
  const waiting = new Promise(resolve => { release = resolve; });
  const { handlers, adapter } = await mainHandlers(async (path, body) => {
    if (path.includes("/revision-outcomes/")) { entered(); await waiting; return outcome(); }
    return runtime.request(path, body);
  }, command);
  const supplied = { ...source };
  const pending = handlers.get("agent:send-prompt")(rendererEvent(), "project", "修复", supplied);
  try {
    await ready;
    supplied.decisionId = "mutated";
    for (const revisionRepairSource of [null, source]) {
      await assert.rejects(handlers.get("agent:send-prompt")(rendererEvent(), "project", "第二轮", revisionRepairSource), /已有创作任务/);
    }
    release();
    const result = await pending;
    assert.deepEqual(plain(result.workflow.task.target), { type: "project", revisionRepairSource: source });
    assert.equal(runtime.tasks.size, 1);
  } finally { release(); await pending.catch(() => {}); adapter.close(); }
});

test("production still requires current provider evidence before repair preflight", async () => {
  const runtime = runtimeFixture();
  const state = { confirmations: [] };
  let requests = 0;
  const { handlers, adapter } = await mainHandlers(async (path, body) => {
    requests++;
    return runtime.request(path, body);
  }, command, state);
  state.fixtureEvidence = [];
  let turns = 0;
  adapter.sendTurn = async () => { turns++; assert.fail("unqualified provider must not send"); };
  try {
    await assert.rejects(handlers.get("agent:send-prompt")(rendererEvent(), "project", "修复", source), /能力|认证|证据|不可用|无法|evidence|qualification|capabilit/i);
    assert.equal(requests, 0);
    assert.equal(turns, 0);
    assert.equal(state.confirmations.length, 0);
    assert.equal(runtime.tasks.size, 0);
  } finally { adapter.close(); }
});

for (const stale of [false, true]) {
  test(`production persisted paused repair ${stale ? "rejects stale source" : "resumes its step and original budget"}`, async () => {
    const runtime = runtimeFixture();
    const deadlineAt = Math.floor(Date.now() / 1000) + 900;
    const originalBudget = { authorizationId: "original-repair-authorization", maxRequests: 3, maxTokens: 700, deadlineAt };
    const target = { type: "project", revisionRepairSource: { ...source } };
    const work = { id: "work-paused-repair", projectId: "project", kind: "conversation", status: "paused", version: 1,
      goal: "过期工作目标", initialBudget: originalBudget,
      steps: [{ id: "step-persisted-repair", operation: "advisory", status: "queued", target,
        currentAttemptId: "original-repair-attempt", dependsOnStepId: null }] };
    runtime.workTasks.set(work.id, work);
    const calls = [];
    const state = { confirmations: [], budgets: [] };
    const turns = [];
    const { handlers, adapter } = await mainHandlers(async (path, body) => {
      calls.push({ path, body });
      if (path.includes("/revision-outcomes/")) return { ...outcome(), state: stale ? "stale" : "current" };
      const result = await runtime.request(path, body);
      if (path.endsWith("/tasks") && body) {
        assert.deepEqual(body.target, { ...target, resumeStepId: work.steps[0].id });
        assert.equal(Object.hasOwn(body, "initialBudget"), false);
        result.inputManifest = { authorIntent: "冻结的修复要求：只细化动机", revisionRepairInput: frozenInput() };
      }
      if (path.endsWith("/complete")) { work.steps[0].status = "completed"; work.status = "completed"; }
      return result;
    }, command, state);
    const sendTurn = adapter.sendTurn;
    adapter.sendTurn = async function (input, options) {
      turns.push(typeof input === "string" ? input : input.map(item => item.text ?? "").join(""));
      return sendTurn.call(this, input, options);
    };
    try {
      const resumed = handlers.get("agent:resume-work-task")(rendererEvent(), "project", work.id);
      if (stale) {
        await assert.rejects(resumed, /最新 repairDraft/);
        assert.equal(turns.length, 0);
        assert.equal(runtime.tasks.size, 0);
        assert.equal(state.budgets.length, 0);
      } else {
        assert.equal((await resumed).status, "completed");
        assert.equal(turns.length, 1);
        assert.match(turns[0], /冻结的修复要求：只细化动机/);
        assert.doesNotMatch(turns[0], /过期工作目标/);
        assert.match(turns[0], /operationContext\.revisionRepairInput/);
        assert.equal(runtime.workTasks.size, 1);
        assert.deepEqual(state.budgets.map(budget => ({ maxRequests: budget.maxRequests, maxTokens: budget.maxTokens })),
          [{ maxRequests: originalBudget.maxRequests, maxTokens: originalBudget.maxTokens }]);
        const created = [...runtime.tasks.values()][0];
        assert.deepEqual(plain(created.target), { ...target, resumeStepId: work.steps[0].id });
        assert.deepEqual(plain(created.inputManifest.revisionRepairInput), frozenInput());
      }
      assert.equal(state.confirmations.length, 0);
      assert.equal(work.initialBudget, originalBudget);
      assert.ok(calls.some(call => call.path === `/api/projects/project/revision-outcomes/${source.writerTaskId}` && call.body === null));
      assert.equal(calls.some(call => call.path.endsWith("/budget-authorization") && call.body), false);
    } finally { adapter.close(); }
  });
}


test("direct-text revision assessment remains repairable without a knowledge batch", async () => {
  const receipt = outcome();
  receipt.assessment.knowledgeBatchId = null;
  receipt.decisions[0].knowledgeBatchId = null;
  await preflightRevisionRepair({ projectId: "project", source,
    runtimeRequest: async () => receipt });
  receipt.decisions[0].knowledgeBatchId = "unrelated-batch";
  await assert.rejects(preflightRevisionRepair({ projectId: "project", source,
    runtimeRequest: async () => receipt }), /最新 repairDraft/);
});
