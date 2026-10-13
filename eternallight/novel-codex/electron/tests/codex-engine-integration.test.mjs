import { mcpRequestIdentity } from "../mcp-authority.mjs";
import assert from "node:assert/strict";
import { nativeTestBinary } from "./helpers/native-binaries.mjs";
import { chmodSync, mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { execFileSync } from "node:child_process";

import {
  CAPABILITIES,
  CapabilityEvidenceStore,
  CapabilityReportStore,
  defineCapabilityReport,
  DELEGATION_EXECUTION_CAPABILITIES,
  ModelBridge,
  TaskBudget,
  compileCodexConfig,
  compileCodexModelCatalog,
  createExecutionLease,
  defineConnection,
  resolveSessionPlan,
} from "@eternallight/agent-engine";
import { verifiedBaselineEvidence } from "@eternallight/agent-engine/test-support";

function activateBridge(bridge, sessionPlan, label) {
  const budget = new TaskBudget({ maxRequests: 64, maxTokens: 1_000_000 });
  bridge.beginTaskBudget(budget);
  bridge.beginExecutionLease(createExecutionLease({ sessionPlan, runId: `integration-${label}`, budgetId: budget.id }));
}
import { CodexAppServerAdapter } from "../codex-adapter.mjs";
import { CodexConformanceRunner } from "../codex-conformance-runner.mjs";
import { createMcpStartupWaiter } from "./helpers/mcp-startup.mjs";
import { CodexCoreIntegrationRunner } from "../codex-core-integration-runner.mjs";
import { NOVEL_COLLABORATION_INSTRUCTIONS, NOVEL_COLLABORATION_MODE } from "../novel-collaboration.mjs";

const codex = fileURLToPath(new URL("../../../../codex-rs/target/debug/codex", import.meta.url));
const mcpServer = fileURLToPath(new URL("./fixture-mcp-server.mjs", import.meta.url));
const novelMcpServer = nativeTestBinary("novel-workspace-mcp");

async function listen(server) {
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  return `http://127.0.0.1:${server.address().port}`;
}

function readRequest(request) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    request.on("data", (chunk) => chunks.push(chunk));
    request.on("end", () => resolve(JSON.parse(Buffer.concat(chunks).toString("utf8"))));
    request.on("error", reject);
  });
}

function stream(response, chunks) {
  response.writeHead(200, { "content-type": "text/event-stream" });
  for (const chunk of chunks) response.write(`data: ${JSON.stringify(chunk)}\n\n`);
  const toolCall = chunks.some((chunk) => chunk.choices?.some((choice) => choice.delta?.tool_calls?.length));
  response.write(`data: ${JSON.stringify({ id: chunks.at(-1)?.id ?? "fixture", choices: [{ delta: {}, finish_reason: toolCall ? "tool_calls" : "stop" }] })}\n\n`);
  // Controlled provider fixtures must emit their synthetic receipt. A missing
  // receipt intentionally blocks finite-budget execution in production.
  if (!chunks.some(chunk => chunk.usage)) response.write(`data: ${JSON.stringify({
    id: chunks.at(-1)?.id ?? "fixture", choices: [],
    usage: { prompt_tokens: 20, completion_tokens: 3, total_tokens: 23 },
  })}\n\n`);
  response.end("data: [DONE]\n\n");
}

async function within(promise, timeoutMs, message) {
  let timer;
  try {
    return await Promise.race([
      promise,
      new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(message)), timeoutMs); }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

test("real conformance runner grants granular engine delegation evidence only after a child MCP round trip", { timeout: 45_000 }, async (t) => {
  const requests = [];
  let downstreamFailure = null;
  let childMcpCalls = 0;
  const downstream = createServer(async (request, response) => {
    try {
      const body = await readRequest(request);
      requests.push(body);
      const transcript = JSON.stringify(body.messages);
      const toolName = (name) => body.tools?.find((tool) => tool.function?.name === name || tool.function?.name?.endsWith(`__${name}`))?.function?.name;
      const isRoot = Boolean(toolName("spawn_agent"));
      if (isRoot) {
        const isMessageScenario = transcript.includes("CONFORMANCE_RECEIPT");
        const isWaitScenario = transcript.includes("WAIT_ACCEPTED");
        const childTerminal = transcript.match(/CHILD_TERMINAL:[A-Za-z0-9_-]+/)?.[0];
        if (!transcript.includes("conformance_spawn")) {
          const spawnAgent = toolName("spawn_agent");
          assert.ok(spawnAgent);
          const message = isMessageScenario
            ? "Call engine_conformance_gate with an empty object. After the gate returns, extract the FOLLOWUP_TOKEN value from the queued parent message and call engine_conformance_echo with that value."
            : `Return exactly ${childTerminal} without calling a tool.`;
          stream(response, [{ id: "conformance-root-spawn", choices: [{ delta: { tool_calls: [{ index: 0, id: "conformance_spawn", function: { name: spawnAgent, arguments: JSON.stringify({ task_name: "engine_conformance", message, fork_turns: "none" }) } }] } }] }]);
          return;
        }
        if (!isMessageScenario && !isWaitScenario) {
          stream(response, [{ id: "conformance-root-spawn-final", choices: [{ delta: { content: "SPAWN_ACCEPTED" } }] }]);
          return;
        }
        if (isWaitScenario && !transcript.includes("conformance_wait_result")) {
          const waitAgent = toolName("wait_agent");
          assert.ok(waitAgent);
          stream(response, [{ id: "conformance-root-wait", choices: [{ delta: { tool_calls: [{ index: 0, id: "conformance_wait_result", function: { name: waitAgent, arguments: JSON.stringify({ timeout_ms: 20_000 }) } }] } }] }]);
          return;
        }
        if (isWaitScenario) {
          stream(response, [{ id: "conformance-root-wait-final", choices: [{ delta: { content: "WAIT_ACCEPTED" } }] }]);
          return;
        }
        if (!transcript.includes("conformance_send")) {
          const sendMessage = toolName("send_message");
          const followup = transcript.match(/FOLLOWUP_TOKEN=([A-Za-z0-9_-]+)/)?.[1];
          assert.ok(sendMessage);
          assert.ok(followup);
          stream(response, [{ id: "conformance-root-send", choices: [{ delta: { tool_calls: [{ index: 0, id: "conformance_send", function: { name: sendMessage, arguments: JSON.stringify({ target: "engine_conformance", message: `FOLLOWUP_TOKEN=${followup}` }) } }] } }] }]);
          return;
        }
        const receipt = transcript.match(/CONFORMANCE_RECEIPT:[A-Za-z0-9_-]+/)?.[0];
        if (!receipt) {
          const waitAgent = toolName("wait_agent");
          assert.ok(waitAgent);
          stream(response, [{ id: "conformance-root-wait-result", choices: [{ delta: { tool_calls: [{ index: 0, id: "conformance_wait_result", function: { name: waitAgent, arguments: JSON.stringify({ timeout_ms: 20_000 }) } }] } }] }]);
          return;
        }
        stream(response, [{ id: "conformance-root-final", choices: [{ delta: { content: receipt } }] }]);
        return;
      }

      const childTerminal = transcript.match(/CHILD_TERMINAL:[A-Za-z0-9_-]+/)?.[0];
      if (childTerminal && !toolName("engine_conformance_gate")) {
        stream(response, [{ id: "conformance-child-terminal", choices: [{ delta: { content: childTerminal } }] }]);
        return;
      }
      const receipt = transcript.match(/CONFORMANCE_RECEIPT:[A-Za-z0-9_-]+/)?.[0];
      if (receipt) {
        stream(response, [{ id: "conformance-child-final", choices: [{ delta: { content: receipt } }] }]);
        return;
      }
      const followup = transcript.match(/FOLLOWUP_TOKEN=([A-Za-z0-9_-]+)/)?.[1];
      if (!transcript.includes("CONFORMANCE_GATE_OPEN")) {
        const gate = toolName("engine_conformance_gate");
        assert.ok(gate);
        stream(response, [{ id: "conformance-child-gate", choices: [{ delta: { tool_calls: [{ index: 0, id: "conformance_child_gate", function: { name: gate, arguments: "{}" } }] } }] }]);
        return;
      }
      if (!followup) {
        stream(response, [{ id: "conformance-child-gate-final", choices: [{ delta: { content: "FOLLOWUP_NOT_DELIVERED" } }] }]);
        return;
      }
      const echo = toolName("engine_conformance_echo");
      assert.ok(echo);
      childMcpCalls += 1;
      stream(response, [{ id: "conformance-child-tool", choices: [{ delta: { tool_calls: [{ index: 0, id: "conformance_child_echo", function: { name: echo, arguments: JSON.stringify({ followup }) } }] } }] }]);
    } catch (error) {
      downstreamFailure = error;
      response.writeHead(500, { "content-type": "application/json" });
      response.end(JSON.stringify({ error: { message: error.message } }));
    }
  });
  const endpoint = await listen(downstream);
  t.after(() => new Promise((resolve) => { downstream.closeAllConnections(); downstream.close(resolve); }));
  const connection = defineConnection({ id: "conformance-model", provider: "fixture", endpoint, model: "fixture-wire-model", credentialRef: "FIXTURE_MODEL_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 } });
  const probeEvidence = verifiedBaselineEvidence(connection, {
    protocol: connection.protocol,
    capabilities: [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.TOOL_LOOP, CAPABILITIES.STREAMING, CAPABILITIES.USAGE],
    routeGrants: [{ parentConnectionId: connection.id, childConnectionId: connection.id }],
  });
  const runner = new CodexConformanceRunner({
    baseEnvironment: process.env,
    baseInstructions: "Upstream Codex conformance integration instructions.",
    command: [codex, "app-server"],
    cwd: dirname(codex),
    nodeCommand: process.execPath,
    timeoutMs: 30_000,
  });
  const result = await runner.verify({ apiKey: "fixture-key", connection, protocol: connection.protocol, probeEvidence });

  assert.ifError(downstreamFailure);
  assert.equal(result.attempt.status, "verified", JSON.stringify(result.attempt));
  assert.equal(result.attempt.diagnostics.stages.every((stage) => stage.status === "passed"), true, JSON.stringify(result.attempt.diagnostics.stages));
  assert.deepEqual(result.records.map((record) => record.capability), DELEGATION_EXECUTION_CAPABILITIES);
  assert.equal(result.records.every((record) => record.status === "verified"), true, `records=${JSON.stringify(result.records.map(({ capability, status, assertions }) => ({ capability, status, assertions })))} downstreamRequests=${requests.map((body) => ({ messages: body.messages?.length ?? 0, tail: body.messages?.slice(-3).map((message) => ({ role: message.role, content: JSON.stringify(message.content ?? message.tool_calls ?? "").slice(0, 1_000) })) })).map(JSON.stringify).join("|")}`);
  assert.equal(result.records.every((record) => record.sample.outcome === "passed"), true);
  assert.match(result.records[0].sample.eventDigest, /^[a-f0-9]{64}$/);
  assert.equal(childMcpCalls, 1);
  assert.ok(requests.length >= 5);
});

test("core integration runner proves a root Codex model and MCP result loop", { timeout: 30_000 }, async (t) => {
  const downstream = createServer(async (request, response) => {
    const body = await readRequest(request);
    const transcript = JSON.stringify(body.messages);
    const receipt = transcript.match(/CORE_CONFORMANCE_RECEIPT:[A-Za-z0-9_-]+/)?.[0];
    if (receipt) {
      stream(response, [{ id: "core-final", choices: [{ delta: { content: receipt } }] },
        { id: "core-final", choices: [], usage: { prompt_tokens: 30, completion_tokens: 4, total_tokens: 34 } }]);
      return;
    }
    const nonce = transcript.match(/nonce ([A-Za-z0-9_-]+)/)?.[1];
    const instruction = transcript.match(/instruction (core-[A-Za-z0-9_-]+)/)?.[1];
    const name = body.tools?.find((tool) => tool.function?.name === "engine_core_echo" || tool.function?.name?.endsWith("__engine_core_echo"))?.function?.name;
    assert.ok(nonce);
    assert.ok(instruction);
    assert.ok(name);
    stream(response, [{ id: "core-tool", choices: [{ delta: { tool_calls: [{ index: 0, id: "core_echo", function: { name, arguments: JSON.stringify({ nonce, instruction }) } }] } }] },
      { id: "core-tool", choices: [], usage: { prompt_tokens: 20, completion_tokens: 3, total_tokens: 23 } }]);
  });
  const endpoint = await listen(downstream);
  t.after(() => new Promise((resolve) => { downstream.closeAllConnections(); downstream.close(resolve); }));
  const connection = defineConnection({ id: "core-model", provider: "fixture", endpoint, model: "fixture-core", credentialRef: "FIXTURE_MODEL_KEY", protocol: "auto", declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 } });
  const probeEvidence = verifiedBaselineEvidence(connection, { protocol: "chat-completions", capabilities: [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.STREAMING, CAPABILITIES.USAGE] });
  const result = await new CodexCoreIntegrationRunner({ baseEnvironment: process.env, baseInstructions: "Upstream Codex core integration instructions.", command: [codex, "app-server"], cwd: dirname(codex), nodeCommand: process.execPath, timeoutMs: 20_000 })
    .verify({ apiKey: "fixture-key", connection, protocol: "chat-completions", probeEvidence });
  assert.equal(result.status, "passed", JSON.stringify(result));
  const persisted = mkdtempSync(join(tmpdir(), "engine-evidence-reload-"));
  t.after(() => rmSync(persisted, { recursive: true, force: true }));
  const records = [...probeEvidence, result.record];
  const report = defineCapabilityReport({ connection, result: { records, selectedProtocol: "chat-completions" }, startedAt: new Date(), completedAt: new Date() });
  await new CapabilityReportStore(join(persisted, "reports")).write(report);
  await new CapabilityEvidenceStore(join(persisted, "evidence.json")).append(records.map((record) => ({ ...record, reportRef: report.reportId, reportDigest: report.reportId })));
  writeFileSync(join(persisted, "connection.json"), JSON.stringify(connection));
  const child = fileURLToPath(new URL("../../../agent-engine/tests/fixtures/reload-evidence-and-execute.mjs", import.meta.url));
  assert.equal(execFileSync(process.execPath, [child, persisted], { encoding: "utf8", timeout: 10000 }), "reloaded execution succeeded");
});

test("real Codex completes source-bound commitment extraction through native Novel MCP", { timeout: 140_000 }, async (t) => {
  let binding;
  let downstreamFailure = null;
  const requests = [];
  const runtime = createServer(async (request, response) => {
    try {
      assert.equal(request.headers.authorization, "Bearer fixture-candidate-token");
      assert.equal(request.headers["x-novel-task-capability"], "fixture-extraction-capability");
      if (request.method === "GET") {
        const url = new URL(request.url, "http://localhost");
        if (url.pathname === "/api/tool-binding") {
          assert.equal(request.headers["x-novel-root-thread"], binding.threadId);
          requests.push({method:"GET",path:url.pathname});
          response.end(JSON.stringify({projectId:binding.projectId,taskId:binding.taskId,
            threadId:binding.threadId,operation:"extractCommitments",inputRevision:0}));
          return;
        }
        if (url.pathname === "/api/projects/project/tasks/recovery-operations") {
          assert.equal(url.searchParams.get("taskId"), binding.taskId);
          response.end(JSON.stringify({operation:null}));
          return;
        }
        assert.equal(url.pathname, "/api/projects/project/context");
        assert.equal(url.searchParams.get("threadId"), binding.threadId);
        assert.equal(url.searchParams.get("purpose"), "critic", "MCP derives purpose from the frozen operation, not caller fields");
        requests.push({ method: "GET", path: url.pathname });
        response.end(JSON.stringify({ extractionFixtureContext: true, relevantText: [{ sourceRevisionId: "source-1", textRepresentation: "body", content: { body: "白塔将亮起。" } }] }));
        return;
      }
      const body = await readRequest(request);
      assert.equal(body.taskId ?? "task", "task");
      assert.equal(body.threadId, binding.threadId);
      requests.push({ method: request.method, path: request.url, body });
      if (request.url.endsWith("/commitment-extractions")) {
        assert.deepEqual(body.payload, { chapterId: "chapter-1", sourceRevisionId: "source-1", examinedRanges: [{ start: 0, end: 6 }], candidates: [] });
        response.statusCode = 201;
        response.end(JSON.stringify({ extractionFixtureReceipt: true, revisionId: "report-1" }));
      } else {
        assert.equal(request.url, "/api/projects/project/tasks/task/complete");
        response.end(JSON.stringify({ id: binding.taskId, projectId: binding.projectId, threadId: binding.threadId,
          status: "completed", resultSummary: body.summary, deliveryStatus: "ready_for_review" }));
      }
    } catch (error) {
      downstreamFailure = error;
      response.statusCode = 500;
      response.end(JSON.stringify({ error: error.message }));
    }
  });
  const runtimeEndpoint = await listen(runtime);
  t.after(() => new Promise((resolve) => { runtime.closeAllConnections(); runtime.close(resolve); }));
  const downstream = createServer(async (request, response) => {
    try {
      const body = await readRequest(request);
      const transcript = JSON.stringify(body.messages);
      const tool = (name) => body.tools?.find((entry) => entry.function?.name?.includes(name))?.function?.name;
      assert.equal(tool("save_draft_checkpoint"), undefined);
      assert.equal(tool("save_outline"), undefined);
      let name;
      let argumentsValue;
      if (!transcript.includes("extractionFixtureContext")) {
        name = tool("read_context");
        argumentsValue = {};
      } else if (!transcript.includes("extractionFixtureReceipt")) {
        name = tool("submit_commitment_extraction");
        argumentsValue = { chapterId: "chapter-1", expectedCandidateRevisionId: null, idempotencyKey: "extraction-once",
          payload: { chapterId: "chapter-1", sourceRevisionId: "source-1", examinedRanges: [{ start: 0, end: 6 }], candidates: [] } };
      } else if (!transcript.includes("acknowledgedRuntimeCompletion")) {
        name = tool("complete_task");
        argumentsValue = { summary: "未提出有可靠证据的承诺" };
      } else {
        stream(response, [{ id: "extraction-final", choices: [{ delta: { content: "Extraction proposal batch saved; no commitments confirmed." } }] }]);
        return;
      }
      assert.ok(name);
      stream(response, [{ id: "extraction-tool", choices: [{ delta: { tool_calls: [{ index: 0, id: `extraction-${requests.length}`, function: { name, arguments: JSON.stringify(argumentsValue) } }] } }] }]);
    } catch (error) {
      downstreamFailure = error;
      response.statusCode = 500;
      response.end(JSON.stringify({ error: error.message }));
    }
  });
  const endpoint = await listen(downstream);
  t.after(() => new Promise((resolve) => { downstream.closeAllConnections(); downstream.close(resolve); }));
  const connection = defineConnection({ id: "extraction-model", provider: "fixture", endpoint, model: "fixture-extraction", credentialRef: "EXTRACTION_KEY",
    protocol: "chat-completions", declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 } });
  const evidence = verifiedBaselineEvidence(connection, { protocol: connection.protocol,
    capabilities: [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.TOOL_LOOP, CAPABILITIES.STREAMING] });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence, policy: { defaultConnectionId: connection.id, allowedMcpServerIds: ["novel_workspace"] } });
  const bridge = new ModelBridge({ sessionPlan, connections: [connection], credentialResolver: async () => "fixture-key", token: "extraction-bridge-key",mcpAuthority:true,requestConfigurationProvider:({threadId})=>mcpRequestIdentity(adapter,threadId) });
  const address = await bridge.start();
  activateBridge(bridge, sessionPlan, "extraction");
  t.after(() => bridge.close());
  const root = mkdtempSync(join(tmpdir(), "novel-extraction-integration-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const catalogPath = join(root, "catalog.json");
  writeFileSync(catalogPath, JSON.stringify(compileCodexModelCatalog(sessionPlan, { baseInstructions: "Extract candidate narrative commitments with the bound tools only." })), { mode: 0o600 });
  writeFileSync(join(root, "config.toml"), compileCodexConfig({ sessionPlan, modelCatalogPath: catalogPath,
    bridge: { baseUrl: address.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" },
    mcpServers: [{ id: "novel_workspace", command: novelMcpServer, args: [], required: true, startupTimeoutMs: 90_000,
      envVars: ["NOVEL_CODEX_RUNTIME_URL", "NOVEL_CODEX_CANDIDATE_TOKEN", "NOVEL_CODEX_TASK_CAPABILITY", "NOVEL_CODEX_OPERATION","NOVEL_CODEX_ENGINE_AUTHORITY_URL","NOVEL_CODEX_ENGINE_AUTHORITY_TOKEN"], approvalMode: "approve" }] }), { mode: 0o600 });
  mkdirSync(join(root, "sqlite"));
  chmodSync(root, 0o700);
  const adapter = new CodexAppServerAdapter({ command: [codex, "app-server"], cwd: dirname(root),
    env: { PATH: process.env.PATH, CODEX_HOME: root, CODEX_SQLITE_HOME: join(root, "sqlite"), AGENT_ENGINE_BRIDGE_TOKEN: address.token,NOVEL_CODEX_ENGINE_AUTHORITY_URL:address.baseUrl,NOVEL_CODEX_ENGINE_AUTHORITY_TOKEN:address.token,
      NOVEL_CODEX_RUNTIME_URL: runtimeEndpoint, NOVEL_CODEX_CANDIDATE_TOKEN: "fixture-candidate-token",
      NOVEL_CODEX_TASK_CAPABILITY: "fixture-extraction-capability", NOVEL_CODEX_OPERATION: "extractCommitments" }, model: connection.id, provider: connection.provider });
  t.after(() => adapter.close());
  // Use the same separate startup allowance as production, with explicit failure
  // and process-exit handling; cold loader time is not model execution time.
  adapter.startProcess();
  const startup = createMcpStartupWaiter({ serverName: "novel_workspace", process: adapter.process, timeoutMs: adapter.timeouts.startupMs });
  t.after(() => startup.dispose());
  const stages = [];
  const began = performance.now();
  adapter.setCallbacks({ onEvent: event => {
    startup.onEvent(event);
    stages.push({ method: event.method, elapsedMs: Math.round(performance.now() - began) });
    if (stages.length > 80) stages.shift();
  } });
  t.after(() => t.diagnostic(JSON.stringify({ stages, runtimeRequests: requests.map(({ method, path }) => ({ method, path })) })));
  await adapter.ensureSession();
  binding = { taskId: "task", projectId: "project", threadId: adapter.threadId, expectedRevision: 0 };
  await startup.ready;
  const result = await adapter.sendTurn([{ type: "text", text: "Read the frozen source and save an extraction proposal report, then complete the task." }], { disableEnvironment: true });
  assert.ifError(downstreamFailure);
  assert.equal(result.message, "Extraction proposal batch saved; no commitments confirmed.");
  assert.deepEqual(requests.map(({ method, path }) => ({ method, path })), [
    { method: "GET", path: "/api/tool-binding" },
    { method: "GET", path: "/api/projects/project/context" },
    { method: "POST", path: "/api/projects/project/chapters/chapter-1/commitment-extractions" },
    { method: "POST", path: "/api/projects/project/tasks/task/complete" },
  ]);
});

test("real Codex App Server completes a model -> MCP -> model loop through Agent Engine", { timeout: 30_000 }, async (t) => {
  const downstreamRequests = [];
  let downstreamFailure = null;
  const downstream = createServer(async (request, response) => {
    try {
      const body = await readRequest(request);
      downstreamRequests.push(body);
      if (!JSON.stringify(body.messages).includes("fixture-context")) {
        const name = body.tools?.find((tool) => tool.function?.name?.includes("read_context"))?.function?.name;
        assert.ok(name, "Codex must expose the MCP tool to the model");
        stream(response, [
          { id: "chat-tool", choices: [{ delta: { tool_calls: [{ index: 0, id: "call_read", function: { name, arguments: "{}" } }] } }] },
          { id: "chat-tool", choices: [], usage: { prompt_tokens: 20, completion_tokens: 3, total_tokens: 23 } },
        ]);
        return;
      }
      stream(response, [
        { id: "chat-final", choices: [{ delta: { content: "MCP loop complete" } }] },
        { id: "chat-final", choices: [], usage: { prompt_tokens: 30, completion_tokens: 4, total_tokens: 34 } },
      ]);
    } catch (error) {
      downstreamFailure = error;
      response.writeHead(500, { "content-type": "application/json" });
      response.end(JSON.stringify({ error: { message: error.message } }));
    }
  });
  const endpoint = await listen(downstream);
  t.after(() => new Promise((resolve) => downstream.close(resolve)));

  const connection = defineConnection({ id: "integration-model", provider: "fixture", endpoint, model: "fixture-model", credentialRef: "FIXTURE_MODEL_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 } });
  const evidence = verifiedBaselineEvidence(connection, { protocol: connection.protocol, capabilities: [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.TOOL_LOOP, CAPABILITIES.STREAMING] });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence, policy: { defaultConnectionId: connection.id, allowedMcpServerIds: ["novel_workspace"] } });
  const canonicalRequests = [];
  const bridge = new ModelBridge({ sessionPlan, connections: [connection], credentialResolver: async () => "fixture-provider-key", token: "fixture-bridge-key", requestObserver: (request) => canonicalRequests.push(request) });
  const bridgeAddress = await bridge.start();
  activateBridge(bridge, sessionPlan, "single");
  t.after(() => bridge.close());

  const root = mkdtempSync(join(tmpdir(), "novel-codex-engine-integration-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const catalogPath = join(root, "model-catalog.json");
  writeFileSync(catalogPath, `${JSON.stringify(compileCodexModelCatalog(sessionPlan, { baseInstructions: "Upstream Codex integration test instructions." }), null, 2)}\n`, { mode: 0o600 });
  writeFileSync(join(root, "config.toml"), compileCodexConfig({
    sessionPlan,
    modelCatalogPath: catalogPath,
    bridge: { baseUrl: bridgeAddress.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" },
    mcpServers: [{ id: "novel_workspace", command: process.execPath, args: [mcpServer], approvalMode: "approve" }],
  }), { mode: 0o600 });
  mkdirSync(join(root, "sqlite"));
  chmodSync(root, 0o700);

  const events = [];
  const adapter = new CodexAppServerAdapter({
    command: [codex, "app-server"],
    cwd: dirname(root),
    env: { PATH: process.env.PATH, CODEX_HOME: root, CODEX_SQLITE_HOME: join(root, "sqlite"), AGENT_ENGINE_BRIDGE_TOKEN: bridgeAddress.token },
    model: connection.id,
    provider: connection.provider,
  });
  t.after(() => adapter.close());
  let markMcpReady;
  const mcpReady = new Promise((resolve) => { markMcpReady = resolve; });
  adapter.setCallbacks({ onEvent: (event) => {
    events.push(event);
    if (event.method === "mcpServer/startupStatus/updated" && event.params?.name === "novel_workspace" && event.params?.status === "ready") markMcpReady();
  } });
  await adapter.ensureSession();
  await mcpReady;
  let result;
  try {
    result = await adapter.sendTurn([{ type: "text", text: "Read context, then answer." }], { disableEnvironment: true });
  } catch (error) {
    throw new Error(`${error.message}; canonical=${JSON.stringify(canonicalRequests)}; events=${JSON.stringify(events.map((event) => event.method).filter(Boolean))}`, { cause: error });
  }

  assert.ifError(downstreamFailure);
  assert.equal(result.message, "MCP loop complete");
  assert.equal(downstreamRequests.length, 2);
  assert.ok(events.some((event) => event.method === "item/agentMessage/delta"));
  assert.ok(events.some((event) => event.method === "item/completed" && event.params?.item?.type === "mcpToolCall"));
});

for (const childCount of [1, 5]) test(`real Codex App Server delegates to a model on another endpoint with ${childCount} children and preserves trusted child identity`, { timeout: 45_000 }, async (t) => {
  const rootRequests = [];
  const childRequests = [];
  let downstreamFailure = null;
  const rootDownstream = createServer(async (request, response) => {
    try {
      const body = await readRequest(request);
      rootRequests.push(body);
      const transcript = JSON.stringify(body.messages);
      const toolName = (name) => body.tools?.find((tool) => tool.function?.name === name || tool.function?.name?.endsWith(`__${name}`))?.function?.name;
      const nextChild = Array.from({ length: childCount }, (_, index) => index).find(index => !transcript.includes(`call_spawn_${index}`));
      if (nextChild !== undefined) {
        const spawnAgent = toolName("spawn_agent");
        assert.ok(spawnAgent, "root must receive spawn_agent");
        stream(response, [
          { id: `root-spawn-${nextChild}`, choices: [{ delta: { tool_calls: [{ index: 0, id: `call_spawn_${nextChild}`, function: { name: spawnAgent, arguments: JSON.stringify({ task_name: `inspect_context_${nextChild}`, message: "Read context and return CHILD_EVIDENCE with the lighthouse fact.", fork_turns: "none" }) } }] } }] },
        ]);
        return;
      }
      if (!transcript.includes("call_wait")) {
        const waitAgent = toolName("wait_agent");
        assert.ok(waitAgent, "root must receive wait_agent");
        stream(response, [
          { id: "root-wait", choices: [{ delta: { tool_calls: [{ index: 0, id: "call_wait", function: { name: waitAgent, arguments: JSON.stringify({ timeout_ms: 10_000 }) } }] } }] },
        ]);
        return;
      }
      if (!transcript.includes("call_list")) {
        const listAgents = toolName("list_agents");
        assert.ok(listAgents, "root must receive list_agents");
        stream(response, [
          { id: "root-list", choices: [{ delta: { tool_calls: [{ index: 0, id: "call_list", function: { name: listAgents, arguments: "{}" } }] } }] },
        ]);
        return;
      }
      if (!/fixture context identifies a white lighthouse/.test(transcript) || childRequests.length < childCount * 2) {
        const waitAgent = toolName("wait_agent");
        assert.ok(waitAgent, "root must retain wait_agent while the child is running");
        stream(response, [
          { id: "root-wait-again", choices: [{ delta: { tool_calls: [{ index: 0, id: `call_wait_again_${rootRequests.length}`, function: { name: waitAgent, arguments: JSON.stringify({ timeout_ms: 20_000 }) } }] } }] },
        ]);
        return;
      }
      stream(response, [
        { id: "root-final", choices: [{ delta: { content: "Parent synthesis: CHILD_EVIDENCE confirms the white lighthouse." } }] },
      ]);
    } catch (error) {
      downstreamFailure = error;
      response.writeHead(500, { "content-type": "application/json" });
      response.end(JSON.stringify({ error: { message: error.message } }));
    }
  });
  const childDownstream = createServer(async (request, response) => {
    try {
      const body = await readRequest(request);
      childRequests.push(body);
      const transcript = JSON.stringify(body.messages);
      if (!transcript.includes("fixture-context")) {
        const name = body.tools?.find((tool) => tool.function?.name?.includes("read_context"))?.function?.name;
        assert.ok(name, "child must receive the MCP context tool");
        stream(response, [
          { id: "child-tool", choices: [{ delta: { tool_calls: [{ index: 0, id: "call_child_read", function: { name, arguments: "{}" } }] } }] },
        ]);
        return;
      }
      stream(response, [
        { id: "child-final", choices: [{ delta: { content: "CHILD_EVIDENCE: fixture context identifies a white lighthouse." } }] },
      ]);
    } catch (error) {
      downstreamFailure = error;
      response.writeHead(500, { "content-type": "application/json" });
      response.end(JSON.stringify({ error: { message: error.message } }));
    }
  });
  const rootEndpoint = await listen(rootDownstream);
  const childEndpoint = await listen(childDownstream);
  t.after(() => Promise.all([
    new Promise((resolve) => { rootDownstream.closeAllConnections(); rootDownstream.close(resolve); }),
    new Promise((resolve) => { childDownstream.closeAllConnections(); childDownstream.close(resolve); }),
  ]));

  const rootConnection = defineConnection({ id: "root-model", provider: "fixture-root", endpoint: rootEndpoint, model: "root-wire-model", credentialRef: "ROOT_MODEL_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 } });
  const childConnection = defineConnection({ id: "child-model", provider: "fixture-child", endpoint: childEndpoint, model: "child-wire-model", credentialRef: "CHILD_MODEL_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 } });
  const capabilities = [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.TOOL_LOOP, CAPABILITIES.STREAMING, ...DELEGATION_EXECUTION_CAPABILITIES];
  const evidence = [
    ...verifiedBaselineEvidence(rootConnection, { protocol: rootConnection.protocol, capabilities, routeGrants: [{ parentConnectionId: rootConnection.id, childConnectionId: rootConnection.id }, { parentConnectionId: rootConnection.id, childConnectionId: childConnection.id }] }),
    ...verifiedBaselineEvidence(childConnection, { protocol: childConnection.protocol, capabilities, routeGrants: [{ parentConnectionId: childConnection.id, childConnectionId: childConnection.id }, { parentConnectionId: childConnection.id, childConnectionId: rootConnection.id }] }),
  ];
  const sessionPlan = resolveSessionPlan({
    connections: [rootConnection, childConnection],
    evidence,
    policy: {
      defaultConnectionId: rootConnection.id,
      allowedChildConnectionIds: [rootConnection.id, childConnection.id],
      requiredCapabilities: DELEGATION_EXECUTION_CAPABILITIES,
      productExtensions: DELEGATION_EXECUTION_CAPABILITIES,
      allowedMcpServerIds: ["novel_workspace"],
    },
  });
  assert.ok(sessionPlan.allowedChildEdges.some((edge) => edge.parentConnectionId === rootConnection.id && edge.childConnectionId === childConnection.id));
  const bridge = new ModelBridge({ sessionPlan, connections: [rootConnection, childConnection], credentialResolver: async () => "fixture-provider-key", token: "fixture-bridge-key" });
  const bridgeAddress = await bridge.start();
  activateBridge(bridge, sessionPlan, "root-child");
  t.after(() => bridge.close());

  const root = mkdtempSync(join(tmpdir(), "novel-codex-multi-agent-integration-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const auditPath = join(root, "mcp-audit.jsonl");
  const catalogPath = join(root, "model-catalog.json");
  writeFileSync(catalogPath, `${JSON.stringify(compileCodexModelCatalog(sessionPlan, { baseInstructions: "Upstream Codex integration test instructions." }), null, 2)}\n`, { mode: 0o600 });
  writeFileSync(join(root, "config.toml"), compileCodexConfig({
    sessionPlan,
    usageHintText: NOVEL_COLLABORATION_INSTRUCTIONS,
    multiAgentModeHintText: NOVEL_COLLABORATION_MODE,
    modelCatalogPath: catalogPath,
    bridge: { baseUrl: bridgeAddress.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" },
    mcpServers: [{ id: "novel_workspace", command: process.execPath, args: [mcpServer], envVars: ["FIXTURE_MCP_AUDIT_PATH"], approvalMode: "approve" }],
  }), { mode: 0o600 });
  mkdirSync(join(root, "sqlite"));
  chmodSync(root, 0o700);

  const events = [];
  const adapter = new CodexAppServerAdapter({
    command: [codex, "app-server"],
    cwd: dirname(root),
    env: { PATH: process.env.PATH, CODEX_HOME: root, CODEX_SQLITE_HOME: join(root, "sqlite"), AGENT_ENGINE_BRIDGE_TOKEN: bridgeAddress.token, FIXTURE_MCP_AUDIT_PATH: auditPath },
    model: rootConnection.id,
    provider: rootConnection.provider,
  });
  t.after(() => adapter.close());
  let markMcpReady;
  const mcpReady = new Promise((resolve) => { markMcpReady = resolve; });
  adapter.setCallbacks({ onEvent: (event) => {
    events.push(event);
    if (event.method === "mcpServer/startupStatus/updated" && event.params?.name === "novel_workspace" && event.params?.status === "ready") markMcpReady();
  } });
  await adapter.ensureSession();
  await mcpReady;
  let result;
  try {
    result = await within(
      adapter.sendTurn([{ type: "text", text: "Delegate context inspection to a child model, wait for it, and synthesize its evidence." }], { disableEnvironment: true }),
      15_000,
      "cross-model delegation did not complete",
    );
  } catch (error) {
    const rootToolResults = rootRequests.at(-1)?.messages?.filter((message) => message.role === "tool").slice(-6) ?? [];
    const collaborationEvents = events.filter((event) => JSON.stringify(event).toLowerCase().includes("collab") || JSON.stringify(event).toLowerCase().includes("agenttool"));
    throw new Error(`${error.message}; rootRequests=${rootRequests.length}; rootModels=${JSON.stringify([...new Set(rootRequests.map((request) => request.model))])}; rootToolResults=${JSON.stringify(rootToolResults)}; childRequests=${JSON.stringify(childRequests)}; collaborationEvents=${JSON.stringify(collaborationEvents)}`, { cause: error });
  }

  assert.ifError(downstreamFailure);
  assert.ok(rootRequests.length >= 4, "root model must spawn, wait, inspect child status, and synthesize");
  const rootInstructions = rootRequests[0].messages.filter(message => message.role === "developer").map(message => message.content).join("\n");
  assert.ok(rootInstructions.includes(NOVEL_COLLABORATION_INSTRUCTIONS), "current Codex must deliver the product role policy to the root model");
  assert.ok(rootInstructions.includes(NOVEL_COLLABORATION_MODE), "current Codex must enable proactive delegation independently of reasoning effort");
  assert.equal(childRequests.length, childCount * 2, "every child model must call MCP and finish");
  assert.equal(result.message, "Parent synthesis: CHILD_EVIDENCE confirms the white lighthouse.");
  assert.ok(events.some((event) => event.method === "item/completed" && event.params?.item?.type === "collabAgentToolCall"));
  const auditRecords = readFileSync(auditPath, "utf8").trim().split("\n").map(JSON.parse);
  assert.equal(auditRecords.length, childCount);
  assert.equal(new Set(auditRecords.map(record => record._meta.threadId)).size, childCount);
  const trustedMeta = auditRecords[0]._meta;
  assert.equal(trustedMeta.sessionId, adapter.threadId);
  assert.notEqual(trustedMeta.threadId, trustedMeta.sessionId);
  assert.ok(trustedMeta.callId);
  // Upstream emits identifiers; Engine authority supplies role/parent/batch to
  // production MCP, covered by the Native delegated isolation test below.

});

test("real Novel MCP denies child writes and cross-task reads in a delegated Codex turn", { timeout: 140_000 }, async (t) => {
  const rootRequests = [];
  const childRequests = [];
  let downstreamFailure = null;
  const rootDownstream = createServer(async (request, response) => {
    try {
      const body = await readRequest(request);
      rootRequests.push(body);
      const transcript = JSON.stringify(body.messages);
      const toolName = (name) => body.tools?.find((tool) => tool.function?.name === name || tool.function?.name?.endsWith(`__${name}`))?.function?.name;
      if (!transcript.includes("policy_spawn")) {
        const spawnAgent = toolName("spawn_agent");
        assert.ok(spawnAgent, "root must receive spawn_agent");
        stream(response, [
          { id: "policy-root-spawn", choices: [{ delta: { tool_calls: [{ index: 0, id: "policy_spawn", function: { name: spawnAgent, arguments: JSON.stringify({ task_name: "probe_policy", model: "policy-child", message: "Attempt a workspace write, then attempt a read using a foreign thread identity. Report both policy results.", fork_turns: "none" }) } }] } }] },
        ]);
        return;
      }
      if (!transcript.includes("policy_wait")) {
        const waitAgent = toolName("wait_agent");
        assert.ok(waitAgent, "root must receive wait_agent");
        stream(response, [
          { id: "policy-root-wait", choices: [{ delta: { tool_calls: [{ index: 0, id: "policy_wait", function: { name: waitAgent, arguments: JSON.stringify({ timeout_ms: 10_000 }) } }] } }] },
        ]);
        return;
      }
      if (!transcript.includes("policy_list")) {
        const listAgents = toolName("list_agents");
        assert.ok(listAgents, "root must receive list_agents");
        stream(response, [
          { id: "policy-root-list", choices: [{ delta: { tool_calls: [{ index: 0, id: "policy_list", function: { name: listAgents, arguments: "{}" } }] } }] },
        ]);
        return;
      }
      if (!/invalid_tool_arguments.*toolValidation/.test(transcript)) {
        const waitAgent = toolName("wait_agent");
        assert.ok(waitAgent, "root must retain wait_agent while the policy child is running");
        stream(response, [
          { id: "policy-root-wait-again", choices: [{ delta: { tool_calls: [{ index: 0, id: `policy_wait_again_${rootRequests.length}`, function: { name: waitAgent, arguments: JSON.stringify({ timeout_ms: 20_000 }) } }] } }] },
        ]);
        return;
      }
      stream(response, [
        { id: "policy-root-final", choices: [{ delta: { content: "Parent verified Novel MCP child isolation." } }] },
      ]);
    } catch (error) {
      downstreamFailure = error;
      response.writeHead(500, { "content-type": "application/json" });
      response.end(JSON.stringify({ error: { message: error.message } }));
    }
  });
  const childDownstream = createServer(async (request, response) => {
    try {
      const body = await readRequest(request);
      childRequests.push(body);
      const transcript = JSON.stringify(body.messages);
      const toolName = (name) => body.tools?.find((tool) => tool.function?.name === name || tool.function?.name?.endsWith(`__${name}`))?.function?.name;
      if (!transcript.includes("policy_child_write")) {
        const saveOutline = toolName("save_outline");
        assert.ok(saveOutline, "child must see save_outline so the trusted MCP policy can reject it");
        stream(response, [
          { id: "policy-child-write", choices: [{ delta: { tool_calls: [{ index: 0, id: "policy_child_write", function: { name: saveOutline, arguments: JSON.stringify({ expectedRevision: 0, payload: { nodes: [{ clientKey: "book", kind: "book", position: 0, title: "Denied", purpose: "Probe", summary: "Probe", dependencyKeys: [], certainty: "open", protection: "free" }] }, expectedCandidateRevisionId: null, idempotencyKey: "policy-write" }) } }] } }] },
        ]);
        return;
      }
      if (!/invalid_tool_arguments|model_identity_forbidden/.test(transcript)) {
        assert.match(transcript, /root_only_tool/);
        const readContext = toolName("read_context");
        assert.ok(readContext, "child must receive read_context");
        stream(response, [
          { id: "policy-child-read", choices: [{ delta: { tool_calls: [{ index: 0, id: "policy_child_read", function: { name: readContext, arguments: JSON.stringify({ threadId: "foreign-thread" }) } }] } }] },
        ]);
        return;
      }
      assert.match(transcript, /invalid_tool_arguments|model_identity_forbidden/);
      stream(response, [
        { id: "policy-child-final", choices: [{ delta: { content: "POLICY_EVIDENCE: child write denied; cross-task read denied." } }] },
      ]);
    } catch (error) {
      downstreamFailure = error;
      response.writeHead(500, { "content-type": "application/json" });
      response.end(JSON.stringify({ error: { message: error.message } }));
    }
  });
  const bindingRequests = [];
  const runtime = createServer((request, response) => {
    bindingRequests.push(request.url);
    assert.equal(request.url, "/api/tool-binding", "denied business calls must never reach Runtime");
    assert.equal(request.headers.authorization, "Bearer fixture-candidate-token");
    response.setHeader("content-type", "application/json");
    response.end(JSON.stringify({projectId:"project-policy",taskId:"task-policy",
      threadId:request.headers["x-novel-root-thread"],operation:"generateOutline",inputRevision:0}));
  });
  const runtimeEndpoint = await listen(runtime);
  t.after(() => new Promise(resolve => { runtime.closeAllConnections(); runtime.close(resolve); }));
  const rootEndpoint = await listen(rootDownstream);
  const childEndpoint = await listen(childDownstream);
  t.after(() => Promise.all([
    new Promise((resolve) => rootDownstream.close(resolve)),
    new Promise((resolve) => childDownstream.close(resolve)),
  ]));

  const rootConnection = defineConnection({ id: "policy-root", provider: "fixture-root", endpoint: rootEndpoint, model: "policy-root-wire", credentialRef: "ROOT_MODEL_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 } });
  const childConnection = defineConnection({ id: "policy-child", provider: "fixture-child", endpoint: childEndpoint, model: "policy-child-wire", credentialRef: "CHILD_MODEL_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 } });
  const capabilities = [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.TOOL_LOOP, CAPABILITIES.STREAMING, ...DELEGATION_EXECUTION_CAPABILITIES];
  const evidence = [
    ...verifiedBaselineEvidence(rootConnection, { protocol: rootConnection.protocol, capabilities, routeGrants: [{ parentConnectionId: rootConnection.id, childConnectionId: rootConnection.id }, { parentConnectionId: rootConnection.id, childConnectionId: childConnection.id }] }),
    ...verifiedBaselineEvidence(childConnection, { protocol: childConnection.protocol, capabilities, routeGrants: [{ parentConnectionId: childConnection.id, childConnectionId: childConnection.id }, { parentConnectionId: childConnection.id, childConnectionId: rootConnection.id }] }),
  ];
  const sessionPlan = resolveSessionPlan({
    connections: [rootConnection, childConnection],
    evidence,
    policy: { defaultConnectionId: rootConnection.id, allowedChildConnectionIds: [rootConnection.id, childConnection.id], requiredCapabilities: DELEGATION_EXECUTION_CAPABILITIES, productExtensions: DELEGATION_EXECUTION_CAPABILITIES, allowedMcpServerIds: ["novel_workspace"] },
  });
  const bridge = new ModelBridge({ sessionPlan, connections: [rootConnection, childConnection], credentialResolver: async () => "fixture-provider-key", token: "fixture-bridge-key",mcpAuthority:true,requestConfigurationProvider:({threadId})=>mcpRequestIdentity(adapter,threadId) });
  const bridgeAddress = await bridge.start();
  activateBridge(bridge, sessionPlan, "nested");
  t.after(() => bridge.close());

  const root = mkdtempSync(join(tmpdir(), "novel-codex-policy-integration-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const catalogPath = join(root, "model-catalog.json");
  writeFileSync(catalogPath, `${JSON.stringify(compileCodexModelCatalog(sessionPlan, { baseInstructions: "Upstream Codex integration test instructions." }), null, 2)}\n`, { mode: 0o600 });
  writeFileSync(join(root, "config.toml"), compileCodexConfig({
    sessionPlan,
    modelCatalogPath: catalogPath,
    bridge: { baseUrl: bridgeAddress.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" },
    mcpServers: [{
      id: "novel_workspace",
      command: novelMcpServer,
      args: [],
      required: true,
      startupTimeoutMs: 90_000,
      envVars: ["NOVEL_CODEX_RUNTIME_URL", "NOVEL_CODEX_CANDIDATE_TOKEN", "NOVEL_CODEX_TASK_CAPABILITY", "NOVEL_CODEX_OPERATION","NOVEL_CODEX_ENGINE_AUTHORITY_URL","NOVEL_CODEX_ENGINE_AUTHORITY_TOKEN"],
      approvalMode: "approve",
    }],
  }), { mode: 0o600 });
  mkdirSync(join(root, "sqlite"));
  chmodSync(root, 0o700);

  const adapter = new CodexAppServerAdapter({
    command: [codex, "app-server"],
    cwd: dirname(root),
    env: {
      PATH: process.env.PATH,
      CODEX_HOME: root,
      CODEX_SQLITE_HOME: join(root, "sqlite"),
      AGENT_ENGINE_BRIDGE_TOKEN: bridgeAddress.token,
      NOVEL_CODEX_ENGINE_AUTHORITY_URL:bridgeAddress.baseUrl,NOVEL_CODEX_ENGINE_AUTHORITY_TOKEN:bridgeAddress.token,
      NOVEL_CODEX_RUNTIME_URL: runtimeEndpoint,
      NOVEL_CODEX_CANDIDATE_TOKEN: "fixture-candidate-token",
      NOVEL_CODEX_TASK_CAPABILITY: "fixture-task-capability-123456789",
      NOVEL_CODEX_OPERATION: "generateOutline",
    },
    model: rootConnection.id,
    provider: rootConnection.provider,
  });
  t.after(() => adapter.close());
  const policyEvents = [];
  adapter.startProcess();
  const startup = createMcpStartupWaiter({ serverName: "novel_workspace", process: adapter.process, timeoutMs: adapter.timeouts.startupMs });
  t.after(() => startup.dispose());
  adapter.setCallbacks({ onEvent: (event) => {
    policyEvents.push(event);
    startup.onEvent(event);
  } });
  await adapter.ensureSession();
  await startup.ready;
  let result;
  try {
    result = await within(adapter.sendTurn([{ type: "text", text: "Delegate a Novel MCP isolation probe to the policy-child model and report the evidence." }], { disableEnvironment: true }), 20_000, "policy delegation did not finish");
  } catch (error) {
    throw new Error(`${error.message}; rootRequests=${JSON.stringify(rootRequests)}; childRequests=${JSON.stringify(childRequests)}; events=${JSON.stringify(policyEvents.slice(-20))}`, { cause: error });
  }

  assert.ifError(downstreamFailure);
  assert.equal(result.message, "Parent verified Novel MCP child isolation.");
  assert.ok(rootRequests.length >= 4);
  assert.equal(childRequests.length, 2, "forged read identity terminates the child before another provider call");
  assert.match(JSON.stringify(childRequests.at(-1).messages), /root_only_tool/);
  assert.match(JSON.stringify(rootRequests.at(-1).messages), /additionalProperties/);
  assert.deepEqual(bindingRequests, ["/api/tool-binding"]);
});

test("interrupting a parent Codex turn aborts its delegated child model request", { timeout: 45_000 }, async (t) => {
  let markChildStarted;
  const childStarted = new Promise((resolve) => { markChildStarted = resolve; });
  let markChildAborted;
  const childAborted = new Promise((resolve) => { markChildAborted = resolve; });
  let downstreamFailure = null;
  const rootDownstream = createServer(async (request, response) => {
    try {
      const body = await readRequest(request);
      const transcript = JSON.stringify(body.messages);
      const toolName = (name) => body.tools?.find((tool) => tool.function?.name === name || tool.function?.name?.endsWith(`__${name}`))?.function?.name;
      if (!transcript.includes("cancel_spawn")) {
        const spawnAgent = toolName("spawn_agent");
        assert.ok(spawnAgent);
        stream(response, [
          { id: "cancel-root-spawn", choices: [{ delta: { tool_calls: [{ index: 0, id: "cancel_spawn", function: { name: spawnAgent, arguments: JSON.stringify({ task_name: "slow_child", message: "Wait for the model response.", fork_turns: "none" }) } }] } }] },
        ]);
        return;
      }
      const waitAgent = toolName("wait_agent");
      assert.ok(waitAgent);
      stream(response, [
        { id: "cancel-root-wait", choices: [{ delta: { tool_calls: [{ index: 0, id: "cancel_wait", function: { name: waitAgent, arguments: JSON.stringify({ timeout_ms: 30_000 }) } }] } }] },
      ]);
    } catch (error) {
      downstreamFailure = error;
      response.writeHead(500, { "content-type": "application/json" });
      response.end(JSON.stringify({ error: { message: error.message } }));
    }
  });
  const childDownstream = createServer(async (request, response) => {
    try {
      await readRequest(request);
      markChildStarted();
      request.once("aborted", markChildAborted);
      response.once("close", () => {
        if (!response.writableEnded) markChildAborted();
      });
    } catch (error) {
      downstreamFailure = error;
    }
  });
  const rootEndpoint = await listen(rootDownstream);
  const childEndpoint = await listen(childDownstream);
  t.after(() => Promise.all([
    new Promise((resolve) => { rootDownstream.closeAllConnections(); rootDownstream.close(resolve); }),
    new Promise((resolve) => { childDownstream.closeAllConnections(); childDownstream.close(resolve); }),
  ]));

  const rootConnection = defineConnection({ id: "cancel-root", provider: "fixture-root", endpoint: rootEndpoint, model: "cancel-root-wire", credentialRef: "ROOT_MODEL_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 } });
  const childConnection = defineConnection({ id: "cancel-child", provider: "fixture-child", endpoint: childEndpoint, model: "cancel-child-wire", credentialRef: "CHILD_MODEL_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 } });
  const capabilities = [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.TOOL_LOOP, CAPABILITIES.STREAMING, ...DELEGATION_EXECUTION_CAPABILITIES];
  const evidence = [
    ...verifiedBaselineEvidence(rootConnection, { protocol: rootConnection.protocol, capabilities, routeGrants: [{ parentConnectionId: rootConnection.id, childConnectionId: rootConnection.id }, { parentConnectionId: rootConnection.id, childConnectionId: childConnection.id }] }),
    ...verifiedBaselineEvidence(childConnection, { protocol: childConnection.protocol, capabilities, routeGrants: [{ parentConnectionId: childConnection.id, childConnectionId: childConnection.id }, { parentConnectionId: childConnection.id, childConnectionId: rootConnection.id }] }),
  ];
  const sessionPlan = resolveSessionPlan({
    connections: [rootConnection, childConnection],
    evidence,
    policy: { defaultConnectionId: rootConnection.id, allowedChildConnectionIds: [rootConnection.id, childConnection.id], requiredCapabilities: DELEGATION_EXECUTION_CAPABILITIES, productExtensions: DELEGATION_EXECUTION_CAPABILITIES },
  });
  const bridge = new ModelBridge({ sessionPlan, connections: [rootConnection, childConnection], credentialResolver: async () => "fixture-provider-key", token: "fixture-bridge-key" });
  const bridgeAddress = await bridge.start();
  activateBridge(bridge, sessionPlan, "authority");
  t.after(() => bridge.close());

  const root = mkdtempSync(join(tmpdir(), "novel-codex-cancel-integration-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const catalogPath = join(root, "model-catalog.json");
  writeFileSync(catalogPath, `${JSON.stringify(compileCodexModelCatalog(sessionPlan, { baseInstructions: "Upstream Codex integration test instructions." }), null, 2)}\n`, { mode: 0o600 });
  writeFileSync(join(root, "config.toml"), compileCodexConfig({ sessionPlan, modelCatalogPath: catalogPath, bridge: { baseUrl: bridgeAddress.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" }, mcpServers: [] }), { mode: 0o600 });
  mkdirSync(join(root, "sqlite"));
  chmodSync(root, 0o700);

  const adapter = new CodexAppServerAdapter({
    command: [codex, "app-server"],
    cwd: dirname(root),
    env: { PATH: process.env.PATH, CODEX_HOME: root, CODEX_SQLITE_HOME: join(root, "sqlite"), AGENT_ENGINE_BRIDGE_TOKEN: bridgeAddress.token },
    model: rootConnection.id,
    provider: rootConnection.provider,
  });
  t.after(() => adapter.close());
  await adapter.ensureSession();
  const turn = adapter.sendTurn([{ type: "text", text: "Delegate this slow task and wait for the child." }], { disableEnvironment: true });
  await within(childStarted, 10_000, "delegated child model request did not start");
  bridge.cancelActiveRequests();
  const interruption = await adapter.interruptTurn();
  assert.equal(interruption.interrupted, true);
  const result = await turn;
  assert.equal(result.turn?.status, "interrupted");
  await within(childAborted, 5_000, "delegated child request was not aborted");
  assert.ifError(downstreamFailure);
});
