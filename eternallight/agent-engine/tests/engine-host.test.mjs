import assert from "node:assert/strict";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { AgentEngineHost, CAPABILITIES, CodexAppServerClient, defineConnection, resolveSessionPlan } from "../src/index.mjs";
import { trustedTestEvidence, verifiedBaselineEvidence } from "../src/testing/index.mjs";

const fixture = fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url));
const connection = defineConnection({ id: "fixture", provider: "Fixture", endpoint: "https://fixture.invalid", model: "fixture", credentialRef: "FIXTURE_API_KEY", protocol: "responses", capabilitySupport: { text: "native", structuredToolCalls: "native" }, declaredLimits: { contextTokens: 32_000, outputTokens: 2_000 } });
const evidence = verifiedBaselineEvidence(connection, { capabilities: [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.TOOL_LOOP] });

function appServerRuntime() {
  return new CodexAppServerClient({ command: [process.execPath, fixture], model: "fixture" });
}

function host(runtimeFactory = async () => appServerRuntime()) {
  return new AgentEngineHost({ connections: [connection], evidenceProvider: async () => trustedTestEvidence(evidence), qualifier: async (request) => ({ request, status: "qualified" }), runtimeFactory });
}

test("engine host refuses raw evidence records at its production boundary", async () => {
  const engine = new AgentEngineHost({ connections: [connection], evidenceProvider: async () => evidence, runtimeFactory: async () => appServerRuntime() });
  await assert.rejects(engine.resolveSession({ policy: { defaultConnectionId: "fixture", capabilityProfileId: "text-v1" } }), /trusted evidence snapshot/);
});

async function session(engine, profile = "base-tools-v1", sessionId = "session-1") {
  const plan = await engine.resolveSession({ policy: { defaultConnectionId: "fixture", capabilityProfileId: profile } });
  return { plan, started: await engine.startSession({ sessionPlan: plan, sessionId }) };
}

test("engine host reuses a session across independently leased runs", async () => {
  const engine = host();
  const inspected = await engine.inspectConnection({ connectionId: "fixture", policy: { credentialScopes: { fixture: "FIXTURE_API_KEY" } } });
  assert.equal(inspected.status, "available");
  const { started } = await session(engine);
  for (const runId of ["run-1", "run-2"]) {
    const run = await engine.startRun({ sessionId: started.sessionId, input: [{ type: "text", text: runId }], budgetId: `budget-${runId}`, runId });
    assert.equal((await run.completion).status, "completed");
    assert.equal(engine.runStatus(runId).sessionId, started.sessionId);
    engine.closeRun(runId);
  }
  assert.equal(engine.sessionStatus(started.sessionId).state, "idle");
  await engine.closeSession(started.sessionId);
});

test("engine host revokes a run before interrupting its active turn", async () => {
  const engine = host();
  const { started } = await session(engine);
  const run = await engine.startRun({ sessionId: started.sessionId, input: [{ type: "text", text: "interrupt" }], budgetId: "budget-2", runId: "run-2" });
  await new Promise((resolve) => engine.subscribeEvents({ runId: "run-2" }, (event) => { if (event.method === "turn/started") resolve(); }));
  const cancelled = await engine.cancelRun("run-2", "test-cancel");
  assert.equal(cancelled.revoked, true);
  assert.equal((await run.completion).status, "cancelled");
  assert.equal(engine.runStatus("run-2").state, "cancelled");
  engine.closeRun("run-2");
  await engine.closeSession(started.sessionId);
});

test("engine host resumes a session, reconciles first, and can start a new run", async () => {
  const engine = host();
  const plan = resolveSessionPlan({ connections: [connection], evidence, policy: { defaultConnectionId: "fixture", capabilityProfileId: "text-v1" } });
  let reconciled = false;
  const resumed = await engine.resumeSession({ sessionPlan: plan, threadId: "thread-existing", sessionId: "resumed", recovery: { reconcile: async () => { reconciled = true; return { canContinue: true, unresolvedRequests: [] }; } } });
  assert.equal(reconciled, true);
  const run = await engine.startRun({ sessionId: resumed.sessionId, input: [{ type: "text", text: "continue" }], budgetId: "budget-3", runId: "run-3" });
  assert.equal((await run.completion).status, "completed");
  assert.equal((await engine.qualifyCapabilities({ connectionId: "fixture" })).status, "qualified");
  engine.closeRun("run-3");
  await engine.closeSession(resumed.sessionId);
});

test("engine host reserves a run id before binding execution", async () => {
  let releaseBinding;
  const barrier = new Promise((resolve) => { releaseBinding = resolve; });
  let bindings = 0;
  const runtime = appServerRuntime();
  runtime.bindExecution = async () => { bindings += 1; await barrier; };
  const engine = host(async () => runtime);
  const { started } = await session(engine, "text-v1", "reserved-session");
  const first = engine.startRun({ sessionId: started.sessionId, input: [{ type: "text", text: "hello" }], budgetId: "budget", runId: "reserved" });
  await assert.rejects(engine.startRun({ sessionId: started.sessionId, input: [{ type: "text", text: "duplicate" }], budgetId: "budget", runId: "reserved" }), /already exists/);
  releaseBinding();
  const run = await first;
  await run.completion;
  assert.equal(bindings, 1);
  engine.closeRun("reserved");
  await engine.closeSession(started.sessionId);
});

test("cancelling while binding execution prevents the turn from being sent", async () => {
  let releaseBinding;
  let bindingStarted;
  const barrier = new Promise((resolve) => { releaseBinding = resolve; });
  const ready = new Promise((resolve) => { bindingStarted = resolve; });
  let sends = 0;
  let revocations = 0;
  const runtime = {
    subscribeEvents: () => () => {}, ensureSession: async () => "thread", resumeSession: async (id) => id,
    bindExecution: async () => { bindingStarted(); await barrier; },
    sendTurn: async () => { sends += 1; return { turn: { id: "turn", status: "completed" } }; },
    revokeExecution: async ({ expectedEpoch }) => { revocations += 1; return { revokedEpoch: expectedEpoch + 1 }; },
    interruptTurn: async () => false, close: async () => {},
  };
  const engine = host(async () => runtime);
  const { started } = await session(engine, "text-v1", "preparing-session");
  const pending = engine.startRun({ sessionId: started.sessionId, input: [{ type: "text", text: "hello" }], budgetId: "budget", runId: "preparing" });
  await ready;
  const cancellation = engine.cancelRun("preparing");
  releaseBinding();
  await assert.rejects(pending, (error) => error.code === "TASK_CANCELLED");
  await cancellation;
  assert.equal(sends, 0);
  assert.equal(revocations, 1);
  assert.equal(engine.runStatus("preparing").state, "cancelled");
  engine.closeRun("preparing");
  await engine.closeSession(started.sessionId);
});

test("recovery blocks a session with unresolved writes", async () => {
  let factoryCalls = 0;
  const engine = host(async () => { factoryCalls += 1; return appServerRuntime(); });
  const plan = resolveSessionPlan({ connections: [connection], evidence, policy: { defaultConnectionId: "fixture", capabilityProfileId: "text-v1" } });
  const result = await engine.resumeSession({ sessionPlan: plan, threadId: "thread", sessionId: "blocked", recovery: { reconcile: async () => ({ canContinue: false, unresolvedRequests: [{ requestId: "write-1", status: "unknown" }] }) } });
  assert.equal(result.recovery.canContinue, false);
  assert.equal(engine.sessionStatus("blocked").state, "recovery-blocked");
  assert.equal(factoryCalls, 0);
});

test("closing a session while its runtime factory is pending closes the late runtime", async () => {
  let releaseFactory;
  const barrier = new Promise((resolve) => { releaseFactory = resolve; });
  let ensures = 0;
  let closes = 0;
  const runtime = {
    subscribeEvents: () => () => {},
    ensureSession: async () => { ensures += 1; return "thread"; },
    resumeSession: async (id) => id,
    bindExecution: async () => {}, sendTurn: async () => ({ turn: { status: "completed" } }),
    revokeExecution: async ({ revokedEpoch }) => ({ revokedEpoch }), interruptTurn: async () => false,
    close: async () => { closes += 1; },
  };
  const engine = host(async () => { await barrier; return runtime; });
  const plan = resolveSessionPlan({ connections: [connection], evidence, policy: { defaultConnectionId: "fixture", capabilityProfileId: "text-v1" } });
  const starting = engine.startSession({ sessionPlan: plan, sessionId: "closing-during-factory" });
  await engine.closeSession("closing-during-factory");
  releaseFactory();
  await assert.rejects(starting, (error) => error.code === "TASK_CANCELLED");
  assert.equal(ensures, 0);
  assert.equal(closes, 1);
});

test("outcome-unknown blocks the session from starting another run", async () => {
  const runtime = {
    subscribeEvents: () => () => {}, ensureSession: async () => "thread", resumeSession: async (id) => id,
    bindExecution: async () => {}, sendTurn: async () => new Promise(() => {}),
    revokeExecution: async ({ revokedEpoch }) => ({ revokedEpoch }), interruptTurn: async () => true,
    reconcileRun: async () => ({ outcome: "outcome-unknown", unresolvedOperations: [{ id: "write" }] }), close: async () => {},
  };
  const engine = host(async () => runtime);
  const { started } = await session(engine, "text-v1", "unknown-session");
  await engine.startRun({ sessionId: started.sessionId, input: [], budgetId: "budget-1", runId: "unknown-run" });
  await engine.cancelRun("unknown-run");
  assert.equal(engine.sessionStatus(started.sessionId).state, "recovery-blocked");
  await assert.rejects(engine.startRun({ sessionId: started.sessionId, input: [], budgetId: "budget-2", runId: "blocked-run" }), /not idle/);
});
