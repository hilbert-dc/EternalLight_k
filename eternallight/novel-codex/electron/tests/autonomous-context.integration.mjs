import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { ModelBridgeProcess, TaskBudget, compileCodexConfig, compileCodexModelCatalog,
  createExecutionLease, defineConnection, resolveSessionPlan } from "@eternallight/agent-engine";
import { verifiedBaselineEvidence } from "@eternallight/agent-engine/test-support";
import { CodexAppServerAdapter } from "../codex-adapter.mjs";
import { composeNovelCodexPrompt, buildCodexModelCatalog } from "../codex-runtime.mjs";
import { NOVEL_CONTEXT_MAINTENANCE } from "../context-maintenance.mjs";
import { NOVEL_ROOT_DEVELOPER_INSTRUCTIONS, NOVEL_DURABLE_WORK_INSTRUCTIONS } from "../creative-mission.mjs";

// Real App Server and native bridge with a deterministic local provider. This
// verifies wire/history/compaction isolation, not literary model quality.
for (const mode of ["explicit", "automatic", "overflow", "efficiency", "working-set", "handoff"]) {
const automatic = mode === "automatic" || mode === "efficiency";
const handoff = mode === "handoff";
const workingSet = mode === "working-set";
const efficiency = mode === "efficiency" || workingSet || handoff;
test(`contextual task material survives ${mode} compaction without becoming retained author input`, { timeout: 35000 }, async t => {
  const directory = await mkdtemp(join(tmpdir(), "novel-autonomous-context-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const requests = [], requestKinds = [], failures = [];
  const server = createServer(async (request, response) => {
    const chunks = [];
    for await (const chunk of request) chunks.push(chunk);
    requests.push(JSON.parse(Buffer.concat(chunks)));
    requestKinds.push(JSON.parse(request.headers["x-codex-turn-metadata"] ?? requests.at(-1).client_metadata?.["x-codex-turn-metadata"] ?? "{}").request_kind);
    response.writeHead(200, { "content-type": "text/event-stream" });
    const index = requests.length;
    const answer = { type: "message", id: `context-answer-${index}`, role: "assistant", status: "completed",
      content: [{ type: "output_text", text: "当前步骤已结束；后续仍需依据持久任务状态。", annotations: [] }] };
    const send = event => response.write(`data: ${JSON.stringify(event)}\n\n`);
    send({ type: "response.created", response: { id: `context-response-${index}` } });
    send({ type: "response.output_item.done", output_index: 0, item: answer });
    const inputTokens = handoff ? (index === 1 ? 40000 : index === 2 ? 60000 : 10) : workingSet ? (index < 3 ? 150000 : index === 3 ? 260000 : 10) : automatic && index === 2 ? (efficiency ? 110000 : 26000) : 10;
    send({ type: "response.completed", response: { id: `context-response-${index}`, status: "completed", output: [answer],
      usage: { input_tokens: inputTokens, output_tokens: 10, total_tokens: inputTokens + 10 } } });
    response.end("data: [DONE]\n\n");
  });
  await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
  t.after(() => new Promise(resolve => { server.closeAllConnections(); server.close(resolve); }));
  const connection = defineConnection({ id: "fixture", provider: "fixture", model: "fixture", protocol: "responses",
    endpoint: `http://127.0.0.1:${server.address().port}`, credentialRef: "FIXTURE", protocolOptions: { reasoningEffort: "none" },
    declaredLimits: { contextTokens: efficiency ? 1048576 : 64000, outputTokens: 8192 } });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: {} });
  const bridge = new ModelBridgeProcess({ sessionPlan, connections: [connection], credentialResolver: async () => "fixture",
    responseObserver: observation => { if (observation.status === "failed") failures.push(observation); } });
  t.after(() => bridge.close());
  const budget = new TaskBudget({ maxRequests: 8, maxTokens: efficiency ? 2000000 : 64000 });
  const address = await bridge.start();
  await bridge.beginTaskBudget(budget);
  await bridge.beginExecutionLease(createExecutionLease({ sessionPlan, runId: "context-test", budgetId: budget.id }));
  const catalog = join(directory, "models.json");
  await writeFile(catalog, JSON.stringify(efficiency ? buildCodexModelCatalog({ sessionPlan, baseInstructions: "Fixture model base policy." }) : compileCodexModelCatalog(sessionPlan, { baseInstructions: "Fixture model base policy." })));
  await writeFile(join(directory, "config.toml"), compileCodexConfig({ sessionPlan, modelCatalogPath: catalog,
    bridge: { baseUrl: address.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" }, mcpServers: [],
    ...(efficiency || mode === "overflow" ? { autoCompactTokenLimitScope: NOVEL_CONTEXT_MAINTENANCE.autoCompactTokenLimitScope } : {}),
    ...(handoff ? { postTurnCompactThresholdPercent: NOVEL_CONTEXT_MAINTENANCE.postTurnCompactThresholdPercent } : {}),
    developerInstructions: NOVEL_ROOT_DEVELOPER_INSTRUCTIONS }));
  const executable = process.platform === "win32" ? "codex.exe" : "codex";
  const codex = fileURLToPath(new URL(`../../../../codex-rs/target/debug/${executable}`, import.meta.url));
  const environment = Object.fromEntries(["PATH", "PATHEXT", "SystemRoot", "WINDIR", "ComSpec", "TEMP", "TMP", "TMPDIR", "LANG", "LC_ALL",
    "SSL_CERT_FILE", "SSL_CERT_DIR", "HTTP_PROXY", "HTTPS_PROXY", "NO_PROXY"].filter(key => typeof process.env[key] === "string").map(key => [key, process.env[key]]));
  const adapter = new CodexAppServerAdapter({ contextMaintenanceTokenTarget: handoff ? NOVEL_CONTEXT_MAINTENANCE.contextMaintenanceTokenTarget : null, command: [codex, "app-server"], cwd: directory, modelRoute: sessionPlan.defaultModel,
    env: { ...environment, CODEX_HOME: directory, CODEX_SQLITE_HOME: join(directory, "sqlite"), AGENT_ENGINE_BRIDGE_TOKEN: address.token } });
  t.after(() => adapter.close());
  const terminalTurns = [];
  adapter.callbacks.onRawEvent = event => { if (event.method === "turn/completed") terminalTurns.push(event.params.turn); };
  const author = "请在本次授权范围内完成作品，独立检查后继续。";
  const send = async turn => adapter.sendTurn(turn.input, { disableEnvironment: true, additionalContext: turn.additionalContext });
  await send(composeNovelCodexPrompt("METHOD_DATA_A", author, "按当前任务职责执行。", { binding: { quote: "SOURCE_DATA_A" + (efficiency ? "REQUIRED_PRIOR_SOURCE ".repeat(8000) : ""), taskId: "first" } }));
  const rootThread = adapter.threadId;
  if (handoff) {
    assert.equal(requestKinds.filter(kind => kind === "compaction").length, 0);
    await adapter.close();
    await adapter.openThread({ resumeId: rootThread });
  }
  await send(composeNovelCodexPrompt("METHOD_DATA_B", author, "按当前任务职责执行。", { internal: true, binding: { quote: "SOURCE_DATA_B", taskId: "second" } }));
  const before = await adapter.readThread(rootThread);
  const actualUsers = before.turns.flatMap(turn => turn.items).filter(item => item.type === "userMessage").map(item => item.text);
  assert.equal(actualUsers.filter(text => text === author).length, 1);
  assert.ok(actualUsers.every(text => !text.includes("SOURCE_DATA_") && !text.includes("METHOD_DATA_")), "task materials must not appear as author messages");
  if (workingSet) {
    assert.equal(requestKinds.filter(kind => kind === "compaction").length, 0,
      "initial working set above the soft interval must remain available");
    await send(composeNovelCodexPrompt("METHOD_GROWTH", author, "继续本任务核对。", {
      internal: true, binding: { quote: "GROWTH_DATA", taskId: "growth" },
    }));
  }
  if (handoff) {
    assert.equal(requestKinds.filter(kind => kind === "compaction").length, 0,
      "completed one-shot work must not pay for an unused summary");
    assert.equal(terminalTurns.at(-1)?.status, "completed");
    await adapter.close();
    await adapter.openThread({ resumeId: rootThread });
  }
  if (mode === "explicit") await adapter.compactThread(rootThread);
  if (mode === "overflow") {
    const dispatched = requests.length;
    await adapter.sendTurn([{ type: "text", text: "内部容量边界测试" }], {
      disableEnvironment: true, additionalContext: { fixtureOversize: { kind: "untrusted", value: "临时资料核对。".repeat(10000) } },
    });
    assert.equal(terminalTurns.at(-1)?.status, "completed");
    assert.ok(failures.length >= 1, "fixture must encounter actual wire admission rejection");
    assert.equal(failures[0].errorCode, "context_input_budget_exceeded");
    assert.equal(requests.length, dispatched + 2, "only the summary and resumed sampling reached the provider");
    assert.equal(budget.snapshot().requests, requests.length);
    assert.equal(requestKinds.filter(kind => kind === "compaction").length, 1, "no caller retry or explicit compact request");
  }
  await send(composeNovelCodexPrompt("METHOD_DATA_C", author, "按当前任务职责执行。", { internal: true, binding: { quote: "SOURCE_DATA_C", taskId: "third" } }));
  assert.equal(requestKinds.filter(kind => kind === "compaction").length, 1,
    "actual Core maintains prior history once before the next input under the original lease");
  const latest = requests.at(-1);
  const text = latest.input.flatMap(item => item.content ?? []).map(part => part.text ?? "").join("\n");
  assert.equal(text.includes("SOURCE_DATA_A"), false, "compaction must not retain the prior contextual task package as a user message");
  // Core may retire the completed context or retain it once as the latest
  // continuity package. Neither outcome may duplicate or promote it to author input.
  assert.ok(text.split("SOURCE_DATA_B").length - 1 <= (handoff ? 1 : 0));
  assert.equal(text.split("SOURCE_DATA_C").length - 1, 1, "the current frozen task is injected once");
  assert.equal(text.includes(NOVEL_DURABLE_WORK_INSTRUCTIONS), true, "durable responsibility remains after compaction");
  if (efficiency) {
    const compactIndex = requestKinds.indexOf("compaction");
    const beforeBytes = Math.max(...requests.slice(0, compactIndex).map(value => Buffer.byteLength(JSON.stringify(value))));
    const afterBytes = Buffer.byteLength(JSON.stringify(requests.at(-1)));
    assert.ok(beforeBytes > 150000, "necessary large source is admitted, not clipped by the maintenance target");
    t.diagnostic(`efficiency fixture wire bytes: ${beforeBytes} -> ${afterBytes}`);
    assert.ok(afterBytes < beforeBytes / 2, "retired task material is not retransmitted after automatic maintenance");
    assert.equal(budget.snapshot().unknownTokens, 0);
  }
  assert.equal(adapter.threadId, rootThread, "context separation must not create another conversation or relax task identity");
  if (workingSet) {
    const usedRequests = budget.snapshot().requests;
    await adapter.close();
    // Generic fixture has no product MCP server; use the same Core resume RPC
    // without the product conversation-only server override.
    await adapter.openThread({ resumeId: rootThread });
    await send(composeNovelCodexPrompt("METHOD_RESUMED", author, "核对持久任务后继续。", {
      internal: true, binding: { quote: "CURRENT_SOURCE_AFTER_RESTART", taskId: "resumed" },
    }));
    assert.equal(adapter.threadId, rootThread);
    assert.ok(budget.snapshot().requests > usedRequests, "restart retains cumulative execution accounting");
    assert.equal(terminalTurns.at(-1)?.status, "completed");
  }
  const after = await adapter.readThread(rootThread);
  assert.equal(after.turns.flatMap(turn => turn.items).filter(item => item.type === "userMessage" && item.text === author).length, 1);
  assert.equal(budget.snapshot().requests, requests.length);
});

}
