import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { validateConversationFocus } from "../conversation-analysis.mjs";
import { mainHandlers, rendererEvent, runtimeFixture } from "./main-handler-harness.mjs";

// Production handlers and Adapter speak real JSON-RPC to the local fixture server;
// Runtime transport is controlled. This test never invokes a paid provider.
const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;
const reportId = `task-${"a".repeat(32)}`;
const focus = { view: "chapters", chapterId: "chapter-two", chapterNumber: 2, chapterStructureTaskId: reportId };
const plain = value => JSON.parse(JSON.stringify(value));
const workspace = () => ({ revision: 12, headSnapshotId: "current-snapshot", sourceDigest: "current-source",
  project: { id: "project" }, chapters: [{ id: "chapter-two", number: 2 }], tasks: [] });
const report = () => ({ id: reportId, projectId: "project", operation: "assessChapterStructure", status: "completed",
  threadId: "thread-structure-report", turnId: "turn-structure-report", inputRevision: 3,
  capabilityHash: "old-read-only-capability", target: { type: "project", structureChangeId: "change-one" } });
const receipt = () => ({ taskId: reportId, projectId: "project", state: "recorded", binding: {
  threadId: "thread-structure-report", turnId: "turn-structure-report", itemId: "report-final", intentRevision: 0,
  text: "独立历史报告原文，仅作建议，不复制进新 prompt。", truncated: false } });
async function harness({ task = report(), reply = receipt(), transformTask, readError } = {}) {
  const runtime = runtimeFixture(); const calls = [], rpc = [];
  const created = await mainHandlers(async (path, body, options) => {
    calls.push({ path, body: body ? plain(body) : body });
    if (path === `/api/projects/project/tasks/${reportId}`) {
      if (readError) throw readError;
      return task;
    }
    if (path === `/api/projects/project/tasks/${reportId}/final-reply`) return reply;
    if (path.endsWith("creative-workspace")) return workspace();
    const result = await runtime.request(path, body, options);
    return path.endsWith("/tasks") && body && transformTask ? transformTask(result) : result;
  }, command);
  const { adapter } = created; const request = adapter.request;
  adapter.request = function(method, params) {
    rpc.push({ method, params: plain(params), operation: this.activeOperation,
      capabilityHash: createHash("sha256").update(this.env.NOVEL_CODEX_TASK_CAPABILITY ?? "").digest("hex") });
    return request.call(this, method, params);
  };
  return { ...created, calls, rpc, runtime,
    send: (prompt, requested = focus, repair = null) => created.handlers.get("agent:send-prompt")(
      rendererEvent(), "project", prompt, repair, requested) };
}
const newTasks = h => h.calls.filter(call => call.path.endsWith("/tasks") && call.body);
const turns = h => h.rpc.filter(call => call.method === "turn/start");
const mutations = h => h.calls.filter(call => call.body && /actions\/[^/]+\/approve|save-candidate|\/accept|\/establish|\/generation-runs|\/modeling-batches/.test(call.path));

test("production moves old advisory to exact report history with fresh advisory authority, then continues normally", async () => {
  const h = await harness();
  try {
    await h.send("此前的一般讨论", { view: "world" });
    await h.send("先不改，只讨论方案");
    await h.send("继续讨论这个方案", { view: "chapters", chapterId: "chapter-two", chapterNumber: 2 });
    const tasks = newTasks(h).map(call => call.body);
    assert.deepEqual(tasks.map(task => ({ operation: task.operation, threadId: task.threadId, inputRevision: task.inputRevision, target: task.target })), [
      { operation: "advisory", threadId: "thr-test", inputRevision: 12, target: { type: "project", conversationFocus: { view: "world" } } },
      { operation: "advisory", threadId: "thread-structure-report", inputRevision: 12, target: { type: "project", conversationFocus: focus } },
      { operation: "advisory", threadId: "thread-structure-report", inputRevision: 12,
        target: { type: "project", conversationFocus: { view: "chapters", chapterId: "chapter-two", chapterNumber: 2 } } },
    ]);
    assert.deepEqual(h.rpc.filter(call => call.method === "thread/resume").map(call => call.params), [
      { threadId: "thread-structure-report" }, { threadId: "thread-structure-report" },
    ]);
    const starts = turns(h);
    assert.equal(starts.length, 3);
    assert.deepEqual(starts.map(call => call.operation), ["advisory", "advisory", "advisory"]);
    assert.deepEqual(starts.map(call => call.capabilityHash), tasks.map(task => task.capabilityHash));
    assert.equal(new Set(tasks.map(task => task.capabilityHash)).size, 3);
    assert.ok(tasks.every(task => task.capabilityHash !== report().capabilityHash && task.initialBudget));
    const input = starts[1].params.input.map(item => item.text ?? "").join("");
    assert.match(input, /旧检查仅是历史建议/);
    assert.match(input, /不沿用旧任务的工具绑定或权限/);
    assert.ok(!input.includes(receipt().binding.text));
    assert.deepEqual(mutations(h), []);
    assert.equal(h.runtime.completions.length, 3);
  } finally { h.adapter.close(); }
});

test("invalid task identity, operation, status or thread/turn never creates a task or resumes fallback history", async () => {
  const invalid = [null, {}, { ...report(), id: "foreign" }, { ...report(), projectId: "foreign" },
    { ...report(), operation: "advisory" }, ...["queued", "running", "failed", "cancelled"].map(status => ({ ...report(), status })),
    ...["threadId", "turnId"].flatMap(key => [null, "", " bad", "bad\n", "字".repeat(54)].map(value => ({ ...report(), [key]: value })))];
  for (const task of invalid) {
    const h = await harness({ task });
    try {
      await assert.rejects(h.send("讨论检查结果"), /没有可继续的完整结果/);
      assert.deepEqual(newTasks(h), []); assert.deepEqual(h.rpc, []);
    } finally { h.adapter.close(); }
  }
  const h = await harness({ readError: new Error("report read unavailable") });
  try {
    await assert.rejects(h.send("讨论检查结果"), /report read unavailable/);
    assert.deepEqual(newTasks(h), []); assert.deepEqual(h.rpc, []);
  } finally { h.adapter.close(); }
});

test("foreign, absent, truncated or mismatched public reply refuses continuation before any new task/model", async () => {
  const invalid = [null, {}, { ...receipt(), taskId: "foreign" }, { ...receipt(), projectId: "foreign" },
    ...["absent", "truncated"].map(state => ({ ...receipt(), state })), { ...receipt(), binding: null },
    ...[{ threadId: "foreign" }, { turnId: "foreign" }, { itemId: "" }, { itemId: "字".repeat(54) },
      { truncated: true }, { truncated: undefined }, { text: "  " }, { text: null }]
      .map(binding => ({ ...receipt(), binding: { ...receipt().binding, ...binding } }))];
  for (const reply of invalid) {
    const h = await harness({ reply });
    try {
      await assert.rejects(h.send("讨论检查结果"), /没有可继续的完整结果/);
      assert.deepEqual(newTasks(h), []); assert.deepEqual(h.rpc, []);
    } finally { h.adapter.close(); }
  }
});

test("Runtime dropping or changing the frozen source focus stops before the model turn", async () => {
  for (const replacement of [null, { view: "chapters", chapterId: "chapter-two", chapterNumber: 2 },
    { ...focus, chapterStructureTaskId: `task-${"b".repeat(32)}` }, { ...focus, view: "world", chapterId: undefined, chapterNumber: undefined }]) {
    const h = await harness({ transformTask: task => ({ ...task, target: { ...task.target, conversationFocus: replacement } }) });
    try {
      await assert.rejects(h.send("先不改"), /精确的讨论来源引用|章节导航上下文/);
      assert.equal(newTasks(h).length, 1); assert.deepEqual(turns(h), []); assert.deepEqual(mutations(h), []);
    } finally { h.adapter.close(); }
  }
});

test("source keys are bounded and explicit reference/repair cannot be mixed with the report source", async () => {
  for (const id of [null, "", " bad", "bad\n", "字".repeat(54)]) assert.throws(() => validateConversationFocus({ ...focus, chapterStructureTaskId: id }), /删章检查讨论来源/);
  const h = await harness();
  try {
    await assert.rejects(h.send("讨论", { ...focus, reference: { kind: "commitment", projectId: "project", snapshotId: "snapshot", commitmentId: "commitment", revisionId: "revision" } }), /来源冲突/);
    assert.deepEqual(newTasks(h), []); assert.deepEqual(h.rpc, []);
  } finally { h.adapter.close(); }
  const source = { writerTaskId: `task-${"c".repeat(32)}`, assessmentRevisionId: "assessment-one", decisionId: "decision-one" };
  const repaired = await harness();
  try {
    await assert.rejects(repaired.send("讨论修复", focus, source), /不能与修复讨论来源混用/);
    assert.deepEqual(newTasks(repaired), []); assert.deepEqual(repaired.rpc, []);
    assert.deepEqual(repaired.calls, []);
  } finally { repaired.adapter.close(); }
});

test("report thread resume failure never falls back to an old advisory or starts another thread", async () => {
  const h = await harness();
  try {
    await h.send("此前讨论", { view: "world" });
    h.calls.length = 0; h.rpc.length = 0;
    const request = h.adapter.request;
    h.adapter.request = function(method, params) {
      if (method === "thread/resume") {
        assert.equal(params.threadId, "thread-structure-report");
        return Promise.reject(new Error("exact report resume unavailable"));
      }
      return request.call(this, method, params);
    };
    await assert.rejects(h.send("先不改"), /exact report resume unavailable/);
    assert.deepEqual(newTasks(h), []); assert.deepEqual(turns(h), []);
    assert.ok(!h.rpc.some(call => call.method === "thread/start"));
  } finally { h.adapter.close(); }
});
