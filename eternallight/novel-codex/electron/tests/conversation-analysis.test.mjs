import assert from "node:assert/strict";
import test from "node:test";
import { bindConversationFocus, conversationFocusInstructions, executeCandidateAction, executeConversationCandidates, validateConversationFocus } from "../conversation-analysis.mjs";
import { registerAdvisoryPromptHandler } from "../revision-repair-handlers.mjs";
import { advanceWorkSteps } from "../work-step-continuation.mjs";
import { fileURLToPath } from "node:url";
import { mainHandlers, rendererEvent, runtimeFixture } from "./main-handler-harness.mjs";

const sourceId = `task-${"a".repeat(32)}`;
const actionId = `action-${"b".repeat(32)}`;
const discussion = { message: "已读取本章，接下来整理事实。", workflow: { task: { id: sourceId, projectId: "project", threadId: "thread-original", operation: "advisory", inputRevision: 5 } } };
const target = { type: "chapter", chapterId: "chapter-two", chapterNumber: 2, sourceRevisionId: "draft-two" };
const proposal = operation => ({ id: actionId, sourceTaskId: sourceId, status: "proposed", inputRevision: 5, intentRevision: 2,
  operation, target: { ...target }, instruction: "整理本章可证实的年龄事实，不改写正文" });

function fixture(operation = "modelKnowledge") {
  const calls = [];
  const action = proposal(operation);
  const workspace = { revision: 5, modeling: [{ objectId: "draft-chapter-two", sourceRevisionId: "draft-two", status: "model_pending" }] };
  let state = "running";
  const dependencies = {
    continueCreativeWork: async (_event, _project, result) => result,
    runConversationReviews: async () => ({ planningReviews: [] }),
    notifyCandidateStart: params => calls.push({ kind: "candidateStart", params }),
    currentWorkFlowState: async () => state,
    assertAvailable: operation => calls.push({ kind: "available", operation }),
    runtimeRequest: async (path, options = {}) => {
      const body = options.body ? JSON.parse(options.body) : null;
      calls.push({ kind: "runtime", path, body });
      if (path.endsWith("/actions")) return { actions: [structuredClone(action)] };
      if (path.endsWith("/approve")) { action.status = "queued"; return structuredClone(action); }
      if (path.endsWith("/creative-workspace")) return structuredClone(workspace);
      if (path.endsWith("/modeling-batches")) return { id: "batch-two" };
      if (path.endsWith("/finish")) { action.status = body.error ? "failed" : "completed"; return {}; }
      assert.fail(path);
    },
    runCreativeAgentTask: async (...args) => {
      const model = args[3].operation === "modelKnowledge";
      calls.push({ kind: model ? "model" : "candidate", args });
      return { workflow: { task: { id: model ? "task-analysis" : "task-extraction" } } };
    },
  };
  return { dependencies, action, calls, workspace, setState: next => { state = next; } };
}

test("chapter focus is exact project-owned navigation context; malformed and foreign targets fail", () => {
  const focus = { view: "chapters", chapterId: "chapter-two", chapterNumber: 2 };
  assert.deepEqual(bindConversationFocus({ chapters: [{ id: "chapter-two", number: 2 }] }, focus), focus);
  assert.equal(bindConversationFocus({}, null), null);
  for (const invalid of [[], {}, { ...focus, chapterNumber: 0 }, { ...focus, chapterNumber: 2.5 },
    { ...focus, chapterId: "bad\nchapter" }, { ...focus, chapterId: "字".repeat(54) }, { ...focus, sourceRevisionId: "draft" }]) {
    assert.throws(() => validateConversationFocus(invalid), /导航上下文/);
  }
  assert.throws(() => bindConversationFocus({ chapters: [{ id: "chapter-two", number: 1 }] }, focus), /不属于/);
  assert.throws(() => bindConversationFocus({ chapters: [{ id: "other", number: 2 }] }, focus), /不属于/);
});

test("page focus remains navigation only and never carries another page's chapter or write authority", () => {
  for (const view of ["brief", "outline", "plans", "characters", "plot", "timeline", "world", "diffs", "chapters"]) {
    const focus = bindConversationFocus({ chapters: [] }, { view });
    assert.deepEqual(focus, { view });
    assert.match(conversationFocusInstructions(focus), /不是写入授权/);
    assert.match(conversationFocusInstructions(focus), /先读取冻结上下文/);
  }
  for (const malformed of [{ view: "world", chapterId: "one", chapterNumber: 1 },
    { view: "world", operation: "accept" }, { view: "unknown" }, { view: "chapters", chapterId: "one" }]) {
    assert.throws(() => bindConversationFocus({ chapters: [] }, malformed), /导航上下文/);
  }
});

test("current-send knowledge candidate automatically executes with original intent and inherited action authority", async () => {
  const f = fixture();
  const result = await executeConversationCandidates({}, "project", discussion, f.dependencies);
  assert.deepEqual(result.candidateResults.map(item => item.taskId), ["task-analysis"]);
  assert.deepEqual(result.proposedActions, []);
  assert.match(result.message, /故事设定分析已保存/);
  assert.deepEqual(f.calls.find(item => item.kind === "candidateStart").params,
    { projectId: "project", sourceTaskId: sourceId, actionId, operation: "modelKnowledge", target });
  assert.ok(f.calls.findIndex(item => item.kind === "candidateStart") < f.calls.findIndex(item => item.kind === "model"));
  const request = f.calls.find(item => item.kind === "model");
  assert.deepEqual(request.args.slice(1), ["project", proposal("modelKnowledge").instruction, { operation: "modelKnowledge" },
    { ...target, batchId: "batch-two", requestedByTaskId: sourceId, requestedActionId: actionId }]);
  const batch = f.calls.find(item => item.path?.endsWith("/modeling-batches"));
  assert.deepEqual(batch.body, { objectId: "draft-chapter-two", sourceRevisionId: "draft-two", expectedRevision: 5, extractorVersion: `novel-knowledge-v1:${actionId}` });
  assert.deepEqual(f.calls.find(item => item.path?.endsWith("/approve")).body, { expectedIntentRevision: 2 });
  assert.deepEqual(f.calls.find(item => item.path?.endsWith("/finish")).body, { resultTaskId: "task-analysis" });
  assert.ok(!f.calls.some(item => /accept|budget-authorization/.test(item.path ?? "")));
});

test("explicit creation requests execute within the same send and leave publication to the author", async () => {
  const f = fixture("extractCommitments");
  const result = await executeConversationCandidates({}, "project", discussion, f.dependencies);
  assert.match(result.message, /伏笔提案已保存/);
  assert.equal(f.calls.filter(item => item.kind === "candidate").length, 1);
  assert.equal(f.calls.filter(item => item.kind === "model").length, 0);
  for (const operation of ["generateChapter", "reviseCommitment", "generatePlan", "generateBrief", "generateOutline", "generatePropertyRules"]) {
    const writing = fixture(operation);
    const result = await executeConversationCandidates({}, "project", discussion, writing.dependencies);
    assert.deepEqual(result.proposedActions, []);
    assert.equal(result.candidateResults.length, 1);
    assert.deepEqual(writing.calls.find(item => item.kind === "candidate").args.slice(1),
      ["project", writing.action.instruction, { operation }, { ...target, requestedByTaskId: sourceId, requestedActionId: actionId }]);
    assert.match(result.message, /已保存/);
    assert.match(result.message, /已发布内容与正式设定未自动改变/);
    assert.ok(!writing.calls.some(item => /accept|establish|budget-authorization/.test(item.path ?? "")));
  }
});

test("discussion without an action and unsupported proposals never start a candidate", async () => {
  for (const operation of [null, "reviewPlanningSeat", "unknown"]) {
    const f = fixture(operation);
    if (!operation) f.dependencies.runtimeRequest = async () => ({ actions: [] });
    const result = await executeConversationCandidates({}, "project", discussion, f.dependencies);
    assert.deepEqual(result.candidateResults, []);
    assert.equal(result.message, undefined);
    assert.ok(!f.calls.some(item => item.path?.endsWith("/approve") || ["model", "candidate"].includes(item.kind)));
  }
});

test("foreign action/source and stale saved chapter never launch a model or rebase", async () => {
  for (const mutate of [f => { f.action.sourceTaskId = `task-${"c".repeat(32)}`; },
    f => { f.action.inputRevision = 4; }, f => { f.workspace.revision = 6; },
    f => { f.workspace.modeling[0].sourceRevisionId = "new-draft"; }]) {
    const f = fixture(); mutate(f);
    await assert.rejects(executeConversationCandidates({}, "project", discussion, f.dependencies), /不属于|来源已变化/);
    assert.ok(!f.calls.some(item => item.kind === "model"));
  }
  const f = fixture();
  await assert.rejects(executeConversationCandidates({}, "other-project", discussion, f.dependencies), /本次作品/);
  assert.deepEqual(f.calls, []);
});

test("pause/cancel and model failure stop the chain without retry or another budget grant", async () => {
  const paused = fixture(); paused.setState("paused");
  const result = await executeConversationCandidates({}, "project", discussion, paused.dependencies);
  assert.deepEqual(result.proposedActions, [proposal("modelKnowledge")]);
  assert.ok(!paused.calls.some(item => item.path?.endsWith("/approve")));
  const cancelled = fixture();
  cancelled.dependencies.currentWorkFlowState = async () => { throw Object.assign(new Error("已停止"), { code: "TASK_CANCELLED" }); };
  await assert.rejects(executeConversationCandidates({}, "project", discussion, cancelled.dependencies), { code: "TASK_CANCELLED" });
  const failure = fixture();
  failure.dependencies.runCreativeAgentTask = async () => { failure.calls.push({ kind: "model" }); throw new Error("provider_output_item_limit"); };
  await assert.rejects(executeConversationCandidates({}, "project", discussion, failure.dependencies), /provider_output_item_limit/);
  assert.equal(failure.calls.filter(item => item.kind === "model").length, 1);
  assert.deepEqual(failure.calls.find(item => item.path?.endsWith("/finish")).body, { error: "provider_output_item_limit" });
  const chinese = fixture();
  chinese.dependencies.runCreativeAgentTask = async () => { throw new Error("分析来源已变化".repeat(100)); };
  await assert.rejects(executeConversationCandidates({}, "project", discussion, chinese.dependencies), /来源已变化/);
  const message = chinese.calls.find(item => item.path?.endsWith("/finish")).body.error;
  assert.ok(Buffer.byteLength(message) <= 400 && !message.includes("�"));
});

test("completed execution with failed finish receipt is not reported as a failed model or retried", async () => {
  const f = fixture();
  const request = f.dependencies.runtimeRequest;
  f.dependencies.runtimeRequest = async (path, options) => {
    if (path.endsWith("/finish")) { f.calls.push({ kind: "receipt", body: JSON.parse(options.body) }); throw new Error("receipt unavailable"); }
    return request(path, options);
  };
  await assert.rejects(executeConversationCandidates({}, "project", discussion, f.dependencies), /receipt unavailable/);
  assert.deepEqual(f.calls.filter(item => item.kind === "receipt"), [{ kind: "receipt", body: { resultTaskId: "task-analysis" } }]);
  assert.equal(f.calls.filter(item => item.kind === "model").length, 1);
});

test("each author analysis has its own batch identity; recovery reuses that identity and ready receipts never rerun", async () => {
  const versions = [];
  for (const id of [actionId, `action-${"c".repeat(32)}`]) {
    const f = fixture(); f.action.id = id;
    await executeCandidateAction({}, "project", sourceId, id, f.dependencies);
    versions.push(f.calls.find(item => item.path?.endsWith("/modeling-batches")).body.extractorVersion);
  }
  assert.deepEqual(versions, [`novel-knowledge-v1:${actionId}`, `novel-knowledge-v1:action-${"c".repeat(32)}`]);
  const f = fixture(); f.action.status = "queued";
  const request = f.dependencies.runtimeRequest;
  f.dependencies.runtimeRequest = async (path, options) => path.endsWith("/modeling-batches")
    ? (f.calls.push({ path, body: JSON.parse(options.body) }), { id: "already-saved", status: "ready" })
    : request(path, options);
  await assert.rejects(executeCandidateAction({}, "project", sourceId, actionId, f.dependencies, { resume: true }), /已保存结果/);
  assert.equal(f.calls.find(item => item.path?.endsWith("/modeling-batches")).body.extractorVersion, versions[0]);
  assert.ok(!f.calls.some(item => item.kind === "model"));
});

test("an earlier saved analysis does not rebase a later action onto its changed project revision", async () => {
  const f = fixture();
  const second = { ...proposal("modelKnowledge"), id: `action-${"d".repeat(32)}` };
  const request = f.dependencies.runtimeRequest;
  f.dependencies.runtimeRequest = async (path, options) => {
    if (path.endsWith("/actions")) return { actions: [structuredClone(f.action), structuredClone(second)] };
    if (path.endsWith(`/${second.id}/approve`)) { second.status = "queued"; return { ...second }; }
    if (path.endsWith(`/${second.id}/finish`)) { second.status = "failed"; return {}; }
    return request(path, options);
  };
  const model = f.dependencies.runCreativeAgentTask;
  f.dependencies.runCreativeAgentTask = async (...args) => { const result = await model(...args); f.workspace.revision = 6; return result; };
  const result = await executeConversationCandidates({}, "project", discussion, f.dependencies);
  assert.match(result.message, /前一项结果已保存.*来源已变化/);
  assert.deepEqual(result.proposedActions, [second]);
  assert.equal(result.candidateResults.length, 1);
  assert.equal(f.action.status, "completed");
  assert.equal(second.inputRevision, 5);
  assert.equal(f.calls.filter(item => item.kind === "model").length, 1);
  assert.equal(f.calls.filter(item => item.path?.endsWith("/modeling-batches")).length, 1);
});

test("send handler forwards chapter focus and only analyzes actions produced by its own discussion", async () => {
  const f = fixture();
  const request = f.dependencies.runtimeRequest;
  f.dependencies.runtimeRequest = (path, options) => {
    if (path.endsWith("/work-task")) return { id: "work-original", projectId: "project", kind: "conversation", version: 1 };
    if (path.endsWith("/conversation-continuation")) return { workTaskId: "work-original", threadId: "thread-original", step: null, stopReason: "noNewResults", receipts: [] };
    return request(path, options);
  };
  let handler;
  const args = [];
  registerAdvisoryPromptHandler({ handle: (_name, value) => { handler = value; } }, {
    withWorkFlow: async (_project, work) => work(), routingState: () => ({}), analysisDependencies: f.dependencies,
    runAdvisoryAgentTask: async (...values) => { args.push(values); return discussion; },
  });
  const focus = { view: "chapters", chapterId: "chapter-two", chapterNumber: 2 };
  const result = await handler({}, "project", "整理本章事实", null, focus, "thread-original");
  assert.deepEqual(args[0].slice(1), ["project", "整理本章事实", null, null, focus, null, "thread-original"]);
  assert.match(result.message, /故事设定分析已保存/);
  await assert.rejects(handler({}, "project", "整理", null, { ...focus, authority: "write" }), /导航上下文/);
  assert.equal(args.length, 1);
});

test("repair discussion never automatically approves analysis even if a proposal is returned", async () => {
  const f = fixture();
  let handler;
  registerAdvisoryPromptHandler({ handle: (_name, value) => { handler = value; } }, {
    withWorkFlow: async (_project, work) => work(), routingState: () => ({}), analysisDependencies: f.dependencies,
    runAdvisoryAgentTask: async () => discussion,
  });
  const source = { writerTaskId: sourceId, assessmentRevisionId: "assessment-one", decisionId: "decision-one" };
  const result = await handler({}, "project", "讨论修复办法", source);
  assert.deepEqual(result.proposedActions, [proposal("modelKnowledge")]);
  assert.ok(!f.calls.some(item => item.path?.endsWith("/approve") || item.kind === "model"));
});

test("explicit recovery of queued knowledge action resolves a host batch in the same action authority", async () => {
  const f = fixture(); f.action.status = "queued";
  const result = await executeCandidateAction({}, "project", sourceId, actionId, f.dependencies, { resume: true });
  assert.equal(result.taskId, "task-analysis");
  assert.ok(!f.calls.some(item => item.path?.endsWith("/approve")));
  const step = { id: actionId, sourceTaskId: sourceId, operation: "modelKnowledge", status: "queued", instruction: "整理事实", target: { batchId: "batch-two" } };
  const work = { id: "work-one", projectId: "project", threadId: "thread-one", status: "running", steps: [step] };
  const calls = [];
  await advanceWorkSteps({}, "project", work.id, {
    currentWorkFlowState: async () => "running", runtimeRequest: async path => {
      if (path.endsWith("/decision")) return { workTaskId: work.id, threadId: work.threadId, step: null, stopReason: "completed" };
      return work;
    },
    runCreativeAgentTask: async (...values) => { calls.push(values); step.status = "completed"; return {}; },
  });
  assert.deepEqual(calls, [[{}, "project", step.instruction, { operation: "modelKnowledge" }, { batchId: "batch-two", resumeStepId: actionId }]]);
});

test("production send executes knowledge or chapter writing in the original Work without another approval", async () => {
  for (const operation of ["modelKnowledge", "generateChapter", "generatePlan"]) {
  const runtime = runtimeFixture();
  const calls = [];
  const state = { confirmations: [], budgets: [] };
  const action = { ...proposal(operation), sourceTaskId: `task-${"0".repeat(32)}` };
  const workspace = { revision: 5, headSnapshotId: "snapshot-five", sourceDigest: "source-five",
    chapters: [{ id: "chapter-two", number: 2 }], modeling: [{ objectId: "draft-chapter-two", sourceRevisionId: "draft-two" }] };
  const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;
  const { handlers, adapter } = await mainHandlers(async (path, body, options) => {
    calls.push({ path, body });
    if (path.endsWith("/creative-workspace")) return workspace;
    if (path.endsWith("/actions")) return { actions: [{ ...action }] };
    if (path.endsWith("/approve")) { action.status = "queued"; return { ...action }; }
    if (path.endsWith("/modeling-batches")) return { id: "batch-two" };
    if (path.endsWith("/finish")) { action.status = "completed"; return {}; }
    if (path.endsWith("/decision")) {
      const work = await runtime.request(path.replace(/\/decision$/, ""));
      work.status = "completed";
      return { workTaskId: work.id, threadId: work.threadId, step: null, stopReason: "completed" };
    }
    return runtime.request(path, body, options);
  }, command, state);
  const inputs = [];
  const sendTurn = adapter.sendTurn;
  adapter.sendTurn = async function (input, options) { inputs.push(input); return sendTurn.call(this, input, options); };
  try {
    const result = await handlers.get("agent:send-prompt")(rendererEvent(), "project", "整理本章年龄事实", null,
      { view: "chapters", chapterId: "chapter-two", chapterNumber: 2 });
    assert.deepEqual(result.proposedActions, []);
    assert.equal(result.candidateResults.length, 1);
    assert.equal(result.executionFailure, undefined);
    assert.match(result.message, /已保存/);
    const tasks = calls.filter(call => call.path.endsWith("/tasks") && call.body);
    assert.deepEqual(tasks.map(call => call.body.operation), ["advisory", operation]);
    assert.deepEqual(tasks[0].body.target, { type: "project", conversationFocus: { view: "chapters", chapterId: "chapter-two", chapterNumber: 2 } });
    assert.deepEqual(tasks[1].body.target, { ...target, ...(operation === "modelKnowledge" ? { batchId: "batch-two" } : {}), requestedByTaskId: action.sourceTaskId, requestedActionId: action.id });
    assert.ok(tasks[0].body.initialBudget.authorizationId);
    assert.equal(tasks[1].body.initialBudget, undefined);
    assert.equal(runtime.workTasks.size, 1);
    assert.equal(state.budgets.length, 2);
    assert.deepEqual(state.confirmations, []);
    assert.match(typeof inputs[0] === "string" ? inputs[0] : inputs[0].map(item => item.text ?? "").join(""), /当前查看第 2 章/);
    assert.equal(inputs.length, 2);
    if (operation === "modelKnowledge") assert.match(typeof inputs[1] === "string" ? inputs[1] : inputs[1].map(item => item.text ?? "").join(""), /只分析绑定的已保存正文/);
    assert.ok(!calls.some(call => /accept|establish/.test(call.path) || call.path.endsWith("/budget-authorization") && call.body));
  } finally { adapter.close(); }
  }
});

test("production foreign chapter focus fails before creating a task, budget or model turn", async () => {
  const runtime = runtimeFixture();
  const state = { budgets: [] };
  const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;
  const { handlers, adapter } = await mainHandlers(async (path, body) => {
    if (path.endsWith("/creative-workspace")) return { revision: 5, chapters: [{ id: "chapter-one", number: 1 }] };
    return runtime.request(path, body);
  }, command, state);
  adapter.sendTurn = async () => assert.fail("foreign navigation context must not call the model");
  try {
    await assert.rejects(handlers.get("agent:send-prompt")(rendererEvent(), "project", "整理本章事实", null,
      { view: "chapters", chapterId: "chapter-two", chapterNumber: 2 }), /不属于/);
    assert.equal(runtime.tasks.size, 0);
    assert.deepEqual(state.budgets, []);
  } finally { adapter.close(); }
});

test("production world-page discussion preserves its exact focus and executes no candidate without a proposal", async () => {
  const runtime = runtimeFixture();
  const calls = [], inputs = [], state = { budgets: [] };
  const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;
  const { handlers, adapter } = await mainHandlers(async (path, body, options) => {
    calls.push({ path, body });
    if (path.endsWith("/creative-workspace")) return { revision: 5, headSnapshotId: "snapshot-five", sourceDigest: "source-five", chapters: [] };
    if (path.endsWith("/actions")) return { actions: [] };
    return runtime.request(path, body, options);
  }, command, state);
  const send = adapter.sendTurn;
  adapter.sendTurn = async function(input, options) { inputs.push(input); return send.call(this, input, options); };
  try {
    await handlers.get("agent:send-prompt")(rendererEvent(), "project", "讨论这里的设定是否清楚，先不要修改", null, { view: "world" });
    assert.deepEqual(calls.filter(call => call.path.endsWith("/tasks") && call.body).map(call => ({ operation: call.body.operation, target: call.body.target })),
      [{ operation: "advisory", target: { type: "project", conversationFocus: { view: "world" } } }]);
    assert.equal(inputs.length, 1);
    assert.match(typeof inputs[0] === "string" ? inputs[0] : inputs[0].map(item => item.text ?? "").join(""), /世界与规则/);
    assert.ok(!calls.some(call => /accept|establish|save-candidate/.test(call.path)));
  } finally { adapter.close(); }
});
