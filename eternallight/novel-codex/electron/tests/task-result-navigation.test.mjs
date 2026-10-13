import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import test from "node:test";
import { build } from "esbuild";

const contracts = {
  chapter: ["generateChapter", "draft"], knowledge: ["modelKnowledge", "knowledgeBatch"],
  brief: ["generateBrief", "brief"], outline: ["generateOutline", "outline"],
  plan: ["generatePlan", "plan"], propertyRules: ["generatePropertyRules", "propertyRules"],
  commitmentExtraction: ["extractCommitments", "commitmentExtraction"], commitment: ["reviseCommitment", "commitment"],
};
function fixture(kind) {
  const [operation, evidenceKind] = contracts[kind];
  const target = { chapterId: "chapter-two", chapterNumber: 2, sourceRevisionId: "source-two", commitmentId: "lamp" };
  const task = { id: "work", projectId: "project", steps: [{ id: "step", operation, target, status: "completed", currentAttemptId: "attempt",
    attempts: [{ id: "attempt", status: "completed" }] }], deliveryReport: { evidence: [{ stepId: "step", attemptId: "attempt", kind: evidenceKind, state: "recorded", referenceId: kind === "chapter" ? "source-two" : "result" }] } };
  const workspace = { project: { id: "project" }, chapters: [
    { id: "chapter-one", number: 1, candidate: null, accepted: { id: "source-one" } },
    { id: "chapter-two", number: 2, candidate: { id: "source-two" }, accepted: { id: "published-two" } },
  ], artifacts: Object.fromEntries(["brief", "outline", "plan", "propertyRules"].map(name => [name, { candidate: { id: "result" } }])),
  modeling: [{ objectId: "draft-chapter-one", batchId: "other", sourceRevisionId: "source-one", status: "ready" },
    { objectId: "draft-chapter-two", batchId: "result", sourceRevisionId: "source-two", status: "ready" }] };
  const report = { reportCurrent: true, sourceCurrent: true, revisionId: "result", payload: { chapterId: "chapter-two", sourceRevisionId: "source-two" } };
  const detail = { projectId: "project", revisionId: "result", sourceCurrent: true, extractionOrigin: { chapterId: "chapter-two" },
    payload: { id: "lamp", setupEvidenceRefs: [{ objectId: "draft-chapter-two" }] } };
  const reads = { getWorkTask: task, getCreativeWorkspace: workspace, getCommitmentExtraction: report, getCommitment: detail };
  const calls = [];
  const api = { runtime: new Proxy({}, { get(_target, method) {
    assert.ok(Object.hasOwn(reads, method), `Navigation attempted a write or model operation: ${String(method)}`);
    return async (...args) => { calls.push([method, ...args]); return reads[method]; };
  } }) };
  return { task, workspace, report, detail, calls, api };
}

test("task result navigation revalidates exact delivery and source without writes or latest-version substitution", async t => {
  const directory = await mkdtemp(join(tmpdir(), "task-result-navigation-"));
  try {
    const path = join(directory, "navigation.mjs");
    await build({ entryPoints: [new URL("../../src/task-result-navigation.ts", import.meta.url).pathname], outfile: path,
      bundle: true, platform: "node", format: "esm" });
    const { resolveTaskResult } = await import(pathToFileURL(path).href);
    const helperPath = join(directory, "results.mjs");
    await build({ entryPoints: [new URL("../../src/task-results.ts", import.meta.url).pathname], outfile: helperPath,
      bundle: true, platform: "node", format: "esm" });
    const { getTaskResults } = await import(pathToFileURL(helperPath).href);
    await t.test("historical analysis opens exact immutable reports without consulting or changing today's draft", async () => {
      for (const kind of ["planningReview", "hypothesis"]) {
        const f = fixture("outline");
        const step = f.task.steps[0];
        step.operation = kind === "planningReview" ? "reviewPlanningSeat" : "advisory";
        step.target = kind === "planningReview" ? { reviewRunId: "review-original", role: "structure", artifactKind: "outline", revisionId: "old-outline" } : {};
        f.task.deliveryReport.evidence[0].kind = kind;
        const run = { id: "review-original", projectId: "project", workTaskId: "work", artifactKind: "outline", revisionId: "old-outline",
          reports: [{ id: "result", runId: "review-original", role: "structure" }], seats: [{ role: "structure", taskId: "attempt" }] };
        const hypothesis = { id: "result", projectId: "project", baseSnapshotId: "old-snapshot", authority: "hypothesisOnly" };
        const calls = [];
        const api = { runtime: {
          getWorkTask: async () => f.task,
          getPlanningReview: async (...args) => { calls.push(args); return run; },
          getHypothesis: async (...args) => { calls.push(args); return hypothesis; },
          getCreativeWorkspace: async () => assert.fail("read-only historic analysis must not replace the editing source"),
        } };
        const destination = getTaskResults(f.task)[0];
        assert.ok(destination);
        const resolved = await resolveTaskResult(api, destination);
        assert.deepEqual(resolved, { result: destination, analysis: kind === "planningReview"
          ? { kind, run, reportId: "result" } : { kind, result: hypothesis } });
        assert.deepEqual(calls, [["project", kind === "planningReview" ? "review-original" : "result"]]);
        if (kind === "planningReview") run.seats[0].taskId = "another-task";
        else hypothesis.projectId = "other-project";
        await assert.rejects(resolveTaskResult(api, destination), /来源不一致|作品或固定快照/);
        f.task.deliveryReport.evidence = [];
        assert.deepEqual(getTaskResults(f.task), [], "model metadata without canonical evidence must not expose a report");
      }
    });
    await t.test("all eight result kinds navigate their own artifact and second chapter", async () => {
      for (const kind of Object.keys(contracts)) {
        const f = fixture(kind), result = getTaskResults(f.task)[0];
        const resolved = await resolveTaskResult(f.api, result);
        const chapterKind = ["chapter", "knowledge", "commitmentExtraction", "commitment"].includes(kind);
        const expected = { result, view: chapterKind ? "chapters" : kind === "plan" ? "plans" : kind === "propertyRules" ? "world" : kind };
        if (chapterKind) expected.chapterId = "chapter-two";
        if (chapterKind && kind !== "commitment") expected.sourceRevisionId = "source-two";
        assert.deepEqual(resolved, expected);
        assert.deepEqual(f.calls.slice(0, 2), [["getWorkTask", "project", "work"], ["getCreativeWorkspace", "project"]]);
        if (kind === "commitmentExtraction") assert.deepEqual(f.calls[2], ["getCommitmentExtraction", "project", "chapter-two", "result"]);
      }
    });
    await t.test("missing or superseded delivery stops before workspace reads", async () => {
      for (const patch of [{ state: "superseded" }, { attemptId: "older-attempt" }, { referenceId: "newer-result" }]) {
        const f = fixture("knowledge"), result = getTaskResults(f.task)[0];
        Object.assign(f.task.deliveryReport.evidence[0], patch);
        await assert.rejects(resolveTaskResult(f.api, result), /任务结果已变化/);
        assert.deepEqual(f.calls, [["getWorkTask", "project", "work"]]);
      }
    });
    await t.test("source, batch, project and report mismatches cannot open a replacement", async () => {
      const cases = [
        ["chapter", f => { f.workspace.chapters[1].candidate.id = "newer-source"; }],
        ["knowledge", f => { f.workspace.modeling[1].batchId = "newer-batch"; }],
        ["knowledge", f => { f.workspace.modeling[1].sourceRevisionId = "source-one"; }],
        ["knowledge", f => { f.workspace.modeling[1].status = "queued"; }],
        ["knowledge", f => { f.workspace.project.id = "other-project"; }],
        ["commitmentExtraction", f => { f.report.payload.chapterId = "chapter-one"; }],
        ["commitmentExtraction", f => { f.report.sourceCurrent = false; }],
        ["commitmentExtraction", f => { f.report.reportCurrent = false; }],
        ["commitmentExtraction", f => { delete f.report.reportCurrent; }],
        ["commitmentExtraction", f => { f.report.revisionId = "new-report"; }],
        ["commitment", f => { f.detail.projectId = "other-project"; }],
        ["commitment", f => { f.detail.sourceCurrent = false; }],
        ["commitment", f => { f.detail.payload.id = "other-commitment"; }],
        ...["brief", "outline", "plan", "propertyRules"].map(kind => [kind, f => { f.workspace.artifacts[kind].candidate.id = "newer-artifact"; }]),
      ];
      for (const [kind, change] of cases) {
        const f = fixture(kind), result = getTaskResults(f.task)[0]; change(f);
        await assert.rejects(resolveTaskResult(f.api, result));
      }
    });
    await t.test("native source omission is bound to the exact ready batch; read errors propagate", async () => {
      const f = fixture("knowledge"); delete f.task.steps[0].target.sourceRevisionId;
      const result = getTaskResults(f.task)[0];
      assert.equal(result.sourceRevisionId, null);
      assert.deepEqual(await resolveTaskResult(f.api, result), { result, view: "chapters", chapterId: "chapter-two", sourceRevisionId: "source-two" });
      const failure = new Error("read unavailable");
      await assert.rejects(resolveTaskResult({ runtime: { getWorkTask: async () => { throw failure; } } }, result), error => error === failure);
    });
    await t.test("commitments without extraction origin require one exact source chapter", async () => {
      const f = fixture("commitment"), result = getTaskResults(f.task)[0];
      delete f.detail.extractionOrigin;
      assert.deepEqual(await resolveTaskResult(f.api, result), { result, view: "chapters", chapterId: "chapter-two" });
      f.detail.payload.setupEvidenceRefs.push({ objectId: "draft-chapter-one" });
      await assert.rejects(resolveTaskResult(f.api, result), /章节|来源/);
      f.detail.extractionOrigin = { chapterId: "chapter-two" };
      assert.deepEqual(await resolveTaskResult(f.api, result), { result, view: "chapters", chapterId: "chapter-two" }, "Explicit extraction origin remains precise despite additional referenced chapters");
    });
  } finally { await rm(directory, { recursive: true, force: true }); }
});
