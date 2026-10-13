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
import { boundedContextFragments } from "../context-fragments.mjs";
import { CodexAppServerAdapter } from "../codex-adapter.mjs";

// Actual Core compaction + actual MCP, scripted local provider, no novel DB or
// paid API. This complements the full Native self-repair integration fixture.
test("actual Codex compaction preserves discovered schemas across a new execution lease", { timeout: 90000 }, async t => {
  const directory = await mkdtemp(join(tmpdir(), "novel-portable-compaction-"));
  let bridge, adapter, failure, phase = "first", echoCount = 0;
  const method = "FROZEN_BEGIN\n" + "独立核对原作，不降低作者标准。𠮷é🛶\n".repeat(230) + "FROZEN_END";
  const additionalContext = { frozenMethod: { kind: "untrusted", value: method } };
  const fragments = Object.values(boundedContextFragments(additionalContext));
  const requests = [], namespaceName = "mcp__portable_fixture";
  const mcp = join(directory, "fixture-mcp.mjs");
  await writeFile(mcp, `import { createInterface } from 'node:readline';
const specs = [
 {name:'discover',description:'Discover available echo metadata and load its schema.',inputSchema:{type:'object',properties:{query:{type:'string'}},required:['query'],additionalProperties:false}},
 {name:'echo',description:'Return the selected test text without side effects.',inputSchema:{type:'object',properties:{text:{type:'string'}},required:['text'],additionalProperties:false}}
];
for await (const line of createInterface({input:process.stdin})) {
 const request=JSON.parse(line); if(request.id===undefined)continue;
 let result;
 if(request.method==='initialize')result={protocolVersion:'2025-06-18',capabilities:{tools:{}},serverInfo:{name:'portable-fixture',version:'1'}};
 else if(request.method==='tools/list')result={tools:specs};
 else if(request.method==='tools/call')result={content:[{type:'text',text:JSON.stringify(request.params.name==='discover'?{toolDiscovery:{version:1,tools:[{name:'echo',namespace:null}]},query:request.params.arguments.query}:{echo:request.params.arguments.text})}]};
 else if(request.method==='resources/list')result={resources:[]};
 else if(request.method==='resources/templates/list')result={resourceTemplates:[]};
 else if(request.method==='prompts/list')result={prompts:[]};
 else {process.stdout.write(JSON.stringify({jsonrpc:'2.0',id:request.id,error:{code:-32601,message:'unsupported'}})+'\\n');continue;}
 process.stdout.write(JSON.stringify({jsonrpc:'2.0',id:request.id,result})+'\\n');
}
`);
  const server = createServer(async (request, response) => {
    try {
      const chunks = []; for await (const chunk of request) chunks.push(chunk);
      const body = JSON.parse(Buffer.concat(chunks)); requests.push({ phase, body, metadata: JSON.parse(request.headers["x-codex-turn-metadata"] ?? body.client_metadata?.["x-codex-turn-metadata"] ?? "{}") });
      const namespace = body.tools?.find(tool => tool.type === "namespace" && tool.name === namespaceName);
      const echo = namespace?.tools.find(tool => tool.name === "echo");
      if (phase === "first") {
        const text = body.input.flatMap(item => item.content ?? []).map(item => item.text ?? "").join("\n");
        for (const fragment of fragments) assert.ok(text.includes(fragment.value), `full frozen method missing in ${phase}`);
      }
      let item;
      if (phase === "compact") {
        assert.deepEqual(body.tools, []); item = { type: "message", id: `summary-${requests.length}`, role: "assistant", status: "completed",
          content: [{ type: "output_text", text: "The fixture previously discovered and called echo. Continue the same scoped test.", annotations: [] }] };
      } else if (!echo) {
        assert.ok(namespace?.tools.some(tool => tool.name === "discover"));
        item = { type: "function_call", id: `discovery-item-${requests.length}`, call_id: `discovery-${requests.length}`,
          namespace: namespaceName, name: "discover", arguments: JSON.stringify({ query: "只读回显" }) };
      } else if (echoCount++ === 0) {
        assert.ok(echo.parameters.required.includes("text")); assert.equal(echo.parameters.additionalProperties, false);
        item = { type: "function_call", id: `echo-item-${requests.length}`, call_id: `echo-${requests.length}`,
          namespace: namespaceName, name: "echo", arguments: JSON.stringify({ text: `ECHO_${phase}` }) };
      } else {
        item = { type: "message", id: `answer-${requests.length}`, role: "assistant", status: "completed",
          content: [{ type: "output_text", text: `ECHO_${phase}`, annotations: [] }] };
      }
      const result = { id: `response-${requests.length}`, status: "completed", output: [item], usage: { input_tokens: 5, output_tokens: 5, total_tokens: 10 } };
      response.writeHead(200, { "content-type": "text/event-stream" });
      for (const event of [{ type: "response.created", response: { id: result.id } },
        { type: "response.output_item.done", output_index: 0, item }, { type: "response.completed", response: result }]) response.write(`data: ${JSON.stringify(event)}\n\n`);
      response.end("data: [DONE]\n\n");
    } catch (error) { failure ??= error; response.writeHead(500); response.end(JSON.stringify({ error: { message: error.message } })); }
  });
  t.after(async () => { adapter?.close(); await bridge?.close(); server.closeAllConnections();
    if(server.listening)await new Promise(resolve => server.close(resolve)); await rm(directory, {recursive:true,force:true}); });
  await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
  const connection = defineConnection({ id: "fixture", provider: "fixture", model: "fixture", protocol: "responses",
    endpoint: `http://127.0.0.1:${server.address().port}`, credentialRef: "FIXTURE", declaredLimits: { contextTokens: 64000, outputTokens: 4000 } });
  const plan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: {
    allowedMcpServerIds: ["portable_fixture"], toolDiscovery: { version: 1, deferredNamespaces: [namespaceName],
      initialTools: [], discoveryTools: [{ namespace: namespaceName, name: "discover" }] } } });
  bridge = new ModelBridgeProcess({ sessionPlan: plan, connections: [connection], credentialResolver: async () => "fixture" });
  const budget = new TaskBudget({ maxRequests: 16, maxTokens: 128000 }), address = await bridge.start();
  await bridge.beginTaskBudget(budget);
  let lease = createExecutionLease({ sessionPlan: plan, runId: "portable", budgetId: budget.id }); await bridge.beginExecutionLease(lease);
  const catalog = join(directory, "models.json");
  await writeFile(catalog, JSON.stringify(compileCodexModelCatalog(plan, { baseInstructions: "Only perform the requested read-only fixture tool calls." })));
  await writeFile(join(directory, "config.toml"), compileCodexConfig({ sessionPlan: plan, modelCatalogPath: catalog,
    bridge: { baseUrl: address.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" },
    mcpServers: [{ id: "portable_fixture", command: process.execPath, args: [mcp], approvalMode: "approve", required: true, startupTimeoutMs: 30000 }] }));
  const codex = fileURLToPath(new URL(`../../../../codex-rs/target/debug/${process.platform === "win32" ? "codex.exe" : "codex"}`, import.meta.url));
  const environment = Object.fromEntries(["PATH", "PATHEXT", "SystemRoot", "WINDIR", "ComSpec", "TEMP", "TMP", "TMPDIR", "LANG", "LC_ALL"].filter(key => typeof process.env[key] === "string").map(key => [key, process.env[key]]));
  adapter = new CodexAppServerAdapter({ command: [codex, "app-server"], cwd: directory, modelRoute: plan.defaultModel,
    env: { ...environment, CODEX_HOME: directory, CODEX_SQLITE_HOME: join(directory, "sqlite"), AGENT_ENGINE_BRIDGE_TOKEN: address.token } });
  const send = async text => {
    try { return await adapter.sendTurn([{ type: "text", text }], { disableEnvironment: true, additionalContext }); }
    catch (error) { assert.ifError(failure); throw error; }
  };
  await send("请发现只读回显并验证当前文字。"); assert.ifError(failure);
  const rootThread = adapter.threadId;
  assert.equal(requests.filter(request => request.phase === "first").length, 3, "discover, echo, final are actual separate model requests");
  phase = "compact"; await adapter.compactThread(rootThread); assert.ifError(failure);
  const compact = requests.find(request => request.phase === "compact"); assert.ok(compact);
  assert.equal(compact.metadata.request_kind, "compaction", "the real Core owns the transport compaction marker");
  phase = "second"; echoCount = 0;
  await send("继续只读回显验收。"); assert.ifError(failure);
  assert.equal(requests.filter(request => request.phase === "second").length, 2, "after actual compaction schema remains loaded without rediscovery");
  await bridge.endExecutionLease(lease); lease = createExecutionLease({ sessionPlan: plan, runId: "new-execution", budgetId: budget.id }); await bridge.beginExecutionLease(lease);
  phase = "restarted"; echoCount = 0;
  await send("恢复后再验证只读回显。"); assert.ifError(failure);
  assert.equal(requests.filter(request => request.phase === "restarted").length, 2, "same caller retains validated schemas across execution leases");
  assert.equal(adapter.threadId, rootThread); assert.equal(budget.snapshot().requests, requests.length);
});
