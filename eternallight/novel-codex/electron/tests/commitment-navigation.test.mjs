import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import test from "node:test";
import { build } from "esbuild";

function fixture() {
  const request = { projectId: "project", commitmentId: "lamp", revisionId: "exact-result", snapshotId: "snapshot" };
  const workspace = { project: { id: "project" }, revision: 3, headSnapshotId: "snapshot", sourceDigest: "sources",
    chapters: [{ id: "chapter-one" }, { id: "chapter-two" }] };
  const detail = { projectId: "project", snapshotId: "snapshot", revisionId: "exact-result", sourceCurrent: true,
    extractionOrigin: { chapterId: "chapter-two" }, payload: { id: "lamp", setupEvidenceRefs: [{ objectId: "draft-chapter-two" }] } };
  const calls = [];
  const state = { request, workspace, detail, calls, afterDetail: () => {} };
  state.api = { runtime: new Proxy({}, { get(_target, method) {
    assert.ok(["getCreativeWorkspace", "getCommitment"].includes(method), `Unexpected write/model/task read: ${String(method)}`);
    return async (...args) => {
      calls.push([method, ...args]);
      if (method === "getCommitment") { const result = structuredClone(detail); state.afterDetail(); return result; }
      return structuredClone(workspace);
    };
  } }) };
  return state;
}

test("directory navigation reads the exact reference and cannot confirm or substitute a task/latest result", async t => {
  const directory = await mkdtemp(join(tmpdir(), "commitment-navigation-"));
  try {
    const path = join(directory, "navigation.mjs");
    await build({ entryPoints: [fileURLToPath(new URL("../../src/commitment-navigation.ts", import.meta.url))],
      outfile: path, bundle: true, platform: "node", format: "esm" });
    const { resolveCommitmentNavigation } = await import(pathToFileURL(path).href);
    await t.test("exact independent commitment opens its origin chapter using only reads", async () => {
      const f = fixture();
      assert.deepEqual(await resolveCommitmentNavigation(f.api, f.request), { chapterId: "chapter-two", detail: f.detail });
      assert.deepEqual(f.calls, [["getCreativeWorkspace", "project"], ["getCommitment", "project", "lamp", "snapshot"], ["getCreativeWorkspace", "project"]]);
    });
    await t.test("without extraction origin, evidence must identify exactly one existing chapter", async () => {
      const f = fixture(); f.detail.extractionOrigin = null;
      assert.equal((await resolveCommitmentNavigation(f.api, f.request)).chapterId, "chapter-two");
      f.detail.payload.setupEvidenceRefs.push({ objectId: "draft-chapter-one" });
      await assert.rejects(resolveCommitmentNavigation(f.api, f.request), /唯一定位/);
      f.detail.payload.setupEvidenceRefs = [{ objectId: "draft-missing" }];
      await assert.rejects(resolveCommitmentNavigation(f.api, f.request), /唯一定位/);
    });
    await t.test("foreign project and old snapshot stop before reading a replacement detail", async () => {
      for (const update of [f => { f.workspace.project.id = "other"; }, f => { f.workspace.headSnapshotId = "later"; }]) {
        const f = fixture(); update(f);
        await assert.rejects(resolveCommitmentNavigation(f.api, f.request), /快照已变化/);
        assert.deepEqual(f.calls, [["getCreativeWorkspace", "project"]]);
      }
    });
    await t.test("wrong detail identity, revision, snapshot or source rejects instead of opening latest", async () => {
      for (const patch of [{ projectId: "other" }, { revisionId: "later" }, { snapshotId: "older" }, { sourceCurrent: false }, { payload: { id: "other" } }]) {
        const f = fixture(); Object.assign(f.detail, patch);
        await assert.rejects(resolveCommitmentNavigation(f.api, f.request), /版本或原文来源/);
        assert.equal(f.calls.length, 2);
      }
    });
    await t.test("an invalid extraction origin cannot silently fall back to another chapter", async () => {
      const f = fixture(); f.detail.extractionOrigin.chapterId = "missing";
      await assert.rejects(resolveCommitmentNavigation(f.api, f.request), /唯一定位/);
    });
    await t.test("a change during the exact detail read invalidates navigation", async () => {
      for (const patch of [{ revision: 4 }, { headSnapshotId: "later" }, { sourceDigest: "new-source" }, { project: { id: "other" } }, { chapters: [] }]) {
        const f = fixture(); f.afterDetail = () => Object.assign(f.workspace, patch);
        await assert.rejects(resolveCommitmentNavigation(f.api, f.request), /读取期间作品已变化/);
        assert.equal(f.calls.length, 3);
      }
    });
    await t.test("malformed directory references do not reach Runtime", async () => {
      for (const patch of [{ snapshotId: "" }, { revisionId: "章".repeat(54) }, { commitmentId: null }, { workTaskId: "invented" }]) {
        const f = fixture(); await assert.rejects(resolveCommitmentNavigation(f.api, { ...f.request, ...patch }), /引用无效/);
        assert.deepEqual(f.calls, []);
      }
    });
    await t.test("read failures propagate without another request or write", async () => {
      const f = fixture(); f.api.runtime = { getCreativeWorkspace: async () => f.workspace, getCommitment: async () => { throw new Error("read unavailable"); } };
      await assert.rejects(resolveCommitmentNavigation(f.api, f.request), /read unavailable/);
    });
  } finally { await rm(directory, { recursive: true, force: true }); }
});
