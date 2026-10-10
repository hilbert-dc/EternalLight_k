import assert from "node:assert/strict";
import { test } from "node:test";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
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
