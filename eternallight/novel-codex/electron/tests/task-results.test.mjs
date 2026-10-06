import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import test from "node:test";
import { build } from "esbuild";
const contracts = [["generateChapter", "draft"], ["modelKnowledge", "knowledgeBatch"], ["generateBrief", "brief"],
  ["generateOutline", "outline"], ["generatePlan", "plan"], ["generatePropertyRules", "propertyRules"],
  ["extractCommitments", "commitmentExtraction"], ["reviseCommitment", "commitment"]];
const task = () => ({ id: "work-one", projectId: "project-one", steps: contracts.map(([operation], index) => ({
  id: `step-${index}`, operation, status: "completed", currentAttemptId: `attempt-${index}`,
  target: { chapterId: `chapter-${index}`, chapterNumber: index + 1, sourceRevisionId: `source-${index}`, commitmentId: "lamp" },
  attempts: [{ id: `attempt-${index}`, status: "completed" }], outputRefs: [{ kind: "draft", revisionId: "unverified-other", objectId: "draft-other" }],
})), deliveryReport: { evidence: contracts.map(([, kind], index) => ({ stepId: `step-${index}`, attemptId: `attempt-${index}`, kind, state: "recorded", referenceId: `reference-${index}` })) } });

test("task results use each exact native delivery proof and reject obsolete, wrong or ambiguous ownership", async () => {
  const directory = await mkdtemp(join(tmpdir(), "task-results-"));
  try {
    const path = join(directory, "logic.mjs");
    await build({ entryPoints: [new URL("../../src/task-results.ts", import.meta.url).pathname], outfile: path, bundle: true, platform: "node", format: "esm" });
    const { getTaskResults, taskResultLabel } = await import(pathToFileURL(path).href);
    const fixture = task();
    const results = getTaskResults(fixture);
    assert.deepEqual(results.map(item => ({ kind: item.kind, stepId: item.stepId, attemptId: item.attemptId, referenceId: item.referenceId })),
      ["chapter", "knowledge", "brief", "outline", "plan", "propertyRules", "commitmentExtraction", "commitment"].map((kind, index) => ({ kind, stepId: `step-${index}`, attemptId: `attempt-${index}`, referenceId: `reference-${index}` })));
    assert.deepEqual(results[1], { projectId: "project-one", workTaskId: "work-one", stepId: "step-1", attemptId: "attempt-1", referenceId: "reference-1", kind: "knowledge", chapterId: "chapter-1", chapterNumber: 2, sourceRevisionId: "source-1" });
    assert.equal(taskResultLabel(results[1]), "检查第 2 章故事设定");
    for (const patch of [{ state: "superseded" }, { state: "missing" }, { attemptId: "old-attempt" }, { kind: "draft" }, { referenceId: null }]) {
      const next = task(); Object.assign(next.deliveryReport.evidence[1], patch);
      assert.deepEqual(getTaskResults(next), results.filter(item => item.kind !== "knowledge"));
    }
    for (const patch of [{ currentAttemptId: "unknown" }, { status: "superseded" }, { attempts: [{ id: "attempt-1", status: "running" }] }, { target: { chapterId: " ", chapterNumber: 2 } }]) {
      const next = task(); Object.assign(next.steps[1], patch);
      assert.deepEqual(getTaskResults(next), results.filter(item => item.kind !== "knowledge"));
    }
    const duplicate = task(); duplicate.deliveryReport.evidence.push({ ...duplicate.deliveryReport.evidence[1] });
    assert.deepEqual(getTaskResults(duplicate), results.filter(item => item.kind !== "knowledge"));
    const unknownSource = task(); unknownSource.steps[1].target = { chapterId: "chapter-1" };
    assert.deepEqual(getTaskResults(unknownSource)[1], { ...results[1], chapterNumber: null, sourceRevisionId: null });
    const metadataOnly = task(); metadataOnly.deliveryReport.evidence = [];
    assert.deepEqual(getTaskResults(metadataOnly), []);
    const failedSaved = task(); failedSaved.steps[0].attempts[0].status = "cancelled";
    assert.deepEqual(getTaskResults(failedSaved), results, "Recorded partial output remains available without claiming execution success");
  } finally { await rm(directory, { recursive: true, force: true }); }
});
