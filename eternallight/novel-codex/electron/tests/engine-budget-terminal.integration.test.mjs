import assert from "node:assert/strict";
import { createServer } from "node:http";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { ModelBridge, TaskBudget, compileCodexConfig, compileCodexModelCatalog, createExecutionLease,
  defineConnection, resolveSessionPlan } from "@eternallight/agent-engine";
import { verifiedBaselineEvidence } from "@eternallight/agent-engine/test-support";
import { CodexAppServerAdapter } from "../codex-adapter.mjs";

test("real Codex terminates a budget-denied turn without HTTP or stream retry", { timeout: 30000 }, async t => {
  const root = await mkdtemp(join(tmpdir(), "novel-budget-terminal-"));
  const connection = defineConnection({ id: "fixture", provider: "fixture", model: "fixture-v1",
    endpoint: "http://127.0.0.1:9", credentialRef: "FIXTURE_KEY", protocol: "chat-completions",
    declaredLimits: { contextTokens: 131072, outputTokens: 8192 } });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: {} });
  const observations = [];
  const bridge = new ModelBridge({ sessionPlan, connections: [connection], credentialResolver: async () => "fixture",
    responseObserver: value => observations.push(value) });
  let adapter;
  t.after(async () => { if (adapter) { await adapter.finishTaskOperation(); adapter.close(); }
    await bridge.close(); await rm(root, { recursive: true, force: true }); });
  const address = await bridge.start();
  const budget = new TaskBudget({ maxRequests: 1, maxTokens: 10000 });
  const reservation = budget.reserveRequest({ estimatedInputTokens: 1 });
  budget.settleReservation(reservation, { total_tokens: 1 });
  await bridge.beginTaskBudget(budget);
  await bridge.beginExecutionLease(createExecutionLease({ sessionPlan, runId: "budget-terminal", budgetId: budget.id }));
  const catalog = join(root, "catalog.json");
  await mkdir(join(root, "sqlite"));
  await writeFile(catalog, JSON.stringify(compileCodexModelCatalog(sessionPlan, { baseInstructions: "Answer briefly." })));
  await writeFile(join(root, "config.toml"), compileCodexConfig({ sessionPlan, modelCatalogPath: catalog,
    bridge: { baseUrl: address.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" } }));
  const codex = fileURLToPath(new URL(`../../../../codex-rs/target/debug/codex${process.platform === "win32" ? ".exe" : ""}`, import.meta.url));
  adapter = new CodexAppServerAdapter({ command: [codex, "app-server"], cwd: root, env: { PATH: process.env.PATH,
    CODEX_HOME: root, CODEX_SQLITE_HOME: join(root, "sqlite"), AGENT_ENGINE_BRIDGE_TOKEN: address.token }, model: connection.id });
  const events = [];
  adapter.setCallbacks({ onEvent: event => { if (event.method === "error") events.push({ willRetry: event.params?.willRetry }); } });
  await assert.rejects(adapter.sendTurn([{ type: "text", text: "Say hello." }], { disableEnvironment: true }), /400|task request budget exhausted/);
  assert.deepEqual(observations.map(({ errorCategory, errorCode, errorStatusCode }) => ({ errorCategory, errorCode, errorStatusCode })),
    [{ errorCategory: "budget", errorCode: "task_request_budget_exhausted", errorStatusCode: 400 }]);
  assert.ok(events.every(event => event.willRetry !== true));
  assert.equal(budget.snapshot().requests, 1);
});

test("real Codex treats a provider output limit as terminal instead of retrying a disconnected stream", { timeout: 30000 }, async t => {
  const root = await mkdtemp(join(tmpdir(), "novel-output-terminal-"));
  let calls = 0;
  const provider = createServer((request, response) => {
    request.resume();
    request.on("end", () => {
      calls++;
      response.writeHead(200, { "content-type": "text/event-stream" });
      response.end([
        { type: "response.created", response: { id: "limited" } },
        { type: "response.incomplete", response: { id: "limited", status: "incomplete", output: [],
          incomplete_details: { reason: "max_output_tokens" }, usage: { input_tokens: 3, output_tokens: 2, total_tokens: 5 } } },
      ].map(event => `data: ${JSON.stringify(event)}\n\n`).join(""));
    });
  });
  await new Promise(resolve => provider.listen(0, "127.0.0.1", resolve));
  const connection = defineConnection({ id: "fixture", provider: "fixture", model: "fixture-v1",
    endpoint: `http://127.0.0.1:${provider.address().port}`, credentialRef: "FIXTURE_KEY", protocol: "responses",
    declaredLimits: { contextTokens: 131072, outputTokens: 8192 } });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: {} });
  const observations = [];
  const bridge = new ModelBridge({ sessionPlan, connections: [connection], credentialResolver: async () => "fixture",
    responseObserver: value => observations.push(value) });
  let adapter;
  t.after(async () => {
    if (adapter) { await adapter.finishTaskOperation(); adapter.close(); }
    await bridge.close();
    await new Promise(resolve => { provider.closeAllConnections(); provider.close(resolve); });
    await rm(root, { recursive: true, force: true });
  });
  const address = await bridge.start();
  const budget = new TaskBudget({ maxRequests: 8, maxTokens: 200000 });
  await bridge.beginTaskBudget(budget);
  await bridge.beginExecutionLease(createExecutionLease({ sessionPlan, runId: "output-terminal", budgetId: budget.id }));
  const catalog = join(root, "catalog.json");
  await mkdir(join(root, "sqlite"));
  await writeFile(catalog, JSON.stringify(compileCodexModelCatalog(sessionPlan, { baseInstructions: "Answer briefly." })));
  await writeFile(join(root, "config.toml"), compileCodexConfig({ sessionPlan, modelCatalogPath: catalog,
    bridge: { baseUrl: address.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" } }));
  const codex = fileURLToPath(new URL(`../../../../codex-rs/target/debug/codex${process.platform === "win32" ? ".exe" : ""}`, import.meta.url));
  adapter = new CodexAppServerAdapter({ command: [codex, "app-server"], cwd: root, env: { PATH: process.env.PATH,
    CODEX_HOME: root, CODEX_SQLITE_HOME: join(root, "sqlite"), AGENT_ENGINE_BRIDGE_TOKEN: address.token }, model: connection.id });
  const errors = [];
  adapter.setCallbacks({ onEvent: event => { if (event.method === "error") errors.push(event.params); } });
  await assert.rejects(adapter.sendTurn([{ type: "text", text: "Say hello." }], { disableEnvironment: true }), /provider_output_limit/);
  assert.equal(calls, 1);
  assert.equal(budget.snapshot().requests, 1);
  assert.equal(budget.snapshot().reservedTokens, 0);
  assert.ok(errors.every(error => error.willRetry !== true));
  assert.deepEqual(observations.map(({ errorCode, retryable }) => ({ errorCode, retryable })),
    [{ errorCode: "provider_output_limit", retryable: false }]);
});
