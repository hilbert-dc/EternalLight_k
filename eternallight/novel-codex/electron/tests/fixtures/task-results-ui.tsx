import { emptyCreationIssueSummary } from "../helpers/creation-issues";
import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { WorkTaskPanel } from "../../../src/WorkTaskPanel";
import { useChapterPublication } from "../../../src/useChapterPublication";
const check = (value, message) => { if (!value) throw Error(message); };
const tick = () => new Promise(resolve => setTimeout(resolve, 25));
async function until(predicate, message) { for (let i = 0; i < 120 && !predicate(); i++) await tick(); check(predicate(), message); }
const root = createRoot(document.getElementById("root"));
const operations = [["generateChapter", "draft"], ["modelKnowledge", "knowledgeBatch"], ["generateBrief", "brief"], ["generateOutline", "outline"],
  ["generatePlan", "plan"], ["generatePropertyRules", "propertyRules"], ["extractCommitments", "commitmentExtraction"], ["reviseCommitment", "commitment"]];
const task = { id: "work", projectId: "A", threadId: "thread", goal: "分析第二章，再整理下一章的安排", kind: "conversation", status: "completed", deliveryStatus: "ready_for_review", version: 1,
  error: null, resumeMode: null, resumeBlockReason: null, deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], pendingActionIds: [], unsettledAttemptIds: [], evidence: operations.map(([,kind], i) => ({ stepId: `step-${i}`, attemptId: `attempt-${i}`, kind, referenceId: `result-${i}`, state: "recorded" })) },
  steps: operations.map(([operation], i) => ({ id: `step-${i}`, operation, expectedOutputKind: operations[i][1], status: "completed", inputRevision: 1,
    target: { chapterId: `chapter-${i}`, chapterNumber: i + 1, sourceRevisionId: `source-${i}`, commitmentId: "lamp" }, currentAttemptId: `attempt-${i}`, dependsOnStepId: null, instruction: null, sourceTaskId: null,
    readingProgress: null, outputRefs: [], changes: [], attempts: [{ id: `attempt-${i}`, threadId: "thread", turnId: `turn-${i}`, attemptNumber: 1, startedAt: i + 1, completedAt: i + 2, status: "completed", error: null }] })) };
const opened = [];
const noMutation = () => { throw Error("Result navigation executed a write"); };
const extraction = { revisionId: "result-6", snapshotId: "head", projectRevision: 1, authority: "candidateProposalBatch", reportCurrent: true, sourceCurrent: true,
  established: [], payload: { chapterId: "chapter-6", sourceRevisionId: "source-6", examinedRanges: [{ start: 0, end: 2 }], candidates: [{ id: "lamp", kind: "suspense", summary: "白塔的秘密", fulfillmentCriteria: "后文解释秘密", relatedEntityIds: [], plotThreadId: null, characterArcId: null, setupEvidenceRefs: [{ objectId: "draft-chapter-6", revisionId: "source-6", quote: "白塔", start: 0, end: 2 }], fulfillmentEvidenceRefs: [], status: "open", decisionReason: null, targetWindow: { kind: "open" } }] } };
async function panel({ callback = true, projectId = "A", running = false, dirty = false, discuss = false } = {}) {
  root.render(<div style={{ width: 300 }}><WorkTaskPanel projectId={projectId} api={{ runtime: { getTaskFinalReply: async () => null,
      getCommitmentExtraction: async (project, chapter, report) => { check(project === "A" && chapter === "chapter-6" && report === "result-6", "Inline report read changed its task source"); return structuredClone(extraction); },
      establishCommitment: noMutation, createCommitmentReview: noMutation, acceptReview: noMutation } }} tasks={[task]}
    activeCount={0} nextCursor={null} loadingMore={false} loaded reading={false} readError={null} onRetry={noMutation} onLoadMore={noMutation}
    revision={1} busy={false} agentRunning={running} dirty={dirty} readOnly onUndo={noMutation} onDiscussResult={discuss ? noMutation : undefined} onOpenResult={callback ? result => opened.push(result) : undefined} /></div>);
  await tick(); await tick();
}
const emptyArtifact = () => ({ candidate: null, accepted: null, working: null, locked: false, workingSelection: null });
let workspace = { project: { id: "A" }, revision: 1, headSnapshotId: "head", sourceDigest: "digest", index: { status: "ready" },
  artifacts: Object.fromEntries(["brief", "outline", "plan", "identity", "propertyRules"].map(kind => [kind, emptyArtifact()])),
  chapters: [{ id: "chapter-1", number: 2, title: "第二章", candidate: { id: "source-1", sceneGroups: [{ id: "scene", body: "正文", locked: false }] }, accepted: null }],
  modeling: [{ objectId: "draft-chapter-1", sourceRevisionId: "source-1", status: "ready", batchId: "result-1", entities: [], assertions: [], confirmationState: "candidate" }],
  tasks: [], reviews: [], factProposals: [], impacts: [], commits: [], workingBaselines: [], continuationIntents: [], planningReviews: [] };
let reads = 0, saves = 0, reviews = 0, targets = [], errors = [], failRead = false, holdRead = false, releaseRead;
const api = { runtime: {
  getCreativeWorkspace: async () => {
    reads += 1;
    if (failRead) { failRead = false; throw Error("检查已创建但读取断线"); }
    if (holdRead) { holdRead = false; const snapshot = structuredClone(workspace); return await new Promise(resolve => { releaseRead = () => resolve(snapshot); }); }
    return structuredClone(workspace);
  },
  createReview: async (project, kind, revisionId, proposals, selection, batch) => {
    check(project === "A" && kind === "draft" && revisionId === "source-1" && batch === "result-1" && selection.modelingBatchId === batch, "Review did not bind the clicked batch and text");
    reviews += 1;
    const review = { id: "review", artifactKind: "draft", artifactRevisionId: revisionId, projectRevision: 1, status: "ready", selections: { modelingBatchId: batch }, checks: [], factProposals: [] };
    workspace.reviews = [review]; failRead = true;
    return structuredClone(review);
  },
} };
let prepare;
function Publisher({ scope = "A:chapters:chapter-1", dirty = false }) {
  const controller = useChapterPublication({ scope, api, projectId: "A", chapterId: "chapter-1", target: null, bodyOnlyConfirmed: false, dirty,
    isBlocked: () => false, save: async () => { saves += 1; return true; }, savedRevision: () => "source-1",
    onWorkspace: () => {}, onTarget: target => targets.push(target), onError: error => errors.push(error) });
  prepare = controller.prepare;
  return <p>{controller.preparing ? "正在检查" : "可以检查"}</p>;
}
async function publisher(props = {}) { root.render(<Publisher {...props} />); await tick(); }
async function run() {
  await panel();
  const resultButtons = [...document.querySelectorAll(".task-result-actions button")];
  check(resultButtons.length === 8 && !document.body.innerText.includes("查看对应内容"), "One generic target replaced precise results: " + JSON.stringify(resultButtons.map(button => button.textContent)));
  const earlier = document.querySelector<HTMLDetailsElement>(".task-earlier-results");
  check(earlier && !earlier.open && document.querySelectorAll(".task-result-actions > .task-result-card").length === 3 && document.querySelector(".task-result-actions > .commitment-result-card [data-commitment-report=\"result-6\"]"), "Results did not start with three recent entries");
  earlier.querySelector<HTMLElement>("summary").click(); await tick();
  check(earlier.open && earlier.querySelectorAll("button").length === 5, "Earlier results are inaccessible");
  resultButtons.find(button => button.textContent === "检查第 2 章故事设定").click();
  check(opened.length === 1 && opened[0].kind === "knowledge" && opened[0].chapterId === "chapter-1" && opened[0].referenceId === "result-1" && opened[0].attemptId === "attempt-1", "Knowledge button opened a different chapter/batch/attempt");
  const actions = document.querySelector(".task-result-actions");
  check(actions.scrollWidth <= actions.clientWidth + 1, "300px results overflowed horizontally");
  const snapshots = { resultLabels: resultButtons.map(button => button.textContent), narrowFits: true };
  await panel({ discuss: true });
  check([...document.querySelectorAll(".task-result-card:not(.commitment-result-card)")].every(card => card.querySelectorAll("button").length === 1 && !/继续讨论|引用到输入框/.test(card.textContent)), "Ordinary result cards duplicated the composer");
  await panel({ running: true });
  const readable = [...document.querySelectorAll<HTMLButtonElement>(".task-result-card:not(.commitment-result-card) button")];
  check(readable.length > 0 && readable.every(button => !button.disabled), "Execution incorrectly blocked saved result reading");
  check([...document.querySelectorAll<HTMLButtonElement>(".commitment-task-actions button")].every(button => button.disabled), "Reading unlocked author mutations");
  await panel({ dirty: true });
  [...document.querySelectorAll<HTMLButtonElement>(".task-result-actions button")].find(button => button.textContent === "检查第 2 章故事设定").click();
  check(opened.length === 2, "Navigation bypassed its owner's opportunity to apply the unsaved-change gate");
  await panel({ callback: false });
  check(document.querySelector('[data-commitment-report="result-6"]') && !document.querySelector('.task-result-actions > .task-result-card:not(.commitment-result-card)'), "Readonly inline report depended on a separate navigation callback");
  check([...document.querySelectorAll('.commitment-task-actions button')].every(button => button.disabled), "Readonly report offered author writes");
  await panel({ projectId: "B" }); check(!document.querySelector(".task-result-actions"), "Another project's result was exposed");
  await publisher({ dirty: true });
  await prepare({ expectedSource: { revisionId: "source-1", batchId: "result-1" } });
  check(saves === 0 && reads === 0 && reviews === 0 && errors.at(-1).includes("未保存"), "Pinned analysis silently saved or checked changed local text");
  await publisher();
  await prepare({ expectedSource: { revisionId: "source-1", batchId: "older-batch" } });
  check(reviews === 0 && targets.length === 0 && errors.at(-1).includes("不会改用其他分析"), "Old result silently opened the latest analysis");
  await prepare({ expectedSource: { revisionId: "source-1", batchId: "result-1" } });
  check(reviews === 1 && targets.length === 0 && errors.at(-1).includes("读取断线"), "Lost read receipt misrepresented a ready check");
  await prepare({ expectedSource: { revisionId: "source-1", batchId: "result-1" } });
  check(reviews === 1 && targets.length === 1 && targets[0].reviewId === "review" && targets[0].revisionId === "source-1", "Read-only recovery recreated or retargeted the check");
  snapshots.recovered = { revisionId: targets[0].revisionId, reviewId: targets[0].reviewId, reviewWrites: reviews, saves };
  holdRead = true;
  const pending = prepare({ expectedSource: { revisionId: "source-1", batchId: "result-1" } });
  await until(() => releaseRead, "Late read did not start");
  await publisher({ scope: "B:chapters:chapter-2" }); releaseRead(); await pending;
  check(reviews === 1 && targets.length === 1, "Late pinned result affected another owner");
  window.testResult = { passed: true, snapshots };
}
run().catch(error => { window.testResult = { passed: false, error: error.stack }; });
