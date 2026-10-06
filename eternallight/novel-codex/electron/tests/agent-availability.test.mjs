import assert from "node:assert/strict";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { CAPABILITIES } from "@eternallight/agent-engine";
import { verifiedBaselineEvidence } from "@eternallight/agent-engine/test-support";
import { assertAgentAvailability, readAgentStatus, MODEL_CONNECTION_UNAVAILABLE } from "../agent-availability.mjs";
import { inspectCodexConnection, listCodexConnections } from "../codex-runtime.mjs";
import { mainHandlers, rendererEvent } from "./main-handler-harness.mjs";

const status = { configured: true, state: "idle", error: null, provider: "deepseek", model: "deepseek-flash" };
const command = `${process.execPath} ${fileURLToPath(new URL("./fixture-app-server.mjs", import.meta.url))}`;

test("idle status and send reject the same invalid current evidence before any task or provider starts", async () => {
  let runtimeCalls = 0;
  const state = {};
  const { handlers, adapter } = await mainHandlers(async () => { runtimeCalls++; throw new Error("No Runtime expected"); }, command, state);
  try {
    assert.equal((await handlers.get("agent:status")()).configured, true);
    state.fixtureEvidence = [];
    assert.equal(adapter.status().configured, true, "the applied Adapter is still configured");
    const current = await handlers.get("agent:status")();
    assert.equal(current.configured, false);
    assert.equal(current.state, "error");
    assert.ok(current.error.startsWith(`${MODEL_CONNECTION_UNAVAILABLE}: 当前模型验证不可用或已失效`));
    assert.match(current.error, /原始诊断：[\s\S]*lacks verified capabilities/);
    await assert.rejects(handlers.get("agent:send-prompt")(rendererEvent(), "project", "请调整当前简报"),
      reason => reason.code === MODEL_CONNECTION_UNAVAILABLE && reason.message.startsWith(`${MODEL_CONNECTION_UNAVAILABLE}: `));
    assert.equal(runtimeCalls, 0);
    assert.equal(adapter.process, null, "no provider process or turn was started");
  } finally { adapter.close(); }
});

test("only full current base-tool evidence marks a connection verified", () => {
  const now = new Date("2026-09-19T12:00:00Z");
  const connection = inspectCodexConnection({ provider: "deepseek", model: "deepseek-flash" }).connection;
  const evidence = verifiedBaselineEvidence(connection, { protocol: "responses", checkedAt: "2026-09-19T00:00:00Z", expiresAt: "2026-09-20T00:00:00Z" });
  const partial = evidence.filter(record => record.capability !== CAPABILITIES.TOOL_LOOP);
  assert.equal(listCodexConnections({ evidence: partial, now })[0].status, "unverified");
  assert.equal(listCodexConnections({ evidence, now })[0].status, "verified");
  assert.equal(listCodexConnections({ evidence, now: new Date("2026-09-21T00:00:00Z") })[0].status, "unverified");
  assert.equal(listCodexConnections({ evidence, now, qualificationContextDigest: "changed-build" })[0].status, "unverified");
});

test("active execution preserves current connection state without evidence revalidation", async () => {
  const executing = { ...status, state: "running" };
  let checks = 0;
  const result = await readAgentStatus({ readStatus: () => executing, isActive: () => true,
    getOwner: () => ({}), check: async () => { checks++; throw new Error("expired"); } });
  assert.deepEqual(result, executing);
  assert.equal(checks, 0);
});

test("a changed applied plan requires explicit application and never mutates the Adapter", async () => {
  const state = {};
  const { handlers, adapter, providerConfig } = await mainHandlers(async () => { throw new Error("No Runtime expected"); }, command, state);
  try {
    providerConfig.sessionPlan = { ...providerConfig.sessionPlan, id: "old-plan" };
    const result = await handlers.get("agent:status")();
    assert.equal(result.configured, false);
    assert.match(result.error, /模型能力证据或路由计划已经变化/);
    assert.equal(providerConfig.sessionPlan.id, "old-plan");
    assert.equal(adapter.status().configured, true);
    assert.equal(adapter.process, null);
  } finally { adapter.close(); }
});

test("a successful current check preserves ordinary task failure and rejects a late configuration owner", async () => {
  let owner = {}, release;
  const taskFailure = { ...status, state: "error", error: "task failed" };
  assert.deepEqual(await readAgentStatus({ readStatus: () => taskFailure, isActive: () => false,
    getOwner: () => owner, check: async () => true }), taskFailure);
  const pending = readAgentStatus({ readStatus: () => status, isActive: () => false,
    getOwner: () => owner, check: () => new Promise(resolve => { release = resolve; }) });
  owner = {};
  release();
  const late = await pending;
  assert.equal(late.configured, false);
  assert.match(late.error, /读取期间模型配置已变化/);
});

test("the local classification keeps bounded diagnostics and does not classify a provider turn", async () => {
  const unavailable = await assertAgentAvailability(async () => { throw new Error("x".repeat(3000)); }).catch(reason => reason);
  assert.equal(unavailable.code, MODEL_CONNECTION_UNAVAILABLE);
  assert.equal(unavailable.message.split("原始诊断：")[1].length, 2000);
  const disabled = { ...status, configured: false, error: "configuration unavailable" };
  let checks = 0;
  const result = await readAgentStatus({ readStatus: () => disabled, isActive: () => false,
    getOwner: () => null, check: async () => { checks++; } });
  assert.equal(result.configured, false);
  assert.ok(result.error.startsWith(`${MODEL_CONNECTION_UNAVAILABLE}: `));
  assert.equal(checks, 0);
});
