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
import { composeNovelCodexPrompt } from "../codex-runtime.mjs";
import { NOVEL_ROOT_DEVELOPER_INSTRUCTIONS, NOVEL_DURABLE_WORK_INSTRUCTIONS } from "../creative-mission.mjs";

// Real App Server and native bridge with a deterministic local provider. This
// verifies wire/history/compaction isolation, not literary model quality.
test("contextual task material survives current injection without becoming retained author input", { timeout: 35000 }, async t => {
  const directory = await mkdtemp(join(tmpdir(), "novel-autonomous-context-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const requests = [];
  const server = createServer(async (request, response) => {
    const chunks = [];
    for await (const chunk of request) chunks.push(chunk);
    requests.push(JSON.parse(Buffer.concat(chunks)));
    response.writeHead(200, { "content-type": "text/event-stream" });
    const index = requests.length;
    const answer = { type: "message", id: `context-answer-${index}`, role: "assistant", status: "completed",
      content: [{ type: "output_text", text: "当前步骤已结束；后续仍需依据持久任务状态。", annotations: [] }] };
    const send = event => response.write(`data: ${JSON.stringify(event)}\n\n`);
    send({ type: "response.created", response: { id: `context-response-${index}` } });
    send({ type: "response.output_item.done", output_index: 0, item: answer });
    send({ type: "response.completed", response: { id: `context-response-${index}`, status: "completed", output: [answer],
      usage: { input_tokens: 10, output_tokens: 10, total_tokens: 20 } } });
    response.end("data: [DONE]\n\n");
  });
  await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
  t.after(() => new Promise(resolve => { server.closeAllConnections(); server.close(resolve); }));
  const connection = defineConnection({ id: "fixture", provider: "fixture", model: "fixture", protocol: "responses",
    endpoint: `http://127.0.0.1:${server.address().port}`, credentialRef: "FIXTURE", protocolOptions: { reasoningEffort: "none" },
    declaredLimits: { contextTokens: 64000, outputTokens: 8192 } });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: {} });
  const bridge = new ModelBridgeProcess({ sessionPlan, connections: [connection], credentialResolver: async () => "fixture" });
  t.after(() => bridge.close());
  const budget = new TaskBudget({ maxRequests: 8, maxTokens: 64000 });
  const address = await bridge.start();
  await bridge.beginTaskBudget(budget);
  await bridge.beginExecutionLease(createExecutionLease({ sessionPlan, runId: "context-test", budgetId: budget.id }));
  const catalog = join(directory, "models.json");
  await writeFile(catalog, JSON.stringify(compileCodexModelCatalog(sessionPlan, { baseInstructions: "Fixture model base policy." })));
  await writeFile(join(directory, "config.toml"), compileCodexConfig({ sessionPlan, modelCatalogPath: catalog,
    bridge: { baseUrl: address.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" }, mcpServers: [],
    developerInstructions: NOVEL_ROOT_DEVELOPER_INSTRUCTIONS }));
  const executable = process.platform === "win32" ? "codex.exe" : "codex";
  const codex = fileURLToPath(new URL(`../../../../codex-rs/target/debug/${executable}`, import.meta.url));
  const environment = Object.fromEntries(["PATH", "PATHEXT", "SystemRoot", "WINDIR", "ComSpec", "TEMP", "TMP", "TMPDIR", "LANG", "LC_ALL",
    "SSL_CERT_FILE", "SSL_CERT_DIR", "HTTP_PROXY", "HTTPS_PROXY", "NO_PROXY"].filter(key => typeof process.env[key] === "string").map(key => [key, process.env[key]]));
  const adapter = new CodexAppServerAdapter({ command: [codex, "app-server"], cwd: directory, modelRoute: sessionPlan.defaultModel,
    env: { ...environment, CODEX_HOME: directory, CODEX_SQLITE_HOME: join(directory, "sqlite"), AGENT_ENGINE_BRIDGE_TOKEN: address.token } });
  t.after(() => adapter.close());
  const author = "请在本次授权范围内完成作品，独立检查后继续。";
  const send = async turn => adapter.sendTurn(turn.input, { disableEnvironment: true, additionalContext: turn.additionalContext });
  await send(composeNovelCodexPrompt("METHOD_DATA_A", author, "按当前任务职责执行。", { binding: { quote: "SOURCE_DATA_A", taskId: "first" } }));
  const rootThread = adapter.threadId;
  await send(composeNovelCodexPrompt("METHOD_DATA_B", author, "按当前任务职责执行。", { internal: true, binding: { quote: "SOURCE_DATA_B", taskId: "second" } }));
  const before = await adapter.readThread(rootThread);
  const actualUsers = before.turns.flatMap(turn => turn.items).filter(item => item.type === "userMessage").map(item => item.text);
  assert.equal(actualUsers.filter(text => text === author).length, 1);
  assert.ok(actualUsers.every(text => !text.includes("SOURCE_DATA_") && !text.includes("METHOD_DATA_")), "task materials must not appear as author messages");
  await adapter.compactThread(rootThread);
  await send(composeNovelCodexPrompt("METHOD_DATA_C", author, "按当前任务职责执行。", { internal: true, binding: { quote: "SOURCE_DATA_C", taskId: "third" } }));
  const latest = requests.at(-1);
  const text = latest.input.flatMap(item => item.content ?? []).map(part => part.text ?? "").join("\n");
  assert.equal(text.includes("SOURCE_DATA_A"), false, "compaction must not retain the prior contextual task package as a user message");
  assert.equal(text.includes("SOURCE_DATA_B"), false);
  assert.equal(text.split("SOURCE_DATA_C").length - 1, 1, "the current frozen task is injected once");
  assert.equal(text.includes(NOVEL_DURABLE_WORK_INSTRUCTIONS), true, "durable responsibility remains after compaction");
  assert.equal(adapter.threadId, rootThread, "context separation must not create another conversation or relax task identity");
  const after = await adapter.readThread(rootThread);
  assert.equal(after.turns.flatMap(turn => turn.items).filter(item => item.type === "userMessage" && item.text === author).length, 1);
  assert.equal(budget.snapshot().requests, requests.length);
});
