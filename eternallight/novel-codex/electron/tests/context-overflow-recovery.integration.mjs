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

// Underreported historical usage reproduces the gap between Core's proactive
// threshold and Engine's complete encoded input admission. No paid provider.
for (const mode of ["recover", "cancel"]) {
  test(`hard context overflow ${mode} continues the original thread and budget through public RPCs`, { timeout: 45000 }, async t => {
    const directory = await mkdtemp(join(tmpdir(), "novel-context-recovery-"));
    let adapter, bridge, compactions = 0, interrupted;
    const requests = [], failures = [], terminal = [];
    const server = createServer(async (request, response) => {
      const chunks = []; for await (const chunk of request) chunks.push(chunk);
      const body = JSON.parse(Buffer.concat(chunks));
      const metadata = JSON.parse(request.headers["x-codex-turn-metadata"] ?? body.client_metadata?.["x-codex-turn-metadata"] ?? "{}");
      requests.push({ body, metadata });
      const compact = metadata.request_kind === "compaction";
      if (compact) {
        compactions++;
        if (mode === "cancel") { interrupted = adapter.interruptTurn(); return; }
      }
      const item = { type: "message", id: `answer-${requests.length}`, role: "assistant", status: "completed",
        content: [{ type: "output_text", text: compact ? "Saved task evidence remains in the original workspace. Continue the requested step." : "已检查素材。".repeat(850), annotations: [] }] };
      const result = { id: `response-${requests.length}`, status: "completed", output: [item],
        usage: { input_tokens: 5, output_tokens: 5, total_tokens: 10 } };
      response.writeHead(200, { "content-type": "text/event-stream" });
      for (const event of [{ type: "response.created", response: { id: result.id } },
        { type: "response.output_item.done", output_index: 0, item }, { type: "response.completed", response: result }]) response.write(`data: ${JSON.stringify(event)}\n\n`);
      response.end("data: [DONE]\n\n");
    });
    t.after(async () => { adapter?.close(); await bridge?.close(); server.closeAllConnections();
      if (server.listening) await new Promise(resolve => server.close(resolve)); await rm(directory, { recursive: true, force: true }); });
    await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
    const connection = defineConnection({ id: "fixture", provider: "fixture", model: "fixture", protocol: "responses",
      endpoint: `http://127.0.0.1:${server.address().port}`, credentialRef: "FIXTURE", declaredLimits: { contextTokens: 32000, outputTokens: 4000 } });
    const plan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: {} });
    bridge = new ModelBridgeProcess({ sessionPlan: plan, connections: [connection], credentialResolver: async () => "fixture",
      responseObserver: value => { if (value.status === "failed") failures.push(value); } });
    const budget = new TaskBudget({ maxRequests: 32, maxTokens: 200000 }), address = await bridge.start();
    await bridge.beginTaskBudget(budget);
    await bridge.beginExecutionLease(createExecutionLease({ sessionPlan: plan, runId: "capacity-recovery", budgetId: budget.id }));
    const catalog = join(directory, "models.json");
    await writeFile(catalog, JSON.stringify(compileCodexModelCatalog(plan, { baseInstructions: "Complete the current requested step; preserve prior saved evidence." })));
    await writeFile(join(directory, "config.toml"), compileCodexConfig({ sessionPlan: plan, modelCatalogPath: catalog,
      bridge: { baseUrl: address.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" }, mcpServers: [] }));
    const codex = fileURLToPath(new URL(`../../../../codex-rs/target/debug/${process.platform === "win32" ? "codex.exe" : "codex"}`, import.meta.url));
    const environment = Object.fromEntries(["PATH", "PATHEXT", "SystemRoot", "WINDIR", "ComSpec", "TEMP", "TMP", "TMPDIR", "LANG", "LC_ALL"].filter(key => typeof process.env[key] === "string").map(key => [key, process.env[key]]));
    adapter = new CodexAppServerAdapter({ command: [codex, "app-server"], cwd: directory, modelRoute: plan.defaultModel,
      env: { ...environment, CODEX_HOME: directory, CODEX_SQLITE_HOME: join(directory, "sqlite"), AGENT_ENGINE_BRIDGE_TOKEN: address.token } });
    adapter.callbacks.onRawEvent = event => { if (event.method === "turn/completed") terminal.push(event.params.turn); };
    let rootThread, attempts = 0, error, outcome;
    while (compactions === 0 && attempts < 8) {
      attempts++;
      try { outcome = await adapter.sendTurn([{ type: "text", text: `Check source step ${attempts}.` }], { disableEnvironment: true }); }
      catch (cause) { error = cause; break; }
      rootThread ??= adapter.threadId;
    }
    assert.ok(failures.some(value => value.errorCode === "context_input_budget_exceeded"), "fixture must cross actual Engine wire admission");
    assert.equal(compactions, 1, "Host must compact once after the rejected turn");
    assert.equal(adapter.threadId, rootThread);
    assert.equal(terminal.length, attempts + (mode === "recover" ? 2 : 1), "failed, compaction and continuation turns stay explicit");
    const history = await adapter.request("thread/read", { threadId: rootThread, includeTurns: true });
    const authorItems = history.thread.turns.flatMap(turn => turn.items ?? []).filter(item => item.type === "userMessage");
    assert.equal(authorItems.length, attempts, "recovery must not replay or invent author inputs");
    if (mode === "recover") {
      assert.ifError(error);
      assert.equal(terminal.at(-1).status, "completed");
      const compactIndex = requests.findIndex(value => value.metadata.request_kind === "compaction");
      assert.ok(compactIndex >= 0 && compactIndex < requests.length - 1);
      assert.equal(budget.snapshot().requests, requests.length);
      assert.equal(budget.snapshot().reservedTokens, 0);
    } else {
      await interrupted;
      assert.equal(outcome?.turn?.status, "interrupted");
      assert.equal(terminal.at(-1).status, "interrupted");
      assert.equal(requests.at(-1).metadata.request_kind, "compaction", "cancel cannot resume sampling");
    }
  });
}
