import assert from "node:assert/strict";
import test from "node:test";
import { interruptCommittedGenerationPause } from "../committed-generation-control.mjs";

function fixture() {
  const execution = { task: { id: "decision", operation: "advisory", projectId: "project", threadId: "root", target: { runId: "generation-selected" } } };
  const flow = { id: "work-generation-selected", projectId: "project" };
  const receipt = { runId: "generation-selected", version: 6, status: "paused", sourceTaskId: "decision", authority: "committedGenerationControl" };
  const message = { method: "item/completed", params: { threadId: "root", item: { type: "mcpToolCall", server: "novel_workspace",
    tool: "pause_generation_run", status: "completed", result: { content: [{ type: "text", text: JSON.stringify({ receipt }) }] } } } };
  const interrupts = [], reads = [];
  const dependencies = { execution, flow, currentExecution: () => execution,
    runtimeRequest: async path => { reads.push(path); return path.includes("/generation-runs/")
      ? { id: receipt.runId, projectId: "project", version: 6, status: "paused" }
      : { id: flow.id, projectId: "project", threadId: "root", status: "paused" }; },
    interrupt: async value => interrupts.push(value) };
  return { execution, flow, receipt, message, dependencies, interrupts, reads };
}

test("committed pause rechecks Native and interrupts only its owning current lease once", async () => {
  const f = fixture();
  assert.equal(await interruptCommittedGenerationPause(f.message, f.dependencies), true);
  assert.deepEqual(f.interrupts, [f.execution]); assert.equal(f.reads.length, 2);
  assert.equal(await interruptCommittedGenerationPause(f.message, f.dependencies), false);
  assert.equal(f.interrupts.length, 1);
});

for (const mutation of ["otherWork", "otherThread", "toolFailure", "staleReceipt", "resumedRun", "changedExecution"])
test(`pause observations cannot interrupt ${mutation}`, async () => {
  const f = fixture();
  if (mutation === "otherWork") f.flow.id = "work-other";
  if (mutation === "otherThread") f.message.params.threadId = "other";
  if (mutation === "toolFailure") f.message.params.item.status = "failed";
  if (mutation === "changedExecution") f.dependencies.currentExecution = () => ({});
  if (["staleReceipt", "resumedRun"].includes(mutation)) f.dependencies.runtimeRequest = async path => path.includes("/generation-runs/")
    ? { id: f.receipt.runId, projectId: "project", version: mutation === "staleReceipt" ? 7 : 6, status: mutation === "resumedRun" ? "running" : "paused" }
    : { id: f.flow.id, projectId: "project", threadId: "root", status: "paused" };
  assert.equal(await interruptCommittedGenerationPause(f.message, f.dependencies), false);
  assert.equal(f.interrupts.length, 0);
});
