import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { ImpactAnalysisPanel } from "../../../src/ImpactAnalysisPanel";

window.addEventListener("error", (event) => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
const tick = () => new Promise((resolve) => setTimeout(resolve, 25));
const check = (condition, message) => { if (!condition) throw new Error(message); };

const calls = [];
const workspace = {
  chapters: [{ id: "chapter-one", number: 1, title: "白塔", candidate: { id: "draft-after", title: "白塔", parentRevisionId: "draft-before" }, accepted: null }],
  modeling: [{ status: "ready", entities: [{ id: "linxi", displayName: "林夕" }] }],
};
const api = { runtime: {
  getHistoricalKnowledgeState: async (_projectId, snapshotId) => {
    check(snapshotId === "snapshot-current", "Historical knowledge was not pinned to the impact snapshot");
    return { snapshotId, coverage: { complete: true, truncatedEntries: false, pendingSources: [] }, entries: [
      { canonicalSubjectId: "linxi", sourceObjectId: "draft-chapter-one", sourceRevisionId: "draft-after", modelingBatchId: "batch-after", assertionIndex: 0, confirmationState: "confirmed", assertion: { kind: "assertion", predicate: "alive", value: true, storyTime: 1, evidence: { quote: "林夕还活着" } } },
      { canonicalSubjectId: "linxi", sourceObjectId: "draft-chapter-one", sourceRevisionId: "draft-after", modelingBatchId: "batch-after", assertionIndex: 1, confirmationState: "candidate", assertion: { kind: "assertion", predicate: "alive", value: true, storyTime: 1, evidence: { quote: "别处也见到林夕" } } },
    ] };
  },
  createHypothesis: async (_projectId, request) => { calls.push({ hypothesis: request }); return { id: "hypothesis-12345678901234567890123456789012" }; },
  getHypothesis: async () => ({ id: "hypothesis-12345678901234567890123456789012", baseSnapshotId: "snapshot-current", intent: "测试", authority: "hypothesisOnly", baseline: { entryCount: 2 }, simulation: { entries: [{ canonicalSubjectId: "linxi", confirmationState: "assumed", assertion: { predicate: "alive", value: false, storyTime: 1 } }], findings: [
    { type: "polarityConflict", subjectId: "linxi", predicate: "alive", confirmationState: "hypothesisConflict", leftSourceRevisionId: "draft-after", rightSourceRevisionId: null, leftEvidence: { quote: "林夕仍在" }, rightEvidence: null },
    { type: "epistemicDisagreement", subjectId: "linxi", predicate: "epistemicState", confirmationState: "candidateConflict", ownerId: "linxi", propositionRef: { batchId: "door-source", propositionId: "door" }, leftState: "known", rightState: "suspected", leftSourceRevisionId: "known-source", rightSourceRevisionId: "suspected-source", leftEvidence: { quote: "林夕确信门开着" }, rightEvidence: { quote: "林夕又怀疑门开着" } },
  ], coverage: { projectionComplete: true, globalImpactComplete: false } } }),
  getImpactAnalysis: async (_projectId, options) => {
  calls.push(options);
  return {
    baseSnapshotId: "snapshot-current", canonicalEntityId: "linxi", dependencyRuleVersion: "explicit-dependencies-v1",
    findings: [{ affectedEntityId: "letter", classification: "possibleImpact", hopCount: 1,
      evidencePath: [{ fromId: "linxi", toId: "letter", relationType: "reveals", dependencyRuleId: "explicit-dependencies-v1/reveals", sourceObjectId: "draft-chapter-one", sourceRevisionId: "draft-after", evidence: { quote: "林夕发现了信" } }] }],
    literalTextCandidates: [{ sourceObjectId: "draft-chapter-one", sourceRevisionId: "draft-after", sourcePath: "body", matchedName: "林夕", evidence: { quote: "林夕" }, snippet: "林夕发现了信" }],
    revisionDelta: { bodyChanged: true, sceneChanges: [], modelCandidateDelta: { status: "comparedCandidates", candidateChanges: [{ change: "newlyExtracted", claim: { subjectId: "letter", predicate: "fate", value: "hidden" }, sourceRevisionId: "draft-after", modelingBatchId: "batch-after", evidence: { quote: "藏信" } }], coverage: { from: "available", to: "available", complete: true, totalCandidateChanges: 1, evidenceReanchoredClaimCount: 0 } } },
    coverage: { globalImpactComplete: false, explicitGraphComplete: true, truncatedPaths: false, graphCoverage: { complete: true }, literalSearch: { literalNameSearchComplete: true, namesTruncated: false, resultsTruncated: false } },
  };
} } };
const root = createRoot(document.getElementById("root"));
root.render(<ImpactAnalysisPanel api={api} projectId="project-a" workspace={workspace} chapterId="chapter-one" dirty={false} refreshToken={0} />);

async function run() {
  await tick(); await tick();
  document.querySelector(".impact-analysis-panel summary").click();
  const input = document.querySelector('input[aria-label="影响分析实体"]');
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
  setter.call(input, "linxi");
  input.dispatchEvent(new Event("input", { bubbles: true }));
  await tick();
  const button = [...document.querySelectorAll("button")].find((item) => item.textContent === "检查影响");
  check(button && !button.disabled, "Impact analysis was not available");
  button.click();
  await tick(); await tick();
  check(calls.length === 1 && calls[0].fromRevisionId === "draft-before" && calls[0].toRevisionId === "draft-after", "Revision pair was not source-bound");
  check(document.body.textContent.includes("未证明全书影响已找全"), "Incomplete global coverage was hidden");
  check(document.body.textContent.includes("explicit-dependencies-v1"), "Dependency rule provenance was hidden");
  check(document.body.textContent.includes("林夕发现了信") && document.body.textContent.includes("藏信"), "Source evidence was not visible");
  setter.call(document.querySelector('input[aria-label="假设属性"]'), "alive");
  document.querySelector('input[aria-label="假设属性"]').dispatchEvent(new Event("input", { bubbles: true }));
  await tick();
  const sourceCheckbox = document.querySelector('.impact-analysis-sources input[type="checkbox"]');
  check(sourceCheckbox, "Bound source evidence was not offered for replacement");
  sourceCheckbox.click();
  await tick();
  const hypothesisButton = [...document.querySelectorAll("button")].find((item) => item.textContent === "检查假设");
  check(hypothesisButton && !hypothesisButton.disabled, "Hypothesis check was unavailable");
  hypothesisButton.click();
  await tick(); await tick();
  check(calls[1].hypothesis.baseSnapshotId === "snapshot-current" && calls[1].hypothesis.assumptions[0].modelingBatchId === "batch-after" && calls[1].hypothesis.assumptions[0].assertionIndex === 0 && calls[1].hypothesis.assumptions[1].subjectId === "linxi", "Hypothesis was not pinned to the selected evidence");
  check(document.body.textContent.includes("与已记录主张冲突"), "Hypothesis conflict was not displayed");
  check(document.body.textContent.includes("认知证据存在分歧（已知 / 怀疑）") && document.body.textContent.includes("door-source") && document.body.textContent.includes("林夕确信门开着") && document.body.textContent.includes("林夕又怀疑门开着"), "Epistemic disagreement did not expose both source-bound claims");
  root.render(<ImpactAnalysisPanel api={api} projectId="project-a" workspace={workspace} chapterId="chapter-one" dirty={true} refreshToken={0} />);
  await tick(); await tick();
  check(document.querySelector('button').disabled, "Dirty editor must not run a saved-revision analysis");
  check(!document.body.textContent.includes("林夕发现了信"), "Stale report remained visible after local edit");
  return { passed: true };
}
run().then((result) => { window.testResult = result; }).catch((error) => { window.testResult = { passed: false, error: error.stack + "\nDOM:" + document.body.innerText }; });
