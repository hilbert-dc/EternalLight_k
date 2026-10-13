import { createMcpAuthorityFixture } from "./helpers/mcp-authority.mjs";
import { nativeTestBinary } from "./helpers/native-binaries.mjs";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { test } from "node:test";
import { defineConnection, getCodec } from "@eternallight/agent-engine";

async function listTools(t, operation) {
  const authority=await createMcpAuthorityFixture(t,"test-capability");
  const child = spawn(nativeTestBinary("novel-workspace-mcp"), [], {
    env: { ...authority.env, PATH: process.env.PATH, NOVEL_CODEX_RUNTIME_URL: "http://127.0.0.1:1",
      NOVEL_CODEX_CANDIDATE_TOKEN: "test-candidate", NOVEL_CODEX_TASK_CAPABILITY: "test-capability", NOVEL_CODEX_OPERATION: operation },
    stdio: ["pipe", "pipe", "pipe"],
  });
  t.after(() => child.kill());
  const listed = new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("MCP tools/list timed out")), 5000);
    const finish = (error, value) => { clearTimeout(timer); if (error) reject(error); else resolve(value); };
    child.once("error", error => finish(error));
    child.once("exit", () => finish(new Error("MCP exited")));
    createInterface({ input: child.stdout }).on("line", line => {
      try { const response = JSON.parse(line); if (response.id === 1) finish(null, response.result.tools); }
      catch (error) { finish(error); }
    });
  });
  child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id: 1, method: "tools/list", params: {} }) + "\n");
  return listed;
}

test("native MCP discovery modes preserve their exclusive contract through the Responses codec", { timeout: 15000 }, async t => {
  const tools = await listTools(t, "advisory");
  const discovery = tools.find(tool => tool.name === "discover_creative_capabilities");
  assert.ok(discovery, "The coordinator catalog must expose discovery");
  const codec = getCodec("responses");
  const connection = defineConnection({ id: "discovery-check", provider: "fixture", model: "discovery-check", endpoint: "https://example.invalid",
    credentialRef: "TEST_KEY", protocol: "responses", declaredLimits: { contextTokens: 64000, outputTokens: 4000 } });
  const prepared = codec.createRequest({ model: connection.id, input: [], tools: [{ type: "function", name: discovery.name,
    parameters: discovery.inputSchema }], stream: false }, connection, "not-a-real-key");
  const response = args => ({ id: "discovery", status: "completed", output: [{ type: "function_call", call_id: "discover-call",
    name: discovery.name, arguments: JSON.stringify(args) }] });
  for (const args of [{}, { query: "正文" }, { roleId: "candidate_writer" }]) {
    assert.deepEqual(codec.parseResponse(response(args), prepared.context), response(args));
  }
  assert.throws(() => codec.parseResponse(response({ query: "正文", roleId: "candidate_writer" }), prepared.context), /invalid_tool_arguments/);
});

test("native MCP knowledge contract preserves structured candidates through the Responses codec", { timeout: 15000 }, async (t) => {
  const tools = await listTools(t, "modelKnowledge");
  assert.ok(!tools.some(tool => tool.name === "discover_creative_capabilities"), "Discovery must not grant a worker coordinator tools");
  const tool = tools.find(tool => tool.name === "publish_knowledge_batch");
  // Codex compacts larger MCP schemas; that can erase nested event evidence requirements.
  assert.ok(Buffer.byteLength(JSON.stringify(tool.inputSchema)) <= 5000, "knowledge schema must preserve nested checks across Codex tool registration");
  const codec = getCodec("responses");
  const connection = defineConnection({ id: "schema-check", provider: "fixture", model: "schema-check", endpoint: "https://example.invalid",
    credentialRef: "TEST_KEY", protocol: "responses", declaredLimits: { contextTokens: 64000, outputTokens: 4000 } });
  const prepared = codec.createRequest({ model: connection.id, input: [], tools: [{ type: "function", name: tool.name, parameters: tool.inputSchema }], stream: false }, connection, "not-a-real-key");
  const evidence = { quote: "她藏信。", start: 0, end: 4 };
  const args = { expectedRevision: 0, batchId: "batch",
    entities: [{ id: "letter", kind: "item", displayName: "信", aliases: [], evidence },
      { id: "hiding", kind: "event", displayName: "藏信", aliases: [], evidence }],
    assertions: [{ kind: "event", subjectId: "hiding", predicate: "hide", storyTime: null, evidence,
      value: { participants: [{ role: "item", entityId: "letter" }], preconditions: [], effects: [], untilExclusive: null } }] };
  const response = value => ({ id: "response", status: "completed", output: [{ type: "function_call", call_id: "call", name: tool.name, arguments: JSON.stringify(value) }] });
  assert.deepEqual(codec.parseResponse(response(args), prepared.context), response(args));
  const quotes = structuredClone(args);
  for (const item of [...quotes.entities, ...quotes.assertions]) item.evidence = { quote: evidence.quote };
  delete quotes.entities[0].aliases;
  assert.deepEqual(codec.parseResponse(response(quotes), prepared.context), response(quotes));
  for (const assertion of [
    { kind: "assertion", subjectId: "letter", predicate: "action", value: "hidden", evidence },
    { kind: "state", subjectId: "letter", predicate: "hidden", value: true, evidence,
      validFrom: null, stateSchema: { cardinality: "single", valueType: { type: "boolean" } } },
  ]) {
    const sample = { ...args, assertions: [assertion] };
    assert.deepEqual(codec.parseResponse(response(sample), prepared.context), response(sample));
  }
  for (const field of ["entities", "assertions"]) {
    const malformed = structuredClone(args);
    malformed[field] = malformed[field].map(JSON.stringify);
    assert.throws(() => codec.parseResponse(response(malformed), prepared.context), /invalid_tool_arguments/);
  }
  const malformed = structuredClone(args);
  malformed.entities[0].evidence = [evidence];
  assert.throws(() => codec.parseResponse(response(malformed), prepared.context), /invalid_tool_arguments/);
  const missingSource = structuredClone(args);
  delete missingSource.assertions[0].evidence;
  assert.throws(() => codec.parseResponse(response(missingSource), prepared.context), /invalid_tool_arguments/);
  const complete = tools.find(tool => tool.name === "complete_task");
  const completionRequest = codec.createRequest({ model: connection.id, input: [], tools: [{ type: "function", name: complete.name, parameters: complete.inputSchema }], stream: false }, connection, "not-a-real-key");
  const completion = { id: "completed", status: "completed", output: [{ type: "function_call", call_id: "complete-call", name: complete.name,
    arguments: JSON.stringify({ expectedRevision: 0, summary: "Source-bound knowledge candidates persisted." }) }] };
  assert.deepEqual(codec.parseResponse(completion, completionRequest.context), completion);
  for (const [name, parameters] of [
    ["read_context", { budget: 24, byteBudget: 60000 }],
    ["search_story", { query: "密函", kind: "draft", entityName: "林夕", entityId: "linxi", cursor: "source-cursor", limit: 2 }],
  ]) {
    const selected = tools.find(tool => tool.name === name);
    const request = codec.createRequest({ model: connection.id, input: [], tools: [{ type: "function", name, parameters: selected.inputSchema }], stream: false }, connection, "not-a-real-key");
    const call = { id: name, status: "completed", output: [{ type: "function_call", call_id: `${name}-call`, name,
      arguments: JSON.stringify({ expectedRevision: 0, ...parameters }) }] };
    assert.deepEqual(codec.parseResponse(call, request.context), call);
  }
});
