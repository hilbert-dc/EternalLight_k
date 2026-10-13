import { mcpRequestIdentity } from "../mcp-authority.mjs";
import { nativeTestBinary } from "./helpers/native-binaries.mjs";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { mkdtemp, mkdir, readdir, readFile, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { ModelBridge, PersistentTaskBudget, compileCodexConfig, compileCodexModelCatalog,
  createExecutionLease, defineConnection, resolveSessionPlan } from "@eternallight/agent-engine";
import { verifiedBaselineEvidence } from "@eternallight/agent-engine/test-support";
import { CodexAppServerAdapter } from "../codex-adapter.mjs";
import { taskProviderBudgetOptions } from "../task-provider-budget.mjs";
import { createMcpStartupWaiter } from "./helpers/mcp-startup.mjs";
import { modelInputTokenBudget } from "../../shared/author-input-budget.mjs";
import { executorValidatedTools } from "../executor-validation-policy.mjs";
import { createBoundedOutputSender } from "../bounded-output-correction.mjs";

// Real Core/bridge/MCP/Runtime and a persistent Work budget. Only the provider
// is synthetic: this proves tool-error correction, not a real model's judgement.
for (const correctionKind of ["MCP validation", "output admission"])
test(`real Core corrects ${correctionKind} in one task, thread and original budget`, { timeout: 240000 }, async t => {
  const root = await mkdtemp(join(tmpdir(), "novel-draft-correction-"));
  const suffix = process.platform === "win32" ? ".exe" : "";
  const binary = name => nativeTestBinary(name);
  const codex = fileURLToPath(new URL(`../../../../codex-rs/target/debug/codex${suffix}`, import.meta.url));
  const authorToken = "draft-correction-author-123456789", candidateToken = "draft-correction-candidate-123456789";
  const capability = "draft-correction-capability-123456789";
  const requests = [], writes = [], mcpEvents = [];
  let adapter, bridge, startup, budget, failure, binding, base, original;
  const runtime = spawn(binary("novel-codex-story-runtime"), ["--port", "0", "--db", join(root, "story.sqlite3"),
    "--engine-budget-db", join(root, "engine-budget.sqlite3"), "--token-stdin"], { stdio: ["pipe", "pipe", "pipe"] });
  runtime.stderr.resume();
  const originPromise = new Promise((resolve, reject) => {
    const lines = createInterface({ input: runtime.stdout });
    lines.on("line", line => { if (line.startsWith("Story Runtime listening on ")) resolve(line.slice("Story Runtime listening on ".length)); });
    runtime.once("error", reject); runtime.once("exit", () => reject(new Error("Runtime exited before listening")));
  });
  runtime.stdin.end(`${JSON.stringify({ authorToken, candidateToken })}\n`);
  const prose = [
    "林夕沿着潮湿的石阶走进旧港她把密函藏进衣襟听见远处船钟响起守门人认出了信封上的印记却没有追问来历她记起父亲临别时的嘱托决定先找到失踪的信使再向城中的议会交出证据",
    "门外传来三声暗号林夕没有立即开门她先检查窗边留下的脚印确认来人走过码头的泥地旧友隔着木门说出约定的地名她才收起短刀打开门把信中的航线铺在桌上两人决定天亮之前赶往灯塔",
  ].map(seed => seed.repeat(40).match(/\p{Script=Han}/gu).slice(0, 2250).join("").replace(/(.{50})/gu, "$1。\n"));
  assert.equal(prose.join("").match(/\p{Script=Han}/gu).length, 4500);
  const corrected = { chapterId: "chapter-one", chapterNumber: 1, title: "留下密函",
    completeness: "chapterComplete", sceneGroups: [
      { id: "opening", sceneId: null, body: ` \n${prose[0]} `, locked: false, writerGuidance: "保留密函线索" },
      { id: "aftermath", sceneId: null, body: `\t${prose[1]}\n`, locked: false },
    ] };
  const canonical = { ...corrected, body: `${prose[0].trim()}\n\n---\n\n${prose[1].trim()}` };
  const invalid = { ...structuredClone(corrected), body: correctionKind === "output admission"
    ? canonical.body.repeat(20) : "这份重复正文不能作为第二份来源。" };
  let origin;
  const receiver = createServer(async (incoming, response) => {
    try {
      const chunks = []; for await (const chunk of incoming) chunks.push(chunk);
      const raw = Buffer.concat(chunks);
      if (incoming.method === "POST" && incoming.url.endsWith("/drafts/checkpoints")) writes.push(JSON.parse(raw));
      const forwarded = await fetch(`${origin}${incoming.url}`, { method: incoming.method, headers: {
        authorization: incoming.headers.authorization, "content-type": "application/json",
        ...Object.fromEntries(Object.entries(incoming.headers).filter(([name]) => name.startsWith("x-novel-"))),
      }, ...(raw.length ? { body: raw } : {}) });
      response.writeHead(forwarded.status, { "content-type": "application/json" }); response.end(await forwarded.text());
    } catch (error) { failure = error; response.writeHead(500); response.end("{}"); }
  });
  const request = async (method, path, payload, actor = authorToken) => {
    const response = await fetch(`${origin}${path}`, { method, headers: { authorization: `Bearer ${actor}`, "content-type": "application/json",
      ...(actor === candidateToken ? { "x-novel-task-capability": capability } : {}) },
      ...(payload === undefined ? {} : { body: JSON.stringify(payload) }) });
    const value = await response.json(); assert.ok(response.ok, `${method} ${path}: ${response.status} ${JSON.stringify(value)}`); return value;
  };
  const provider = createServer(async (incoming, response) => {
    try {
      const chunks = []; for await (const chunk of incoming) chunks.push(chunk);
      const body = JSON.parse(Buffer.concat(chunks)); requests.push(body);
      const sequence = requests.length;
      const namespace = body.tools.find(tool => tool.type === "namespace" && tool.name === "mcp__novel_workspace");
      const tool = namespace?.tools.find(tool => tool.type === "function" && tool.name === "save_draft_checkpoint");
      assert.ok(tool, `the real MCP advertised schema must reach Core: ${JSON.stringify(body.tools.map(t => ({type:t.type,name:t.name,tools:t.tools?.map(s=>s.name)})))}`);
      const outputs = body.input.filter(item => item.type === "function_call_output");
      let item;
      if (sequence === 1 || sequence === 2) {
        assert.equal(writes.length, 0, "the invalid payload must never dispatch an HTTP write");
        if (sequence === 2) {
          const output = outputs.find(item => item.call_id === "invalid-draft");
          if (correctionKind === "MCP validation") {
            assert.ok(output, "the genuine error must keep the original call ID");
            assert.match(JSON.stringify(output.output), /invalid_draft_payload/);
            assert.match(JSON.stringify(output.output), /provide exactly one of body or sceneGroups/);
          } else {
            assert.equal(output, undefined, "rejected provider output cannot execute a tool or create a receipt");
            assert.equal(JSON.stringify(body.input).includes("invalid-draft"), false, "rejected output cannot pollute the original thread");
            assert.match(JSON.stringify(body.input), /10K/);
          }
          const workspace = await request("GET", `${base}/creative-workspace`);
          assert.equal(workspace.chapters[0].candidate.id, original.revisionId);
          assert.equal(workspace.chapters[0].candidate.body, "林夕烧掉密函。");
        }
        item = { type: "function_call", id: sequence === 1 ? "bad-item" : "good-item",
          call_id: sequence === 1 ? "invalid-draft" : "corrected-draft", namespace: namespace.name, name: tool.name,
          arguments: JSON.stringify({ expectedRevision: binding.expectedRevision, expectedCandidateRevisionId: original.revisionId,
            idempotencyKey: "one-corrected-save", payload: sequence === 1 ? invalid : corrected }) };
        if (sequence === 1 && correctionKind === "output admission") {
          assert.ok(Buffer.byteLength(JSON.stringify(item)) > 262144, "oversized duplicate prose must exercise the actual bridge byte limit");
        }
        if (sequence === 2) {
          const outputItemTokens = modelInputTokenBudget(JSON.stringify(item));
          assert.ok(Buffer.byteLength(JSON.stringify(item)) <= 262144, "4500-Han single-source call must fit the current output item cap");
          t.diagnostic(`Synthetic corrected call: 4500 Han, ${Buffer.byteLength(item.arguments)} argument UTF-8 bytes, ${outputItemTokens} conservative output item tokens; real bridge admission and complete Native readback required.`);
        }
      } else {
        assert.equal(sequence, 3, "one error, one correction and one final answer");
        assert.equal(writes.length, 1);
        const output = outputs.find(item => item.call_id === "corrected-draft");
        assert.ok(output); assert.doesNotMatch(JSON.stringify(output.output), /invalid_draft_payload|TaskOutputForbidden/);
        const workspace = await request("GET", `${base}/creative-workspace`);
        const saved = workspace.chapters[0].candidate;
        const rendered = /^Wall time: [0-9.]+ seconds\nOutput:\s*([\s\S]*)$/.exec(output.output);
        assert.ok(rendered, "Core retains its actual MCP output wrapper");
        const receipt = JSON.parse(rendered[1]);
        assert.deepEqual(Object.keys(receipt).sort(), ["chapterId", "checkpointId", "completeness", "instruction", "nextAction", "recoveryOperation", "revisionId", "saved", "savedSource", "version"]);
        assert.equal(receipt.recoveryOperation.outcome, "committed");
        assert.equal(receipt.recoveryOperation.callId, "corrected-draft");
        assert.equal(receipt.saved, true);
        assert.equal(receipt.completeness, "chapterComplete");
        assert.equal(receipt.savedSource.status, "available");
        assert.equal(receipt.savedSource.revisionId, saved.id);
        assert.equal(receipt.savedSource.contentRead, false);
        assert.equal(receipt.savedSource.read.arguments.sourceId, receipt.savedSource.sourceId);
        assert.equal(receipt.nextAction, "complete_task");
        assert.equal(typeof receipt.instruction, "string");
        assert.equal(receipt.chapterId, saved.chapterId);
        assert.equal(receipt.revisionId, saved.id);
        assert.equal(receipt.version, saved.version);
        assert.ok(receipt.checkpointId.startsWith("checkpoint-"));
        assert.ok(modelInputTokenBudget(JSON.stringify({ ...receipt, body: saved.body, sceneGroups: saved.sceneGroups })) > 10000,
          "the old full DTO would unnecessarily repeat over 10K estimated tokens for this saved chapter");
        const { recoveryOperation: proof, ...saveReceipt } = receipt;
        assert.ok(modelInputTokenBudget(JSON.stringify(saveReceipt)) < 500, "save receipt stays bounded without echoing prose");
        assert.ok(modelInputTokenBudget(JSON.stringify(output)) < 1000, "receipt plus exact recovery proof stays bounded");
        assert.equal(receipt.body, undefined); assert.equal(receipt.sceneGroups, undefined);
        assert.deepEqual({ chapterId: saved.chapterId, title: saved.title, body: saved.body, sceneGroups: saved.sceneGroups },
          { chapterId: canonical.chapterId, title: canonical.title, body: canonical.body, sceneGroups: corrected.sceneGroups });
        item = { type: "message", id: "final-item", role: "assistant", status: "completed",
          content: [{ type: "output_text", text: "修订已保存。", annotations: [] }] };
      }
      const id = `response-${sequence}`;
      const events = [{ type: "response.created", response: { id } }, { type: "response.output_item.done", output_index: 0, item },
        { type: "response.completed", response: { id, status: "completed", output: [item], usage: { input_tokens: 3, output_tokens: 2, total_tokens: 5 } } }];
      response.writeHead(200, { "content-type": "text/event-stream" }); response.end(events.map(event => `data: ${JSON.stringify(event)}\n\n`).join(""));
    } catch (error) { failure = error; response.writeHead(400); response.end(JSON.stringify({ error: { message: "fixture assertion failed" } })); }
  });
  t.after(async () => {
    startup?.dispose();
    if (adapter) { await adapter.finishTaskOperation(); adapter.close(); }
    if (bridge) await bridge.close(); budget?.close();
    for (const server of [provider, receiver]) if (server.listening) await new Promise(resolve => { server.closeAllConnections(); server.close(resolve); });
    if (runtime.exitCode === null && runtime.signalCode === null) { const stopped = new Promise(resolve => runtime.once("exit", resolve)); runtime.kill(); await stopped; }
    await rm(root, { recursive: true, force: true });
  });
  origin = await originPromise;
  const { project } = await request("POST", "/api/projects", { title: "工具纠错验收", logline: "", medium: "novel", idempotencyKey: "draft-correction-project" }); base = `/api/projects/${project.id}`;
  original = await request("POST", `${base}/drafts/checkpoints`, { expectedRevision: 0, expectedCandidateRevisionId: null,
    idempotencyKey: "original", payload: { chapterId: "chapter-one", chapterNumber: 1, title: "密函", body: "林夕烧掉密函。" } });
  await Promise.all([receiver, provider].map(server => new Promise(resolve => server.listen(0, "127.0.0.1", resolve))));
  const connection = defineConnection({ id: "fixture", provider: "fixture", model: "fixture-v1", endpoint: `http://127.0.0.1:${provider.address().port}`,
    credentialRef: "FIXTURE_KEY", protocol: "responses", declaredLimits: { contextTokens: 131072, outputTokens: 8192 } });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: { allowedMcpServerIds: ["novel_workspace"] } });
  bridge = new ModelBridge({mcpAuthority:true,requestConfigurationProvider:({threadId})=>mcpRequestIdentity(adapter,threadId), sessionPlan, connections: [connection], credentialResolver: async () => "fixture", executorValidatedTools }); const address = await bridge.start();
  const catalog = join(root, "catalog.json"); await mkdir(join(root, "sqlite"));
  await writeFile(catalog, JSON.stringify(compileCodexModelCatalog(sessionPlan, { baseInstructions: "Revise only the bound chapter. Correct a payload rejected by MCP without changing the task binding." })));
  const keys = ["NOVEL_CODEX_RUNTIME_URL", "NOVEL_CODEX_CANDIDATE_TOKEN", "NOVEL_CODEX_TASK_CAPABILITY", "NOVEL_CODEX_OPERATION","NOVEL_CODEX_ENGINE_AUTHORITY_URL","NOVEL_CODEX_ENGINE_AUTHORITY_TOKEN"];
  await writeFile(join(root, "config.toml"), compileCodexConfig({ sessionPlan, modelCatalogPath: catalog,
    bridge: { baseUrl: address.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" },
    mcpServers: [{ id: "novel_workspace", command: binary("novel-workspace-mcp"), args: [], envVars: keys, approvalMode: "approve", required: true, startupTimeoutMs: 90000 }] }));
  const environment = Object.fromEntries(["PATH", "PATHEXT", "SystemRoot", "WINDIR", "ComSpec", "TEMP", "TMP", "TMPDIR", "LANG", "LC_ALL"]
    .filter(key => process.env[key]).map(key => [key, process.env[key]]));
  adapter = new CodexAppServerAdapter({ command: [codex, "app-server"], cwd: root, env: { ...environment, CODEX_HOME: root,
    CODEX_SQLITE_HOME: join(root, "sqlite"), AGENT_ENGINE_BRIDGE_TOKEN:address.token,NOVEL_CODEX_ENGINE_AUTHORITY_URL:address.baseUrl,NOVEL_CODEX_ENGINE_AUTHORITY_TOKEN:address.token,
    NOVEL_CODEX_RUNTIME_URL: `http://127.0.0.1:${receiver.address().port}`, NOVEL_CODEX_CANDIDATE_TOKEN: candidateToken,
    NOVEL_CODEX_TASK_CAPABILITY: capability, NOVEL_CODEX_OPERATION: "generateChapter" }, model: connection.id });
  adapter.startProcess(); startup = createMcpStartupWaiter({ serverName: "novel_workspace", process: adapter.process, timeoutMs: 90000 });
  adapter.setCallbacks({ onEvent: event => { startup.onEvent(event); if (event.params?.item?.type === "mcpToolCall") mcpEvents.push(event.params.item); } });
  await adapter.ensureSession(); await startup.ready;
  const workspace = await request("GET", `${base}/creative-workspace`);
  const authorizationId = randomUUID();
  const task = await request("POST", `${base}/tasks`, { operation: "generateChapter", target: {
    type: "chapter", chapterId: "chapter-one", chapterNumber: 1, readingChapter: 1, sceneOrder: 1, scope: { type: "wholeChapter" } },
    threadId: adapter.threadId, inputRevision: 0, authorIntent: "把烧信改为藏信，整章修订。",
    capabilityHash: createHash("sha256").update(capability).digest("hex"), capabilityExpiresAt: Math.floor(Date.now() / 1000) + 600,
    initialBudget: { authorizationId, authorMessageId: randomUUID(), expectedBaseSnapshotId: workspace.headSnapshotId,
      expectedSourceDigest: workspace.sourceDigest, maxRequests: 3, maxTokens: 200000, deadlineAt: Math.floor(Date.now() / 1000) + 600 } });
  const grant = await request("GET", `${base}/tasks/${task.id}/provider-budget`);
  const authorized = await request("GET", `${base}/work-tasks/${grant.workTaskId}/budget-authorization`);
  budget = new PersistentTaskBudget({ databasePath: join(root, "engine-budget.sqlite3"), runId: task.id,
    ...taskProviderBudgetOptions(grant, { taskId: task.id, workTaskId: grant.workTaskId, deadlineAt: grant.deadlineAt }) });
  await bridge.beginTaskBudget(budget); await bridge.beginExecutionLease(createExecutionLease({ sessionPlan, runId: task.id, budgetId: budget.id }));
  binding = { taskId: task.id, projectId: project.id, threadId: adapter.threadId, expectedRevision: 0 };
  let result;
  const originalProcess = adapter.process, originalThread = adapter.threadId;
  const sender = createBoundedOutputSender(adapter, task, path => request("GET", path), () => {});
  try { result = await sender([{ type: "text", text: "把本章烧信改为藏信，保存整章修订。" }], { disableEnvironment: true }); }
  catch (error) { throw failure ?? error; }
  assert.equal(adapter.process, originalProcess); assert.equal(adapter.threadId, originalThread);
  assert.ifError(failure); assert.equal(result.turn.status, "completed");
  const usage = budget.snapshot();
  assert.equal(usage.requests, 3);
  assert.equal(usage.tokens, 15, "all three provider receipts are charged, including the rejected output");
  assert.equal(usage.unknownTokens, 0, "each synthetic response supplies an authoritative usage receipt");
  assert.equal(requests.length, 3); assert.equal(writes.length, 1);
  const { projectId: _projectId, ...runtimeBinding } = binding;
  assert.deepEqual(writes[0], { ...runtimeBinding, expectedCandidateRevisionId: original.revisionId, idempotencyKey: "one-corrected-save", payload: canonical });
  await request("POST", `${base}/tasks/${task.id}/complete`, { threadId: binding.threadId, summary: "整章修订已保存" }, candidateToken);
  budget.complete();
  const charged = await request("POST", `${base}/work-tasks/${grant.workTaskId}/provider-usage`, { taskId: task.id });
  assert.deepEqual(charged.charges.map(charge => ({ taskId: charge.taskId, budgetId: charge.budgetId,
    requests: charge.requests, tokens: charge.tokens, unknownTokens: charge.unknownTokens })),
  [{ taskId: task.id, budgetId: budget.id, requests: 3, tokens: usage.tokens, unknownTokens: usage.unknownTokens }]);
  assert.deepEqual(await request("GET", `${base}/work-tasks/${grant.workTaskId}/budget-authorization`), authorized, "correction must not issue a new budget authorization");
  await adapter.finishTaskOperation();
  const rollouts = (await readdir(join(root, "sessions"), { recursive: true })).filter(path => path.endsWith(".jsonl")); assert.equal(rollouts.length, 1);
  const items = (await readFile(join(root, "sessions", rollouts[0]), "utf8")).trim().split("\n").map(line => JSON.parse(line))
    .filter(entry => entry.type === "response_item").map(entry => entry.payload);
  const badCall = items.find(item => item.type === "function_call" && item.call_id === "invalid-draft");
  if (correctionKind === "MCP validation") {
    assert.ok(badCall); assert.deepEqual(JSON.parse(badCall.arguments).payload, invalid);
    assert.ok(items.some(item => item.type === "function_call_output" && item.call_id === "invalid-draft" && JSON.stringify(item.output).includes("invalid_draft_payload")));
    assert.ok(mcpEvents.some(item => item.id === "invalid-draft" && item.status === "failed"
      && JSON.stringify(item).includes("invalid_draft_payload")), "Core must expose the genuine MCP error as a failed tool item");
  } else {
    assert.equal(badCall, undefined); assert.equal(mcpEvents.some(item => item.id === "invalid-draft"), false);
  }
  const goodCall = items.find(item => item.type === "function_call" && item.call_id === "corrected-draft");
  assert.ok(goodCall); assert.deepEqual(JSON.parse(goodCall.arguments).payload, corrected);
});
