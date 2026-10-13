import assert from "node:assert/strict";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { readFile } from "node:fs/promises";
import { runInNewContext } from "node:vm";
import { CodexAppServerAdapter } from "../codex-adapter.mjs";
import { coreRpcError, coreTurnError, coreTurnFailure, isRejectedSteer } from "../core-turn-failure.mjs";
import { advanceWorkSteps } from "../work-step-continuation.mjs";
import { taskFailureDiagnostic } from "../tool-validation-recovery.mjs";

const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;

test("terminal turn failure is admitted from the matched Core event, not provider prose or copied properties", async t => {
  const adapter = new CodexAppServerAdapter({ command });
  t.after(() => adapter.close());
  let failure;
  await assert.rejects(adapter.sendTurn([{ type: "text", text: "failed-turn" }]), error => {
    failure = coreTurnFailure(error);
    return failure?.turnId === "turn-test" && failure.automaticContinuation === "stopped";
  });
  assert.equal(failure.threadId, "thr-test");
  assert.equal(coreTurnFailure({ code: "CORE_TURN_FAILED", coreTurnFailure: failure }), null);
  const fake = Object.assign(new Error("no active turn to steer"), { rpcCode: -32600 });
  assert.equal(isRejectedSteer(fake), false);
  assert.equal(isRejectedSteer(coreRpcError("thread/read", { code: -32600 })), false);
  const terminal = coreTurnError("thread", { id: "turn", error: { message: "response incomplete: max_output_tokens" } });
  assert.equal(coreTurnFailure(new Error("settlement pending", { cause: terminal })).turnId, "turn");
  assert.equal(taskFailureDiagnostic(terminal), "core_turn_failed; response incomplete: max_output_tokens");
  const misleading = coreTurnError("thread", { id: "turn", error: { message: "invalid_tool_arguments; provider prose" } });
  assert.equal(taskFailureDiagnostic(misleading).split(";")[0], "core_turn_failed");
  assert.equal(taskFailureDiagnostic(new Error("core_turn_failed; provider prose")), "execution_failed; core_turn_failed; provider prose");
});

for (const code of [-32600, -32603]) {
  test(`actual steer response ${code} distinguishes rejected input from unknown dispatch`, { timeout: 5000 }, async t => {
    const adapter = new CodexAppServerAdapter({ command, env: { ...process.env, FIXTURE_STEER_ERROR_CODE: String(code) } });
    t.after(() => adapter.close());
    const started = new Promise(resolve => {
      adapter.setCallbacks({ onEvent: event => { if (event.method === "turn/started") resolve(); } });
    });
    const lease = { runId: "run", leaseId: "lease", epoch: 1 };
    await adapter.bindExecution({ lease });
    const completion = adapter.sendTurn([{ type: "text", text: "steer" }]);
    void completion.catch(() => {});
    await started;
    const steer = adapter.steerTurn([{ type: "text", text: "keep original scope" }], {
      ...lease, expectedThreadId: adapter.threadId, expectedTurnId: adapter.turnId });
    if (code === -32600) assert.deepEqual(await steer, { steered: false, delivery: "notSent", reason: "turnRejected" });
    else await assert.rejects(steer, error => error.rpcCode === code && !isRejectedSteer(error));
    assert.ok(adapter.turnWaiter, "rejection does not invent a completed turn");
    await adapter.interruptTurn();
    await completion;
  });
}

test("product continuation cannot create a new attempt after a Core terminal failure", async () => {
  const error = coreTurnError("thread", { id: "turn", error: { message: "response incomplete" } });
  const source = await readFile(new URL("../creative-task-coordinator.mjs", import.meta.url), "utf8");
  const context = { coreTurnFailure };
  runInNewContext(source.replace(/^import .+;\n/gm, "").replace("export function ", "function ")
    + "\nglobalThis.createCoordinator = createCreativeTaskCoordinator;", context);
  const coordinator = context.createCoordinator({
    runtimeRequest: () => assert.fail("terminal failure must not dispatch recovery or touch receipts"),
  });
  await assert.rejects(coordinator.continueFailedCreativeWork({}, "project", error), value => value === error);
});

test("a subsequent Work step cannot open automatic correction after Core termination", async () => {
  const error = coreTurnError("thread", { id: "turn", error: { message: "response incomplete" } });
  const work = { id: "work", projectId: "project", threadId: "thread", status: "running",
    steps: [{ id: "step", status: "queued", operation: "advisory", target: {}, instruction: "original goal" }] };
  const reads = [];
  await assert.rejects(advanceWorkSteps({}, "project", "work", {
    runtimeRequest: async path => { reads.push(path); assert.equal(path, "/api/projects/project/work-tasks/work"); return work; },
    currentWorkFlowState: async () => "running",
    runAdvisoryAgentTask: async () => { throw error; },
    runCreativeAgentTask: async () => assert.fail("wrong operation"),
    runQueuedPlanningReview: async () => assert.fail("no planning review"),
  }), value => value === error);
  assert.deepEqual(reads, ["/api/projects/project/work-tasks/work"]);
});

test("late terminal events from a retired process cannot stop a replacement turn with the same fixture IDs", { timeout: 5000 }, async t => {
  const adapter = new CodexAppServerAdapter({ command });
  t.after(() => adapter.close());
  await adapter.ensureSession();
  const priorReader = adapter.reader;
  adapter.close();
  const started = new Promise(resolve => adapter.setCallbacks({ onEvent: event => {
    if (event.method === "turn/started") resolve();
  } }));
  const completion = adapter.sendTurn([{ type: "text", text: "steer" }]);
  void completion.catch(() => {});
  await started;
  const waiter = adapter.turnWaiter;
  priorReader.emit("line", JSON.stringify({ method: "turn/completed", params: {
    threadId: adapter.threadId, turn: { id: adapter.turnId, status: "failed", error: { message: "retired failure" } } } }));
  assert.equal(adapter.turnWaiter, waiter);
  await adapter.interruptTurn();
  await completion;
});
