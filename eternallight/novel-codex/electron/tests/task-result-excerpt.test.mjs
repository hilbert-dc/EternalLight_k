import assert from "node:assert/strict";
import test from "node:test";
import { build } from "esbuild";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

test("result follow-up captures proven revision verbatim, rejects changed ownership and never truncates", async () => {
  const dir = await mkdtemp(join(tmpdir(), "result-excerpt-"));
  try {
    const file = join(dir, "excerpt.mjs");
    await build({ entryPoints: [new URL("../../src/task-result-excerpt.ts", import.meta.url).pathname], outfile: file, bundle: true, platform: "node", format: "esm" });
    const { taskResultExcerpt } = await import(pathToFileURL(file).href);
    const result = { projectId: "project", workTaskId: "work", stepId: "step", attemptId: "attempt", referenceId: "draft-v2", kind: "chapter", chapterId: "chapter", chapterNumber: 1 };
    const task = { id: "work", projectId: "project", steps: [{ id: "step", operation: "generateChapter", currentAttemptId: "attempt", status: "completed", target: { chapterId: "chapter", chapterNumber: 1 }, attempts: [{ id: "attempt", status: "completed" }] }], deliveryReport: { evidence: [{ stepId: "step", attemptId: "attempt", kind: "draft", state: "recorded", referenceId: "draft-v2" }] } };
    const source = { id: "draft-v2", body: "  保留原文与空白。\n另一段。" };
    const api = { runtime: { getWorkTask: async () => task, getCreativeWorkspace: async () => ({ project: { id: "project" }, headSnapshotId: "snapshot", chapters: [{ id: "chapter", candidate: source }] }) } };
    const excerpt = await taskResultExcerpt(api, result);
    assert.equal(excerpt.text, source.body); assert.equal(excerpt.revisionId, "draft-v2"); assert.equal(excerpt.snapshotId, "snapshot"); assert.equal(excerpt.end, source.body.length);
    source.id = "draft-v3"; await assert.rejects(taskResultExcerpt(api, result), /替换/);
    source.id = "draft-v2"; source.body = "长".repeat(25000); await assert.rejects(taskResultExcerpt(api, result), /较长/);
    task.deliveryReport.evidence[0].attemptId = "foreign"; await assert.rejects(taskResultExcerpt(api, result), /变化/);
  } finally { await rm(dir, { recursive: true, force: true }); }
});

function commitmentFixture(kind = "commitmentExtraction", deliveryState = null) {
  const proposal = { id: "bell", kind: "foreshadowing", summary: "  钟声只持续四十七秒。\n保留原文😀", fulfillmentCriteria: "解释钟声持续时间的原因",
    relatedEntityIds: [], plotThreadId: null, characterArcId: null, targetWindow: { kind: "open" }, status: "open", decisionReason: null,
    setupEvidenceRefs: [{ objectId: "draft-chapter", revisionId: "source", quote: "白塔只亮四十七秒。", start: 0, end: 9 }], fulfillmentEvidenceRefs: [] };
  const candidates = [proposal, { ...structuredClone(proposal), id: "lamp", summary: "灯芯上的墨迹" }];
  const report = { revisionId: "report", snapshotId: "snapshot", projectRevision: 3, authority: "candidateProposalBatch", reportCurrent: true, sourceCurrent: true,
    established: [{ proposalId: "bell", commitmentId: "independent" }], payload: { chapterId: "chapter", sourceRevisionId: "source", examinedRanges: [{ start: 0, end: 9 }], candidates },
    coverage: { semanticCompleteness: "unknown", modelReportedRanges: true, literaryFulfillmentVerified: false, globalCompletenessClaimed: false } };
  const detail = { projectId: "project", objectId: "commitment-independent", revisionId: "commitment-revision", snapshotId: "snapshot",
    payload: { ...structuredClone(proposal), id: "independent" }, sourceCurrent: true, authorControls: { projectRevision: 3, undoAvailable: false },
    confirmationState: "candidate", confirmationCommitId: null, interpretation: "sourceBoundNarrativeClaim", literaryFulfillmentVerified: false,
    extractionOrigin: { chapterId: "chapter", reportRevisionId: "report", proposalId: "bell" } };
  const target = kind === "commitmentExtraction" ? { chapterId: "chapter", chapterNumber: 2, sourceRevisionId: "source" } : { commitmentId: "independent" };
  const result = { projectId: "project", workTaskId: "work", stepId: "step", attemptId: "attempt", referenceId: kind === "commitmentExtraction" ? "report" : "commitment-revision", kind, ...target,
    ...(deliveryState ? { deliveryState } : {}) };
  const task = { id: "work", projectId: "project", steps: [{ id: "step", operation: kind === "commitmentExtraction" ? "extractCommitments" : "reviseCommitment",
    currentAttemptId: "attempt", status: deliveryState === "superseded" ? "superseded" : "completed", target, attempts: [{ id: "attempt", status: "completed" }] }],
    deliveryReport: { evidence: [{ stepId: "step", attemptId: "attempt", kind, state: deliveryState ?? "recorded", referenceId: result.referenceId }] } };
  const f = { result, task, report, detail, calls: [], writes: [], afterSource: () => {} };
  f.api = { runtime: new Proxy({}, { get(_object, method) {
    if (!["getWorkTask", "getCommitmentExtraction", "getCommitment"].includes(method)) {
      f.writes.push(method); throw Error(`Unexpected write or substitute read: ${String(method)}`);
    }
    return async (...args) => {
      f.calls.push([method, ...args]);
      if (method === "getWorkTask") { assert.deepEqual(args, ["project", "work"]); return structuredClone(f.task); }
      if (method === "getCommitmentExtraction") {
        assert.deepEqual(args, ["project", "chapter", "report"]);
        const captured = structuredClone(f.report); f.afterSource(); return captured;
      }
      assert.deepEqual(args, ["project", "independent"]);
      const captured = structuredClone(f.detail); f.afterSource(); return captured;
    };
  } }) };
  return f;
}

test("commitment discussion captures exact report/proposal/detail data without granting writes or replacing history", async t => {
  const dir = await mkdtemp(join(tmpdir(), "commitment-result-excerpt-"));
  try {
    const file = join(dir, "excerpt.mjs");
    await build({ entryPoints: [new URL("../../src/task-result-excerpt.ts", import.meta.url).pathname], outfile: file, bundle: true, platform: "node", format: "esm" });
    const { taskResultExcerpt, taskResultDiscussionExcerpt, canDiscussTaskResult } = await import(pathToFileURL(file).href);
    await t.test("current extraction works through the existing result interface and preserves the whole report", async () => {
      const f = commitmentFixture();
      assert.equal(canDiscussTaskResult(f.result), true);
      const excerpt = await taskResultExcerpt(f.api, f.result), quoted = JSON.parse(excerpt.text);
      assert.deepEqual(quoted.report, f.report); assert.equal(quoted.capture.scope, "completeReport"); assert.equal(quoted.capture.deliveryState, "recorded");
      assert.equal(excerpt.revisionId, "report"); assert.equal(excerpt.objectId, "commitment-extraction-chapter"); assert.equal(excerpt.snapshotId, "snapshot");
      assert.equal(excerpt.end, excerpt.text.length); assert.equal(excerpt.state, "displayed"); assert.ok(excerpt.label.includes("第 2 章"));
      assert.deepEqual(f.calls.map(call => call[0]), ["getWorkTask", "getCommitmentExtraction", "getWorkTask"]); assert.deepEqual(f.writes, []);
    });
    await t.test("one proposal preserves its complete object, report/source/status metadata and explicit scope", async () => {
      const f = commitmentFixture("commitmentExtraction", "recorded");
      const excerpt = await taskResultDiscussionExcerpt(f.api, { result: f.result, proposalId: "bell" }), quoted = JSON.parse(excerpt.text);
      assert.deepEqual(quoted.proposal, f.report.payload.candidates[0]); assert.equal(quoted.capture.scope, "singleProposal"); assert.equal(quoted.capture.proposalId, "bell");
      assert.equal(quoted.reportMetadata.candidateCount, 2); assert.equal(quoted.reportMetadata.authority, "candidateProposalBatch");
      assert.equal(quoted.reportMetadata.payload.sourceRevisionId, "source"); assert.deepEqual(quoted.reportMetadata.established, f.report.established);
      assert.deepEqual(quoted.reportMetadata.coverage, f.report.coverage); assert.equal(quoted.reportMetadata.payload.candidates, undefined);
      assert.ok(!excerpt.text.includes("灯芯上的墨迹")); assert.ok(quoted.capture.notice.includes("不代表作者已确认")); assert.deepEqual(f.writes, []);
    });
    await t.test("superseded and source-stale reports quote the original metadata and never become current or force a mode", async () => {
      const f = commitmentFixture("commitmentExtraction", "superseded"); f.report.reportCurrent = false; f.report.sourceCurrent = false;
      const excerpt = await taskResultDiscussionExcerpt(f.api, { result: f.result }), quoted = JSON.parse(excerpt.text);
      assert.deepEqual(quoted.report, f.report); assert.equal(quoted.capture.deliveryState, "superseded");
      assert.equal(quoted.report.reportCurrent, false); assert.equal(quoted.report.sourceCurrent, false);
      assert.equal(excerpt.state, "displayed"); assert.equal(Object.hasOwn(excerpt, "readOnly"), false);
      assert.ok(quoted.capture.notice.includes("历史或来源变化")); assert.deepEqual(f.writes, []);
    });
    await t.test("foreign work, wrong attempt, missing proof and mismatched delivery state stop before source reads", async () => {
      for (const mutate of [f => { f.task.projectId = "other"; }, f => { f.task.id = "other"; },
        f => { f.task.deliveryReport.evidence[0].attemptId = "foreign"; }, f => { f.task.deliveryReport.evidence = []; },
        f => { f.result.deliveryState = "superseded"; }]) {
        const f = commitmentFixture("commitmentExtraction", "recorded"); mutate(f);
        await assert.rejects(taskResultDiscussionExcerpt(f.api, { result: f.result }), /来源已变化/);
        assert.equal(f.calls.length, 1); assert.deepEqual(f.writes, []);
      }
    });
    await t.test("missing or wrong report revision/chapter/source, invalid authority and missing capture flags reject without replacement", async () => {
      for (const mutate of [f => { f.report = null; }, f => { f.report.revisionId = "later"; }, f => { f.report.payload.chapterId = "other"; },
        f => { f.report.payload.sourceRevisionId = "other"; }, f => { f.report.authority = "confirmedFact"; },
        f => { delete f.report.reportCurrent; }, f => { delete f.report.sourceCurrent; }]) {
        const f = commitmentFixture(); mutate(f);
        await assert.rejects(taskResultDiscussionExcerpt(f.api, { result: f.result }), /未用其他版本/);
        assert.equal(f.calls.length, 2); assert.deepEqual(f.writes, []);
      }
    });
    await t.test("unknown proposal and duplicate proposals or established mappings cannot choose an ambiguous object", async () => {
      const missing = commitmentFixture();
      await assert.rejects(taskResultDiscussionExcerpt(missing.api, { result: missing.result, proposalId: "missing" }), /不在对应/);
      for (const mutate of [f => { f.report.payload.candidates.push(structuredClone(f.report.payload.candidates[0])); },
        f => { f.report.established.push({ proposalId: "bell", commitmentId: "another" }); },
        f => { f.report.established[0].proposalId = "missing"; }]) {
        const f = commitmentFixture(); mutate(f);
        await assert.rejects(taskResultDiscussionExcerpt(f.api, { result: f.result, proposalId: "bell" }), /来源已变化/);
        assert.deepEqual(f.writes, []);
      }
    });
    await t.test("invalid proposal selectors and selectors on other result kinds never reach Runtime", async () => {
      for (const value of ["", " ", "x".repeat(161)]) {
        const f = commitmentFixture(); await assert.rejects(taskResultDiscussionExcerpt(f.api, { result: f.result, proposalId: value }), /单条建议引用/);
        assert.deepEqual(f.calls, []);
      }
      const f = commitmentFixture("commitment");
      await assert.rejects(taskResultDiscussionExcerpt(f.api, { result: f.result, proposalId: "bell" }), /单条建议引用/);
      assert.deepEqual(f.calls, []);
    });
    await t.test("a proof invalidated while the source read is pending cannot be captured", async () => {
      for (const kind of ["commitmentExtraction", "commitment"]) {
        const f = commitmentFixture(kind); f.afterSource = () => { f.task.steps[0].currentAttemptId = "later-attempt"; };
        await assert.rejects(taskResultDiscussionExcerpt(f.api, { result: f.result }), /来源已变化/);
        assert.deepEqual(f.calls.map(call => call[0]), ["getWorkTask", kind === "commitmentExtraction" ? "getCommitmentExtraction" : "getCommitment", "getWorkTask"]);
        assert.deepEqual(f.writes, []);
      }
    });
    await t.test("a large full report is rejected; explicitly choosing a bounded proposal preserves that proposal without truncating", async () => {
      const f = commitmentFixture(); f.report.payload.candidates[1].summary = "长".repeat(25000);
      await assert.rejects(taskResultExcerpt(f.api, f.result), /较长.*没有截断/);
      const excerpt = await taskResultDiscussionExcerpt(f.api, { result: f.result, proposalId: "bell" });
      assert.deepEqual(JSON.parse(excerpt.text).proposal, f.report.payload.candidates[0]);
      f.report.payload.candidates[0].summary = "长".repeat(25000);
      await assert.rejects(taskResultDiscussionExcerpt(f.api, { result: f.result, proposalId: "bell" }), /单条建议.*没有截断/);
      assert.deepEqual(f.writes, []);
    });
    await t.test("current independent revisions preserve real candidate/confirmed state rather than deriving it from mappings", async () => {
      const f = commitmentFixture("commitment"); assert.equal(canDiscussTaskResult(f.result), true);
      for (const confirmationState of ["candidate", "confirmed"]) {
        f.detail.confirmationState = confirmationState; f.detail.confirmationCommitId = confirmationState === "confirmed" ? "actual-author-receipt" : null;
        const excerpt = await taskResultExcerpt(f.api, f.result), quoted = JSON.parse(excerpt.text);
        assert.deepEqual(quoted.commitment, f.detail); assert.equal(excerpt.objectId, "commitment-independent");
        assert.equal(excerpt.revisionId, "commitment-revision"); assert.ok(quoted.capture.notice.includes("真实作者决定及回执"));
      }
      f.detail.sourceCurrent = false;
      assert.equal(JSON.parse((await taskResultExcerpt(f.api, f.result)).text).commitment.sourceCurrent, false);
      assert.deepEqual(f.writes, []);
    });
    await t.test("foreign or revised independent objects and unsupported confirmation receipts are not replaced with latest", async () => {
      for (const patch of [{ projectId: "other" }, { objectId: "commitment-other" }, { revisionId: "later" },
        { payload: { id: "other" } }, { confirmationState: "confirmed", confirmationCommitId: null }]) {
        const f = commitmentFixture("commitment"); Object.assign(f.detail, patch);
        await assert.rejects(taskResultExcerpt(f.api, f.result), /未用其他版本/);
        assert.equal(f.calls.length, 2); assert.deepEqual(f.writes, []);
      }
    });
  } finally { await rm(dir, { recursive: true, force: true }); }
});
