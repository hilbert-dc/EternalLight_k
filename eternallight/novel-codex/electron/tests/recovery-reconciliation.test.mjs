import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { createHash, randomUUID } from "node:crypto";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

import { HostRecoveryJournal, RecoveryIndex } from "@eternallight/agent-engine";
import { reconcileRecoveryIndex } from "../recovery-reconciliation.mjs";

function runCrashFixture(indexPath) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [fileURLToPath(new URL("./fixture-recovery-crash.mjs", import.meta.url)), indexPath], { stdio: "ignore" });
    child.once("error", reject);
    child.once("exit", (code, signal) => resolve({ code, signal }));
  });
}

async function pendingWrite(index, { runId, requestId, idempotencyKey = null, operation = "save_outline" }) {
  const journal = new HostRecoveryJournal(index);
  await journal.beginRun({ runId, planId: "plan-1", threadId: `thread-${runId}`, projectId: "project-1" });
  await journal.observe({
    method: "item/started",
    params: { item: { id: requestId, type: "mcpToolCall", tool: operation, arguments: { ...(idempotencyKey ? { idempotencyKey } : {}), payload: { secret: "not persisted" }, expectedRevision: 0, expectedCandidateRevisionId: null } } },
  });
  await journal.abandonRun();
}

function snapshot(tasks, operations = []) {
  return { fence: "fence-1", watermark: operations.length, complete: true, nextCursor: null, tasks, operations };
}

test("a proven Runtime rejection closes recovery without claiming a committed write", async () => {
  const root = await mkdtemp(join(tmpdir(), "novel-rejected-recovery-"));
  try {
    const index = new RecoveryIndex(join(root, "host.json"));
    await index.beginRun({ runId: "knowledge", planId: "plan", threadId: "root", projectId: "project-1" });
    await reconcileRecoveryIndex(index, async () => snapshot([{ id: "knowledge", status: "completed" }], [{
      operation_id: "invalid-publish", run_id: "knowledge", thread_id: "root", turn_id: "turn",
      tool_call_id: "call-invalid", tool_name: "publish_knowledge_batch", request_hash: "hash",
      state: "rejected", receipt_ref: null, updated_at: 10,
    }]));
    assert.deepEqual(await index.recoveryReport(), []);
    const operation = (await index.read()).runs.knowledge.requests["mcp:call-invalid"];
    assert.deepEqual({ state: operation.state, outcome: operation.outcome, receipt: operation.receiptRef }, {
      state: "terminal", outcome: "rejected", receipt: "runtime:operation:invalid-publish:rejected",
    });
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("real Runtime and MCP recover a committed write across process restart without a Host success event", { timeout: 25_000 }, async () => {
  const root = await mkdtemp(join(tmpdir(), "novel-authoritative-recovery-"));
  const author = "test-recovery-author-1234567890123456";
  const candidate = "test-recovery-candidate-1234567890123";
  const capability = "test-recovery-capability-123456789012";
  const suffix = process.platform === "win32" ? ".exe" : "";
  const runtimePath = fileURLToPath(new URL(`../../native/target/debug/novel-codex-story-runtime${suffix}`, import.meta.url));
  const mcpPath = fileURLToPath(new URL(`../../native/target/debug/novel-workspace-mcp${suffix}`, import.meta.url));
  let runtime;
  async function stop() {
    if (!runtime || runtime.exitCode !== null || runtime.signalCode !== null) return;
    const stopped = new Promise((resolve) => runtime.once("exit", resolve));
    runtime.kill();
    await stopped;
  }
  async function start() {
    runtime = spawn(runtimePath, ["--port", "0", "--db", join(root, "story.sqlite"), "--engine-budget-db", join(root, "engine-budget.sqlite3"), "--token-stdin"], { stdio: ["pipe", "pipe", "pipe"] });
    const origin = new Promise((resolve, reject) => {
      const lines = createInterface({ input: runtime.stdout });
      lines.on("line", (line) => { if (line.startsWith("Story Runtime listening on ")) resolve(line.slice("Story Runtime listening on ".length)); });
      runtime.once("error", reject);
      runtime.once("exit", () => reject(new Error("Runtime exited before listening")));
    });
    runtime.stdin.end(`${JSON.stringify({ authorToken: author, candidateToken: candidate })}\n`);
    return origin;
  }
  try {
    let origin = await start();
    const request = async (path, options = {}) => {
      const response = await fetch(`${origin}${path}`, { ...options, headers: { authorization: `Bearer ${author}`, "content-type": "application/json" } });
      const body = await response.json();
      assert.ok(response.ok, JSON.stringify(body));
      return body;
    };
    const { project } = await request("/api/projects", { method: "POST", body: JSON.stringify({ title: "恢复证明" }) });
    const projectPath = `/api/projects/${project.id}`;
    const workspace = await request(`${projectPath}/creative-workspace`);
    const task = await request(`${projectPath}/tasks`, { method: "POST", body: JSON.stringify({ operation: "generateBrief", inputRevision: 0, threadId: "root", authorIntent: "写候选简报", target: { type: "artifact" }, capabilityHash: createHash("sha256").update(capability).digest("hex"), capabilityExpiresAt: Math.floor(Date.now() / 1000) + 600,
      initialBudget: { authorizationId: randomUUID(), authorMessageId: randomUUID(), expectedBaseSnapshotId: workspace.headSnapshotId,
        expectedSourceDigest: workspace.sourceDigest, maxRequests: 16, maxTokens: 200_000, deadlineAt: Math.floor(Date.now()/1000)+600 } }) });
    await request(`${projectPath}/tasks/${task.id}/turn`, { method: "POST", body: JSON.stringify({ threadId: "root", turnId: "turn" }) });
    const arguments_ = { taskId: task.id, projectId: project.id, threadId: "root", expectedRevision: 0, expectedCandidateRevisionId: null, idempotencyKey: "save-one", payload: { premise: "不应保存进恢复日记的正文", draftCount: 1 } };
    const indexPath = join(root, "host.json");
    const index = new RecoveryIndex(indexPath);
    const journal = new HostRecoveryJournal(index);
    await journal.beginRun({ runId: task.id, threadId: "root", projectId: project.id, planId: "test" });
    await journal.observe({ method: "item/started", params: { turnId: "turn", item: { id: "write-call", type: "mcpToolCall", tool: "save_brief", arguments: arguments_ } } });
    await journal.flush();
    const mcp = spawn(mcpPath, [], { env: { ...process.env, NOVEL_CODEX_RUNTIME_URL: origin, NOVEL_CODEX_CANDIDATE_TOKEN: candidate, NOVEL_CODEX_TASK_CAPABILITY: capability, NOVEL_CODEX_OPERATION: "generateBrief" }, stdio: ["pipe", "pipe", "pipe"] });
    const output = [];
    mcp.stdout.on("data", (chunk) => output.push(chunk));
    const exited = new Promise((resolve, reject) => { mcp.once("error", reject); mcp.once("exit", resolve); });
    const meta = { sessionId: "root", rootThreadId: "root", callerThreadId: "root", turnId: "turn" };
    const write = JSON.stringify({ jsonrpc: "2.0", id: 1, method: "tools/call", params: { name: "save_brief", arguments: arguments_, _meta: { ...meta, callId: "write-call" } } }).replace('"draftCount":1', '"draftCount":1.0');
    const complete = JSON.stringify({ jsonrpc: "2.0", id: 2, method: "tools/call", params: { name: "complete_task", arguments: { taskId: task.id, projectId: project.id, threadId: "root", expectedRevision: 0, summary: "completed" }, _meta: { ...meta, callId: "complete-call" } } });
    mcp.stdin.end(`${write}\n${complete}\n`);
    assert.equal(await exited, 0);
    const responses = Buffer.concat(output).toString().trim().split("\n").map((line) => JSON.parse(line));
    assert.equal(responses.length, 2);
    for (const response of responses) assert.notEqual(response.result.isError, true, JSON.stringify(response));
    const response = responses[0];
    const revisionId = JSON.parse(response.result.content[0].text).revisionId;
    // Neither item/completed nor its receipt reaches the Host journal.
    await journal.abandonRun();
    await stop();
    origin = await start();
    await request("/api/tasks/recover-orphans", { method: "POST", body: "{}" });
    const reopened = new RecoveryIndex(indexPath);
    await reconcileRecoveryIndex(reopened, request);
    assert.deepEqual(await reopened.recoveryReport(), []);
    const stored = await reopened.read();
    assert.equal(stored.runs[task.id].requests["mcp:write-call"].receiptRef, `runtime:candidate:${revisionId}`);
    assert.equal(stored.runs[task.id].requests["mcp:write-call"].outcome, "committed");
    assert.equal(stored.runs[task.id].requests["mcp:complete-call"].receiptRef, `runtime:task:${task.id}:completed`);
    assert.equal(stored.runs[task.id].requests["mcp:complete-call"].outcome, "committed");
    assert.equal(JSON.stringify(stored).includes("不应保存进恢复日记的正文"), false);
  } finally {
    await stop();
    await rm(root, { recursive: true, force: true });
  }
});

test("Runtime proofs cannot settle another request hash or invent a committed receipt", async () => {
  const root = await mkdtemp(join(tmpdir(), "novel-proof-binding-"));
  try {
    const index = new RecoveryIndex(join(root, "host.json"));
    await pendingWrite(index, { runId: "task-proof", requestId: "call" });
    const original = (await index.read()).runs["task-proof"].requests["mcp:call"];
    for (const bad of [{ request_hash: "different", receipt_ref: "runtime:wrong" }, { request_hash: original.requestHash, receipt_ref: null }]) {
      await assert.rejects(index.observeRuntimeOperations("task-proof", [{ operation_id: "operation", run_id: "task-proof", tool_call_id: "call", tool_name: "save_outline", state: "committed", ...bad }]));
      assert.equal((await index.read()).runs["task-proof"].requests["mcp:call"].outcome, null);
    }
  } finally { await rm(root, { recursive: true, force: true }); }
});

test("startup reconciliation closes a write only after the fenced Runtime ledger proves its receipt", async () => {
  const root = await mkdtemp(join(tmpdir(), "novel-reconciliation-"));
  try {
    const index = new RecoveryIndex(join(root, "recovery.json"));
    await pendingWrite(index, { runId: "task-1", requestId: "write-1", idempotencyKey: "outline-attempt-1" });
    const requestHash=(await index.read()).runs["task-1"].requests["mcp:write-1"].requestHash;
    await reconcileRecoveryIndex(index, async (path, options) => {
      if (path.endsWith("/recovery-snapshot")) return snapshot([{ id: "task-1", status: "cancelled" }], [{
        operation_id: "operation-1",
        run_id: "task-1",
        tool_call_id: "write-1",
        tool_name: "save_outline",
        request_hash: requestHash,
        state: "committed",
        receipt_ref: "runtime:receipt-1",
        turn_id: "turn-1",
        updated_at: "2026-01-01T00:00:00.000Z",
      }]);
      throw new Error(`unexpected path: ${path}`);
    });

    assert.deepEqual(await index.recoveryReport(), []);
    assert.equal(JSON.stringify(await index.read()).includes("not persisted"), false);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("startup reconciliation never fabricates receipts for non-idempotent write tools", async () => {
  const root = await mkdtemp(join(tmpdir(), "novel-domain-reconciliation-"));
  try {
    const index = new RecoveryIndex(join(root, "recovery.json"));
    await pendingWrite(index, { runId: "review-task", requestId: "review-write", operation: "submit_planning_review_report" });
    await pendingWrite(index, { runId: "complete-task", requestId: "complete-write", operation: "complete_task" });
    await reconcileRecoveryIndex(index, async (path, options) => {
      if (path.endsWith("/recovery-snapshot")) {
        const { taskIds } = JSON.parse(options.body);
        return snapshot(taskIds.map((taskId) => taskId === "review-task"
          ? { id: taskId, status: "cancelled", outputKinds: ["critique"] }
          : { id: taskId, status: "completed", outputKinds: [] }));
      }
      throw new Error(`unexpected path: ${path}`);
    });

    const report = await index.recoveryReport();
    assert.deepEqual(report.map((run) => ({
      runId: run.runId,
      runtimeStatus: run.runtimeStatus,
      requests: run.requests.map((request) => ({
        operation: request.operation,
        receiptRef: request.receiptRef,
        recovery: request.recovery,
        state: request.state,
      })),
    })), [
      {
        runId: "review-task",
        runtimeStatus: "cancelled",
        requests: [{ operation: "submit_planning_review_report", receiptRef: null, recovery: "manual-reconciliation", state: "tool-dispatched" }],
      },
      {
        runId: "complete-task",
        runtimeStatus: "completed",
        requests: [{ operation: "complete_task", receiptRef: null, recovery: "manual-reconciliation", state: "tool-dispatched" }],
      },
    ]);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("startup reconciliation retains a write when Runtime has no receipt", async () => {
  const root = await mkdtemp(join(tmpdir(), "novel-reconciliation-missing-"));
  try {
    const index = new RecoveryIndex(join(root, "recovery.json"));
    await pendingWrite(index, { runId: "task-2", requestId: "write-2", idempotencyKey: "outline-attempt-2" });
    await reconcileRecoveryIndex(index, async (path) => {
      if (path.endsWith("/recovery-snapshot")) return snapshot([{ id: "task-2", status: "cancelled" }]);
      throw new Error(`unexpected path: ${path}`);
    });

    const report = await index.recoveryReport();
    assert.equal(report[0].runtimeStatus, "cancelled");
    assert.equal(report[0].requests[0].recovery, "manual-reconciliation");
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("startup reconciliation discovers writes that never reached the Host journal", async () => {
  const root = await mkdtemp(join(tmpdir(), "novel-runtime-ledger-recovery-"));
  try {
    const index = new RecoveryIndex(join(root, "recovery.json"));
    await index.beginRun({ runId: "task-ledger", planId: "plan-1", threadId: "root-thread", projectId: "project-1" });
    await reconcileRecoveryIndex(index, async (path) => {
      if (path.endsWith("/recovery-snapshot")) return snapshot([{ id: "task-ledger", status: "completed" }], [{
        operation_id: "operation-1",
        run_id: "task-ledger",
        tool_call_id: "call-1",
        tool_name: "save_outline",
        request_hash: "hash-1",
        state: "committed",
        receipt_ref: "runtime:operation-1",
        turn_id: "turn-1",
        updated_at: "2026-01-01T00:00:00.000Z",
      }]);
      throw new Error(`unexpected path: ${path}`);
    });

    assert.deepEqual(await index.recoveryReport(), []);
    const stored = await index.read();
    assert.equal(stored.runs["task-ledger"].requests["mcp:call-1"].outcome, "committed");
    assert.equal(stored.runs["task-ledger"].requests["mcp:call-1"].receiptRef, "runtime:operation-1");
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("runtime outcome-unknown writes remain blocked for manual reconciliation", async () => {
  const root = await mkdtemp(join(tmpdir(), "novel-runtime-unknown-recovery-"));
  try {
    const index = new RecoveryIndex(join(root, "recovery.json"));
    await index.beginRun({ runId: "task-unknown", planId: "plan-1", threadId: "root-thread", projectId: "project-1" });
    await reconcileRecoveryIndex(index, async (path) => {
      if (path.endsWith("/recovery-snapshot")) return snapshot([{ id: "task-unknown", status: "failed" }], [{
        operation_id: "operation-unknown",
        run_id: "task-unknown",
        tool_call_id: "call-unknown",
        tool_name: "save_outline",
        request_hash: "hash-unknown",
        state: "outcome-unknown",
        receipt_ref: null,
        turn_id: "turn-unknown",
        updated_at: "2026-01-01T00:00:00.000Z",
      }]);
      throw new Error(`unexpected path: ${path}`);
    });

    const report = await index.recoveryReport();
    assert.equal(report[0].requests[0].outcome, "outcome-unknown");
    assert.equal(report[0].requests[0].recovery, "manual-reconciliation");
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("runtime cancellation before execution closes recovery without claiming a commit", async () => {
  const root = await mkdtemp(join(tmpdir(), "novel-runtime-cancelled-recovery-"));
  try {
    const index = new RecoveryIndex(join(root, "recovery.json"));
    await index.beginRun({ runId: "task-cancelled", planId: "plan-1", threadId: "root-thread", projectId: "project-1" });
    await reconcileRecoveryIndex(index, async (path) => {
      if (path.endsWith("/recovery-snapshot")) return snapshot([{ id: "task-cancelled", status: "cancelled" }], [{
        operation_id: "operation-cancelled",
        run_id: "task-cancelled",
        tool_call_id: "call-cancelled",
        tool_name: "save_outline",
        request_hash: "hash-cancelled",
        state: "cancelled-before-execution",
        receipt_ref: null,
        turn_id: "turn-cancelled",
        updated_at: "2026-01-01T00:00:00.000Z",
      }]);
      throw new Error(`unexpected path: ${path}`);
    });

    assert.deepEqual(await index.recoveryReport(), []);
    const stored = await index.read();
    assert.equal(stored.runs["task-cancelled"].requests["mcp:call-cancelled"].outcome, "cancelled-before-execution");
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("a failed fenced snapshot keeps the run open and explicitly blocks reconciliation", async () => {
  const root = await mkdtemp(join(tmpdir(), "novel-runtime-snapshot-failure-"));
  try {
    const index = new RecoveryIndex(join(root, "recovery.json"));
    await index.beginRun({ runId: "task-ledger-down", planId: "plan-1", threadId: "root-thread", projectId: "project-1" });
    const errors = [];
    await reconcileRecoveryIndex(index, async () => { throw new Error("ledger unavailable"); }, (...args) => errors.push(args));

    const report = await index.recoveryReport();
    assert.equal(report.length, 1);
    assert.equal(report[0].runtimeStatus, null);
    assert.match(report[0].reconciliationRequired.reason, /ledger unavailable/);
    assert.equal(errors[0][1], "snapshot");
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("a new process recovers a write journaled before an abrupt process exit", async () => {
  const root = await mkdtemp(join(tmpdir(), "novel-process-crash-recovery-"));
  try {
    const indexPath = join(root, "recovery.json");
    assert.deepEqual(await runCrashFixture(indexPath), { code: 17, signal: null });

    const recovered = new RecoveryIndex(indexPath);
    const report = await recovered.recoveryReport();
    assert.equal(report[0].runId, "crashed-task");
    assert.equal(report[0].requests[0].recovery, "manual-reconciliation");
    assert.equal(JSON.stringify(await recovered.read()).includes("must not be journaled"), false);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
