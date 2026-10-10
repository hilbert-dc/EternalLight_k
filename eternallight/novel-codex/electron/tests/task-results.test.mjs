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

test("inline commitment decisions preserve exact report history without granting obsolete result navigation", async () => {
  const directory = await mkdtemp(join(tmpdir(), "commitment-decision-results-"));
  try {
    const path = join(directory, "logic.mjs");
    await build({ entryPoints: [new URL("../../src/task-results.ts", import.meta.url).pathname], outfile: path, bundle: true, platform: "node", format: "esm" });
    const { getCommitmentDecisionResults, getTaskResults } = await import(pathToFileURL(path).href);
    const expected = [{ projectId: "project-one", workTaskId: "work-one", stepId: "step-6", attemptId: "attempt-6",
      referenceId: "reference-6", kind: "commitmentExtraction", chapterId: "chapter-6", chapterNumber: 7,
      sourceRevisionId: "source-6", deliveryState: "recorded" }];
    assert.deepEqual(getCommitmentDecisionResults(task()), expected);

    for (const state of ["completed", "failed", "cancelled"]) {
      const fixture = task(); fixture.steps[6].attempts[0].status = state;
      assert.deepEqual(getCommitmentDecisionResults(fixture), expected, "A recorded report remains readable independently of terminal execution success");
    }
    for (const [recordState, stepState] of [["superseded", "completed"], ["recorded", "superseded"], ["superseded", "superseded"]]) {
      const fixture = task(); fixture.deliveryReport.evidence[6].state = recordState; fixture.steps[6].status = stepState;
      assert.deepEqual(getCommitmentDecisionResults(fixture), [{ ...expected[0], deliveryState: "superseded" }]);
      assert.equal(getTaskResults(fixture).some(result => result.kind === "commitmentExtraction"), false,
        "Exact historical proposal display must not reopen superseded current-result navigation");
    }
    for (const patch of [{ state: "pending" }, { state: "missing" }, { kind: "draft" }, { attemptId: "foreign-attempt" },
      { stepId: "foreign-step" }, { referenceId: null }, { referenceId: " " }, { referenceId: "report\nforeign" }]) {
      const fixture = task(); Object.assign(fixture.deliveryReport.evidence[6], patch);
      assert.deepEqual(getCommitmentDecisionResults(fixture), [], `Incomplete or foreign delivery proof was accepted: ${JSON.stringify(patch)}`);
    }
    for (const patch of [{ currentAttemptId: null }, { currentAttemptId: "foreign-attempt" }, { id: " " },
      { operation: "generateChapter" }, { attempts: [] }, { attempts: [{ id: "attempt-6", status: "running" }] },
      { attempts: [{ id: "attempt-6", status: "queued" }] }, { target: { chapterId: " " } },
      { target: { chapterId: "chapter-6", sourceRevisionId: " " } },
      { target: { chapterId: "chapter-6", sourceRevisionId: "source\u0000foreign" } }]) {
      const fixture = task(); Object.assign(fixture.steps[6], patch);
      assert.deepEqual(getCommitmentDecisionResults(fixture), [], `Invalid current step ownership was accepted: ${JSON.stringify(patch)}`);
    }
    const duplicates = task(); duplicates.deliveryReport.evidence.push({ ...duplicates.deliveryReport.evidence[6] });
    assert.deepEqual(getCommitmentDecisionResults(duplicates), [], "Ambiguous report proof was accepted");
    const duplicateAttempts = task(); duplicateAttempts.steps[6].attempts.push({ ...duplicateAttempts.steps[6].attempts[0] });
    assert.deepEqual(getCommitmentDecisionResults(duplicateAttempts), [], "Ambiguous current attempt was accepted");
    const foreignEvidence = task(); foreignEvidence.deliveryReport.evidence.push({ ...foreignEvidence.deliveryReport.evidence[6], stepId: "foreign-step", attemptId: "foreign-attempt", referenceId: "foreign-report" });
    assert.deepEqual(getCommitmentDecisionResults(foreignEvidence), expected, "Unrelated report evidence changed this exact step’s identity");
    const oldAttempt = task(); oldAttempt.steps[6].attempts.unshift({ id: "older-attempt", status: "completed" });
    oldAttempt.steps[6].outputRefs.push({ kind: "commitmentExtraction", revisionId: "foreign-report", objectId: "commitment-extraction-other" });
    assert.deepEqual(getCommitmentDecisionResults(oldAttempt), expected, "Old attempts or unverified output references replaced the saved proof");
    const metadataOnly = task(); metadataOnly.deliveryReport.evidence = [];
    assert.deepEqual(getCommitmentDecisionResults(metadataOnly), [], "Metadata alone fabricated a saved report");
    for (const patch of [{ projectId: " " }, { id: " " }, { projectId: "project\u0000foreign" }]) {
      const fixture = task(); Object.assign(fixture, patch);
      assert.deepEqual(getCommitmentDecisionResults(fixture), [], "Invalid task or project identity was accepted");
    }
    const optionalSource = task(); optionalSource.steps[6].target = { chapterId: "chapter-6", chapterNumber: -1 };
    assert.deepEqual(getCommitmentDecisionResults(optionalSource), [{ ...expected[0], chapterNumber: null, sourceRevisionId: null }],
      "Unknown chapter display metadata should remain unknown rather than be guessed");
    const sameChapter = task();
    sameChapter.steps.push({ ...structuredClone(sameChapter.steps[6]), id: "new-step", currentAttemptId: "new-attempt", attempts: [{ id: "new-attempt", status: "completed" }] });
    sameChapter.deliveryReport.evidence[6].state = "superseded";
    sameChapter.deliveryReport.evidence.push({ stepId: "new-step", attemptId: "new-attempt", kind: "commitmentExtraction", referenceId: "new-report", state: "recorded" });
    assert.deepEqual(getCommitmentDecisionResults(sameChapter), [{ ...expected[0], deliveryState: "superseded" },
      { ...expected[0], stepId: "new-step", attemptId: "new-attempt", referenceId: "new-report" }], "Reports for one chapter were collapsed into latest");
    assert.deepEqual(getTaskResults(sameChapter).filter(result => result.kind === "commitmentExtraction").map(result => result.referenceId), ["new-report"]);
  } finally { await rm(directory, { recursive: true, force: true }); }
});
