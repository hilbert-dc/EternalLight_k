import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { getEncoding } from "js-tiktoken";
import { CAPABILITIES, ModelBridge, PersistentTaskBudget, compileCodexConfig, compileCodexModelCatalog,
  createExecutionLease, defineConnection, resolveSessionPlan } from "@eternallight/agent-engine";
import { verifiedBaselineEvidence } from "@eternallight/agent-engine/test-support";
import { CodexAppServerAdapter } from "../codex-adapter.mjs";
import { taskProviderBudgetOptions } from "../task-provider-budget.mjs";

test("real Codex reconstructs frozen rule pages and saves a candidate without author adoption", { timeout: 45_000 }, async (t) => {
  const root = await mkdtemp(join(tmpdir(), "novel-rule-input-"));
  const suffix = process.platform === "win32" ? ".exe" : "";
  const binary = (name) => fileURLToPath(new URL(`../../native/target/debug/${name}${suffix}`, import.meta.url));
  const author = "rules-engine-author-token-1234567890";
  const candidate = "rules-engine-candidate-token-1234567";
  const capability = "rules-engine-capability-1234567890";
  let runtime, downstream, bridge, adapter, budget;
  t.after(async () => {
    await adapter?.close(); await bridge?.close();
    budget?.close();
    if (runtime && runtime.exitCode === null && runtime.signalCode === null) {
      const stopped = new Promise((resolve) => runtime.once("exit", resolve));
      runtime.kill(); await stopped;
    }
    if (downstream?.listening) await new Promise((resolve) => { downstream.closeAllConnections(); downstream.close(resolve); });
    await rm(root, { recursive: true, force: true });
  });
  runtime = spawn(binary("novel-codex-story-runtime"), ["--port", "0", "--db", join(root, "story.sqlite3"), "--engine-budget-db", join(root, "engine-budget.sqlite3"), "--token-stdin"], { stdio: ["pipe", "pipe", "pipe"] });
  const origin = await new Promise((resolve, reject) => {
    const lines = createInterface({ input: runtime.stdout });
    lines.on("line", (line) => { if (line.startsWith("Story Runtime listening on ")) resolve(line.slice("Story Runtime listening on ".length)); });
    runtime.once("error", reject); runtime.once("exit", () => reject(new Error("Runtime exited before listening")));
    runtime.stdin.end(`${JSON.stringify({ authorToken: author, candidateToken: candidate })}\n`);
  });
  const authorRequest = async (path, payload) => {
    const response = await fetch(`${origin}${path}`, { method: "POST", headers: { authorization: `Bearer ${author}`, "content-type": "application/json" }, body: JSON.stringify(payload) });
    const value = await response.json(); assert.ok(response.ok, JSON.stringify(value)); return value;
  };
  const { project } = await authorRequest("/api/projects", { title: "规则基线分页回环" });
  const path = `/api/projects/${project.id}`;
  const payload = { eventRules: [], definitions: Array.from({ length: 6 }, (_, index) => ({ predicate: `rule-${index}`, subjectKind: "character",
    schema: { cardinality: "single", valueType: { type: "text" } }, description: "🙂港".repeat(70), exceptions: [] })) };
  payload.eventRules.push({ eventType: "departure", description: "出发前的行动者状态", exceptions: [], condition: { type: "any", conditions: [
    { type: "state", role: "actor", subjectKind: "character", predicate: "rule-0", quantifier: "all", operator: "eq", operand: { type: "text", value: "准备就绪" } },
    { type: "not", condition: { type: "state", role: "actor", subjectKind: "character", predicate: "rule-1", quantifier: "any", operator: "ne", operand: { type: "text", value: "港城" } } },
  ] } });
  const saved = await authorRequest(`${path}/artifacts/propertyRules`, { expectedRevision: 0, expectedCandidateRevisionId: null, idempotencyKey: "base", payload });
  const encodings = [getEncoding("cl100k_base"), getEncoding("o200k_base")];
  let binding, descriptor, firstPage, candidateReceipt, failure, phase = "context", combined = "", pageCount = 0, maxTextBytes = 0;
  const tokenMaxima = [0, 0];
  downstream = createServer(async (request, response) => {
    try {
      const chunks = []; for await (const chunk of request) chunks.push(chunk);
      const body = JSON.parse(Buffer.concat(chunks).toString("utf8"));
      const tool = (name) => body.tools.find((entry) => entry.function?.name?.endsWith(`__${name}`) || entry.function?.name === name)?.function.name;
      assert.equal(tool("save_draft_checkpoint"), undefined);
      let name, args;
      const previous = body.messages.findLast((message) => message.role === "tool");
      const readResult = () => JSON.parse(previous.content.slice(previous.content.indexOf("\nOutput:") + 8).trim());
      if (phase === "context") {
        name = "read_context"; args = { ...binding }; phase = "first";
      } else if (phase === "first") {
        descriptor = readResult().taskSources.entries.find(entry => entry.sectionPath === "/operationContext/propertyRulesBase");
        assert.ok(descriptor?.sourceId, "the task must expose its frozen rule source");
        name = "read_task_source"; args = { ...binding, sourceId: descriptor.sourceId, cursor: null }; phase = "pages";
      } else if (phase === "pages") {
        const page = readResult(); firstPage ??= page; pageCount += 1;
        assert.ok(Buffer.byteLength(JSON.stringify(page)) <= 6500);
        maxTextBytes = Math.max(maxTextBytes, Buffer.byteLength(previous.content));
        for (const [index, encoding] of encodings.entries()) {
          const tokens = encoding.encode(previous.content).length;
          tokenMaxima[index] = Math.max(tokenMaxima[index], tokens);
          assert.ok(tokens < 10000);
        }
        assert.equal(page.contentHash, descriptor.contentHash);
        assert.deepEqual(page.byteRange, { start: Buffer.byteLength(combined), end: Buffer.byteLength(combined + page.payloadPart), total: descriptor.payloadBytes });
        combined += page.payloadPart;
        if (page.nextCursor) { name = "read_task_source"; args = { ...binding, sourceId: descriptor.sourceId, cursor: page.nextCursor }; }
        else {
          assert.equal(JSON.parse(combined).id, saved.revisionId);
          assert.deepEqual(JSON.parse(combined).payload, payload);
          assert.equal(createHash("sha256").update(combined).digest("hex"), descriptor.contentHash);
          const proposed = JSON.parse(combined).payload; proposed.definitions[0].description = "读回完整基线后提出的修改";
          name = "save_property_rules"; args = { ...binding, payload: proposed, expectedCandidateRevisionId: saved.revisionId, idempotencyKey: "model-candidate" }; phase = "saved";
        }
      } else if (phase === "saved") {
        candidateReceipt = readResult(); assert.ok(candidateReceipt.revisionId);
        name = "read_task_source"; args = { ...binding, sourceId: descriptor.sourceId, cursor: null }; phase = "frozen";
      } else if (phase === "frozen") {
        assert.deepEqual(readResult(), firstPage);
        name = "complete_task"; args = { ...binding, summary: "规则候选已形成，等待作者采用" }; phase = "done";
      } else {
        assert.equal(readResult().deliveryStatus, "ready_for_review");
        response.writeHead(200, { "content-type": "text/event-stream" });
        response.end(`data: ${JSON.stringify({ id: "answer", choices: [{ delta: { content: "规则候选已形成，尚未正式采用。" }, finish_reason: "stop" }] })}\n\ndata: [DONE]\n\n`); return;
      }
      const wireName = tool(name); assert.ok(wireName, name);
      response.writeHead(200, { "content-type": "text/event-stream" });
      response.write(`data: ${JSON.stringify({ id: phase, choices: [{ delta: { tool_calls: [{ index: 0, id: `rules-call-${phase}-${pageCount}`, function: { name: wireName, arguments: JSON.stringify(args) } }] } }] })}\n\n`);
      response.end(`data: ${JSON.stringify({ id: phase, choices: [{ delta: {}, finish_reason: "tool_calls" }] })}\n\ndata: [DONE]\n\n`);
    } catch (error) { failure = error; response.writeHead(500, { "content-type": "application/json" }); response.end(JSON.stringify({ error: { message: error.message } })); }
  });
  await new Promise((resolve, reject) => { downstream.once("error", reject); downstream.listen(0, "127.0.0.1", resolve); });
  const connection = defineConnection({ id: "rule-model", provider: "fixture", endpoint: `http://127.0.0.1:${downstream.address().port}`, model: "fixture-rules",
    credentialRef: "FIXTURE_KEY", protocol: "chat-completions", declaredLimits: { contextTokens: 64000, outputTokens: 4000 } });
  const evidence = verifiedBaselineEvidence(connection, { protocol: connection.protocol,
    capabilities: [CAPABILITIES.TEXT, CAPABILITIES.STRUCTURED_TOOL_CALLS, CAPABILITIES.TOOL_LOOP, CAPABILITIES.STREAMING] });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence, policy: { defaultConnectionId: connection.id, allowedMcpServerIds: ["novel_workspace"] } });
  bridge = new ModelBridge({ sessionPlan, connections: [connection], credentialResolver: async () => "fixture-key", token: "rule-bridge-key" });
  const address = await bridge.start();
  const home = join(root, "codex-home"); await mkdir(join(home, "sqlite"), { recursive: true, mode: 0o700 });
  const catalog = join(home, "catalog.json"); await writeFile(catalog, JSON.stringify(compileCodexModelCatalog(sessionPlan, { baseInstructions: "使用受任务绑定的候选工具；规则采用由作者决定。" })), { mode: 0o600 });
  await writeFile(join(home, "config.toml"), compileCodexConfig({ sessionPlan, modelCatalogPath: catalog, bridge: { baseUrl: address.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" },
    mcpServers: [{ id: "novel_workspace", command: binary("novel-workspace-mcp"), args: [], envVars: ["NOVEL_CODEX_RUNTIME_URL", "NOVEL_CODEX_CANDIDATE_TOKEN", "NOVEL_CODEX_TASK_CAPABILITY", "NOVEL_CODEX_OPERATION"], approvalMode: "approve" }] }), { mode: 0o600 });
  const codex = fileURLToPath(new URL(`../../../../codex-rs/target/debug/codex${suffix}`, import.meta.url));
  adapter = new CodexAppServerAdapter({ command: [codex, "app-server"], cwd: root, env: { PATH: process.env.PATH, CODEX_HOME: home, CODEX_SQLITE_HOME: join(home, "sqlite"),
    AGENT_ENGINE_BRIDGE_TOKEN: address.token, NOVEL_CODEX_RUNTIME_URL: origin, NOVEL_CODEX_CANDIDATE_TOKEN: candidate,
    NOVEL_CODEX_TASK_CAPABILITY: capability, NOVEL_CODEX_OPERATION: "generatePropertyRules" }, model: connection.id, provider: connection.provider });
  let ready; const started = new Promise((resolve) => { ready = resolve; });
  adapter.setCallbacks({ onEvent: (event) => { if (event.method === "mcpServer/startupStatus/updated" && event.params?.name === "novel_workspace" && event.params?.status === "ready") ready(); } });
  await adapter.ensureSession();
  const beforeResponse = await fetch(`${origin}${path}/creative-workspace`, { headers: { authorization: `Bearer ${author}` } });
  assert.equal(beforeResponse.ok, true);
  const before = await beforeResponse.json();
  const task = await authorRequest(`${path}/tasks`, { operation: "generatePropertyRules", inputRevision: 0, threadId: adapter.threadId, target: { type: "artifact" }, authorIntent: "完整基线上的规则候选",
    capabilityHash: createHash("sha256").update(capability).digest("hex"), capabilityExpiresAt: Math.floor(Date.now() / 1000) + 600,
    initialBudget: { authorizationId: randomUUID(), authorMessageId: randomUUID(), expectedBaseSnapshotId: before.headSnapshotId,
      expectedSourceDigest: before.sourceDigest, maxRequests: 16, maxTokens: 1_000_000, deadlineAt: Math.floor(Date.now()/1000)+600 } });
  const grantResponse = await fetch(`${origin}${path}/tasks/${task.id}/provider-budget`, { headers: { authorization: `Bearer ${author}` } });
  assert.equal(grantResponse.ok, true);
  const grant = await grantResponse.json();
  budget = new PersistentTaskBudget({ databasePath: join(root,"engine-budget.sqlite3"), runId: task.id,
    ...taskProviderBudgetOptions(grant, { taskId: task.id, workTaskId: `work-${task.id}`, deadlineAt: grant.deadlineAt }) });
  bridge.beginTaskBudget(budget);
  bridge.beginExecutionLease(createExecutionLease({ sessionPlan, runId: task.id, budgetId: budget.id }));
  binding = { taskId: task.id, projectId: project.id, threadId: adapter.threadId, expectedRevision: 0 }; await started;
  const result = await adapter.sendTurn([{ type: "text", text: "完整读取规则并形成候选，不采用。" }], { disableEnvironment: true }).catch((error) => { assert.ifError(failure); throw error; });
  assert.ifError(failure); assert.ok(pageCount > 1); assert.match(result.message, /尚未正式采用/);
  const recovery = await authorRequest(`${path}/tasks/recovery-snapshot`, { taskIds: [task.id] });
  assert.equal(recovery.operations.some((entry) => entry.tool_name === "read_task_source"), false);
  assert.equal(recovery.operations.find((entry) => entry.tool_name === "save_property_rules").state, "committed");
  const response = await fetch(`${origin}${path}/creative-workspace`, { headers: { authorization: `Bearer ${author}` } });
  const workspace = await response.json(); assert.ok(response.ok);
  assert.equal(workspace.artifacts.propertyRules.candidate.id, candidateReceipt.revisionId);
  assert.equal(workspace.artifacts.propertyRules.candidate.source, "agent");
  assert.deepEqual(workspace.artifacts.propertyRules.candidate.payload.eventRules, payload.eventRules);
  assert.equal(workspace.artifacts.propertyRules.accepted, null);
  budget.complete();
  const usage = await authorRequest(`${path}/work-tasks/work-${task.id}/provider-usage`, { taskId: task.id });
  assert.ok(usage.usedRequests > pageCount);
  t.diagnostic(`frozen rule source: ${pageCount} pages, max tool text ${maxTextBytes} bytes; cl100k_base/o200k_base token maxima ${tokenMaxima.join("/")}`);
});
