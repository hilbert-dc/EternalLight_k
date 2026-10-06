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
  error: null, resumeMode: null, resumeBlockReason: null, deliveryReport: { pendingActionIds: [], unsettledAttemptIds: [], evidence: operations.map(([,kind], i) => ({ stepId: `step-${i}`, attemptId: `attempt-${i}`, kind, referenceId: `result-${i}`, state: "recorded" })) },
  steps: operations.map(([operation], i) => ({ id: `step-${i}`, operation, expectedOutputKind: operations[i][1], status: "completed", inputRevision: 1,
    target: { chapterId: `chapter-${i}`, chapterNumber: i + 1, sourceRevisionId: `source-${i}`, commitmentId: "lamp" }, currentAttemptId: `attempt-${i}`, dependsOnStepId: null, instruction: null, sourceTaskId: null,
    readingProgress: null, outputRefs: [], changes: [], attempts: [{ id: `attempt-${i}`, threadId: "thread", turnId: `turn-${i}`, attemptNumber: 1, startedAt: i + 1, completedAt: i + 2, status: "completed", error: null }] })) };
const opened = [];
const noMutation = () => { throw Error("Result navigation executed a write"); };
async function panel({ callback = true, projectId = "A", running = false, dirty = false } = {}) {
  root.render(<div style={{ width: 300 }}><WorkTaskPanel projectId={projectId} api={{ runtime: { getTaskFinalReply: async () => null } }} tasks={[task]}
    activeCount={0} nextCursor={null} loadingMore={false} loaded reading={false} readError={null} onRetry={noMutation} onLoadMore={noMutation}
    revision={1} busy={false} interruptionBusy={false} agentRunning={running} agentAvailable dirty={dirty} readOnly
    onControl={noMutation} onUndo={noMutation} onOpenResult={callback ? result => opened.push(result) : undefined} /></div>);
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
    isBlocked: () => false, save: async () => { saves += 1; return true; }, savedRevision: () => "source-1", manualProposals: () => [],
    onWorkspace: () => {}, onTarget: target => targets.push(target), onError: error => errors.push(error) });
  prepare = controller.prepare;
  return <p>{controller.preparing ? "正在检查" : "可以检查"}</p>;
}
async function publisher(props = {}) { root.render(<Publisher {...props} />); await tick(); }
async function run() {
  await panel();
  const resultButtons = [...document.querySelectorAll(".task-result-actions button")];
  check(resultButtons.length === 8 && !document.body.innerText.includes("查看对应内容"), "One generic target replaced precise results");
  resultButtons[1].click();
  check(opened.length === 1 && opened[0].kind === "knowledge" && opened[0].chapterId === "chapter-1" && opened[0].referenceId === "result-1" && opened[0].attemptId === "attempt-1", "Knowledge button opened a different chapter/batch/attempt");
  const actions = document.querySelector(".task-result-actions");
  check(actions.scrollWidth <= actions.clientWidth + 1, "300px results overflowed horizontally");
  const snapshots = { resultLabels: resultButtons.map(button => button.innerText), narrowFits: true };
  await panel({ running: true });
  check([...document.querySelectorAll(".task-result-actions button")].every(button => button.disabled), "Execution did not lock result checks");
  await panel({ dirty: true });
  document.querySelectorAll(".task-result-actions button")[1].click();
  check(opened.length === 2, "Navigation bypassed its owner's opportunity to apply the unsaved-change gate");
  await panel({ callback: false }); check(!document.querySelector(".task-result-actions"), "Unavailable navigation exposed inert results");
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
