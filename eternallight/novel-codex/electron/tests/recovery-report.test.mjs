import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { HostRecoveryJournal, RecoveryIndex } from "@eternallight/agent-engine";
import { mainHandlers, rendererEvent, runtimeFixture } from "./main-handler-harness.mjs";

const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;
async function pending(index) {
  const journal = new HostRecoveryJournal(index);
  await journal.beginRun({ runId: "saved-task", planId: "plan", threadId: "root", projectId: "project" });
  await journal.observe({ method: "item/started", params: { threadId: "root", turnId: "turn", item: {
    id: "save-call", type: "mcpToolCall", tool: "save_outline", arguments: { idempotencyKey: "same-save", payload: { nodes: [] } },
  } } });
  await journal.abandonRun();
  return (await index.read()).runs["saved-task"].requests["mcp:save-call"].requestHash;
}

test("idle recovery refresh retains unknown writes then closes only a matching authoritative receipt", async () => {
  const root = await mkdtemp(join(tmpdir(), "novel-recovery-report-"));
  const index = new RecoveryIndex(join(root, "recovery.json"));
  const hash = await pending(index);
  let mode = "offline";
  const calls = [];
  const { handlers, adapter } = await mainHandlers(async (path, body) => {
    calls.push({ path, body });
    assert.equal(path, "/api/projects/project/tasks/recovery-snapshot");
    assert.deepEqual(Array.from(body.taskIds), ["saved-task"]);
    if (mode === "offline") throw Error("controlled snapshot unavailable");
    return { fence: "fence", complete: true, nextCursor: null, watermark: 1,
      tasks: [{ id: "saved-task", status: "failed" }], operations: mode === "unknown" ? [] : [{
        operation_id: "operation", run_id: "saved-task", thread_id: "root", turn_id: "turn",
        tool_call_id: "save-call", tool_name: "save_outline", request_hash: hash,
        state: "committed", receipt_ref: "runtime:saved-revision", updated_at: 1,
      }] };
  }, command, { recoveryIndex: index });
  try {
    const report = () => handlers.get("agent:recovery-report")(rendererEvent());
    assert.equal((await report()).length, 1);
    mode = "unknown";
    assert.equal((await report()).length, 1);
    mode = "committed";
    assert.equal((await report()).length, 0);
    assert.equal(calls.length, 3);
    assert.equal(handlers.has("agent:resolve-recovery"), false, "no manual authority to dismiss unknown writes");
    assert.equal(adapter.process, null, "reconciliation never invokes the model");
  } finally { adapter.close(); await rm(root, { recursive: true, force: true }); }
});

test("active engine and generation runs are never fenced by recovery report reads", async () => {
  for (const active of [{ activeAgentExecution: { task: { id: "live" } } }, { activeGenerationRunId: "live-run" }]) {
    let fences = 0;
    const index = { unfinishedRuns: async () => { fences++; throw Error("must remain passive"); }, recoveryReport: async () => [] };
    const { handlers, adapter } = await mainHandlers(async () => { throw Error("no Runtime calls while active"); }, command, { recoveryIndex: index, ...active });
    try {
      assert.equal((await handlers.get("agent:recovery-report")(rendererEvent())).length, 0);
      assert.equal(fences, 0);
    } finally { adapter.close(); }
  }
});

test("new execution waits for an in-flight recovery snapshot and concurrent readers share its fence", async () => {
  const runtime = runtimeFixture();
  let release, entered;
  const ready = new Promise(resolve => { entered = resolve; });
  const held = new Promise(resolve => { release = resolve; });
  let fences = 0, executionRequests = 0;
  const index = {
    unfinishedRuns: async () => [{ runId: "old", projectId: "project" }],
    observeRuntimeOperations: async () => {}, reconcileRun: async () => {}, recoveryReport: async () => [],
  };
  const { handlers, adapter } = await mainHandlers(async (path, body) => {
    if (path.endsWith("/recovery-snapshot")) {
      fences++; entered(); await held;
      return { fence: "fence", complete: true, tasks: [{ id: "old", status: "failed" }], operations: [] };
    }
    executionRequests++;
    return runtime.request(path, body);
  }, command, { recoveryIndex: index });
  try {
    const one = handlers.get("agent:recovery-report")(rendererEvent());
    await ready;
    const two = handlers.get("agent:recovery-report")(rendererEvent());
    const execution = handlers.get("agent:send-prompt")(rendererEvent(), "project", "讨论故事");
    await new Promise(resolve => setTimeout(resolve, 30));
    assert.equal(executionRequests, 0);
    assert.equal(fences, 1);
    release(); await Promise.all([one, two, execution]);
    assert.ok(executionRequests > 0);
  } finally { release(); adapter.close(); }
});
