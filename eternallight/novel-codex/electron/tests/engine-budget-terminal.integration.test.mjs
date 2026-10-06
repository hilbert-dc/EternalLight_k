import assert from "node:assert/strict";
import { createServer } from "node:http";
import { mkdtemp, mkdir, readdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { ModelBridge, TaskBudget, compileCodexConfig, compileCodexModelCatalog, createExecutionLease,
  defineConnection, resolveSessionPlan } from "@eternallight/agent-engine";
import { verifiedBaselineEvidence } from "@eternallight/agent-engine/test-support";
import { CodexAppServerAdapter } from "../codex-adapter.mjs";

const environment = Object.fromEntries(["PATH", "PATHEXT", "SystemRoot", "WINDIR", "ComSpec", "TEMP", "TMP", "TMPDIR", "LANG", "LC_ALL"]
  .filter(key => typeof process.env[key] === "string" && process.env[key]).map(key => [key, process.env[key]]));

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
  adapter = new CodexAppServerAdapter({ command: [codex, "app-server"], cwd: root, env: { ...environment,
    CODEX_HOME: root, CODEX_SQLITE_HOME: join(root, "sqlite"), AGENT_ENGINE_BRIDGE_TOKEN: address.token }, model: connection.id });
  const events = [];
  adapter.setCallbacks({ onEvent: event => { if (event.method === "error") events.push({ willRetry: event.params?.willRetry }); } });
  await assert.rejects(adapter.sendTurn([{ type: "text", text: "Say hello." }], { disableEnvironment: true }), /400|task request budget exhausted/);
  assert.deepEqual(observations.map(({ errorCategory, errorCode, errorStatusCode }) => ({ errorCategory, errorCode, errorStatusCode })),
    [{ errorCategory: "budget", errorCode: "task_request_budget_exhausted", errorStatusCode: 400 }]);
  assert.ok(events.every(event => event.willRetry !== true));
  assert.equal(budget.snapshot().requests, 1);
});

for (const diagnostic of ["provider_output_limit", "invalid_tool_arguments"]) {
test(`real Codex treats ${diagnostic} as terminal instead of retrying a disconnected stream`, { timeout: 30000 }, async t => {
  const root = await mkdtemp(join(tmpdir(), "novel-output-terminal-"));
  let calls = 0;
  const provider = createServer((request, response) => {
    const chunks = [];
    request.on("data", chunk => chunks.push(chunk));
    request.on("end", () => {
      calls++;
      const body = JSON.parse(Buffer.concat(chunks));
      const usage = { input_tokens: 3, output_tokens: 2, total_tokens: 5 };
      const tool = body.tools.find(tool => tool.type === "function" && tool.parameters?.required?.length);
      assert.ok(tool, "The real Core request must expose a tool with required arguments");
      const invalid = { type: "function_call", id: "invalid-item", call_id: "must-not-execute", name: tool.name, arguments: "{}" };
      const events = diagnostic === "invalid_tool_arguments" ? [
        { type: "response.created", response: { id: "invalid-arguments" } },
        { type: "response.output_item.done", output_index: 0, item: invalid },
        { type: "response.completed", response: { id: "invalid-arguments", status: "completed", output: [invalid], usage } },
      ] : [
        { type: "response.created", response: { id: "limited" } },
        { type: "response.incomplete", response: { id: "limited", status: "incomplete", output: [],
          incomplete_details: { reason: "max_output_tokens" }, usage } },
      ];
      response.writeHead(200, { "content-type": "text/event-stream" });
      response.end(events.map(event => `data: ${JSON.stringify(event)}\n\n`).join(""));
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
  adapter = new CodexAppServerAdapter({ command: [codex, "app-server"], cwd: root, env: { ...environment,
    CODEX_HOME: root, CODEX_SQLITE_HOME: join(root, "sqlite"), AGENT_ENGINE_BRIDGE_TOKEN: address.token }, model: connection.id });
  const errors = [], items = [];
  adapter.setCallbacks({ onEvent: event => {
    if (event.method === "error") errors.push(event.params);
    if (event.method === "item/started" || event.method === "item/completed") items.push(event.params?.item);
  } });
  await assert.rejects(adapter.sendTurn([{ type: "text", text: "Say hello." }], { disableEnvironment: true }), new RegExp(diagnostic));
  assert.equal(calls, 1);
  assert.equal(budget.snapshot().requests, 1);
  assert.equal(budget.snapshot().reservedTokens, 0);
  assert.ok(errors.every(error => error.willRetry !== true));
  assert.deepEqual(observations.map(({ errorCode, retryable }) => ({ errorCode, retryable })),
    [{ errorCode: diagnostic, retryable: false }]);
  assert.equal(budget.snapshot().tokens, 5);
  assert.equal(budget.snapshot().unknownTokens, 0);
  assert.equal(JSON.stringify(items).includes("must-not-execute"), false);
  assert.equal(items.some(item => ["commandExecution", "mcpToolCall"].includes(item?.type)), false);
  await adapter.finishTaskOperation();
  const rollouts = (await readdir(join(root, "sessions"), { recursive: true })).filter(path => path.endsWith(".jsonl"));
  assert.equal(rollouts.length, 1);
  const history = (await readFile(join(root, "sessions", rollouts[0]), "utf8")).trim().split("\n").map(line => JSON.parse(line));
  assert.equal(history.some(entry => entry.type === "response_item"
    && ["function_call", "custom_tool_call"].includes(entry.payload?.type)), false);
});

}
