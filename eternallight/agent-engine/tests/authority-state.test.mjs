import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { ExecutionAuthority, HostRecoveryJournal, PersistentTaskBudget, RecoveryCoordinator, RecoveryIndex, RouteLineage, TaskBudget, assertExecutionLease, authorizeMcpOperation, createExecutionLease, defineRecoveryOperationContract, digest, trustedIdentityFromMcpMeta } from "../src/index.mjs";

function testOperationContract({ reconcile = () => ({ outcome: "unknown", receiptRef: null }) } = {}) {
  return defineRecoveryOperationContract({
    classify(toolCall) {
      return {
        effect: toolCall.tool === "query_record" ? "read" : "write",
        operation: toolCall.tool,
        idempotencyRef: toolCall.arguments?.idempotencyKey ? `idempotency:${digest(toolCall.arguments.idempotencyKey)}` : null,
      };
    },
    hash(toolCall) {
      return digest({ operation: toolCall.tool, payload: toolCall.arguments?.payload ?? null });
    },
    receipt({ toolCall }) {
      return toolCall.arguments?.authoritativeReceipt
        ? { outcome: "committed", receiptRef: toolCall.arguments.authoritativeReceipt }
        : null;
    },
    reconcile,
  });
}

test("execution leases expire, revoke by epoch, and enforce directed model lineage", () => {
  const now = new Date("2026-09-20T00:00:00Z");
  const plan = { id: "plan-1", rootRoute: "root", expiresAt: "2026-09-20T01:00:00Z", allowedChildEdges: [{ parentConnectionId: "root", childConnectionId: "critic" }] };
  const lease = createExecutionLease({ sessionPlan: plan, runId: "run-1", budgetId: "budget-1", now, maxDurationMs: 60_000 });
  assert.equal(assertExecutionLease(lease, plan, { now: now.getTime() }), lease);
  assert.throws(() => assertExecutionLease(lease, plan, { epoch: 2, now: now.getTime() }), /revoked/);
  assert.throws(() => assertExecutionLease(lease, plan, { now: now.getTime() + 60_000 }), /expired/);
  const lineage = new RouteLineage(lease);
  assert.equal(lineage.authorize({ routeId: "root", threadId: "root-thread" }), "root");
  assert.equal(lineage.authorize({ routeId: "critic", threadId: "child-thread", parentThreadId: "root-thread" }), "critic");
  assert.throws(() => lineage.authorize({ routeId: "root", threadId: "grandchild", parentThreadId: "child-thread" }), /not authorized/);
  assert.throws(() => lineage.authorize({ routeId: "critic", threadId: "foreign", parentThreadId: "missing" }), /outside/);
});

test("execution authority makes revocation and terminal outcomes monotonic", () => {
  const authority = new ExecutionAuthority();
  const lease = { runId: "run-authority", leaseId: "lease-authority", planId: "plan", epoch: 1 };
  assert.equal(authority.issue({ lease }).state, "active");
  assert.equal(authority.validate({ runId: lease.runId, leaseId: lease.leaseId, epoch: 1 }).epoch, 1);
  assert.equal(authority.revoke({ runId: lease.runId, leaseId: lease.leaseId, expectedEpoch: 1, reason: "stop" }).epoch, 2);
  assert.throws(() => authority.validate({ runId: lease.runId, leaseId: lease.leaseId, epoch: 1 }), /rejected/);
  authority.markDrained(lease.runId);
  authority.markReconciled(lease.runId);
  assert.equal(authority.terminalize(lease.runId, "cancelled").outcome, "cancelled");
  assert.throws(() => authority.terminalize(lease.runId, "completed"), /immutable/);
  authority.release(lease.runId);
});

test("trusted MCP identity binds child reads to the parent and denies writes", () => {
  const child = trustedIdentityFromMcpMeta({ callerThreadId: "child-1", parentThreadId: "parent-1", rootThreadId: "root-1", sessionId: "root-1", turnId: "turn-1", callId: "call-1", sessionSource: "subagent", agentRole: "critic", agentDepth: 2 });
  assert.deepEqual(child, { callerThreadId: "child-1", parentThreadId: "parent-1", rootThreadId: "root-1", sessionId: "root-1", turnId: "turn-1", toolCallId: "call-1", source: "subagent", role: "critic", depth: 2, kind: "subagent" });
  assert.deepEqual(authorizeMcpOperation(child, "read_context", "root-1"), { allowed: true, reason: null, boundThreadId: "root-1" });
  assert.deepEqual(authorizeMcpOperation(child, "save_outline", "root-1"), { allowed: false, reason: "subagent_read_only", boundThreadId: "root-1" });
  assert.deepEqual(authorizeMcpOperation(child, "read_context", "another-root"), { allowed: false, reason: "thread_identity_mismatch", boundThreadId: "root-1" });
});

test("non-spawn delegates cannot become roots when parent metadata is absent", () => {
  const review = trustedIdentityFromMcpMeta({ callerThreadId: "review-1", rootThreadId: "root-1", sessionId: "root-1", sessionSource: "subagent_review" });
  assert.equal(review.kind, "subagent");
  assert.equal(authorizeMcpOperation(review, "save_outline", "root-1").allowed, false);
  assert.throws(() => trustedIdentityFromMcpMeta({ callerThreadId: "child-1", rootThreadId: "root-1", sessionId: "other" }), /disagree/);
});

test("recovery index never replays a write whose receipt is unknown", async () => {
  const root = await mkdtemp(join(tmpdir(), "agent-engine-recovery-"));
  try {
    const index = new RecoveryIndex(join(root, "recovery.json"));
    await index.beginRun({ runId: "run-1", planId: "plan-1", threadId: "thread-1" });
    await index.recordRequest("run-1", { requestId: "request-1", turnId: "turn-1" });
    await index.transitionRequest("run-1", "request-1", "streaming");
    await index.transitionRequest("run-1", "request-1", "tool-dispatched");
    const report = await index.recoveryReport();
    assert.match(report[0].requests[0].updatedAt, /^\d{4}-\d{2}-\d{2}T/);
    assert.deepEqual(report, [{
      runId: "run-1",
      planId: "plan-1",
      threadId: "thread-1",
      projectId: null,
      operation: null,
      runtimeStatus: null,
      reconciledAt: null,
      recoveryFence: null,
      reconciliationRequired: null,
      requests: [{ requestId: "request-1", turnId: "turn-1", kind: "model", operation: null, state: "tool-dispatched", updatedAt: report[0].requests[0].updatedAt, receiptRef: null, hasIdempotencyRef: false, hasRequestHash: false, recovery: "resume-core" }],
    }]);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("domain contracts classify and hash writes without exposing keys or payloads", async () => {
  const root = await mkdtemp(join(tmpdir(), "agent-engine-receipt-recovery-"));
  try {
    const index = new RecoveryIndex(join(root, "recovery.json"));
    const journal = new HostRecoveryJournal(index, testOperationContract());
    await journal.beginRun({ runId: "task-1", planId: "plan-1", sessionPlan: { credentialRef: "SECRET_PLAN_VALUE" }, threadId: "thread-1", projectId: "project-1" });
    const arguments_ = { idempotencyKey: "SECRET_IDEMPOTENCY_KEY", payload: { private: "SECRET_PAYLOAD" } };
    await journal.observe({ method: "item/started", params: { turnId: "turn-1", item: { id: "write-1", type: "mcpToolCall", tool: "persist_record", arguments: arguments_, status: "inProgress" } } });
    await journal.abandonRun();

    assert.deepEqual(await index.reconciliationCandidates(), [{ runId: "task-1", projectId: "project-1", requestId: "mcp:write-1", operation: "persist_record", idempotencyRef: `idempotency:${digest(arguments_.idempotencyKey)}`, requestHash: digest({ operation: "persist_record", payload: arguments_.payload }) }]);
    const pending = await index.recoveryReport();
    assert.equal(pending[0].requests[0].hasIdempotencyRef, true);
    assert.equal(pending[0].requests[0].hasRequestHash, true);
    assert.equal("idempotencyRef" in pending[0].requests[0], false);
    assert.equal("requestHash" in pending[0].requests[0], false);
    const stored = await readFile(join(root, "recovery.json"), "utf8");
    assert.equal(stored.includes("SECRET_PLAN_VALUE"), false);
    assert.equal(stored.includes("SECRET_IDEMPOTENCY_KEY"), false);
    assert.equal(stored.includes("SECRET_PAYLOAD"), false);
    assert.equal(stored.includes("sessionPlan"), false);

    await index.confirmRuntimeReceipt("task-1", "mcp:write-1", "runtime:receipt-1");
    await index.reconcileRun("task-1", "cancelled");
    assert.deepEqual(await index.recoveryReport(), []);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("runtime reconciliation closes safe terminal runs but preserves unknown writes", async () => {
  const root = await mkdtemp(join(tmpdir(), "agent-engine-runtime-recovery-"));
  try {
    const index = new RecoveryIndex(join(root, "recovery.json"));
    await index.beginRun({ runId: "safe-run", planId: "plan-1", threadId: "thread-1" });
    await index.recordRequest("safe-run", { requestId: "model-1", state: "streaming" });
    await index.transitionRequest("safe-run", "model-1", "terminal");
    await index.reconcileRun("safe-run", "cancelled", "2026-09-19T10:00:00.000Z");

    await index.beginRun({ runId: "uncertain-run", planId: "plan-1", threadId: "thread-2" });
    await index.recordRequest("uncertain-run", { requestId: "write-1", kind: "write", operation: "persist_record", state: "tool-dispatched" });
    await index.reconcileRun("uncertain-run", "cancelled", "2026-09-19T10:01:00.000Z");

    const report = await index.recoveryReport();
    assert.equal(report.length, 1);
    assert.equal(report[0].runId, "uncertain-run");
    assert.equal(report[0].runtimeStatus, "cancelled");
    assert.equal(report[0].reconciledAt, "2026-09-19T10:01:00.000Z");
    assert.equal(report[0].requests[0].recovery, "manual-reconciliation");
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("manual recovery resolution requires an unresolved write and remains auditable", async () => {
  const root = await mkdtemp(join(tmpdir(), "agent-engine-manual-resolution-"));
  try {
    const index = new RecoveryIndex(join(root, "recovery.json"));
    await index.beginRun({ runId: "manual-run", planId: "plan-1", threadId: "thread-1" });
    await index.recordRequest("manual-run", { requestId: "write-1", kind: "write", operation: "persist_record", state: "uncertain" });
    await index.resolveRun("manual-run", "manual-reviewed", "2026-09-19T11:00:00.000Z");
    assert.deepEqual(await index.recoveryReport(), []);
    const document = await index.read();
    assert.equal(document.runs["manual-run"].resolution, "manual-reviewed");
    assert.equal(document.runs["manual-run"].terminalAt, "2026-09-19T11:00:00.000Z");

    await index.beginRun({ runId: "safe-run", planId: "plan-1", threadId: "thread-2" });
    await assert.rejects(index.resolveRun("safe-run", "manual-reviewed"), /does not require manual review/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("host recovery journal preserves unknown writes for reconciliation and completes observed receipts", async () => {
  const root = await mkdtemp(join(tmpdir(), "agent-engine-host-recovery-"));
  try {
    const index = new RecoveryIndex(join(root, "recovery.json"));
    const journal = new HostRecoveryJournal(index, testOperationContract());
    await journal.beginRun({ runId: "task-1", planId: "plan-1", threadId: "thread-1", projectId: "project-1", operation: "generateOutline" });
    await journal.observe({ method: "turn/started", params: { turn: { id: "turn-1" } } });
    await journal.observe({ method: "item/started", params: { turnId: "turn-1", item: { id: "write-1", type: "mcpToolCall", tool: "persist_record", status: "inProgress" } } });
    await journal.observe({ method: "item/completed", params: { turnId: "turn-1", item: { id: "read-1", type: "mcpToolCall", tool: "query_record", status: "completed", result: { content: [{ type: "text", text: "ok" }], isError: false } } } });
    await journal.abandonRun();
    const report = await index.recoveryReport();
    assert.equal(report[0].projectId, "project-1");
    assert.equal(report[0].requests.find((request) => request.requestId === "mcp:write-1").recovery, "manual-reconciliation");
    assert.equal(report[0].requests.find((request) => request.requestId === "mcp:read-1").recovery, "none");
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("completing a host run never hides an unresolved write", async () => {
  const root = await mkdtemp(join(tmpdir(), "agent-engine-host-completion-"));
  try {
    const index = new RecoveryIndex(join(root, "recovery.json"));
    const journal = new HostRecoveryJournal(index, testOperationContract());
    await journal.beginRun({ runId: "task-1", planId: "plan-1", threadId: "thread-1", projectId: "project-1" });
    await journal.observe({ method: "item/started", params: { turnId: "turn-1", item: { id: "write-1", type: "mcpToolCall", tool: "persist_record", status: "inProgress" } } });
    assert.equal(await journal.completeRun(), false);
    assert.equal((await index.recoveryReport())[0].requests[0].recovery, "manual-reconciliation");
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("only an injected authoritative receipt can commit an observed write", async () => {
  const root = await mkdtemp(join(tmpdir(), "agent-engine-authoritative-receipt-"));
  try {
    const index = new RecoveryIndex(join(root, "recovery.json"));
    const journal = new HostRecoveryJournal(index, testOperationContract());
    await journal.beginRun({ runId: "task-1", planId: "plan-1", threadId: "thread-1", projectId: "project-1" });
    await journal.observe({
      method: "item/completed",
      params: {
        turnId: "turn-1",
        item: {
          id: "write-1",
          type: "mcpToolCall",
          tool: "persist_record",
          arguments: { authoritativeReceipt: "ledger:receipt-1", payload: "SECRET_RESULT_INPUT" },
          status: "completed",
          result: { private: "SECRET_RESULT_BODY" },
        },
      },
    });
    await journal.abandonRun();
    assert.deepEqual(await index.reconciliationCandidates(), []);
    const document = await index.read();
    assert.equal(document.runs["task-1"].requests["mcp:write-1"].outcome, "committed");
    assert.equal(document.runs["task-1"].requests["mcp:write-1"].receiptRef, "ledger:receipt-1");
    const stored = await readFile(join(root, "recovery.json"), "utf8");
    assert.equal(stored.includes("SECRET_RESULT_INPUT"), false);
    assert.equal(stored.includes("SECRET_RESULT_BODY"), false);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("recovery coordinator delegates reconciliation and preserves unknown outcomes", async () => {
  const root = await mkdtemp(join(tmpdir(), "agent-engine-recovery-coordinator-"));
  try {
    const index = new RecoveryIndex(join(root, "recovery.json"));
    await index.beginRun({ runId: "run-1", planId: "plan-1", threadId: "thread-1", projectId: "project-1" });
    await index.recordRequest("run-1", { requestId: "write-committed", kind: "write", operation: "persist_record", requestHash: "hash-1", state: "uncertain" });
    await index.recordRequest("run-1", { requestId: "write-unknown", kind: "write", operation: "persist_record", requestHash: "hash-2", state: "uncertain" });
    const reconciled = [];
    const contract = testOperationContract({
      reconcile(candidate, context) {
        reconciled.push({ candidate, context });
        return candidate.requestId === "write-committed"
          ? { outcome: "committed", receiptRef: "ledger:receipt-1" }
          : { outcome: "unknown", receiptRef: null };
      },
    });
    const coordinator = new RecoveryCoordinator({ index, operationContract: contract });
    const first = coordinator.reconcileRun("run-1", { context: { ledger: "domain-ledger" }, runtimeStatus: "cancelled", fence: "fence-1" });
    const second = coordinator.reconcileRun("run-1", { context: { ledger: "ignored-duplicate" } });
    assert.equal(first, second);
    const result = await first;
    assert.equal(result.status, "outcome-unknown");
    assert.deepEqual(result.unresolved, [{ requestId: "write-unknown", operation: "persist_record" }]);
    assert.equal(reconciled.length, 2);
    assert.equal(reconciled[0].context.ledger, "domain-ledger");
    const document = await index.read();
    assert.equal(document.runs["run-1"].requests["write-committed"].outcome, "committed");
    assert.equal(document.runs["run-1"].requests["write-unknown"].outcome, "unknown");
    assert.equal(document.runs["run-1"].recoveryFence, "fence-1");
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("recovery index serializes concurrent updates", async () => {
  const root = await mkdtemp(join(tmpdir(), "agent-engine-recovery-concurrency-"));
  try {
    const index = new RecoveryIndex(join(root, "recovery.json"));
    await index.beginRun({ runId: "run-1", planId: "plan-1", threadId: "thread-1" });
    await Promise.all(Array.from({ length: 12 }, (_, indexValue) => index.recordRequest("run-1", { requestId: `request-${indexValue}` })));
    const document = await index.read();
    assert.equal(Object.keys(document.runs["run-1"].requests).length, 12);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("task budgets cap request trees, tokens, and duration", () => {
  const budget = new TaskBudget({ maxRequests: 2, maxTokens: 10, maxDurationMs: 100, startedAt: 1_000 });
  const first = budget.reserveRequest({ estimatedInputTokens: 2, maxOutputTokens: 2 }, 1_010);
  const second = budget.reserveRequest({ estimatedInputTokens: 1, maxOutputTokens: 2 }, 1_020);
  assert.throws(() => budget.reserveRequest({}, 1_030), /request budget/);
  budget.settleReservation(first, { total_tokens: 3 });
  budget.settleReservation(second, null, { outcome: "unknown" });
  assert.deepEqual({ ...budget.snapshot(), id: "[BOUND]" }, { id: "[BOUND]", requests: 2, tokens: 3, unknownTokens: 3, reservedTokens: 0, maxRequests: 2, maxTokens: 10, maxDurationMs: 100, startedAt: 1_000, overrun: false });
  const repeated = budget.settleReservation(first, { total_tokens: 9 });
  assert.equal(repeated.status, "settled");
  assert.equal(budget.snapshot().tokens, 3);
  const tokenBudget = new TaskBudget({ maxRequests: 4, maxTokens: 4, maxDurationMs: 100, startedAt: 1_000 });
  assert.throws(() => tokenBudget.reserveRequest({ estimatedInputTokens: 2, maxOutputTokens: 3 }, 1_040), /token budget/);
  const expired = new TaskBudget({ maxDurationMs: 10, startedAt: 1_000 });
  assert.throws(() => expired.reserveRequest({}, 1_010), /duration budget/);
  const overrun = new TaskBudget({ maxRequests: 2, maxTokens: 5 });
  const reservation = overrun.reserveRequest({ estimatedInputTokens: 1, maxOutputTokens: 1 });
  assert.equal(overrun.settleReservation(reservation, { total_tokens: 6 }).status, "overrun");
  assert.throws(() => overrun.reserveRequest({}), /token budget/);
});

test("persistent host budgets recover only outstanding reservations as unknown", async () => {
  const root = await mkdtemp(join(tmpdir(), "agent-engine-budget-"));
  const databasePath = join(root, "engine.sqlite3");
  try {
    const budget = new PersistentTaskBudget({ databasePath, runId: "run-1", maxRequests: 4, maxTokens: 100, startedAt: 1_000_000, maxDurationMs: 100_000 });
    const settled = budget.reserveRequest({ estimatedInputTokens: 5, maxOutputTokens: 5 }, 1_000_001);
    budget.settleReservation(settled, { total_tokens: 7 });
    budget.reserveRequest({ estimatedInputTokens: 11, maxOutputTokens: 13 }, 1_000_002);
    assert.deepEqual(PersistentTaskBudget.recoverOrphans(databasePath), []);
    budget.close();

    assert.deepEqual(PersistentTaskBudget.recoverOrphans(databasePath), [{ budgetId: budget.id, unknownTokens: 24, overrun: false }]);
    assert.deepEqual(PersistentTaskBudget.recoverOrphans(databasePath), []);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
