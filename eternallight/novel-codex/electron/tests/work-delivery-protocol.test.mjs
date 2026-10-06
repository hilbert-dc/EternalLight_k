import assert from "node:assert/strict";
import test from "node:test";
import { enforceWorkDeliveryProtocol } from "../work-delivery-protocol.mjs";

test("missing delivery review stops after two corrections without host writes or task recreation", async () => {
  const sends = [], reads = [];
  const task = { id: "decision", projectId: "project", threadId: "thread", inputRevision: 7, inputManifest: { workDecision: {} } };
  const adapter = { sendTurn: async (input, options) => { sends.push({ input, options }); return { message: "文字回复" }; } };
  const request = async (path, options) => { assert.equal(options, undefined); reads.push(path);
    return path.includes("work-decision?") ? { deliveryReviewRequired: true } : { status: "running" }; };
  await assert.rejects(enforceWorkDeliveryProtocol(adapter, [{ type: "text", text: "原要求" }], {}, task, request, () => {}),
    error => error.code === "WORK_DELIVERY_PROTOCOL_INCOMPLETE");
  assert.equal(sends.length, 3);
  assert.equal(reads.length, 6);
  assert.ok(reads.every(path => path.includes("/tasks/decision")));
});

test("author cancellation prevents protocol correction", async () => {
  let sends = 0;
  await assert.rejects(enforceWorkDeliveryProtocol({ sendTurn: async () => { sends++; return {}; } }, [], {},
    { id: "decision", projectId: "project", threadId: "thread", inputRevision: 0, inputManifest: { workDecision: {} } },
    async () => ({ status: "cancelled" }), () => {}), /已停止/);
  assert.equal(sends, 1);
});

test("Native queued attempt remains active while its Engine lease executes", async () => {
  let sends = 0;
  const task = { id: "decision", projectId: "project", threadId: "thread", inputRevision: 0, inputManifest: { workDecision: {} } };
  const result = await enforceWorkDeliveryProtocol({ sendTurn: async () => ({ message: `turn ${++sends}` }) }, [], {}, task,
    async path => path.includes("work-decision?") ? { deliveryReviewRequired: true } : { status: sends === 1 ? "queued" : "completed" }, () => {});
  assert.equal(result.message, "turn 2");
  assert.equal(sends, 2);
});
