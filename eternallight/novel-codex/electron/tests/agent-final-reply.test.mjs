import test from "node:test";
import assert from "node:assert/strict";
import { persistTaskFinalReply, selectFinalReply } from "../agent-final-reply.mjs";
import { mainHandlers, rendererEvent, runtimeFixture } from "./main-handler-harness.mjs";
import { fileURLToPath } from "node:url";

test("final messages exclude commentary and reasoning; UTF-8 bounds never split a character", () => {
  const final = selectFinalReply(null, { id: "final", type: "agentMessage", phase: "final_answer", text: "结论" });
  for (const item of [{ id: "later", type: "agentMessage", phase: "commentary", text: "执行记录" },
    { id: "thought", type: "reasoning", text: "private" }, { id: "unphased", type: "agentMessage", text: "晚到内容" }]) {
    assert.deepEqual(selectFinalReply(final, item), final);
  }
  assert.equal(selectFinalReply(null, { id: "unfinished", type: "agentMessage", phase: "final_answer", status: "inProgress", text: "尚未完成" }), null);
  const long = selectFinalReply(null, { id: "long", type: "agentMessage", text: "🌙".repeat(17000) });
  assert.deepEqual(long, { itemId: "long", text: "🌙".repeat(16384), truncated: true, phase: null });
});

test("unknown save outcome reads the exact receipt, without replaying any model execution", async () => {
  const task = { id: "task", threadId: "thread", operation: "advisory" };
  const result = { turn: { id: "turn", status: "completed" }, finalReply: { itemId: "item", text: "回复", truncated: false } };
  const calls = [];
  let stored;
  const request = async (path, options) => {
    calls.push([path, options?.method ?? "GET"]);
    if (path.endsWith("/intents")) return { intents: [{ intentRevision: 1, status: "delivered" }] };
    if (options) { stored = { taskId: task.id, projectId: "project", binding: JSON.parse(options.body) }; throw new Error("lost response"); }
    return stored;
  };
  assert.deepEqual(await persistTaskFinalReply(request, "project", task, result), stored);
  assert.equal(stored.binding.intentRevision, 1);
  assert.deepEqual(calls.map((call) => call[1]), ["GET", "POST", "GET"]);
  for (const status of ["failed", "interrupted", "inProgress"]) {
    await assert.rejects(persistTaskFinalReply(request, "project", task, { ...result, turn: { id: "turn", status } }), /没有成功结束/);
  }
  await assert.rejects(persistTaskFinalReply(async () => ({ intents: [{ status: "pending" }] }), "project", task, result), /尚未送达/);
  await assert.rejects(persistTaskFinalReply(async () => { throw new Error("not saved"); }, "project", { ...task, operation: "generateBrief" }, result), /not saved/);
});

test("production advisory saves final reply before domain completion and propagates storage failure", async () => {
  for (const fail of [false, true]) {
    const runtime = runtimeFixture();
    const calls = [];
    const { handlers, adapter } = await mainHandlers(async (path, body) => {
      calls.push(path);
      if (fail && path.endsWith("/final-reply")) throw new Error("reply store unavailable");
      return runtime.request(path, body);
    }, `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`);
    try {
      const pending = handlers.get("agent:send-prompt")(rendererEvent(), "project", "分析故事");
      if (fail) { await assert.rejects(pending, /reply store unavailable/); assert.equal(runtime.completions.length, 0); }
      else {
        const result = await pending;
        assert.equal(result.finalReplyReceipt.binding.text, "hello");
        assert.ok(calls.findIndex((path) => path.endsWith("/final-reply")) < calls.findIndex((path) => path.endsWith("/complete")));
        assert.deepEqual(await handlers.get("runtime:get-task-final-reply")({}, "project", result.workflow.task.id), result.finalReplyReceipt);
      }
    } finally { adapter.close(); }
  }
});
