import assert from "node:assert/strict";
import { mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { listModelConnections, prepareEvidenceReverification, readModelEvidenceCache } from "../model-evidence-cache.mjs";
import { listCodexConnections, loadCodexEvidence } from "../codex-runtime.mjs";

for (const [name, bytes] of [
  ["invalid JSON", "{ interrupted write"],
  ["invalid evidence dependencies", JSON.stringify({ version: 1, records: [{ dependencies: null }] })],
]) test(`rejected ${name} stays read-only and re-verification preserves it`, async t => {
  const directory = await mkdtemp(join(tmpdir(), "novel-evidence-cache-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const path = join(directory, "evidence.json");
  await writeFile(path, bytes, { mode: 0o600 });
  await assert.rejects(loadCodexEvidence(path));
  const snapshot = await readModelEvidenceCache(path);
  assert.deepEqual(snapshot.records, []);
  assert.match(snapshot.error, /重新验证模型/);
  assert.equal(await readFile(path, "utf8"), bytes);
  const connections = await listModelConnections({ evidencePath: path });
  assert.ok(connections.length > 0);
  assert.ok(connections.every(connection => connection.status === "unverified"
    && connection.verifiedCapabilities.length === 0 && connection.verifiedProfiles.length === 0 && connection.selectedProtocol === null));
  assert.equal((await readdir(directory)).filter(name => name.includes("rejected")).length, 0);
  const rejected = await prepareEvidenceReverification(path);
  assert.equal(await readFile(rejected, "utf8"), bytes);
  assert.deepEqual(await readModelEvidenceCache(path), { records: [], error: null });
});

test("an empty current cache and missing cache keep normal model metadata", async t => {
  const directory = await mkdtemp(join(tmpdir(), "novel-evidence-cache-"));
  t.after(() => rm(directory, { recursive: true, force: true }));
  const path = join(directory, "evidence.json");
  assert.deepEqual(await listModelConnections({ evidencePath: path }), listCodexConnections());
  assert.equal(await prepareEvidenceReverification(path), null);
  await writeFile(path, JSON.stringify({ version: 1, records: [] }));
  assert.deepEqual(await readModelEvidenceCache(path), { records: [], error: null });
  assert.equal(await prepareEvidenceReverification(path), null);
});
