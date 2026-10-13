import assert from "node:assert/strict";
import test from "node:test";
import { admitBoundedOutputFailure, createBoundedOutputSender, isBoundedOutputFailure } from "../bounded-output-correction.mjs";

const task = { id: "writer", projectId: "project", threadId: "thread" };
const diagnostic = { type: "function_call", utf8Bytes: 31700, conservativeTokens: 14000, limitKind: "conservativeTokens", limit: 10000 };
const failure = (data = diagnostic, code = "other") => {
  const error = new Error(`provider_output_item_limit; outputItem=${JSON.stringify(data)}`);
  admitBoundedOutputFailure(error, { status: "failed", error: { codexErrorInfo: code } });
  return error;
};

test("only bounded trusted terminal diagnostics admit a correction", () => {
  assert.ok(isBoundedOutputFailure(failure()));
  assert.ok(isBoundedOutputFailure(failure({ ...diagnostic, utf8Bytes: 300000, conservativeTokens: null, limitKind: "utf8Bytes", limit: 262144 })));
  for (const error of [new Error(failure().message), failure(diagnostic, "contextWindowExceeded"),
    failure({ ...diagnostic, type: "unknown" }), failure({ ...diagnostic, limit: 20000 }),
    failure({ ...diagnostic, conservativeTokens: 9999 }), failure({ ...diagnostic, utf8Bytes: -1 })]) {
    assert.equal(isBoundedOutputFailure(error), false);
  }
});

test("two corrections are cumulative across protocol turns in the same task", async () => {
  const sends = []; let failures = 2;
  const adapter = { threadId: task.threadId, sendTurn: async (input, options) => {
    sends.push({ input, options }); if (failures-- > 0) throw failure(); return { message: "stored" };
  } };
  const read = async path => { assert.equal(path, "/api/projects/project/tasks/writer"); return { ...task, status: "queued" }; };
  const sender = createBoundedOutputSender(adapter, task, read, () => {});
  assert.equal((await sender([{ type: "text", text: "original" }], { threadName: "same task" })).message, "stored");
  assert.equal(sends.length, 3);
  assert.equal(sends[1].options.disableEnvironment, true);
  assert.equal(sends[1].options.additionalContext.novelOutputCorrection.kind, "application");
  assert.equal(sends[1].input[0].text, "应用内部输出纠正触发；保持原作者要求。");
  failures = 1;
  await assert.rejects(sender([], {}), isBoundedOutputFailure);
  assert.equal(sends.length, 4);
});

test("cancellation, stopped tasks and changed thread identity never resample", async () => {
  for (const change of ["cancelled", "completed", "thread", "cancel-during-read"]) {
    let sends = 0, cancelled = false;
    const adapter = { threadId: task.threadId, sendTurn: async () => { sends++; throw failure(); } };
    const sender = createBoundedOutputSender(adapter, task, async () => {
      if (change === "thread") adapter.threadId = "replacement";
      if (change === "cancel-during-read") cancelled = true;
      return { ...task, status: ["cancelled", "completed"].includes(change) ? change : "running" };
    }, () => { if (cancelled) throw new Error("author cancelled"); });
    await assert.rejects(sender([], {})); assert.equal(sends, 1);
  }
});

test("unrelated errors never enter correction or read task state", async () => {
  const sender = createBoundedOutputSender({ sendTurn: async () => { throw new Error("provider quota exhausted"); } }, task,
    async () => assert.fail("unexpected recovery read"), () => {});
  await assert.rejects(sender([], {}), /quota exhausted/);
});
