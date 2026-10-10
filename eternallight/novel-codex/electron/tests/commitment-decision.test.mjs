import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import test from "node:test";
import { build } from "esbuild";

function fixture(createCommitmentDecision) {
  const proposal = { id: "proposal", kind: "foreshadowing", summary: "白塔只亮四十七秒", fulfillmentCriteria: "揭示钟声持续时间的原因",
    setupEvidenceRefs: [{ objectId: "draft-chapter", revisionId: "source", quote: "白塔只亮四十七秒", start: 0, end: 10 }] };
  const report = { revisionId: "report", snapshotId: "snapshot", projectRevision: 3, authority: "candidateProposalBatch",
    sourceCurrent: true, reportCurrent: true, payload: { chapterId: "chapter", sourceRevisionId: "source", candidates: [proposal] }, established: [] };
  const state = { report, detail: null, writes: [], reads: [], failEstablish: false, afterReview: () => {}, current: true };
  const api = { runtime: {
    getCommitmentExtraction: async (projectId, chapterId) => {
      assert.equal(projectId, "project"); assert.equal(chapterId, "chapter");
      state.reads.push("report"); return structuredClone(state.report);
    },
    getCommitment: async (projectId, commitmentId) => {
      assert.equal(projectId, "project"); assert.equal(commitmentId, "independent");
      state.reads.push("detail"); assert.ok(state.detail); return structuredClone(state.detail);
    },
    establishCommitment: async (projectId, chapterId, request) => {
      assert.equal(projectId, "project"); assert.equal(chapterId, "chapter");
      assert.deepEqual({ ...request, idempotencyKey: "key" }, {
        reportRevisionId: "report", proposalId: "proposal", baseSnapshotId: "snapshot", expectedRevision: 3, idempotencyKey: "key",
      });
      assert.ok(request.idempotencyKey);
      state.writes.push(["establish", request.idempotencyKey]);
      state.report.established = [{ proposalId: "proposal", commitmentId: "independent" }];
      state.detail = { projectId: "project", objectId: "commitment-independent", revisionId: "independent-revision",
        payload: { ...structuredClone(proposal), id: "independent" }, sourceCurrent: true, authorControls: { projectRevision: 3 },
        extractionOrigin: { chapterId: "chapter", reportRevisionId: "report", proposalId: "proposal" },
        confirmationState: "candidate", confirmationCommitId: null };
      if (state.failEstablish) { state.failEstablish = false; throw Error("建立已保存但回执断线"); }
      return { commitmentId: "independent", revisionId: "independent-revision" };
    },
    createCommitmentReview: async (projectId, commitmentId, revisionId, key) => {
      assert.equal(projectId, "project"); assert.equal(commitmentId, "independent"); assert.equal(revisionId, "independent-revision"); assert.ok(key);
      state.writes.push(["review", key]); state.afterReview();
      return { id: "review", artifactKind: "commitment", artifactRevisionId: revisionId, status: "ready", projectRevision: 3 };
    },
    acceptReview: async (projectId, reviewId, revision) => {
      assert.equal(projectId, "project"); assert.equal(reviewId, "review"); assert.equal(revision, 3);
      state.writes.push(["accept"]);
      Object.assign(state.detail, { confirmationState: "confirmed", confirmationCommitId: "accepted" });
      return { commitId: "accepted" };
    },
  } };
  const decision = createCommitmentDecision();
  state.decide = (displayed = report) => decision.decide({ api, projectId: "project", chapterId: "chapter", report: structuredClone(displayed),
    proposalId: "proposal", isCurrent: () => state.current });
  return state;
}

test("author confirmation requires an explicitly current displayed and fresh report at each guarded stage", async t => {
  const directory = await mkdtemp(join(tmpdir(), "commitment-decision-"));
  try {
    const path = join(directory, "decision.mjs");
    await build({ entryPoints: [fileURLToPath(new URL("../../src/commitment-decision.ts", import.meta.url))], outfile: path,
      bundle: true, platform: "node", format: "esm" });
    const { createCommitmentDecision } = await import(pathToFileURL(path).href);
    await t.test("a superseded or unspecified displayed report cannot initiate a read or write", async () => {
      for (const reportCurrent of [false, undefined]) {
        const f = fixture(createCommitmentDecision); f.report.reportCurrent = reportCurrent;
        await assert.rejects(f.decide(), /提案来源已变化/);
        assert.deepEqual(f.reads, []); assert.deepEqual(f.writes, []);
      }
    });
    await t.test("fresh reportCurrent false rejects before establishment even when revision and source match", async () => {
      for (const reportCurrent of [false, undefined]) {
        const f = fixture(createCommitmentDecision); const displayed = structuredClone(f.report); f.report.reportCurrent = reportCurrent;
        await assert.rejects(f.decide(displayed), /报告或正文来源已变化/);
        assert.deepEqual(f.reads, ["report"]); assert.deepEqual(f.writes, []);
      }
    });
    await t.test("a report superseded during review cannot continue into acceptance", async () => {
      const f = fixture(createCommitmentDecision); f.afterReview = () => { f.report.reportCurrent = false; };
      await assert.rejects(f.decide(), /报告或正文来源已变化/);
      assert.deepEqual(f.writes.map(write => write[0]), ["establish", "review"]);
      assert.equal(f.detail.confirmationState, "candidate"); assert.equal(f.detail.confirmationCommitId, null);
    });
    await t.test("unknown establishment outcome recovers only by reads until a second explicit decision", async () => {
      const f = fixture(createCommitmentDecision); f.failEstablish = true;
      await assert.rejects(f.decide(), /候选已建立，尚未确认/);
      assert.deepEqual(f.writes.map(write => write[0]), ["establish"]); assert.equal(f.detail.confirmationState, "candidate");
      const confirmed = await f.decide();
      assert.deepEqual(f.writes.map(write => write[0]), ["establish", "review", "accept"]);
      assert.equal(confirmed.confirmationState, "confirmed"); assert.equal(confirmed.confirmationCommitId, "accepted");
    });
    await t.test("a changed conversation owner stops further author writes after review", async () => {
      const f = fixture(createCommitmentDecision); f.afterReview = () => { f.current = false; };
      await assert.rejects(f.decide(), /确认来源已切换/);
      assert.deepEqual(f.writes.map(write => write[0]), ["establish", "review"]);
    });
    await t.test("current reports confirm only after the independent commitment carries its real receipt", async () => {
      const f = fixture(createCommitmentDecision);
      const confirmed = await f.decide();
      assert.deepEqual(f.writes.map(write => write[0]), ["establish", "review", "accept"]);
      assert.equal(confirmed.confirmationState, "confirmed"); assert.equal(confirmed.confirmationCommitId, "accepted");
    });
  } finally { await rm(directory, { recursive: true, force: true }); }
});
