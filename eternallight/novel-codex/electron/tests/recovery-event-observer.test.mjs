import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { HostRecoveryJournal, RecoveryIndex, defineRecoveryOperationContract, digest } from "@eternallight/agent-engine";
import { observeRecoveryEvent, projectRecoveryEvent, recoveryEventDiagnostic } from "../recovery-event-observer.mjs";
import { reconcileRecoveryIndex } from "../recovery-reconciliation.mjs";

async function journalFixture(operationContract) {
  const root = await mkdtemp(join(tmpdir(), "novel-recovery-event-"));
  const index = new RecoveryIndex(join(root, "recovery.json"));
  const journal = new HostRecoveryJournal(index, operationContract);
  await journal.beginRun({ runId: "task", planId: "plan", projectId: "project", threadId: "root" });
  return { index, journal, cleanup: () => rm(root, { recursive: true, force: true }) };
}

test("recovery projection preserves exact MCP argument/result references and logs only type and UTF-8 bytes", () => {
  const item = { id: "write", type: "mcpToolCall", tool: "save_brief", arguments: { body: "私有输入" }, result: { receipt: "权威回执", private: "私有结果" }, status: "completed" };
  const event = { method: "item/completed", params: { threadId: "root", turnId: "turn", item, history: "无关历史" } };
  const projected = projectRecoveryEvent(event);
  assert.equal(projected.params.item, item);
  assert.equal(projected.params.item.arguments, item.arguments);
  assert.equal(projected.params.item.result, item.result);
  assert.equal("history" in projected.params, false);
  assert.deepEqual(recoveryEventDiagnostic(event), {
    method: "item/completed", bytes: Buffer.byteLength(JSON.stringify(event), "utf8"),
  });
  const unknown = { method: "SECRET_METHOD", params: { item } };
  const diagnostic = recoveryEventDiagnostic(unknown);
  assert.equal(diagnostic.method, "mcp/other");
  assert.equal(Object.keys(diagnostic).length, 2);
  assert.equal(JSON.stringify(diagnostic).includes("SECRET_METHOD"), false);
});

test("greater than 4 MiB history, RPC replies and turn aggregates do not poison a real journal flush", async () => {
  const { index, journal, cleanup } = await journalFixture();
  try {
    const privateHistory = "PRIVATE_HISTORY" + "章".repeat(1_600_000);
    assert.ok(Buffer.byteLength(privateHistory, "utf8") > 4 * 1024 * 1024);
    await observeRecoveryEvent(journal, { method: "thread/started", params: { thread: { id: "root", turns: [privateHistory] } } });
    await observeRecoveryEvent(journal, { id: 7, result: { thread: { turns: [privateHistory] } } });
    await observeRecoveryEvent(journal, { method: "item/agentMessage/delta", params: { delta: privateHistory } });
    await observeRecoveryEvent(journal, { method: "turn/started", params: { threadId: "root", turn: { id: "turn", items: [privateHistory] } } });
    await observeRecoveryEvent(journal, { method: "turn/completed", params: { threadId: "root", turn: { id: "turn", items: [{ id: "historical-write", type: "mcpToolCall", arguments: { body: privateHistory } }] } } });
    await journal.flush();
    await journal.abandonRun();
    const stored = await index.read();
    assert.deepEqual(Object.keys(stored.runs.task.requests), ["model:turn"]);
    assert.equal(stored.runs.task.requests["model:turn"].state, "terminal");
    assert.equal(JSON.stringify(stored).includes("PRIVATE_HISTORY"), false);
  } finally { await cleanup(); }
});

test("a large MCP completion fails closed, preserves its complete request hash and settles only from the Runtime ledger", async () => {
  const { index, journal, cleanup } = await journalFixture();
  try {
    const arguments_ = { idempotencyKey: "key", payload: { premise: "PRIVATE_ARGUMENT" + "章".repeat(450_000), tail: "exact-tail" } };
    const item = { id: "write", type: "mcpToolCall", tool: "save_brief", arguments: arguments_, status: "inProgress" };
    await observeRecoveryEvent(journal, { method: "item/started", params: { threadId: "root", turnId: "turn", item } });
    const result = { content: [{ type: "text", text: "PRIVATE_RESULT" + "文".repeat(1_600_000) }] };
    const completed = { method: "item/completed", params: { threadId: "root", turnId: "turn", item: { ...item, status: "completed", result } } };
    const diagnostics = [];
    await assert.rejects(observeRecoveryEvent(journal, completed, diagnostic => diagnostics.push(diagnostic)), /recovery event exceeds limit/);
    await assert.rejects(journal.flush(), /recovery event exceeds limit/);
    assert.deepEqual(diagnostics, [{ method: "item/completed", bytes: Buffer.byteLength(JSON.stringify(completed), "utf8") }]);
    await journal.abandonRun();
    const recorded = (await index.read()).runs.task.requests["mcp:write"];
    assert.equal(recorded.requestHash, digest({ tool: item.tool, arguments: arguments_ }));
    assert.equal(recorded.outcome, null);
    assert.equal((await index.recoveryReport())[0].requests[0].recovery, "manual-reconciliation");

    const snapshot = operations => ({ fence: "fence", complete: true, nextCursor: null, tasks: [{ id: "task", status: "completed" }], operations });
    await reconcileRecoveryIndex(index, async () => snapshot([]));
    assert.equal((await index.recoveryReport())[0].requests[0].recovery, "manual-reconciliation", "task completion alone must not prove a write receipt");
    await reconcileRecoveryIndex(index, async () => snapshot([{
      operation_id: "operation", run_id: "task", thread_id: "root", turn_id: "turn",
      tool_call_id: "write", tool_name: "save_brief", request_hash: recorded.requestHash,
      state: "committed", receipt_ref: "runtime:candidate:revision", updated_at: 10,
    }]));
    assert.deepEqual(await index.recoveryReport(), []);
    const settled = (await index.read()).runs.task.requests["mcp:write"];
    assert.equal(settled.outcome, "committed");
    assert.equal(settled.receiptRef, "runtime:candidate:revision");
    const stored = JSON.stringify(await index.read());
    assert.equal(stored.includes("PRIVATE_ARGUMENT"), false);
    assert.equal(stored.includes("PRIVATE_RESULT"), false);
  } finally { await cleanup(); }
});

test("an injected authoritative receipt still sees the unmodified MCP result", async () => {
  const contract = defineRecoveryOperationContract({
    classify: call => ({ effect: "write", operation: call.tool, idempotencyRef: null }),
    hash: call => digest({ tool: call.tool, arguments: call.arguments }),
    receipt: ({ result }) => result?.receiptRef ? { outcome: "committed", receiptRef: result.receiptRef } : null,
    reconcile: () => ({ outcome: "unknown", receiptRef: null }),
  });
  const { index, journal, cleanup } = await journalFixture(contract);
  try {
    await observeRecoveryEvent(journal, { method: "item/completed", params: { threadId: "root", turnId: "turn", item: {
      id: "write", type: "mcpToolCall", tool: "save_brief", status: "completed",
      arguments: { payload: "PRIVATE_ARGUMENT" }, result: { receiptRef: "runtime:candidate:proof", body: "PRIVATE_RESULT" },
    } } });
    await journal.flush();
    const request = (await index.read()).runs.task.requests["mcp:write"];
    assert.equal(request.outcome, "committed");
    assert.equal(request.receiptRef, "runtime:candidate:proof");
    assert.equal(JSON.stringify(await index.read()).includes("PRIVATE_"), false);
  } finally { await cleanup(); }
});

test("an oversized first MCP event leaves the run open for a fenced Runtime proof even without a Host request record", async () => {
  const { index, journal, cleanup } = await journalFixture();
  try {
    const arguments_ = { payload: "PRIVATE_ARGUMENT" + "章".repeat(1_600_000) };
    await assert.rejects(observeRecoveryEvent(journal, { method: "item/started", params: { threadId: "root", item: {
      id: "unobserved-write", type: "mcpToolCall", tool: "save_brief", arguments: arguments_,
    } } }), /recovery event exceeds limit/);
    await assert.rejects(journal.flush(), /recovery event exceeds limit/);
    assert.deepEqual((await index.read()).runs.task.requests, {});
    assert.equal((await index.unfinishedRuns()).length, 1);
    const operation = {
      operation_id: "accepted-write", run_id: "task", thread_id: "root", turn_id: "turn",
      tool_call_id: "unobserved-write", tool_name: "save_brief",
      request_hash: digest({ tool: "save_brief", arguments: arguments_ }),
      state: "accepted", receipt_ref: null, updated_at: 10,
    };
    const snapshot = () => ({ fence: "fence", complete: true, nextCursor: null,
      tasks: [{ id: "task", status: "completed" }], operations: [operation] });
    await reconcileRecoveryIndex(index, async () => snapshot());
    assert.equal((await index.unfinishedRuns()).length, 1);
    assert.equal((await index.recoveryReport())[0].requests[0].recovery, "manual-reconciliation");
    operation.state = "committed";
    operation.receipt_ref = "runtime:candidate:revision";
    await reconcileRecoveryIndex(index, async () => snapshot());
    assert.deepEqual(await index.recoveryReport(), []);
    assert.equal(JSON.stringify(await index.read()).includes("PRIVATE_ARGUMENT"), false);
  } finally { await cleanup(); }
});

test("child and unidentified events cannot settle or collide with the bound root's recovery requests", async () => {
  const contract = defineRecoveryOperationContract({
    classify: call => ({ effect: "write", operation: call.tool, idempotencyRef: null }),
    hash: call => digest({ tool: call.tool, arguments: call.arguments }),
    receipt: ({ result }) => result?.receiptRef ? { outcome: "committed", receiptRef: result.receiptRef } : null,
    reconcile: () => ({ outcome: "unknown", receiptRef: null }),
  });
  const { index, journal, cleanup } = await journalFixture(contract);
  const turn = (method, threadId) => ({ method, params: { threadId, turn: { id: "reused-turn" } } });
  const write = (method, threadId, result) => ({ method, params: { threadId, turnId: "reused-turn", item: {
    id: "reused-call", type: "mcpToolCall", tool: "save_draft_checkpoint", arguments: { payload: "ROOT_PROSE" },
    status: method === "item/started" ? "inProgress" : "completed", result,
  } } });
  try {
    await observeRecoveryEvent(journal, turn("turn/started", "root"));
    await observeRecoveryEvent(journal, write("item/started", "root"));
    for (const threadId of ["child", "foreign-root", undefined]) {
      await observeRecoveryEvent(journal, turn("turn/started", threadId));
      await observeRecoveryEvent(journal, turn("turn/completed", threadId));
      await observeRecoveryEvent(journal, write("item/completed", threadId, { receiptRef: "INVALID_CHILD_RECEIPT" }));
    }
    await journal.flush();
    const pending = (await index.read()).runs.task.requests;
    assert.equal(pending["model:reused-turn"].state, "streaming");
    assert.equal(pending["mcp:reused-call"].state, "tool-dispatched");
    assert.equal(pending["mcp:reused-call"].outcome, null);
    assert.deepEqual(Object.keys(pending).sort(), ["mcp:reused-call", "model:reused-turn"]);
    for (let replay = 0; replay < 2; replay++) {
      await observeRecoveryEvent(journal, write("item/completed", "root", { receiptRef: "runtime:root:checkpoint" }));
      await observeRecoveryEvent(journal, turn("turn/completed", "root"));
    }
    await journal.flush();
    const settled = (await index.read()).runs.task.requests;
    assert.equal(settled["model:reused-turn"].state, "terminal");
    assert.equal(settled["mcp:reused-call"].receiptRef, "runtime:root:checkpoint");
    assert.deepEqual(Object.keys(settled), Object.keys(pending));
    assert.equal(JSON.stringify(settled).includes("INVALID_CHILD_RECEIPT"), false);
  } finally { await cleanup(); }
});

test("journal restart and late child results preserve the original task ledger without restarting execution", async () => {
  const { index, journal, cleanup } = await journalFixture();
  const event = (threadId, id = "same-call") => ({ method: "item/started", params: { threadId, turnId: "turn", item: {
    id, type: "mcpToolCall", tool: "save_draft_checkpoint", arguments: { idempotencyKey: id }, status: "inProgress",
  } } });
  try {
    await observeRecoveryEvent(journal, event("root"));
    await journal.abandonRun();
    await observeRecoveryEvent(journal, event("root", "late-old-root"));
    const restarted = new HostRecoveryJournal(index);
    await restarted.beginRun({ runId: "next-attempt", planId: "same-plan", projectId: "project", threadId: "next-root" });
    await observeRecoveryEvent(restarted, event("child"));
    await observeRecoveryEvent(restarted, event("root"));
    await observeRecoveryEvent(restarted, event("next-root"));
    await restarted.abandonRun();
    const stored = await index.read();
    assert.deepEqual(Object.keys(stored.runs.task.requests), ["mcp:same-call"]);
    assert.deepEqual(Object.keys(stored.runs["next-attempt"].requests), ["mcp:same-call"]);
    assert.equal(stored.runs.task.terminalAt, null, "Host abandonment cannot establish a write outcome");
    assert.equal((await index.recoveryReport()).length, 2);
    assert.equal(JSON.stringify(stored).includes("late-old-root"), false);
  } finally { await cleanup(); }
});
