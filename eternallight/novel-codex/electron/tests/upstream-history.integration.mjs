import assert from "node:assert/strict";
import { test } from "node:test";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "node:http";
import { CodexAppServerAdapter } from "../codex-adapter.mjs";
import { terminateEngineProcess } from "../engine-process-lifecycle.mjs";

test("rebuilt Core provides precise item lookup and revision-checked durable read state without inference", { timeout: 30_000 }, async t => {
  const home = await mkdtemp(join(tmpdir(), "novel-upstream-history-"));
  const binary = fileURLToPath(new URL("../../../../codex-rs/target/debug/codex", import.meta.url));
  await writeFile(join(home, "config.toml"), 'model = "fixture"\nmodel_provider = "fixture"\n[model_providers.fixture]\nname = "Local contract fixture"\nbase_url = "http://127.0.0.1:9"\nwire_api = "responses"\n', { mode: 0o600 });
  const adapter = new CodexAppServerAdapter({ command: `"${binary}" app-server`, cwd: home,
    env: { ...process.env, CODEX_HOME: home }, model: "fixture", modelRoute: "fixture", provider: "fixture" });
  t.after(async () => {
    const child = adapter.process;
    adapter.close();
    await terminateEngineProcess(child);
    await rm(home, { recursive: true, force: true });
  });
  await adapter.ensureInitialized({ localOnly: true });
  const { thread } = await adapter.request("thread/start", { model: "fixture", cwd: home });
  await adapter.request("thread/name/set", { threadId: thread.id, name: "Read state contract" });
  const result = await adapter.request("thread/read", { threadId: thread.id, includeTurns: false });
  assert.equal(result.thread.id, thread.id);
  assert.equal(typeof result.readState?.revision, "string");
  const updated = await adapter.updateThreadReadState(thread.id, { operation: "unread", expectedRevision: result.readState.revision });
  assert.deepEqual(updated.readState.firstUnread, { type: "threadStart" });
  await assert.rejects(adapter.updateThreadReadState(thread.id, { operation: "read", expectedRevision: result.readState.revision }));
  const read = await adapter.updateThreadReadState(thread.id, { operation: "read", expectedRevision: updated.readState.revision });
  assert.equal(read.readState.firstUnread, null);
  assert.deepEqual(await adapter.request("thread/items/read", { threadId: thread.id, turnId: "absent-turn", itemIds: ["absent-item"] }), { data: [] });
  await assert.rejects(adapter.readRecoveryOperationEvidence({ projectId: "project", taskId: "task",
    threadId: thread.id, turnId: "absent-turn", callId: "absent-item", toolName: "save_brief", requestHash: "a".repeat(64) }));
  assert.equal(adapter.turnWaiter, null);
});

test("cold Core resume restores saved read-only permissions before current full-access defaults", { timeout: 30_000 }, async t => {
  const home = await mkdtemp(join(tmpdir(), "novel-cold-permissions-"));
  const binary = fileURLToPath(new URL("../../../../codex-rs/target/debug/codex", import.meta.url));
  const server = createServer((request, response) => {
    request.resume();
    response.writeHead(200, { "content-type": "text/event-stream" });
    for (const event of [
      { type: "response.created", response: { id: "permissions-response" } },
      { type: "response.output_item.done", item: { id: "permissions-message", type: "message", role: "assistant",
        phase: "final_answer", content: [{ type: "output_text", text: "Recorded." }] } },
      { type: "response.completed", response: { id: "permissions-response", usage: { input_tokens: 5, output_tokens: 1, total_tokens: 6 } } },
    ]) response.write(`data: ${JSON.stringify(event)}\n\n`);
    response.end();
  });
  await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
  t.after(() => new Promise(resolve => { server.closeAllConnections(); server.close(resolve); }));
  const endpoint = `http://127.0.0.1:${server.address().port}`;
  const provider = `model = "fixture"\nmodel_provider = "fixture"\n[model_providers.fixture]\nname = "Local permission fixture"\nbase_url = "${endpoint}"\nwire_api = "responses"\n`;
  await writeFile(join(home, "config.toml"), provider, { mode: 0o600 });
  const adapter = new CodexAppServerAdapter({ command: `"${binary}" app-server`, cwd: home,
    env: { ...process.env, CODEX_HOME: home }, model: "fixture", modelRoute: "fixture", provider: "fixture" });
  const stop = async () => { const child = adapter.process; adapter.close(); await terminateEngineProcess(child); };
  t.after(async () => { await stop(); await rm(home, { recursive: true, force: true }); });
  await adapter.ensureInitialized({ localOnly: true });
  const started = await adapter.request("thread/start", { model: "fixture", cwd: home, sandbox: "read-only", approvalPolicy: "never" });
  assert.deepEqual(started.sandbox, { type: "readOnly", networkAccess: false });
  // Owned settings are checkpointed with the first recorded turn, not merely
  // by creating an empty thread. Match the actual upstream persistence contract.
  const completed = new Promise(resolve => adapter.setCallbacks({ onEvent: event => {
    if (event.method === "turn/completed" && event.params?.threadId === started.thread.id) resolve(event.params.turn);
  } }));
  await adapter.request("turn/start", { threadId: started.thread.id, input: [{ type: "text", text: "persist original permissions" }] });
  assert.equal((await completed).status, "completed");
  await adapter.request("thread/name/set", { threadId: started.thread.id, name: "Cold permissions" });
  await stop();
  await writeFile(join(home, "config.toml"), 'default_permissions = ":danger-full-access"\n' + provider, { mode: 0o600 });
  await adapter.ensureInitialized({ localOnly: true });
  const resumed = await adapter.request("thread/resume", { threadId: started.thread.id });
  assert.equal(resumed.thread.id, started.thread.id);
  assert.deepEqual(resumed.sandbox, { type: "readOnly", networkAccess: false });
  assert.equal(resumed.approvalPolicy, "never");
});
