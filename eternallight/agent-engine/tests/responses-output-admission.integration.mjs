import assert from "node:assert/strict";
import { mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { CodexAppServerClient, ModelBridgeProcess, TaskBudget, compileCodexConfig, compileCodexModelCatalog,
  createExecutionLease, defineConnection, resolveSessionPlan } from "../src/index.mjs";
import { verifiedBaselineEvidence } from "../src/testing/index.mjs";

// Real upstream App Server + native bridge, local provider only. No paid model.
for (const rejectedText of ["界".repeat(90000), "abcdefghijklmnopqrstuvwxyz".repeat(11000)])
test("real Codex rejects oversized items and preserves long high-token history", { timeout: 30000 }, async t => {
  const directory = await mkdtemp(join(tmpdir(), "engine-output-admission-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const inputs = [];
  const item = { type: "reasoning", id: "rs_rejected", summary: [], content: [{ type: "reasoning_text", text: rejectedText }], encrypted_content: "opaque-rejected" };
  const retained = { ...item, id: "rs_retained", content: [{ type: "reasoning_text", text: "界🙂".repeat(6000) }], encrypted_content: "opaque-retained" };
  const provider = createServer(async (request, response) => {
    const chunks = [];
    for await (const chunk of request) chunks.push(chunk);
    const body = JSON.parse(Buffer.concat(chunks));
    inputs.push(body.input);
    const answer = { type: "message", id: "answer", role: "assistant", status: "completed",
      content: [{ type: "output_text", text: "New attempt preserved.", annotations: [] }] };
    const outputs = inputs.length === 1 ? [item] : inputs.length === 2 ? [retained, answer] : [answer];
    response.writeHead(200, { "content-type": "text/event-stream" });
    for (const event of [
      { type: "response.created", response: { id: "admission-fixture" } },
      ...(inputs.length === 1 ? [
        { type: "response.output_item.added", output_index: 0, item: { ...item, content: [] } },
        { type: "response.reasoning_text.delta", item_id: item.id, output_index: 0, content_index: 0, delta: "Incremental preview." },
      ] : []),
      ...outputs.map((output, output_index) => ({ type: "response.output_item.done", output_index, item: output })),
      { type: "response.completed", response: { id: "admission-fixture", status: "completed", output: outputs, usage: { input_tokens: 10, output_tokens: 10, total_tokens: 20 } } },
    ]) response.write(`data: ${JSON.stringify(event)}\n\n`);
    response.end("data: [DONE]\n\n");
  });
  await new Promise(resolve => provider.listen(0, "127.0.0.1", resolve));
  t.after(() => new Promise(resolve => { provider.closeAllConnections(); provider.close(resolve); }));
  const connection = defineConnection({ id: "fixture", provider: "fixture", model: "fixture", protocol: "responses",
    endpoint: `http://127.0.0.1:${provider.address().port}`, credentialRef: "FIXTURE", declaredLimits: { contextTokens: 64000, outputTokens: 8192 } });
  const sessionPlan = resolveSessionPlan({ connections: [connection], evidence: verifiedBaselineEvidence(connection), policy: {} });
  const observations = [];
  const bridge = new ModelBridgeProcess({ sessionPlan, connections: [connection], credentialResolver: async () => "fixture",
    responseObserver: value => observations.push(value) });
  t.after(() => bridge.close());
  const budget = new TaskBudget();
  const address = await bridge.start();
  await bridge.beginTaskBudget(budget);
  await bridge.beginExecutionLease(createExecutionLease({ sessionPlan, runId: "admission-real-codex", budgetId: budget.id }));
  const catalog = join(directory, "models.json");
  await writeFile(catalog, JSON.stringify(compileCodexModelCatalog(sessionPlan, { baseInstructions: "Answer concisely." })));
  await writeFile(join(directory, "config.toml"), compileCodexConfig({ sessionPlan, modelCatalogPath: catalog,
    bridge: { baseUrl: address.baseUrl, credentialRef: "AGENT_ENGINE_BRIDGE_TOKEN" }, mcpServers: [] }));
  const executable = process.platform === "win32" ? "codex.exe" : "codex";
  const codex = fileURLToPath(new URL(`../../../codex-rs/target/debug/${executable}`, import.meta.url));
  const environment = Object.fromEntries([
    "PATH", "PATHEXT", "SystemRoot", "WINDIR", "ComSpec", "TEMP", "TMP", "TMPDIR", "LANG", "LC_ALL",
    "SSL_CERT_FILE", "SSL_CERT_DIR", "HTTP_PROXY", "HTTPS_PROXY", "NO_PROXY",
  ].filter(key => typeof process.env[key] === "string" && process.env[key]).map(key => [key, process.env[key]]));
  const client = new CodexAppServerClient({ command: [codex, "app-server"], cwd: directory,
    env: { ...environment, CODEX_HOME: directory, CODEX_SQLITE_HOME: directory, AGENT_ENGINE_BRIDGE_TOKEN: address.token },
    model: sessionPlan.defaultModel, turnTimeoutMs: 15000 });
  t.after(() => client.close());
  const events = [];
  client.subscribeEvents(event => events.push(event));
  await assert.rejects(client.sendTurn([{ type: "text", text: "First analysis attempt." }], { disableEnvironment: true }), /provider_output_item_limit/);
  assert.equal(inputs.length, 1, "the deterministic admission error must not trigger reconnect retries");
  assert.equal(events.some(event => event.method === "item/reasoning/textDelta"), true);
  assert.equal(events.some(event => event.method === "item/completed" && event.params?.item?.type === "reasoning"), false);
  assert.equal((await client.sendTurn([{ type: "text", text: "Start a fresh bounded analysis." }], { disableEnvironment: true })).message, "New attempt preserved.");
  assert.equal(inputs.length, 2);
  assert.equal(inputs[1].some(input => input.type === "reasoning" || input.id === item.id), false);
  assert.equal(JSON.stringify(inputs[1]).includes("opaque-rejected"), false);
  assert.equal((await client.sendTurn([{ type: "text", text: "Keep the accepted history exactly." }], { disableEnvironment: true })).message, "New attempt preserved.");
  assert.equal(inputs.length, 3);
  assert.deepEqual(inputs[2].find(input => input.id === retained.id), retained);
  await client.close();
  const rollouts = (await readdir(join(directory, "sessions"), { recursive: true })).filter(path => path.endsWith(".jsonl"));
  assert.equal(rollouts.length, 1);
  const history = (await readFile(join(directory, "sessions", rollouts[0]), "utf8")).trim().split("\n").map(line => JSON.parse(line));
  assert.equal(history.some(entry => entry.type === "response_item" && entry.payload.id === item.id), false);
  const retainedTurnId = events.find(event => event.method === "item/completed" && event.params?.item?.id === retained.id)?.params?.turnId;
  assert.equal(typeof retainedTurnId, "string");
  assert.deepEqual(history.find(entry => entry.type === "response_item" && entry.payload.id === retained.id)?.payload,
    { ...retained, internal_chat_message_metadata_passthrough: { turn_id: retainedTurnId } });
  assert.deepEqual(observations.map(({ status, errorCode }) => ({ status, errorCode })),
    [{ status: "failed", errorCode: "provider_output_item_limit" }, { status: "completed", errorCode: undefined }, { status: "completed", errorCode: undefined }]);
});
