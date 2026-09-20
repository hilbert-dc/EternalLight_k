import assert from "node:assert/strict";
import { chmodSync, mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

import {
  CAPABILITIES,
  ModelBridge,
  TaskBudget,
  compileCodexConfig,
  compileCodexModelCatalog,
  createExecutionLease,
  defineConnection,
  resolveSessionPlan,
} from "@eternallight/agent-engine";
import { verifiedBaselineEvidence } from "@eternallight/agent-engine/test-support";
import { CodexAppServerAdapter } from "../codex-adapter.mjs";

const codex = fileURLToPath(new URL("../../../../codex-rs/target/debug/codex", import.meta.url));
const mcpServer = fileURLToPath(new URL("./fixture-mcp-server.mjs", import.meta.url));

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

function json(response, payload) {
  response.writeHead(200, { "content-type": "application/json" });
  response.end(JSON.stringify(payload));
}

function responsesStream(response, output, id) {
  const envelope = { id, object: "response", status: "completed", output, usage: { input_tokens: 8, output_tokens: 3, total_tokens: 11 } };
  response.writeHead(200, { "content-type": "text/event-stream" });
  response.write(`event: response.created\ndata: ${JSON.stringify({ type: "response.created", response: { id } })}\n\n`);
  for (const [output_index, item] of output.entries()) {
    response.write(`event: response.output_item.done\ndata: ${JSON.stringify({ type: "response.output_item.done", output_index, item })}\n\n`);
  }
  response.end(`event: response.completed\ndata: ${JSON.stringify({ type: "response.completed", response: envelope })}\n\ndata: [DONE]\n\n`);
}

function protocolReply(protocol, response, { final, toolName }) {
  if (protocol === "responses") {
    const output = final
      ? [{ id: "msg_final", type: "message", role: "assistant", status: "completed", content: [{ type: "output_text", text: "Protocol loop complete", annotations: [] }] }]
      : [{ id: "fc_read", type: "function_call", call_id: "call_read", namespace: "mcp__novel_workspace", name: "read_context", arguments: "{}", status: "completed" }];
    responsesStream(response, output, final ? "resp_final" : "resp_tool");
    return;
  }
  if (protocol === "messages") {
    json(response, {
      id: final ? "msg-final" : "msg-tool",
      stop_reason: final ? "end_turn" : "tool_use",
      content: final ? [{ type: "text", text: "Protocol loop complete" }] : [{ type: "tool_use", id: "call_read", name: toolName, input: {} }],
      usage: { input_tokens: 8, output_tokens: 3 },
    });
    return;
  }
  json(response, {
    responseId: final ? "gen-final" : "gen-tool",
    candidates: [{ finishReason: "STOP", content: { parts: final ? [{ text: "Protocol loop complete" }] : [{ functionCall: { id: "call_read", name: toolName, args: {} } }] } }],
    usageMetadata: { promptTokenCount: 8, candidatesTokenCount: 3, totalTokenCount: 11 },
  });
}

function exposedToolName(protocol, body) {
  if (protocol === "responses") return "mcp__novel_workspace__read_context";
  if (protocol === "messages") return body.tools?.find((tool) => tool.name?.endsWith("__read_context"))?.name;
  return body.tools?.[0]?.functionDeclarations?.find((tool) => tool.name?.endsWith("__read_context"))?.name;
}

for (const protocol of ["responses", "messages", "generate-content"]) {
  test(`real Codex App Server completes an MCP loop through the ${protocol} codec`, { timeout: 30_000 }, async (t) => {
    const requests = [];
    let downstreamFailure = null;
    const downstream = createServer(async (request, response) => {
      try {
        const body = await readRequest(request);
        requests.push({ path: request.url, body });
        const final = JSON.stringify(body).includes("fixture-context");
        const toolName = exposedToolName(protocol, body);
        if (!final) assert.ok(toolName, `${protocol} request must expose the namespaced MCP tool`);
        protocolReply(protocol, response, { final, toolName });
      } catch (error) {
        downstreamFailure = error;
        response.writeHead(500, { "content-type": "application/json" });
        response.end(JSON.stringify({ error: { message: error.message } }));
      }
    });
    const endpoint = await listen(downstream);
    t.after(() => new Promise((resolve) => { downstream.closeAllConnections(); downstream.close(resolve); }));

    const connection = defineConnection({ id: `${protocol}-model`, provider: "fixture", endpoint, model: `${protocol}-wire-model`, credentialRef: "FIXTURE_MODEL_KEY", protocol, declaredLimits: { contextTokens: 64_000, outputTokens: 4_000 } });
    const evidence = verifiedBaselineEvidence(connection, {
      protocol,
      capabilities: [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.TOOL_LOOP, ...(protocol === "responses" ? [CAPABILITIES.STREAMING] : [])],
    });
    const sessionPlan = resolveSessionPlan({ connections: [connection], evidence, policy: { defaultConnectionId: connection.id, forceBridge: true, allowedMcpServerIds: ["novel_workspace"] } });
    const bridge = new ModelBridge({ sessionPlan, connections: [connection], credentialResolver: async () => "fixture-provider-key", token: "fixture-bridge-key" });
    const bridgeAddress = await bridge.start();
    const budget = new TaskBudget({ maxRequests: 16, maxTokens: 200_000 });
    bridge.beginTaskBudget(budget);
    bridge.beginExecutionLease(createExecutionLease({ sessionPlan, runId: `protocol-${protocol}`, budgetId: budget.id }));
    t.after(() => bridge.close());

    const root = mkdtempSync(join(tmpdir(), `novel-codex-${protocol}-integration-`));
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
      if (event.method === "mcpServer/startupStatus/updated" && event.params?.name === "novel_workspace" && event.params?.status === "ready") markMcpReady();
    } });
    await adapter.ensureSession();
    await mcpReady;
    const result = await adapter.sendTurn([{ type: "text", text: "Read context, then answer." }], { disableEnvironment: true });

    assert.ifError(downstreamFailure);
    assert.equal(result.message, "Protocol loop complete");
    assert.equal(requests.length, 2);
    if (protocol === "responses") assert.match(requests[0].path, /\/v1\/responses$/);
    if (protocol === "messages") assert.match(requests[0].path, /\/v1\/messages$/);
    if (protocol === "generate-content") {
      assert.match(requests[0].path, /:generateContent\?key=fixture-provider-key$/);
      const functionResponse = requests[1].body.contents.flatMap((content) => content.parts).find((part) => part.functionResponse)?.functionResponse;
      assert.equal(functionResponse?.name, "n20__mcp__novel_workspace__read_context");
    }
  });
}
