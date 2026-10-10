import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { createInterface } from "node:readline";
import test from "node:test";
import { ModelBridgeProcess, TaskBudget, compileCodexConfig, compileCodexModelCatalog,
  createExecutionLease, defineConnection, resolveSessionPlan } from "@eternallight/agent-engine";
import { verifiedBaselineEvidence } from "@eternallight/agent-engine/test-support";
import { CodexAppServerAdapter } from "../codex-adapter.mjs";
import { NOVEL_TOOL_OUTPUT_POLICY } from "../codex-runtime.mjs";
import { nativeTestBinary } from "./helpers/native-binaries.mjs";

test("actual Core preserves bounded evidence with a small model output allowance and still limits oversized results", { timeout: 90000 }, async t => {
  const directory = await mkdtemp(join(tmpdir(), "novel-tool-output-policy-"));
  let bridge, adapter, failure, phase = "page";
  const calls = [], namespace = "mcp__output_fixture";
  const quote = "actual bounded evidence ".repeat(1300);
  const payload = { issueId: "bounded-issue", entries: [{ evidence: [{ quote }] }], totalCount: 1, nextCursor: null };
  const encoded = JSON.stringify(payload);
  assert.ok(Buffer.byteLength(encoded) > 30000 && Buffer.byteLength(encoded) < 32768);
  const mcp = join(directory, "fixture.mjs");
  await writeFile(mcp, `import {createInterface} from 'node:readline';
const payload=${encoded};
for await(const line of createInterface({input:process.stdin})){
 const q=JSON.parse(line); if(q.id===undefined)continue; let result;
 if(q.method==='initialize')result={protocolVersion:'2025-06-18',capabilities:{tools:{}},serverInfo:{name:'output-fixture',version:'1'}};
 else if(q.method==='tools/list')result={tools:[{name:'read_work_issues',description:'Read an isolated test evidence page.',inputSchema:{type:'object',properties:{size:{type:'string',enum:['page','oversize']}},required:['size'],additionalProperties:false}}]};
 else if(q.method==='tools/call')result={content:[{type:'text',text:q.params.arguments.size==='page'?JSON.stringify(payload):JSON.stringify({quote:'x'.repeat(60000)})}]};
 else if(['resources/list','resources/templates/list','prompts/list'].includes(q.method))result={resources:[],resourceTemplates:[],prompts:[]};
 else{process.stdout.write(JSON.stringify({jsonrpc:'2.0',id:q.id,error:{code:-32601,message:'unsupported'}})+'\\n');continue;}
 process.stdout.write(JSON.stringify({jsonrpc:'2.0',id:q.id,result})+'\\n');
}
`);
  const provider = createServer(async (request, response) => {
    try {
      const chunks = []; for await (const chunk of request) chunks.push(chunk);
      const body = JSON.parse(Buffer.concat(chunks)); calls.push({ phase, body });
      let item;
      if (phase === "compact") {
        item = { type: "message", id: `summary-${calls.length}`, role: "assistant", status: "completed",
          content: [{ type: "output_text", text: "Continue the isolated evidence test.", annotations: [] }] };
      } else {
        const previous = body.input.findLast(entry => entry.type === "function_call_output");
        const previousCall = body.input.findLast(entry => entry.type === "function_call");
        if (calls.filter(call => call.phase === phase).length === 2) {
          assert.ok(previous && previousCall);
          assert.equal(JSON.parse(previousCall.arguments).size, phase === "oversize" ? "oversize" : "page");
          const text = typeof previous.output === "string" ? previous.output
            : previous.output.filter(part => part.type === "input_text").map(part => part.text).join("\n");
          if (phase === "oversize") {
            assert.match(text, /truncated/);
            assert.ok(Buffer.byteLength(text) < 45000);
            assert.ok(!text.includes("x".repeat(60000)));
          } else {
            assert.ok(text.includes(encoded), "the model must receive every evidence byte, not a clipped JSON report");
            assert.deepEqual(JSON.parse(text.slice(text.indexOf("Output:") + 7).trim()), payload);
          }
          item = { type: "message", id: `answer-${calls.length}`, role: "assistant", status: "completed",
            content: [{ type: "output_text", text: `observed ${phase}`, annotations: [] }] };
        } else {
          const tool = body.tools.find(entry => entry.type === "namespace" && entry.name === namespace)?.tools.find(entry => entry.name === "read_work_issues");
          assert.ok(tool);
          item = { type: "function_call", id: `read-${calls.length}`, call_id: `read-${calls.length}`,
            namespace, name: tool.name, arguments: JSON.stringify({ size: phase === "oversize" ? "oversize" : "page" }) };
        }
      }
      const result = { id: `r-${calls.length}`, status: "completed", output: [item], usage: { input_tokens: 10, output_tokens: 5, total_tokens: 15 } };
      response.writeHead(200, { "content-type": "text/event-stream" });
      for (const event of [{ type: "response.created", response: { id: result.id } },
        { type: "response.output_item.done", output_index: 0, item }, { type: "response.completed", response: result }]) response.write(`data: ${JSON.stringify(event)}\n\n`);
      response.end("data: [DONE]\n\n");
    } catch (error) { failure ??= error; response.writeHead(500); response.end(JSON.stringify({ error: { message: error.message } })); }
  });
  t.after(async () => {
    adapter?.close(); await bridge?.close(); provider.closeAllConnections();
    if (provider.listening) await new Promise(resolve => provider.close(resolve));
    await rm(directory, { recursive: true, force: true });
  });
  await new Promise(resolve => provider.listen(0, "127.0.0.1", resolve));
  const connection = defineConnection({ id: "fixture", provider: "fixture", model: "fixture", protocol: "responses",
    endpoint: `http://127.0.0.1:${provider.address().port}`, credentialRef: "FIXTURE", declaredLimits: { contextTokens: 64000, outputTokens: 512 } });
  const plan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection),
    policy: { allowedMcpServerIds: ["output_fixture"] } });
  bridge = new ModelBridgeProcess({ sessionPlan: plan, connections: [connection], credentialResolver: async () => "fixture" });
  const budget = new TaskBudget({ maxRequests: 12, maxTokens: 128000 }), address = await bridge.start();
  await bridge.beginTaskBudget(budget);
  await bridge.beginExecutionLease(createExecutionLease({ sessionPlan: plan, runId: "output-test", budgetId: budget.id }));
  const catalog = join(directory, "models.json");
  await writeFile(catalog, JSON.stringify(compileCodexModelCatalog(plan, { baseInstructions: "Only run the isolated read-only evidence test." })));
  await writeFile(join(directory, "config.toml"), compileCodexConfig({ sessionPlan: plan, modelCatalogPath: catalog,
    bridge: { baseUrl: address.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" },
    mcpServers: [{ id: "output_fixture", command: process.execPath, args: [mcp], approvalMode: "approve", required: true,
      startupTimeoutMs: 30000, tools: NOVEL_TOOL_OUTPUT_POLICY }] }));
  const codex = fileURLToPath(new URL(`../../../../codex-rs/target/debug/${process.platform === "win32" ? "codex.exe" : "codex"}`, import.meta.url));
  const environment = Object.fromEntries(["PATH", "PATHEXT", "SystemRoot", "WINDIR", "ComSpec", "TEMP", "TMP", "TMPDIR", "LANG", "LC_ALL"]
    .filter(key => typeof process.env[key] === "string").map(key => [key, process.env[key]]));
  adapter = new CodexAppServerAdapter({ command: [codex, "app-server"], cwd: directory, modelRoute: plan.defaultModel,
    env: { ...environment, CODEX_HOME: directory, CODEX_SQLITE_HOME: join(directory, "sqlite"), AGENT_ENGINE_BRIDGE_TOKEN: address.token } });
  const send = () => adapter.sendTurn([{ type: "text", text: `Read isolated ${phase} evidence.` }], { disableEnvironment: true });
  await send(); assert.ifError(failure);
  const threadId = adapter.threadId;
  phase = "compact"; await adapter.compactThread(threadId); assert.ifError(failure);
  phase = "after-compact"; await send(); assert.ifError(failure);
  phase = "oversize"; await send(); assert.ifError(failure);
  assert.equal(adapter.threadId, threadId);
  for (const name of ["page", "after-compact", "oversize"]) assert.equal(calls.filter(call => call.phase === name).length, 2);
  assert.equal(budget.snapshot().requests, calls.length);
});

test("actual Core reads complete Native prose pages and cursors with a small model output allowance before and after compaction", { timeout: 180000 }, async t => {
  const directory = await mkdtemp(join(tmpdir(), "novel-native-read-budget-"));
  const authorToken = "read-budget-author-fixture-123456789", candidateToken = "read-budget-candidate-fixture-123456789";
  const capability = "read-budget-task-capability-123456789", namespace = "mcp__novel_workspace";
  let runtime, bridge, adapter, provider, failure, task, phase = "page", state;
  const calls = [], completed = [];
  t.after(async () => {
    adapter?.close(); await bridge?.close();
    if (provider?.listening) await new Promise(resolve => { provider.closeAllConnections(); provider.close(resolve); });
    if (runtime && runtime.exitCode === null && runtime.signalCode === null) {
      const stopped = new Promise(resolve => runtime.once("exit", resolve)); runtime.kill(); await stopped;
    }
    await rm(directory, { recursive: true, force: true });
  });
  runtime = spawn(nativeTestBinary("novel-codex-story-runtime"), ["--port", "0", "--db", join(directory, "story.sqlite3"),
    "--engine-budget-db", join(directory, "engine.sqlite3"), "--token-stdin"], { stdio: ["pipe", "pipe", "pipe"] });
  let runtimeError = "";
  runtime.stderr.on("data", chunk => { runtimeError = (runtimeError + chunk).slice(-2000); });
  const origin = await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(Error(`Read-budget Runtime startup timed out: ${runtimeError}`)), 90000);
    const finish = (error, value) => { clearTimeout(timer); error ? reject(error) : resolve(value); };
    createInterface({ input: runtime.stdout }).on("line", line => {
      if (line.startsWith("Story Runtime listening on ")) finish(null, line.slice("Story Runtime listening on ".length));
    });
    runtime.once("error", finish); runtime.once("exit", () => finish(Error("Read-budget Runtime exited before listening")));
    runtime.stdin.end(`${JSON.stringify({ authorToken, candidateToken })}\n`);
  });
  const http = async (path, payload) => {
    const response = await fetch(`${origin}${path}`, { method: payload ? "POST" : "GET",
      headers: { authorization: `Bearer ${authorToken}`, "content-type": "application/json" },
      ...(payload ? { body: JSON.stringify(payload) } : {}) });
    const value = await response.json(); assert.ok(response.ok, `${path}: ${response.status} ${JSON.stringify(value)}`); return value;
  };
  const { project } = await http("/api/projects", { title: "原文页额度隔离验收", logline: "临时只读来源测试", medium: "novel", idempotencyKey: randomUUID() });
  const base = `/api/projects/${project.id}`;
  const prose = '她看见纸上写着"不要开门"，雨水沿门锁滴落。\n'.repeat(100);
  const original = await http(`${base}/drafts/checkpoints`, { expectedRevision: 0, expectedCandidateRevisionId: null,
    idempotencyKey: "prose-source", payload: { chapterId: "chapter-one", chapterNumber: 1, title: "雨夜来信", body: prose.trim(),
      sceneGroups: [{ id: "rain-letter", sceneId: null, body: prose.trim(), locked: true }] } });
  const parse = body => {
    const previous = body.input.findLast(entry => entry.type === "function_call_output");
    assert.ok(previous, "the complete actual MCP result must reach the model");
    const text = typeof previous.output === "string" ? previous.output
      : previous.output.filter(part => part.type === "input_text").map(part => part.text).join("\n");
    assert.doesNotMatch(text, /\[\.\.\.\d+ tokens truncated\.\.\.\]/);
    const value = JSON.parse(text.slice(text.indexOf("Output:") + 7).trim());
    assert.equal(value.isError, undefined); assert.equal(value.error, undefined); return value;
  };
  provider = createServer(async (request, response) => {
    try {
      const chunks = []; for await (const chunk of request) chunks.push(chunk);
      const body = JSON.parse(Buffer.concat(chunks)); calls.push({ phase, body });
      const finish = () => ({ type: "message", id: `answer-${calls.length}`, role: "assistant", status: "completed",
        content: [{ type: "output_text", text: "隔离原文读取已完成。", annotations: [] }] });
      const next = (name, args, step) => {
        state.step = step;
        assert.ok(body.tools.find(entry => entry.type === "namespace" && entry.name === namespace)?.tools.find(entry => entry.name === name));
        return { type: "function_call", id: `read-${calls.length}`, call_id: `read-${calls.length}`, namespace, name,
          arguments: JSON.stringify({ projectId: project.id, taskId: task.id, threadId: task.threadId, expectedRevision: task.inputRevision, ...args }) };
      };
      let item;
      if (phase === "compact") item = finish();
      else if (!state) {
        state = { step: "context", raw: "", rawRepeats: 0, parsedRepeats: 0, statusRepeats: 0, offset: 0, fields: new Map(), cursors: new Set(), rawPages: 0, parsedPages: 0 };
        item = next("read_context", { byteBudget: 24000 }, "context");
      } else {
        const value = parse(body);
        if (state.step === "context") {
          assert.ok(Buffer.byteLength(JSON.stringify(value)) > 2457, "the actual Native context exceeds the old 512-output presentation allowance");
          assert.equal(value.executionIdentity.rootThreadId, task.threadId);
          state.source = value.taskSources.entries.find(entry => entry.sectionPath === "/relevantText");
          assert.ok(state.source); item = next("read_work_status", {}, "status");
        } else if (state.step === "status") {
          assert.equal(value.mode, "conversation");
          if (++state.statusRepeats < 3) item = next("read_work_status", {}, "status");
          else {
            assert.equal(value.progressHint.kind, "repeatedUnchangedRead");
            item = next("read_task_source", { sourceId: state.source.sourceId }, "raw");
          }
        } else if (state.step === "raw") {
          assert.equal(value.sourceId, state.source.sourceId); assert.equal(value.contentHash, state.source.contentHash);
          if (value.byteRange.start === 0) assert.ok(Buffer.byteLength(JSON.stringify(value)) > 2457, "actual prose page exceeds the old 512-output presentation allowance");
          if (value.byteRange.start === 0 && state.rawRepeats++ < 2) item = next("read_task_source", { sourceId: value.sourceId }, "raw");
          else {
            if (value.byteRange.start === 0) assert.equal(value.progressHint.kind, "repeatedUnchangedRead");
            assert.equal(value.byteRange.start, Buffer.byteLength(state.raw)); state.raw += value.payloadPart; state.rawPages++;
            assert.equal(value.byteRange.end, Buffer.byteLength(state.raw));
            if (value.nextCursor) {
              assert.ok(!state.cursors.has(value.nextCursor)); state.cursors.add(value.nextCursor);
              item = next("read_task_source", { sourceId: value.sourceId, cursor: value.nextCursor }, "raw");
            } else {
              assert.equal(value.byteRange.total, Buffer.byteLength(state.raw));
              assert.equal(createHash("sha256").update(state.raw).digest("hex"), state.source.contentHash);
              const text = JSON.parse(state.raw); assert.equal(text[0].sourceRevisionId, original.revisionId);
              assert.equal(text[0].content.sceneGroups[0].body, prose.trim());
              item = next("read_source_content", { sourceId: value.sourceId }, "parsed");
            }
          }
        } else if (state.step === "parsed") {
          assert.equal(value.sourceId, state.source.sourceId); assert.equal(value.contentHash, state.source.contentHash);
          if (value.coverage.fromEntry === 0 && state.parsedRepeats++ < 2) item = next("read_source_content", { sourceId: value.sourceId }, "parsed");
          else {
            if (value.coverage.fromEntry === 0) assert.equal(value.progressHint.kind, "repeatedUnchangedRead");
            assert.equal(value.coverage.fromEntry, state.offset); state.offset = value.coverage.toEntry; state.parsedPages++;
            for (const field of value.entries) state.fields.set(field.path, Object.hasOwn(field, "value") ? field.value : (state.fields.get(field.path) ?? "") + field.text);
            if (value.nextCursor) item = next("read_source_content", { sourceId: value.sourceId, cursor: value.nextCursor }, "parsed");
            else {
              assert.equal(state.offset, value.coverage.totalEntries); assert.ok([...state.fields.values()].includes(prose.trim()));
              assert.ok(state.rawPages > 1 && state.parsedPages > 1); completed.push({ phase, sourceId: value.sourceId, hash: value.contentHash }); item = finish();
            }
          }
        } else throw Error(`unknown Native read phase ${state.step}`);
      }
      const result = { id: `r-${calls.length}`, status: "completed", output: [item], usage: { input_tokens: 10, output_tokens: 5, total_tokens: 15 } };
      response.writeHead(200, { "content-type": "text/event-stream" });
      for (const event of [{ type: "response.created", response: { id: result.id } },
        { type: "response.output_item.done", output_index: 0, item }, { type: "response.completed", response: result }]) response.write(`data: ${JSON.stringify(event)}\n\n`);
      response.end("data: [DONE]\n\n");
    } catch (error) { failure ??= error; response.writeHead(500); response.end(JSON.stringify({ error: { message: error.message } })); }
  });
  await new Promise(resolve => provider.listen(0, "127.0.0.1", resolve));
  const connection = defineConnection({ id: "fixture", provider: "fixture", model: "fixture", protocol: "responses",
    endpoint: `http://127.0.0.1:${provider.address().port}`, credentialRef: "FIXTURE", declaredLimits: { contextTokens: 131072, outputTokens: 512 } });
  const plan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: { allowedMcpServerIds: ["novel_workspace"] } });
  bridge = new ModelBridgeProcess({ sessionPlan: plan, connections: [connection], credentialResolver: async () => "fixture" });
  const budget = new TaskBudget({ maxRequests: 96, maxTokens: 1000000 }), address = await bridge.start();
  const catalog = join(directory, "models.json"); await writeFile(catalog, JSON.stringify(compileCodexModelCatalog(plan, { baseInstructions: "Only read the isolated Native prose pages." })));
  const codexConfig = compileCodexConfig({ sessionPlan: plan, modelCatalogPath: catalog,
    bridge: { baseUrl: address.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" }, mcpServers: [{ id: "novel_workspace",
      command: nativeTestBinary("novel-workspace-mcp"), args: [], approvalMode: "approve", required: true, startupTimeoutMs: 90000,
      inputSchemaMaxBytes: 40000,
      envVars: ["NOVEL_CODEX_RUNTIME_URL", "NOVEL_CODEX_CANDIDATE_TOKEN", "NOVEL_CODEX_TASK_CAPABILITY", "NOVEL_CODEX_OPERATION"], tools: NOVEL_TOOL_OUTPUT_POLICY }] });
  assert.match(codexConfig, /^tool_input_schema_max_bytes = 40000$/m);
  await writeFile(join(directory, "config.toml"), codexConfig);
  const codex = fileURLToPath(new URL(`../../../../codex-rs/target/debug/${process.platform === "win32" ? "codex.exe" : "codex"}`, import.meta.url));
  const environment = Object.fromEntries(["PATH", "PATHEXT", "SystemRoot", "WINDIR", "ComSpec", "TEMP", "TMP", "TMPDIR", "LANG", "LC_ALL"]
    .filter(key => typeof process.env[key] === "string").map(key => [key, process.env[key]]));
  adapter = new CodexAppServerAdapter({ command: [codex, "app-server"], cwd: directory, modelRoute: plan.defaultModel,
    env: { ...environment, CODEX_HOME: directory, CODEX_SQLITE_HOME: join(directory, "sqlite"), AGENT_ENGINE_BRIDGE_TOKEN: address.token,
      NOVEL_CODEX_RUNTIME_URL: origin, NOVEL_CODEX_CANDIDATE_TOKEN: candidateToken } });
  const { thread } = await adapter.createThread();
  const threadId = thread.id, metadataProcess = adapter.process;
  assert.equal(adapter.threadId, threadId);
  const threadName = "原文页额度隔离验收";
  const savedThread = await adapter.saveThreadName(threadId, threadName);
  assert.equal(savedThread.thread.id, threadId);
  assert.equal(savedThread.thread.name, threadName);
  const workspace = await http(`${base}/creative-workspace`);
  task = await http(`${base}/tasks`, { operation: "advisory", threadId, inputRevision: workspace.revision, target: { type: "project" },
    authorIntent: "只读检查当前第1章原文，不修改、不采用、不发布。", capabilityHash: createHash("sha256").update(capability).digest("hex"),
    capabilityExpiresAt: Math.floor(Date.now() / 1000) + 600,
    initialBudget: { authorizationId: randomUUID(), authorMessageId: randomUUID(), expectedBaseSnapshotId: workspace.headSnapshotId,
      expectedSourceDigest: workspace.sourceDigest, maxRequests: 96, maxTokens: 1000000, deadlineAt: Math.floor(Date.now() / 1000) + 600 } });
  assert.equal(task.threadId, threadId);
  await bridge.beginTaskBudget(budget); await bridge.beginExecutionLease(createExecutionLease({ sessionPlan: plan, runId: task.id, budgetId: budget.id }));
  adapter.prepareAdvisorySession(capability);
  const restored = await adapter.resumeThread(threadId);
  assert.equal(restored.thread.id, task.threadId);
  assert.equal(adapter.threadId, task.threadId);
  assert.ok(adapter.process);
  assert.notEqual(adapter.process, metadataProcess, "task execution must rebind the metadata-only process before enabling the required Native MCP");
  const send = async () => { await adapter.sendTurn([{ type: "text", text: "读取隔离的原文分页。" }], { disableEnvironment: true }); assert.ifError(failure); };
  await send();
  phase = "compact"; await adapter.compactThread(threadId); assert.ifError(failure);
  phase = "after-compact"; state = null; await send();
  assert.equal(adapter.threadId, threadId); assert.equal(completed.length, 2);
  assert.equal(completed[0].sourceId, completed[1].sourceId); assert.equal(completed[0].hash, completed[1].hash);
  assert.equal(budget.snapshot().requests, calls.length);
  assert.equal((await http(`${base}/creative-workspace`)).chapters[0].candidate.id, original.revisionId, "read-only validation never changes the manuscript");
});
