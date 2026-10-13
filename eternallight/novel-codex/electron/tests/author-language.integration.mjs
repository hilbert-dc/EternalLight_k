import assert from "node:assert/strict";
import { mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { ModelBridgeProcess, TaskBudget, compileCodexConfig, compileCodexModelCatalog,
  createExecutionLease, defineConnection, resolveSessionPlan } from "@eternallight/agent-engine";
import { verifiedBaselineEvidence } from "@eternallight/agent-engine/test-support";
import { CodexAppServerAdapter } from "../codex-adapter.mjs";
import { LANGUAGE_CONSISTENCY_INSTRUCTIONS } from "../author-language.mjs";
import { NOVEL_ROOT_DEVELOPER_INSTRUCTIONS, NOVEL_CREATIVE_MISSION, CREATION_PREPARATION_INSTRUCTIONS } from "../creative-mission.mjs";
import { NOVEL_SUBAGENT_DEVELOPER_INSTRUCTIONS, composeNovelCodexPrompt } from "../codex-runtime.mjs";
import { authorSteerInput } from "../author-steer.mjs";

// Real Codex + native bridge; this local provider verifies the request contract,
// not a model's compliance with the requested conversation language.
for (const { developerRoleMode, dynamicReasoning } of [
  ...["native", "system", "instructions"].map(developerRoleMode => ({ developerRoleMode, dynamicReasoning: false })),
  { developerRoleMode: "instructions", dynamicReasoning: true },
]) {
test(`developer creative mission and language survive new thread, resumed history and active author steer (${developerRoleMode}${dynamicReasoning ? ": dynamic reasoning without author steer" : ""})`, { timeout: 35000 }, async t => {
  const directory = await mkdtemp(join(tmpdir(), "novel-language-wire-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const requests = [];
  let selectedEffort = "low";
  const retained = { type: "reasoning", id: "rs_language", summary: [],
    content: [{ type: "reasoning_text", text: "Private source stays original." }], encrypted_content: "opaque-language-source" };
  const gatePath = join(directory, "release-tool");
  let secondStarted;
  const secondReady = new Promise(resolve => { secondStarted = resolve; });
  const server = createServer(async (request, response) => {
    const chunks = [];
    for await (const chunk of request) chunks.push(chunk);
    requests.push(JSON.parse(Buffer.concat(chunks)));
    const index = requests.length;
    response.writeHead(200, { "content-type": "text/event-stream" });
    const send = event => response.write(`data: ${JSON.stringify(event)}\n\n`);
    send({ type: "response.created", response: { id: `language-response-${index}` } });
    const answer = { type: "message", id: `msg_language_${index}`, role: "assistant", status: "completed",
      content: [{ type: "output_text", text: index === 1 ? "正文保持不变。" : "The story remains unchanged.", annotations: [] }] };
    const tool = requests.at(-1).tools.find(item => item.name?.endsWith("__read_context"));
    const output = index === 1 ? [retained, answer] : index === 2
      ? [{ type: "function_call", id: "language-read", call_id: "language-read", name: tool.name, arguments: "{}", status: "completed" }]
      : [answer];
    for (const [output_index, item] of output.entries()) send({ type: "response.output_item.done", output_index, item });
    send({ type: "response.completed", response: { id: `language-response-${index}`, status: "completed", output,
      usage: { input_tokens: 10, output_tokens: 10, total_tokens: 20 } } });
    response.end("data: [DONE]\n\n");
  });
  await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
  t.after(() => new Promise(resolve => { server.closeAllConnections(); server.close(resolve); }));
  const connection = defineConnection({ id: "fixture", provider: "fixture", model: "fixture", protocol: "responses",
    endpoint: `http://127.0.0.1:${server.address().port}`, credentialRef: "FIXTURE", protocolOptions: { reasoningEffort: dynamicReasoning ? "low" : "none", ...(dynamicReasoning ? { allowedReasoningEfforts: ["low", "high", "max"] } : {}), namespaceToolsMode: "flatten", developerRoleMode }, declaredLimits: { contextTokens: 64000, outputTokens: 8192 } });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: { allowedMcpServerIds: ["novel_workspace"] } });
  const bridge = new ModelBridgeProcess({ sessionPlan, connections: [connection], credentialResolver: async () => "fixture", requestConfigurationProvider: dynamicReasoning ? () => ({ reasoningEffort: selectedEffort }) : null });
  t.after(() => bridge.close());
  const budget = new TaskBudget();
  t.after(() => { if (requests.length !== 3) t.diagnostic(JSON.stringify({ requests: requests.length, budget: budget.snapshot() })); });
  const address = await bridge.start();
  await bridge.beginTaskBudget(budget);
  const lease = createExecutionLease({ sessionPlan, runId: "language-wire", budgetId: budget.id });
  await bridge.beginExecutionLease(lease);
  const catalog = join(directory, "models.json");
  const baseInstructions = "Upstream base instructions must remain byte-for-byte unchanged.";
  await writeFile(catalog, JSON.stringify(compileCodexModelCatalog(sessionPlan, { baseInstructions })));
  await writeFile(join(directory, "config.toml"), compileCodexConfig({ sessionPlan, modelCatalogPath: catalog,
    bridge: { baseUrl: address.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" },
    mcpServers: [{ id: "novel_workspace", command: process.execPath,
      args: [fileURLToPath(new URL("./fixtures/language-probe-mcp.mjs", import.meta.url))], envVars: ["LANGUAGE_PROBE_GATE"], approvalMode: "approve" }],
    developerInstructions: NOVEL_ROOT_DEVELOPER_INSTRUCTIONS, subagentDeveloperInstructions: NOVEL_SUBAGENT_DEVELOPER_INSTRUCTIONS }));
  const executable = process.platform === "win32" ? "codex.exe" : "codex";
  const codex = fileURLToPath(new URL(`../../../../codex-rs/target/debug/${executable}`, import.meta.url));
  const environment = Object.fromEntries(["PATH", "PATHEXT", "SystemRoot", "WINDIR", "ComSpec", "TEMP", "TMP", "TMPDIR",
    "LANG", "LC_ALL", "SSL_CERT_FILE", "SSL_CERT_DIR", "HTTP_PROXY", "HTTPS_PROXY", "NO_PROXY"]
    .filter(key => typeof process.env[key] === "string").map(key => [key, process.env[key]]));
  const createAdapter = () => new CodexAppServerAdapter({ languagePreferences: () => ({ preferredLanguages: ["zh-CN", "en-US"] }), command: [codex, "app-server"], cwd: directory, modelRoute: sessionPlan.defaultModel,
    env: { ...environment, CODEX_HOME: directory, CODEX_SQLITE_HOME: join(directory, "sqlite"), AGENT_ENGINE_BRIDGE_TOKEN: address.token, LANGUAGE_PROBE_GATE: gatePath } });
  const first = createAdapter(); t.after(() => first.close());
  const initial = composeNovelCodexPrompt("", "检查本章，保持正文不变。");
  assert.equal((await first.sendTurn(initial.input, { disableEnvironment: true, additionalContext: initial.additionalContext })).message, "正文保持不变。");
  const threadId = first.threadId;
  // The production adapter's close terminates its process; wait for the native
  // rollout writer before testing a separate resumed process.
  let persisted = false;
  for (let attempt = 0; attempt < 100 && !persisted; attempt++) {
    const paths = await readdir(join(directory, "sessions"), { recursive: true });
    for (const path of paths.filter(path => path.endsWith(".jsonl"))) {
      const source = await readFile(join(directory, "sessions", path), "utf8");
      persisted ||= source.includes(retained.id) && source.includes(retained.encrypted_content);
    }
    if (!persisted) await new Promise(resolve => setTimeout(resolve, 20));
  }
  assert.ok(persisted, "the first process must persist its original reasoning and opaque item before restart");
  first.close();
  const resumed = createAdapter(); t.after(() => resumed.close());
  let mcpReady;
  const ready = new Promise(resolve => { mcpReady = resolve; });
  resumed.setCallbacks({ onEvent: event => {
    if (event.method === "mcpServer/startupStatus/updated" && event.params.status === "ready") mcpReady();
    if (event.method === "item/started" && event.params.item.type === "mcpToolCall") secondStarted();
  } });
  // Resume execution with the explicitly configured synthetic read-only MCP;
  // the conversation-only management entry intentionally disables task tools.
  await resumed.openThread({ resumeId: threadId });
  await ready;
  await resumed.bindExecution({ lease });
  const next = composeNovelCodexPrompt("", "Please discuss this chapter in English.");
  if (dynamicReasoning) selectedEffort = "high";
  const turn = resumed.sendTurn(next.input, { disableEnvironment: true, additionalContext: next.additionalContext });
  await secondReady;
  const steer = authorSteerInput({ id: "language-task", projectId: "fixture", threadId }, {
    taskId: "language-task", status: "pending", intentRevision: 1, authorMessageId: "language-steer",
    authorText: "Please keep all progress and discussion in English; do not change the story." });
  if (dynamicReasoning) selectedEffort = "max";
  else assert.deepEqual(await resumed.steerTurn(steer.input, { additionalContext: steer.additionalContext, expectedThreadId: threadId, expectedTurnId: resumed.turnId, runId: lease.runId, leaseId: lease.leaseId, epoch: lease.epoch }), { steered: true });
  await writeFile(gatePath, "ready");
  assert.equal((await turn).message, "The story remains unchanged.");
  assert.equal(requests.length, 3, "steer must stay in the active turn and reach its next request");
  assert.deepEqual(requests.map(request => request.reasoning.effort), (dynamicReasoning ? ["low", "high", "max"] : ["none", "none", "none"]), "real Codex wire must use the configured effort through new, resumed and steered requests");
  for (const request of requests) {
    // Current Codex emits base instructions as the first developer input item.
    const policyRole = developerRoleMode === "native" ? "developer" : "system";
    if (developerRoleMode === "instructions") {
      assert.ok(request.instructions.includes(`<developer>\n${baseInstructions}\n</developer>`));
    } else {
      assert.equal(request.input[0].role, policyRole);
      assert.equal(request.input[0].content.map(part => part.text ?? "").join(""), baseInstructions);
    }
    const developer = [request.instructions ?? "", ...request.input.filter(item => item.role === policyRole).flatMap(item => item.content ?? []).map(part => part.text ?? "")].join("\n");
    assert.equal(developer.split(LANGUAGE_CONSISTENCY_INSTRUCTIONS).length - 1, 1, "language policy is transmitted once across new, resumed and steered turns");
    assert.ok(developer.includes('"preferredLocale":"zh-CN"'), "locale survives actual new, resumed and steered model requests");
    assert.equal(developer.split(NOVEL_CREATIVE_MISSION).length - 1, 1, "creative responsibility persists independently of delegation and user turns");
    assert.equal(developer.split(CREATION_PREPARATION_INSTRUCTIONS).length - 1, 1, "preparation and adaptation responsibility survives resumed and steered turns");
    assert.ok(request.tools.some(tool => tool.name?.endsWith("request_user_input")
      || tool.tools?.some(child => child.name?.endsWith("request_user_input"))), "Default mode must actually expose optional author input to the provider");
  }
  assert.deepEqual(requests[1].input.find(item => item.id === retained.id), retained);
  assert.deepEqual(requests[2].input.find(item => item.id === retained.id), retained);
  const users = request => request.input.filter(item => item.role === "user").flatMap(item => item.content ?? []).map(part => part.text ?? "").join("\n");
  assert.ok(users(requests[2]).includes("检查本章，保持正文不变。"));
  if (dynamicReasoning) {
    assert.equal(users(requests[2]), users(requests[1]), "changing reasoning must not inject an author message");
    assert.equal(budget.snapshot().requests, 3);
    assert.equal(budget.snapshot().tokens, 60);
  } else assert.ok(users(requests[2]).includes("Please keep all progress and discussion in English; do not change the story."));
  assert.equal(resumed.threadId, threadId);
});
}
