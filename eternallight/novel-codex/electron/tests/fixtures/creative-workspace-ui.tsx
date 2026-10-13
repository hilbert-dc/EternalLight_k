import { emptyCreationIssueSummary } from "../helpers/creation-issues";
import "../../../src/styles.css";
import React from "react";
import { createAgentRun } from "../../../src/agent-events";
import { createRoot } from "react-dom/client";
import { CreativeWorkspaceView, revealControl } from "./creative-workspace-harness";
import { UserSettings } from "../../../src/UserSettings";
import { WorkspaceDiagnostics } from "../../../src/WorkspaceDiagnostics";
window.addEventListener("error", (event) => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
const tick = () => new Promise((resolve) => setTimeout(resolve, 30));
const check = (condition, message) => { if (!condition) throw new Error(message); };
const waitForFocus = async (label) => {
  const deadline = performance.now() + 2000;
  while (document.activeElement?.getAttribute("aria-label") !== label && performance.now() < deadline) await tick();
  check(document.activeElement?.getAttribute("aria-label") === label, `Focus did not reach ${label}`);
};
const button = (text) => [...document.querySelectorAll("button")].find((item) => item.textContent === text || item.getAttribute("aria-label") === text);
const click = async (text) => {
  const sceneAction = /^(?:上移场景|下移场景|删除场景|保护场景|取消保护场景) (\d+)$/.exec(text);
  if (sceneAction && !document.querySelector(`[role="menu"][aria-label="场景 ${sceneAction[1]} 操作"]`)) {
    const trigger = document.querySelector<HTMLButtonElement>(`button[aria-label="场景 ${sceneAction[1]} 操作"]`);
    check(trigger && !trigger.matches(":disabled"), `Unavailable scene menu: ${text}`); await revealControl(trigger); trigger.scrollIntoView({ block: "nearest" }); await tick(); trigger.focus({ preventScroll: true }); trigger.click(); await tick();
  }
  const element = button(text); check(element && !element.matches(":disabled"), `Unavailable: ${text}`); await revealControl(element); element.click(); await tick();
};
const input = async (label, value) => {
  const element = document.querySelector(`textarea[aria-label="${label}"]`);
  check(element && !element.matches(":disabled"), `Unavailable: ${label}`);
  await revealControl(element);
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(element, value);
  element.dispatchEvent(new Event("input", { bubbles: true })); await tick();
};
const chapter = { id: "chapter-1", number: 1, title: "第一章", passages: [] };
const scenes = [{ id: "a", sceneId: "original-a", body: "原场景 A", locked: false, baseAcceptedRevisionId: "accepted-1", candidateDraftRevisionId: "draft-1" }, { id: "b", sceneId: "original-b", body: "原场景 B", locked: false }];
const revision = { id: "draft-1", version: 1, title: "第一章", body: "原场景 A\n\n---\n\n原场景 B", sceneGroups: scenes };
let workspace = { project: { id: "ui-test" }, revision: 1, headSnapshotId: "snapshot-ui-1", sourceDigest: "a".repeat(64), index: { status: "ready" }, artifacts: Object.fromEntries(["brief", "outline", "plan"].map((kind) => [kind, { candidate: null, accepted: null, working: null, locked: false, workingSelection: null }])), chapters: [{ ...chapter, candidate: revision, accepted: revision, working: null, workingSelection: null }], workingBaselines: [], continuationIntents: [], planningReviews: [], commits: [], tasks: [], impacts: [], modeling: [], factProposals: [], reviews: [], chapterPlanBindings: [] };
let releaseSave, saveResult, selectedForAccept, planningReviewRequest, reviewedModelingBatch, resumedWorkTask, recoveredWorkTask, requestedGenerationIntent, feedbackChoice, retriedRun, releaseRebind, rebindRequest;
let failRebindRefresh = false;
let lastCheckpointExpected, workspaceReads = 0, holdWorkspaceRead = false, releaseWorkspaceRead, checkpointCalls = 0;
let workTasks = [];
const emptyDeliveryReport = { evidence: [], pendingActionIds: [], unsettledAttemptIds: [] };
let pagedWorkTasks = null;
let staleWorkTaskPage = false;
let auditJob = null;
let showDiagnostics = false;
let diagnosticReads = 0;
let entityAuditJob = null;
let relationAuditJob = null;
let knowledgeAuditJob = null;
let snapshotKnowledgeAuditJob = null;
let factAuditJob = null;
let generationRuns = [];
let failGenerationList = false;
let savedFutureIntent;
let savedCurrentRevision;
let failSave = false, failPendingSave = false;
const chapterHistory = new Map();
let historyOverride = null;
let holdHistory = false, releaseHistory = null;
const api = { runtime: {
  getWorkCatalog: async () => ({ revision: 0, volumes: [], placements: [] }),
  getIdentityCatalog: async () => ({ entries: [], ambiguousNames: [], identityRevisionId: null, identityDecisions: [], truncated: false, ambiguityScanComplete: true }),
  getTaskFinalReply: async () => null,
  getChapterDraftHistory: async (projectId, chapterId, revisionId) => {
    const revision = chapterHistory.get(revisionId);
    if (!revision) throw new Error("历史章节暂时不可读取，草稿仍保留。");
    const result = structuredClone({ projectId, revision });
    if (historyOverride) result.revision.chapterId = historyOverride;
    if (holdHistory) { holdHistory = false; await new Promise(resolve => { releaseHistory = () => resolve(); }); }
    return result;
  },
  getCreativeWorkspace: async () => { workspaceReads++; if (failRebindRefresh) throw new Error("刷新连接中断"); const value = structuredClone(workspace); if (holdWorkspaceRead) { holdWorkspaceRead = false; await new Promise(resolve => { releaseWorkspaceRead = (projectId) => { if (projectId) value.project.id = projectId; resolve(); }; }); } return value; },
  rebindChapterPlan: async (_projectId, request) => {
    rebindRequest = request;
    await new Promise(resolve => { releaseRebind = resolve; });
    workspace.revision++;
    workspace.chapterPlanBindings[0].plan_revision_id = request.planRevisionId;
    failRebindRefresh = true;
    return { previousPlanRevisionId: request.previousPlanRevisionId, previousChapterPlanId: request.previousChapterPlanId, sourceRevisionId: request.sourceRevisionId, binding: { planRevisionId: request.planRevisionId, chapterPlanId: request.chapterPlanId, chapterId: request.chapterId, chapterNumber: 1, resultRevision: workspace.revision } };
  },
  listGenerationRuns: async () => { if (failGenerationList) throw new Error("创作状态读取中断"); return { runs: structuredClone(generationRuns) }; },
  getSceneAssessment: async () => ({ revisionId: "assessment-ui", authority: "candidateAnalysis", state: "current", assessment: { assessments: [{ sceneId: "scene-ui", verdict: "insufficientEvidence", recommendation: "askAuthor", rationale: "第二场尚无可靠事件证据", effectAssessments: [{ effectId: "effect-ui", verdict: "insufficientEvidence", rationale: "无可定位的事件", actualEventRefs: [], commitmentAssessments: [{ reference: { type: "commitment", commitmentId: "lamp", revisionId: "lamp-1" }, referenceState: "sourceChanged", outcome: "insufficientEvidence", rationale: "原铺垫正文已经改变，不能判断兑现", actualEventRefs: [] }] }] }] } }),
  listWorkTasks: async (_projectId, cursor = null) => {
    if (cursor !== null && staleWorkTaskPage) throw new Error("WORK_TASK_STALE");
    return pagedWorkTasks
      ? structuredClone(cursor === null ? pagedWorkTasks.first : pagedWorkTasks.second)
      : ({ tasks: structuredClone(workTasks), nextCursor: null, activeCount: workTasks.filter((task) => ["running", "paused", "blocked"].includes(task.status)).length });
  },
  getSearchAudit: async () => { diagnosticReads++; return { job: structuredClone(auditJob) }; },
  startSearchAudit: async () => { auditJob = { id: "search-audit-00000000000000000000000000000001", projectId: "ui-test", status: "queued", version: 0, sourceCount: 12, indexGeneration: 0, nextRowid: 0, verifiedConsistent: null, error: null, audit: { revision_count: 0, mismatch_count: 0, orphaned_documents: 0, orphaned_grams: 0 } }; return structuredClone(auditJob); },
  controlSearchAudit: async (_projectId, _jobId, command) => { auditJob = { ...auditJob, version: auditJob.version + 1, status: command === "pause" ? "paused" : command === "resume" ? "queued" : "cancelled" }; return structuredClone(auditJob); },
  getEntityAudit: async () => ({ job: structuredClone(entityAuditJob) }),
  startEntityAudit: async () => { entityAuditJob = { id: "entity-audit-00000000000000000000000000000001", projectId: "ui-test", status: "queued", version: 0, sourceCount: 5, indexGeneration: 0, nextRowid: 0, verifiedConsistent: null, error: null, audit: { publishedBatches: 0, expectedClaims: 0, expectedNames: 0, missingClaims: 0, extraClaims: 0, changedClaims: 0, missingNames: 0, extraNames: 0, affectedBatchIds: [] } }; return structuredClone(entityAuditJob); },
  controlEntityAudit: async (_projectId, _jobId, command) => { entityAuditJob = { ...entityAuditJob, version: entityAuditJob.version + 1, status: command === "pause" ? "paused" : command === "resume" ? "queued" : "cancelled" }; return structuredClone(entityAuditJob); },
  getRelationAudit: async () => ({ job: structuredClone(relationAuditJob) }),
  startRelationAudit: async () => { relationAuditJob = { id: "relation-audit-00000000000000000000000000000001", projectId: "ui-test", status: "queued", version: 0, sourceCount: 7, indexGeneration: 0, nextRowid: 0, verifiedConsistent: null, error: null, audit: { publishedBatches: 0, expectedClaims: 0, missingClaims: 0, extraClaims: 0, changedClaims: 0, affectedBatchIds: [] } }; return structuredClone(relationAuditJob); },
  controlRelationAudit: async (_projectId, _jobId, command) => { relationAuditJob = { ...relationAuditJob, version: relationAuditJob.version + 1, status: command === "pause" ? "paused" : command === "resume" ? "queued" : "cancelled" }; return structuredClone(relationAuditJob); },
  getKnowledgeAudit: async () => ({ job: structuredClone(knowledgeAuditJob) }),
  startKnowledgeAudit: async () => { knowledgeAuditJob = { id: "knowledge-audit-00000000000000000000000000000001", projectId: "ui-test", status: "queued", version: 0, sourceCount: 9, indexGeneration: 0, entityGeneration: 0, nextRowid: 0, verifiedConsistent: null, error: null, audit: { entities: { publishedBatches: 0, expectedClaims: 0, expectedNames: 0, missingClaims: 0, extraClaims: 0, changedClaims: 0, missingNames: 0, extraNames: 0, affectedBatchIds: [] }, assertions: { publishedBatches: 0, expectedClaims: 0, missingClaims: 0, extraClaims: 0, changedClaims: 0, missingDependencies: 0, extraDependencies: 0, affectedBatchIds: [] }, epistemic: { publishedBatches: 0, expectedClaims: 0, missingClaims: 0, extraClaims: 0, changedClaims: 0, affectedBatchIds: [] }, heads: { publishedBatches: 0, expectedHeads: 0, missingHeads: 0, changedHeads: 0, extraHeads: 0, invalidReadyBatches: 0, ambiguousReadyBatches: 0, affectedObjectIds: [] } } }; return structuredClone(knowledgeAuditJob); },
  controlKnowledgeAudit: async (_projectId, _jobId, command) => { knowledgeAuditJob = { ...knowledgeAuditJob, version: knowledgeAuditJob.version + 1, status: command === "pause" ? "paused" : command === "resume" ? "queued" : "cancelled" }; return structuredClone(knowledgeAuditJob); },
  getSnapshotKnowledgeAudit: async () => ({ job: structuredClone(snapshotKnowledgeAuditJob) }),
  startSnapshotKnowledgeAudit: async () => { snapshotKnowledgeAuditJob = { id: "snapshot-knowledge-audit-00000000000000000000000000000001", projectId: "ui-test", status: "queued", version: 0, sourceCount: 13, sourceMaxRowid: 13, sourceGeneration: 0, nextRowid: 0, verifiedConsistent: null, error: null, audit: { members: 0, readyCaptures: 0, pendingCaptures: 0, missingCaptures: 0, invalidCaptures: 0, invalidMembers: 0, orphanCaptures: 0, affectedMembers: [] } }; return structuredClone(snapshotKnowledgeAuditJob); },
  controlSnapshotKnowledgeAudit: async (_projectId, _jobId, command) => { snapshotKnowledgeAuditJob = { ...snapshotKnowledgeAuditJob, version: snapshotKnowledgeAuditJob.version + 1, status: command === "pause" ? "paused" : command === "resume" ? "queued" : "cancelled" }; return structuredClone(snapshotKnowledgeAuditJob); },
  getFactAudit: async () => ({ job: structuredClone(factAuditJob) }),
  startFactAudit: async () => { factAuditJob = { id: "fact-audit-00000000000000000000000000000001", projectId: "ui-test", status: "queued", version: 0, sourceCount: 11, sourceMaxRowid: 11, sourceGeneration: 0, nextRowid: 0, verifiedConsistent: null, error: null, audit: { facts: 0, invalidLinks: 0, invalidPayloads: 0, affectedFacts: [] } }; return structuredClone(factAuditJob); },
  controlFactAudit: async (_projectId, _jobId, command) => { factAuditJob = { ...factAuditJob, version: factAuditJob.version + 1, status: command === "pause" ? "paused" : command === "resume" ? "queued" : "cancelled" }; return structuredClone(factAuditJob); },
  listPlanningImpacts: async () => ({ data: [], nextCursor: null, counts: { total: 0, open: 0, retained: 0, lockedConflict: 0 } }),
  saveDraftCheckpoint: async (_, payload, expectedRevision, expectedCandidateRevisionId) => { lastCheckpointExpected = { expectedRevision, expectedCandidateRevisionId }; checkpointCalls++; saveResult = payload; if (failSave) throw new Error("磁盘写入失败"); await new Promise((resolve) => { releaseSave = resolve; }); if (failPendingSave) { failPendingSave = false; throw new Error("保存期间版本变化"); } workspace = { ...workspace, revision: 2, chapters: [{ ...workspace.chapters[0], candidate: { ...revision, ...payload, id: "draft-2" } }] }; return { revisionId: "draft-2" }; },
  createReview: async (_, kind, revisionId, proposals, __, modelingBatchId) => {
    if (modelingBatchId) {
      reviewedModelingBatch = modelingBatchId;
      workspace.reviews = [{ id: "review-modeling", artifactKind: "draft", artifactRevisionId: revisionId, projectRevision: workspace.revision, status: "ready", checks: [], selections: { modelingBatchId } }];
      workspace.factProposals = [{ id: "fact-modeling", draft_revision_id: revisionId, sceneGroupId: "a", factKind: "world", subjectId: "linxi", predicate: "hidLetter", value: true, epistemicOwnerId: null, epistemicState: "unknown", evidence: "原场景 A", source: "agent", reviewDecision: "candidate", readingChapter: 1, sceneOrder: 1,
        stateProperty: { schema: { cardinality: "single", valueType: { type: "boolean" } }, validFrom: null, validUntilExclusive: null, schemaAuthority: "modelDeclared" } },
        { id: "fact-event", draft_revision_id: revisionId, sceneGroupId: "a", factKind: "world", subjectId: "hide-letter", predicate: "hideLetterEvent", value: {}, source: "agent", reviewDecision: "candidate", evidence: "原场景 A", sceneOrder: 1,
          storyEvent: { eventId: "hide-letter", eventType: "hideLetterEvent", storyTime: null, untilExclusive: null,
            evaluationTarget: { snapshotId: null, sourceBatchId: "batch-event", assertionIndex: 0 },
            participants: [{ role: "actor", entityId: "linxi" }, { role: "item", entityId: "letter" }], preconditions: [{ sourceBatchId: "prior-batch", assertionIndex: 2 }], effects: [{sourceBatchId:null,assertionIndex:0}],
            temporalAssessment: { basis: "sourceBoundTemporalReferences", preconditions: [{ referenceIndex: 0, timing: "applicableBeforeEvent" }], effects: [{ referenceIndex: 0, timing: "unknownTime" }], semanticTruthInferred: false, globalConditionsVerified: false, stateChangesExecuted: false },
            conditionAssessment: { basis: "authorRulesSourceClaims", status: "interpreted", result: "both", observedResult: "both", ruleRevisionId: "rules-adopted", identityRevisionId: null, sourceClaims: [{ sourceBatchId: "batch-counter", assertionIndex: 0 }], coverage: { complete: true, sources: { pendingSources: 0, totalSources: 213, scannedSources: 2, omittedSources: 0, sourceBytes: 1000, relevantSources: 2, sourceProofId: "audit-verified", entityKindsComplete: true }, unresolvedChecks: 0, evidenceTruncated: false }, stateChangesExecuted: false, worldTruthCertified: false } } }];
      return { id: "review-modeling", projectRevision: workspace.revision, factProposals: workspace.factProposals, checks: [], provisionalImpacts: [], artifactKind: kind, artifactRevisionId: revisionId, status: "ready" };
    }
    return { id: "review-1", projectRevision: workspace.revision, factProposals: proposals, checks: [], provisionalImpacts: [], artifactKind: kind, artifactRevisionId: revisionId, status: "ready" };
  },
  acceptReview: async (_, reviewId, expectedRevision, selections) => {
    check(expectedRevision === workspace.revision, "Publication used a stale workspace revision");
    selectedForAccept = selections;
    workspace.reviews.find(review => review.id === reviewId).status = "accepted";
    workspace.chapters[0].accepted = workspace.chapters[0].candidate;
    workspace.revision++;
  },
}, agent: {
  addFutureIntent: async (_projectId, runId, instruction) => { savedFutureIntent = { runId, instruction }; return { intentGeneration: 1 }; },
  reviseCurrentChapter: async (_projectId, runId, instruction) => { savedCurrentRevision = { runId, instruction }; return { intentGeneration: 2, pendingCurrentIntentGeneration: 2 }; },
  resumeWorkTask: async (projectId, taskId) => { resumedWorkTask = taskId; const result = { ...workTasks.find(task => task.id === taskId), id: taskId, projectId, status: "completed" }; workTasks = []; return result; },
  recoverWorkTask: async (_projectId, taskId) => { recoveredWorkTask = taskId; workTasks = []; },
  controlWorkTask: async () => {},
  interrupt: async () => {},
} };
const discussedPrompts = [];
const props = { api, projectId: "ui-test", view: "chapters", state: { novelEdition: { chapters: [chapter] }, canon: { characters: [], relationships: [], events: [], locations: [], worldRules: [] } }, selectedChapterId: chapter.id, agentAvailable: true, agentRunning: false, refreshToken: 0, onSelectChapter() {}, onDiscuss(prompt) { discussedPrompts.push(prompt); }, async onGenerateWork(intent, targetChapters, expectedProjectRevision, expectedBaseSnapshotId, expectedSourceDigest) { requestedGenerationIntent = { intent, targetChapters, expectedProjectRevision, expectedBaseSnapshotId, expectedSourceDigest }; }, async onContinueWork() {}, async onRetryWork(runId) { retriedRun = runId; }, async onResolveFeedback(runId, revisionId, choice, instruction) { feedbackChoice = { runId, revisionId, choice, instruction }; }, async onReviewPlanning(request) { planningReviewRequest = request; }, async onCancelPlanningReview() {}, async onGenerateChapter() {}, onOpenPlanningTarget() {}, onReturnToFeedback() {}, async onFormalChange() {}, onDirtyChange() {} };
const taskConversationContainer = document.createElement("aside");
taskConversationContainer.className = "conversation-rail-scroll conversation-task-records";
taskConversationContainer.setAttribute("aria-label", "创作对话");
Object.assign(taskConversationContainer.style, { position: "fixed", right: "0", top: "0", width: "340px", height: "100vh" });
document.body.append(taskConversationContainer);
document.getElementById("root").style.marginRight = "340px";
let root = createRoot(document.getElementById("root"));
const render = async (extra = {}) => { root.render(<><CreativeWorkspaceView taskConversationContainer={taskConversationContainer} conversation={extra.conversationContinuation ? { run: { ...createAgentRun(extra.conversationContinuation.intent), state: "completed", answer: "已讨论后续创作目标。" }, renderCurrent: () => <p data-current-reply>已讨论后续创作目标。</p>, renderHistory: () => null } : null} {...props} {...extra} />
  {showDiagnostics && <UserSettings profile={{ name: "测试作者", email: "writer@example.test" }} theme="light" onTheme={() => {}}
    onSaveProfile={async () => null} onChangePassword={async () => null} onSignOut={async () => {}} onDeleteAccount={async () => {}}
    diagnostics={<WorkspaceDiagnostics api={api} projectId={props.projectId} refreshToken={extra.refreshToken ?? 0} />} />}
</>); await tick(); await tick(); await tick(); };
async function run() {
  let saveEditor, discardEditor, dirtyEditor = false, finishEditorSave;
  const commitmentPayload = { id: "ui-commitment", kind: "suspense", summary: "白塔的秘密", fulfillmentCriteria: "正文解释秘密", relatedEntityIds: [], plotThreadId: null, characterArcId: null,
    setupEvidenceRefs: [{ objectId: "draft-chapter-1", revisionId: "draft-1", quote: "原场景 A", start: 0, end: 5 }], fulfillmentEvidenceRefs: [], status: "open", decisionReason: null, targetWindow: { kind: "open" } };
  let commitmentDetail = { projectId: "ui-test", objectId: "commitment-ui-commitment", revisionId: "commitment-ui-1", snapshotId: "commitment-snapshot-1", payload: commitmentPayload,
    extractionOrigin: { chapterId: "chapter-1", reportRevisionId: "report-ui-1", proposalId: "proposal-ui-1" }, sourceCurrent: true, confirmationState: "candidate", confirmationCommitId: null,
    authorControls: { projectRevision: 1, undoAvailable: false }, interpretation: "sourceBoundNarrativeClaim", literaryFulfillmentVerified: false };
  api.runtime.getCommitmentExtraction = async () => ({ revisionId: "report-ui-1", snapshotId: "snapshot-ui-1", projectRevision: 1, authority: "candidateProposalBatch", reportCurrent: true, sourceCurrent: true,
    established: [{ proposalId: "proposal-ui-1", commitmentId: "ui-commitment" }], payload: { chapterId: "chapter-1", sourceRevisionId: "draft-1", examinedRanges: [{ start: 0, end: 5 }], candidates: [{ ...commitmentPayload, id: "proposal-ui-1" }] } });
  api.runtime.getCommitment = async () => structuredClone(commitmentDetail);
  api.runtime.saveCommitment = async (_projectId, _id, request) => {
    check(request.expectedRevision === 1 && request.expectedCandidateRevisionId === "commitment-ui-1" && request.baseSnapshotId === "commitment-snapshot-1", "Workspace save did not freeze commitment CAS");
    await new Promise((resolve) => { finishEditorSave = resolve; });
    commitmentDetail = { ...commitmentDetail, payload: request.payload, revisionId: "commitment-ui-2", snapshotId: "commitment-snapshot-2" };
    return { revisionId: commitmentDetail.revisionId, snapshotId: commitmentDetail.snapshotId };
  };
  const editProps = { onDirtyChange: (value) => { dirtyEditor = value; }, onSaveHandlerChange: (handler) => { saveEditor = handler; }, onDiscardHandlerChange: (handler) => { discardEditor = handler; } };
  workTasks = [{ id: "extraction-ui", projectId: "ui-test", threadId: "thread-ui", goal: "提取本章伏笔", status: "completed", deliveryStatus: "ready_for_review", deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], ...emptyDeliveryReport, evidence: [{ stepId: "extraction-step", attemptId: "extraction-attempt", kind: "commitmentExtraction", state: "recorded", referenceId: "report-ui-1" }] }, resumeBlockReason: null, resumeMode: null, version: 1,
    steps: [{ id: "extraction-step", currentAttemptId: "extraction-attempt", operation: "extractCommitments", status: "completed", target: { chapterId: chapter.id, chapterNumber: 1, sourceRevisionId: "draft-1" }, attempts: [{ id: "extraction-attempt", status: "completed" }], changes: [], outputRefs: [], readingProgress: null }] }];
  await render(editProps);
  check(taskConversationContainer.querySelector('[data-commitment-result="report-ui-1"]') && !taskConversationContainer.querySelector('.commitment-panel'), "Choices did not belong to the original task or global panel remained in conversation");
  check(document.querySelector('.chapter-commitment-management').hidden, "Task extraction automatically opened chapter management");
  await tick(); await tick();
  await click("调整后再决定"); await tick(); await tick();
  check(document.querySelector('.commitment-management-dialog[open]') && document.querySelector('.commitment-editor'), "Inline adjustment did not open the single chapter editor");
  check(document.activeElement?.closest('.commitment-editor'), "Adjustment focus did not reach its editor");
  await input("承诺摘要", "导航保护中的作者输入"); await tick();
  check(dirtyEditor && document.querySelector('textarea[aria-label="场景 1 正文"]')?.disabled, "Commitment editor escaped global protection or allowed concurrent draft edits");
  const editDialog = document.querySelector('.commitment-management-dialog');
  editDialog.dispatchEvent(new Event('cancel', { cancelable: true })); await tick();
  check(editDialog.open && button("关闭伏笔调整").disabled, "Dirty adjustment allowed Escape or close to discard input");
  workspace.modeling = [{ objectId: "draft-chapter-1", status: "model_pending", sourceRevisionId: "draft-1" }];
  await render({ ...editProps, refreshToken: 700 });
  check(document.querySelector('[aria-label="承诺摘要"]').value === "导航保护中的作者输入" && dirtyEditor, "Workspace refresh unmounted dirty commitment editor");
  check(!button("继续分析本章"), "Optional knowledge analysis exposed an unsolicited manual action");
  const savingEditor = saveEditor(); await tick();
  check(finishEditorSave && discardEditor() === false && dirtyEditor, "In-flight save permitted discard and navigation");
  check(await saveEditor() === false, "In-flight save allowed a second navigation save");
  finishEditorSave();
  check(await savingEditor === true, "Global save failed to persist commitment input"); await tick(); await tick();
  check(!dirtyEditor && commitmentDetail.payload.summary === "导航保护中的作者输入", "Global save did not clear only persisted input");
  await input("承诺摘要", "将放弃的本地修改"); await tick();
  check(dirtyEditor && discardEditor() === true, "Global discard did not accept local-only abandonment"); await tick(); await tick();
  check(!dirtyEditor && !document.querySelector(".commitment-editor") && commitmentDetail.payload.summary === "导航保护中的作者输入", "Discard changed persisted commitment or left stale protection");
  await click("关闭伏笔调整");
  check(!document.querySelector(".commitment-management-dialog"), "Clean adjustment could not close");
  workspace.modeling = [];
  await render();
  check(!document.querySelector('.workspace-diagnostics') && diagnosticReads === 0, "Writing opened technical diagnostics");
  workTasks = []; await render({ refreshToken: "clear-extraction-history" });
  check(!document.querySelector('.work-task-panel'), "Idle writing displayed an empty task card");
  check(document.querySelector('.chapter-commitment-management').hidden, "Idle chapter showed unused extraction results");
  check(!button("查看伏笔与悬念") && !document.querySelector(".chapter-manual-tools,.writer-runtime-preview,.fact-proposal-form"), "Chapter retained duplicate analysis or technical tool controls");
  await render({ requestedCommitment: { requestId: "directory-ui-commitment", chapterId: chapter.id,
    request: { projectId: "ui-test", snapshotId: commitmentDetail.snapshotId, commitmentId: commitmentPayload.id, revisionId: commitmentDetail.revisionId } } });
  check(!document.querySelector('.chapter-commitment-management').hidden && document.querySelector('.creative-workspace-view .commitment-panel') && !taskConversationContainer.querySelector('.commitment-panel'), "Explicit chapter management became unreachable or returned to conversation bottom");
  showDiagnostics = true;
  await render();
  const diagnostics = document.querySelector('[aria-label="当前作品诊断"] .workspace-diagnostics');
  check(diagnostics && !diagnostics.open && diagnostics.querySelectorAll('.workspace-diagnostic-panels > details').length === 0 && diagnosticReads === 0, "Diagnostics were not grouped and collapsed by default");
  check(document.querySelector('textarea[aria-label="场景 1 正文"]').getBoundingClientRect().top < 550, "Diagnostics pushed the author editor off the first screen");
  window.uiReady = true; await new Promise((resolve) => setTimeout(resolve, 200));
  diagnostics.querySelector(':scope > summary').click(); await tick();
  for (const panel of diagnostics.querySelectorAll('.workspace-diagnostic-panels > details')) panel.querySelector('summary').click();
  await tick();
  await click("开始全量校验");
  check(document.body.textContent.includes("0 / 12 个修订") && document.body.textContent.includes("进行中"), "Search audit did not show honest in-progress coverage");
  await click("暂停校验");
  check(document.body.textContent.includes("已暂停"), "Search audit pause did not update UI");
  await click("继续校验");
  await click("取消校验");
  check(document.body.textContent.includes("开始全量校验"), "Cancelled audit cannot be restarted");
  auditJob = null;
  await click("开始人物索引校验");
  check(document.body.textContent.includes("0 / 5 个建模批次"), "Entity audit did not show bounded progress");
  await click("暂停人物校验");
  check(document.body.textContent.includes("已暂停"), "Entity audit pause did not update UI");
  await click("继续人物校验");
  await click("取消人物校验");
  entityAuditJob = null;
  await click("开始关系索引校验");
  check(document.body.textContent.includes("0 / 7 个建模批次"), "Relation audit did not show bounded progress");
  check(!document.body.textContent.includes("关系索引校验一致"), "Incomplete relation audit looked verified");
  await click("暂停关系校验");
  check(document.body.textContent.includes("关系索引校验已暂停"), "Relation audit pause did not update UI");
  await click("继续关系校验");
  await click("取消关系校验");
  relationAuditJob = null;
  await click("开始断言与认知索引校验");
  check(document.body.textContent.includes("0 / 9 个建模批次"), "Knowledge audit did not show bounded progress");
  check(!document.body.textContent.includes("断言与认知索引校验一致"), "Incomplete knowledge audit looked verified");
  await click("暂停断言与认知索引校验");
  check(document.body.textContent.includes("断言与认知索引校验已暂停"), "Knowledge audit pause did not update UI");
  await click("继续断言与认知索引校验");
  await click("取消断言与认知索引校验");
  knowledgeAuditJob = { ...knowledgeAuditJob, status: "completed", verifiedConsistent: false,
    audit: { ...knowledgeAuditJob.audit, entities: { ...knowledgeAuditJob.audit.entities, missingClaims: 2, missingNames: 3 }, heads: { ...knowledgeAuditJob.audit.heads, missingHeads: 1, affectedObjectIds: ["chapter-1"] } } };
  await render({ refreshToken: 8 });
  check(document.body.textContent.includes("当前知识头缺失 1 个"), "Missing knowledge head was hidden from author");
  check(document.body.textContent.includes("实体索引缺失 2 条") && document.body.textContent.includes("名称索引缺失 3 条"), "Unified source audit hid entity index gaps");
  check(document.body.textContent.includes("断言与认知索引校验发现不一致"), "Missing knowledge head looked verified");
  knowledgeAuditJob = null;
  await click("开始历史快照知识校验");
  check(document.body.textContent.includes("0 / 13 个快照成员"), "Historical knowledge audit did not show bounded progress");
  check(!document.body.textContent.includes("历史快照知识校验一致"), "Incomplete historical audit looked verified");
  await click("暂停历史快照知识校验");
  check(document.body.textContent.includes("历史快照知识校验已暂停"), "Historical knowledge audit pause did not update UI");
  await click("继续历史快照知识校验");
  await click("取消历史快照知识校验");
  snapshotKnowledgeAuditJob = { ...snapshotKnowledgeAuditJob, status: "completed", verifiedConsistent: false,
    audit: { ...snapshotKnowledgeAuditJob.audit, members: 13, missingCaptures: 1, affectedMembers: ["snapshot-1/draft-1"] } };
  await render({ refreshToken: 9 });
  check(document.body.textContent.includes("历史知识捕获缺失 1 处"), "Missing historical capture was hidden from author");
  check(document.body.textContent.includes("历史快照知识校验发现不一致"), "Missing historical capture looked verified");
  snapshotKnowledgeAuditJob = null;
  await click("开始确认事实链校验");
  check(document.body.textContent.includes("0 / 11 个确认事实"), "Fact audit did not show bounded progress");
  check(!document.body.textContent.includes("确认事实链校验一致"), "Incomplete fact audit looked verified");
  await click("暂停确认事实链校验");
  check(document.body.textContent.includes("确认事实链校验已暂停"), "Fact audit pause did not update UI");
  await click("继续确认事实链校验");
  await click("取消确认事实链校验");
  factAuditJob = { ...factAuditJob, status: "completed", verifiedConsistent: false,
    audit: { ...factAuditJob.audit, facts: 11, invalidLinks: 1, affectedFacts: ["fact-1"] } };
  await render({ refreshToken: 90 });
  check(document.body.textContent.includes("确认事实关联错误 1 条"), "Broken fact link was hidden from author");
  check(document.body.textContent.includes("确认事实链校验发现不一致"), "Broken fact link looked verified");
  factAuditJob = null;
  showDiagnostics = false;
  workTasks = [{ id: "finished-ui-task", goal: "已保存本章正文", status: "completed", deliveryStatus: "ready_for_review", deliveryReport: emptyDeliveryReport, resumeBlockReason: null, resumeMode: null, steps: [], version: 1 }];
  await render({ refreshToken: 91 });
  const idleHistory = document.querySelector('.work-task-panel');
  check(idleHistory && idleHistory.tagName === "SECTION" && idleHistory.querySelector(".user-message").textContent.includes("已保存本章正文") && !idleHistory.querySelector(".work-task-execution"), "Finished work did not become a readable conversation turn");
  check(taskConversationContainer.contains(idleHistory) && !document.querySelector(".creative-workspace-view .work-task-panel"), "Finished work was not moved into the conversation");
  const chromeSnapshot = { history: "作者要求 → 执行过程 → 保存的回复", placement: "右侧对话时间线", processCollapsed: !idleHistory.querySelector(".work-task-execution"), writingAuditReads: 0, diagnosticsEntry: "用户设置 · 当前作品诊断" };
  check(document.body.innerText.includes("已保存本章正文") && !idleHistory.querySelector('.work-task-history'), "A single record required redundant nested history expansion");
  workTasks[0] = { ...workTasks[0], deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], ...emptyDeliveryReport, pendingActionIds: ["requires-author-confirmation"] } };
  await render({ refreshToken: 92 });
  const attentionPanel = document.querySelector('.work-task-panel');
  check(attentionPanel?.innerText.includes("行动提案待确认") && !attentionPanel.querySelector(".work-task-execution"), "Finished execution hid author decisions still needed");
  check(taskConversationContainer.contains(attentionPanel) && taskConversationContainer.innerText.includes("行动提案待确认") && !document.querySelector(".creative-workspace-view .work-task-panel"), "Pending author action was not visible in the conversation");
  chromeSnapshot.attention = "待确认事项直接可见";
  workTasks = [];
  pagedWorkTasks = {
    first: { tasks: [{ id: "work-paused", goal: "旧的暂停任务", status: "paused", deliveryStatus: "pending", deliveryReport: emptyDeliveryReport, resumeBlockReason: null, resumeMode: "continueExecution", steps: [], version: 1 }], nextCursor: "0:1:2:3", activeCount: 1 },
    second: { tasks: [{ id: "work-history", goal: "历史任务", status: "completed", deliveryStatus: "pending", deliveryReport: emptyDeliveryReport, resumeBlockReason: null, resumeMode: null, steps: [], version: 2 }], nextCursor: null, activeCount: 1 },
  };
  await render({ refreshToken: 10 });
  check(!button("继续任务") && document.body.textContent.includes("旧的暂停任务"), "Paged task list hid the older actionable task or active count");
  check(taskConversationContainer.innerText.includes("已暂停"), "Paused task controls were hidden in conversation");
  await click("加载更早对话");
  await tick();
  check(document.body.innerText.includes("历史任务"), "Expanded history hid its result");
  check(document.body.textContent.includes("历史任务") && !button("加载更早对话"), "Task list did not append the next page");
  pagedWorkTasks.first = { tasks: [{ id: "work-new", goal: "状态变更后的任务", status: "running", deliveryStatus: "pending", deliveryReport: emptyDeliveryReport, resumeBlockReason: null, resumeMode: null, steps: [], version: 1 }], nextCursor: "0:2:2:4", activeCount: 1 };
  staleWorkTaskPage = true;
  await render({ refreshToken: 101 });
  pagedWorkTasks.first = { tasks: [{ id: "work-refreshed", goal: "刷新后的任务", status: "running", deliveryStatus: "pending", deliveryReport: emptyDeliveryReport, resumeBlockReason: null, resumeMode: null, steps: [], version: 1 }], nextCursor: null, activeCount: 1 };
  await click("加载更早对话");
  check(document.body.textContent.includes("刷新后的任务") && !document.body.textContent.includes("状态变更后的任务") && !button("加载更早对话"), "Stale task cursor did not refresh the first page");
  staleWorkTaskPage = false;
  pagedWorkTasks = null;
  workTasks = [{ id: "work-ui", projectId: "ui-test", threadId: "thread-ui", goal: "续写白塔", kind: "direct", status: "blocked", deliveryStatus: "partial", deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [],
    evidence: [{ stepId: "step-ui", attemptId: "attempt-ui", kind: "draft", referenceId: "draft-1", state: "recorded" }], pendingActionIds: [], unsettledAttemptIds: [] }, resumeBlockReason: "partialOutput", resumeMode: "continueExecution", version: 3, error: null,
    steps: [{ id: "step-ui", operation: "generateChapter", expectedOutputKind: "draft", instruction: null, sourceTaskId: null, target: {}, status: "blocked", inputRevision: 1,
      dependsOnStepId: null, currentAttemptId: "attempt-ui", readingProgress: null, attempts: [{ id: "attempt-ui", threadId: "thread-ui", turnId: "turn-ui", status: "failed", error: "建模中断" }],
      outputRefs: [{ kind: "draft", objectId: `draft-${chapter.id}`, revisionId: "draft-1" }], changes: [] }] }];
  await render({ refreshToken: 11 });
  check(document.body.textContent.includes("已保存部分内容") && document.body.textContent.includes("建模中断"), "Blocked task hid its partial result or failure reason");
  check(taskConversationContainer.textContent.includes("建模中断"), "Blocked task failure should remain visible");
  check(!button("继续任务") && document.body.textContent.includes("请先查看成果，避免重复生成"), "Partial write offered an unsafe resume action");
  check(!document.querySelector(".work-reading-progress"), "Absent checkpoint invented reading progress");
  const readingTask = workTasks[0];
  for (const status of ["paused", "blocked", "completed", "cancelled"]) {
    workTasks = [{ ...readingTask, status }];
    await render({ refreshToken: `single-flow-${status}` });
    check(!document.querySelector(".work-task-execution, .work-task-controls, .work-reading-progress"), "Task bookkeeping returned to the conversation");
  }
  workTasks = [{ ...readingTask, status: "running" }];
  await render({ refreshToken: "live-reading", agentRunning: true });
  const listWorkTasks = api.runtime.listWorkTasks;
  let finishList, finishNextProject, listCalls = 0;
  api.runtime.listWorkTasks = async (projectId) => {
    if (projectId !== "ui-test") return new Promise((resolve) => { finishNextProject = resolve; });
    listCalls += 1;
    return new Promise((resolve) => { finishList = resolve; });
  };
  await render({ refreshToken: "held-reading", agentRunning: true });
  await new Promise((resolve) => setTimeout(resolve, 1600));
  check(listCalls === 1, "Progress polling overlapped an in-flight request");
  await render({ projectId: "another-project", agentRunning: false, refreshToken: "switch-reading" });
  check(!document.querySelector(".work-task-row") && !document.querySelector(".work-task-panel"), "Project switch retained old tasks or displayed an empty task card");
  finishList({ tasks: structuredClone(workTasks), nextCursor: null, activeCount: 1 });
  await tick(); await tick();
  check(!document.querySelector(".work-task-row"), "Late progress response leaked into a different project");
  finishNextProject({ tasks: [], nextCursor: null, activeCount: 0 });
  await tick();
  api.runtime.listWorkTasks = listWorkTasks;
  workTasks = [readingTask];
  workTasks = [{ ...workTasks[0], steps: [{ ...workTasks[0].steps[0], outputRefs: [] }], deliveryStatus: "pending", resumeBlockReason: null }];
  await render({ refreshToken: 12 });
  workTasks = [{ ...workTasks[0], steps: [{ ...workTasks[0].steps[0], inputRevision: 0 }], resumeBlockReason: "inputChanged" }];
  await render({ refreshToken: 13 });
  check(!button("继续任务") && document.body.textContent.includes("作品版本已变化"), "Stale task input offered an unsafe resume action");
  workTasks = [{ ...workTasks[0], goal: "自动分析章节知识", resumeBlockReason: "attemptBudget" }];
  await render({ refreshToken: 131 });
  check(!button("继续任务") && document.body.textContent.includes("已停止重复尝试") && !button("取消任务")?.disabled, "Automatic modeling budget did not block retries while preserving cancellation");
  // Explicit retry/stop races now run through the actual App composer fixture.
  workTasks = [];
  await render({ refreshToken: 0 });
  check(!button("让 AI 起草本章") && !button("讨论本章") && !button("新建章节"), "Saved chapter retained duplicate creation/writer/discussion entries");
  await revealControl(document.querySelector(".writer-runtime-preview"));
  workspace.modeling = [{ objectId: "draft-chapter-1", sourceRevisionId: "draft-1", status: "model_pending", batchId: null, entities: [], assertions: [], confirmationState: "candidate" }];
  await render({ refreshToken: 1 });
  check(!button("继续分析本章") && !document.querySelector(".chapter-analysis-status"), "Pending knowledge became an author todo without a running task");
  workspace.modeling = [{ objectId: "draft-chapter-1", sourceRevisionId: "draft-1", status: "ready", batchId: "modeling-1", entities: [], assertions: [], confirmationState: "candidate" }];
  await render({ refreshToken: 2 });
  const knowledgeDetails = [...document.querySelectorAll("details")].find(item => item.querySelector("summary")?.textContent === "故事设定变化");
  check(knowledgeDetails, "Published chapter has no explicit knowledge update entry");
  knowledgeDetails.open = true;
  await click("更新故事设定");
  check(reviewedModelingBatch === "modeling-1" && document.body.textContent.includes("hidLetter") && document.querySelector(".fact-review-card"), "Source-bound knowledge candidates did not reach the author review panel");
  check(document.querySelector(".state-property-details")?.textContent.includes("开始时间未知") && document.querySelector(".state-property-details")?.textContent.includes("不等于确认全局世界规则"), "State claim review hid unknown time or granted schema authority");
  const decision = document.querySelector('select[aria-label="hidLetter 的确认结论"]');
  check(decision, "Knowledge candidate has no explicit author decision control");
  decision.value = "confirmed"; decision.dispatchEvent(new Event("change", { bubbles: true })); await tick();
  const eventCard = [...document.querySelectorAll(".fact-review-card")].find((card) => card.textContent.includes("hideLetterEvent"));
  check(eventCard?.textContent.includes("行动者：linxi") && eventCard.textContent.includes("效果主张：本批次 #1") && eventCard.textContent.includes("确认事件不自动确认关联主张"), "Event review lost participants, source-bound references or independent confirmation");
  check(eventCard.textContent.includes("前提时序：批次 prior-batch #3 区间在事件前适用") && eventCard.textContent.includes("#1 时间未知") && eventCard.textContent.includes("不证明前提真实或因果成立"), "Event temporal assessment lost source identity, uncertainty or claimed causality");
  check(eventCard.textContent.includes("规则条件：来源冲突") && eventCard.textContent.includes("已校验索引筛选，读取 2/2 个待检查批次，范围共 213 批") && eventCard.textContent.includes("批次 batch-counter #1") && eventCard.textContent.includes("不认证世界真相"), "Event conditions lost counterevidence, coverage or candidate-only authority");
  check(!eventCard.textContent.includes("值：") && !eventCard.textContent.includes("认知状态："), "Event review leaked generic value or epistemic controls");
  const eventDecision = eventCard.querySelector('select[aria-label="hideLetterEvent 的确认结论"]');
  check(eventDecision.value === "", "Related event was implicitly confirmed");
  eventDecision.value = "rejected"; eventDecision.dispatchEvent(new Event("change", { bubbles: true })); await tick();
  await click("确认发布章节");
  check(selectedForAccept?.factDecisions?.["fact-modeling"] === "confirmed" && selectedForAccept.sceneGroupIds.join(",") === workspace.chapters[0].candidate.sceneGroups.map(group => group.id).join(","), "Publication did not carry the explicit fact decision and complete current chapter");
  check(selectedForAccept?.factDecisions?.["fact-event"] === "rejected", "Independent event decision was not preserved");
  selectedForAccept = undefined;
  workspace.modeling = []; workspace.reviews = []; workspace.factProposals = [];
  await render({ refreshToken: 2 });
  root.unmount(); root = createRoot(document.getElementById("root")); await render();
  await input("场景 1 正文", "修改后的 A"); await click("＋ 添加场景"); await input("场景 3 正文", "新增 C"); await click("＋ 添加场景"); await waitForFocus("场景 4 正文"); await click("删除场景 4"); await waitForFocus("场景 3 正文"); await click("上移场景 3"); await click("上移场景 2");
  await click("保存");
  check([...document.querySelectorAll('[data-scene-id] textarea, [data-scene-id] button')].every((element) => element.matches(":disabled")), "Save did not lock editing");
  check(saveResult.sceneGroups[1].id === "a" && saveResult.sceneGroups[1].sceneId === "original-a" && saveResult.sceneGroups[1].body === "修改后的 A", "Edit/insertion changed existing identity");
  check(Object.keys(saveResult.sceneGroups[1]).sort().join(",") === "body,id,locked,sceneId", "Save leaked read-only Runtime metadata");
  check(saveResult.sceneGroups[2].id === "b" && !["a", "b"].includes(saveResult.sceneGroups[0].id), "Scene move/new identity failed");
  releaseSave();
  await tick(); await tick();
  check(localStorage.getItem(`novel-codex:chapter-editor-draft:${JSON.stringify(["fixture-author", "ui-test", chapter.id])}`) === null, "Successful save retained a recovery draft");
  check(document.querySelector('[aria-label="场景 1 正文"]').value === saveResult.sceneGroups[0].body && document.body.textContent.includes("这一章已保存") && document.querySelector('[aria-label="章节发布状态"]')?.textContent === "有未发布修改", "Saved body or publication status did not reach the workspace");
  await render({ view: "brief" });
  check(!document.querySelector('[aria-label="规划生成意图"]') && !button("生成"), "Planning retained a second composer or direct generation");
  await click("开始规划");
  check(discussedPrompts.at(-1).includes("创作简报") && !requestedGenerationIntent, "Planning did not prepare the main conversation without executing");
  await render({ view: "outline" }); await click("开始规划");
  check(discussedPrompts.at(-1).includes("全书大纲"), "Outline preparation lost the current artifact context");
  const outlineRevision = { id: "outline-review-1", version: 1, source: "author", payload: { nodes: [{ id: "book", kind: "book", position: 0, title: "全书", purpose: "讲述选择", summary: "全书安排", dependencyKeys: [], certainty: "tentative", protection: "free" }, { id: "volume-1", parentId: "book", kind: "volume", position: 0, title: "第一卷", purpose: "建立冲突", summary: "卷安排", dependencyKeys: [], certainty: "tentative", protection: "free" }] } };
  workspace.artifacts.outline.candidate = outlineRevision;
  await render({ view: "outline", refreshToken: 1 });
  check(document.body.textContent.includes("指定规划检查视角") && document.body.textContent.includes("只生成意见，不修改或采用规划"), "Planning review entry or read-only boundary is missing");
  const reviewIntent = document.querySelector('.planning-review-intent textarea');
  await revealControl(reviewIntent); check(reviewIntent.checkVisibility(), "Planning review editor was not revealed before interaction");
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(reviewIntent, "检查第一卷人物选择是否推动主冲突");
  reviewIntent.dispatchEvent(new Event("input", { bubbles: true }));
  await tick();
  const roleChecks = [...document.querySelectorAll<HTMLInputElement>('.planning-review-depth input[type="checkbox"]')];
  check(roleChecks.length === 3 && roleChecks.filter(input => input.checked).length === 1, "Planning checks should start with one needed view instead of a fixed multi-view pipeline");
  await click("开始检查");
  check(planningReviewRequest.revisionId === "outline-review-1" && planningReviewRequest.targetNodeId === null && planningReviewRequest.roles.join(",") === "structure", "A single-view check lost its frozen target or was expanded to unwanted views");
  roleChecks[0].click(); await tick();
  check(button("开始检查").disabled, "Planning review allowed an empty selection");
  for (const input of roleChecks) { input.click(); await tick(); }
  await click("开始检查");
  check(planningReviewRequest.roles.join(",") === "structure,character,experience", "Explicitly selecting all views was not respected");
  await render();
  await input("场景 2 正文", "失败时保留的正文");
  failSave = true; await click("保存");
  check(document.querySelector('[aria-label="场景 2 正文"]').value === "失败时保留的正文", "Failed save discarded local prose");
  failSave = false; await click("保存");
  check(Object.keys(saveResult.sceneGroups[1]).sort().join(",") === "body,id,locked,sceneId", "Second save leaked metadata");
  await render({ view: "brief" });
  releaseSave(); await tick(); await tick();
  check(document.querySelector(".planning-workspace-header") && !document.querySelector('[data-scene-id]'), "Old save completion polluted another editor");
  check(!document.body.textContent.includes("这一章已保存。"), "Late save status crossed editor lifetimes");
  generationRuns = [{ id: "generation-ui", authorMessageId: "author-message-generation-ui", status: "running", stage: "coordinate", activeTaskId: "task-ui", currentChapterId: "chapter-1", currentChapterNumber: 1, pendingCurrentIntentGeneration: null, startChapterNumber: 1, completedChapters: 0, targetChapters: 3, blockedReason: null,
    maxTaskAttempts: 21, usedTaskAttempts: 3, providerBudget: { maxRequests: 208, maxTokens: 4_000_000, usedRequests: 3, usedTokens: 24_000, unknownTokens: 0, overrun: false, charges: [] } }];
  const activeGenerationStep = { id: "generation-writer-step", operation: "generateChapter", status: "running", currentAttemptId: "task-ui", expectedOutputKind: "draft", inputRevision: 1, readingProgress: null, target: { chapterId: "chapter-1", chapterNumber: 1 }, instruction: null, sourceTaskId: null, dependsOnStepId: null, outputRefs: [], changes: [], attempts: [{ id: "task-ui", attemptNumber: 1, startedAt: 1, completedAt: null, threadId: "generation-thread", turnId: null, status: "running", error: null }] };
  workTasks = [{ id: "work-generation-ui", projectId: "ui-test", kind: "generation", threadId: "generation-thread", goal: "续写三章", version: 1, status: "running", deliveryStatus: "pending", deliveryReport: emptyDeliveryReport, error: null, resumeMode: null, resumeBlockReason: null, steps: [activeGenerationStep] }];
  await render({ view: "brief", agentRunning: true, refreshToken: 100 });
  const futureIntentInput = document.querySelector("#future-intent");
  check(futureIntentInput && !futureIntentInput.matches(":disabled"), "Future chapter intent is unavailable while the first chapter is running");
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(futureIntentInput, "第二章调查白塔");
  futureIntentInput.dispatchEvent(new Event("input", { bubbles: true })); await tick();
  await click("应用于后续章节");
  check(savedFutureIntent?.runId === "generation-ui" && savedFutureIntent.instruction === "第二章调查白塔", "Future chapter direction was not submitted during the current draft");
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(futureIntentInput, "第一章补写白塔证据");
  futureIntentInput.dispatchEvent(new Event("input", { bubbles: true })); await tick();
  await click("当前章：本步结束后重写");
  check(savedCurrentRevision?.runId === "generation-ui" && savedCurrentRevision.instruction === "第一章补写白塔证据", "Current-chapter revision was not queued while the draft task was active");
  generationRuns = [{ ...generationRuns[0], pendingCurrentIntentGeneration: 2 }];
  await render({ view: "brief", agentRunning: true, refreshToken: 103 });
  check(button("本章调整已排队")?.matches(":disabled"), "The current chapter accepted a duplicate queued revision");
  generationRuns = [{ ...generationRuns[0], pendingCurrentIntentGeneration: null }];
  workTasks = [{ ...workTasks[0], steps: [{ ...activeGenerationStep, operation: "extractCommitments", expectedOutputKind: "commitments" }] }];
  await render({ view: "brief", agentRunning: true, refreshToken: 105 });
  check(document.body.textContent.includes("进行到 安排创作步骤") && button("当前章：本步结束后重写")?.matches(":disabled"), "Current-chapter revision must require an actual active writer, not an old run stage");
  check(!document.querySelector("#future-intent").matches(":disabled"), "Unrelated future direction was blocked by commitment extraction");
  generationRuns = [{ ...generationRuns[0], currentChapterId: null, currentChapterNumber: null }];
  await render({ view: "brief", agentRunning: true, refreshToken: 101 });
  check(document.querySelector("#future-intent").matches(":disabled"), "Active planning task incorrectly accepted a future-chapter change");
  generationRuns = [{ ...generationRuns[0], activeTaskId: null }];
  await render({ view: "brief", agentRunning: true, refreshToken: 102 });
  check(!document.querySelector("#future-intent").matches(":disabled"), "An idle coordination checkpoint must allow future direction within the original range");
  generationRuns = [{ ...generationRuns[0], startChapterNumber: 4, completedChapters: 3, targetChapters: 6, activeTaskId: "task-ui", stage: "coordinate" }];
  await render({ view: "brief", agentRunning: false, refreshToken: 104 });
  check(document.body.textContent.includes("自动创作：0/3 章"), "Continuation progress includes chapters from an earlier run");
  generationRuns = [{ ...generationRuns[0], id: "generation-feedback", status: "blocked", stage: "coordinate", activeTaskId: null, currentChapterId: "chapter-feedback", blockedReason: "scene_feedback_needs_author", feedbackRepairCount: 0, usedTaskAttempts: 7, maxTaskAttempts: 21 }];
  await render({ view: "brief", agentRunning: false, refreshToken: 106 });
  check(document.body.textContent.includes("第二场尚无可靠事件证据"), "Author feedback gate hid the assessment rationale");
  check(document.body.textContent.includes("承诺判断 · lamp · 证据不足（候选分析）"), "Commitment feedback is missing or presented as a formal decision");
  const commitmentDetails = [...document.querySelectorAll("details")].find((item) => item.querySelector("summary")?.textContent.includes("承诺判断"));
  commitmentDetails.open = true;
  check(commitmentDetails.textContent.includes("来源正文已改变") && commitmentDetails.textContent.includes("lamp-1") && commitmentDetails.textContent.includes("没有可定位的兑现事件证据"), "Frozen reference and evidence state are missing from author feedback");
  await input("本章重写补充要求", "让张衡的行动有证据");
  await click("重写本章");
  check(feedbackChoice?.runId === "generation-feedback" && feedbackChoice.revisionId === "assessment-ui" && feedbackChoice.choice === "repairDraft" && feedbackChoice.instruction === "让张衡的行动有证据", "Feedback repair did not bind author intent and assessment revision");
  await click("接受现稿并继续");
  check(feedbackChoice?.choice === "continue" && feedbackChoice.instruction === "", "Author acceptance did not remain separate from repair instructions");
  generationRuns = [{ ...generationRuns[0], feedbackRepairCount: 2 }];
  await render({ view: "brief", agentRunning: false, refreshToken: 107 });
  check(!button("重写本章")?.matches(":disabled"), "A removed fixed two-repair limit must not override the original remaining task budget");
  generationRuns = [{ ...generationRuns[0], maxTaskAttempts: generationRuns[0].usedTaskAttempts }];
  await render({ view: "brief", agentRunning: false, refreshToken: "actual-attempt-limit" });
  check(button("重写本章")?.matches(":disabled") && document.body.textContent.includes("任务尝试或模型额度已用尽"), "The actual finite attempt limit was not shown before offering repair");
  generationRuns = [{ ...generationRuns[0], maxTaskAttempts: 21, blockedReason: "agent_task_failed_after_output", feedbackRepairPrompt: "重写第二场", feedbackRepairCount: 1 }];
  await render({ view: "brief", agentRunning: false, refreshToken: 108 });
  await click("按评估要求重试本章");
  check(retriedRun === "generation-feedback", "A partial feedback repair did not offer the safe retry entry");
  generationRuns = [{ ...generationRuns[0], usageCheck: { state: "unavailable", phase: "usageSettlement", code: null } }];
  await render({ view: "brief", agentRunning: false, refreshToken: "usage-unsettled" });
  check(document.body.textContent.includes("原任务费用登记尚未完成") && button("按评估要求重试本章")?.disabled, "Unsettled provider usage still offered a retry");
  generationRuns = [{ ...generationRuns[0], usageCheck: { state: "current" } }];
  await click("重新核对状态"); await tick(); await tick();
  check(!button("按评估要求重试本章")?.disabled, "Reconciled usage did not restore the author retry entry");
  generationRuns = [{ ...generationRuns[0], id: "generation-checkpoint", pendingCurrentIntentGeneration: null, feedbackRepairPrompt: null }];
  await render({ view: "brief", agentRunning: false, refreshToken: "checkpoint-retry" });
  const retrySnapshot = { label: button("从已保存候选继续")?.textContent, disabled: button("从已保存候选继续")?.disabled };
  await click("从已保存候选继续");
  check(retriedRun === "generation-checkpoint", "Ordinary partial output could not resume through existing retry");
  await render({ view: "brief", agentRunning: true, refreshToken: "checkpoint-running" });
  check(button("从已保存候选继续")?.disabled, "Checkpoint retry was offered during active execution");
  generationRuns = [];
  workTasks = [];
  await render({ view: "brief", agentRunning: false, refreshToken: 105 });
  workspace.artifacts.plan.candidate = { id: "new-ui-plan", payload: { chapters: [{ chapterPlanId: "plan-ui", chapterNumber: 1, publicTitle: "藏信", writerGuidance: "保留密函" }] } };
  workspace.chapterPlanBindings = [{ project_id: "ui-test", chapter_id: "chapter-1", chapter_plan_id: "plan-ui", plan_revision_id: "old-ui-plan", lineageStatus: "historical", writerProjection: { publicTitle: "焚信", writerGuidance: "烧毁密函" } }];
  await render({ view: "chapters", agentRunning: false, refreshToken: "rebind-ui" });
  await click("预览章纲调整");
  document.querySelector('.rebind-acknowledgment input').click(); await tick();
  await click("确认替换章纲关联");
  check(document.querySelector('[aria-label="场景 1 正文"]').disabled && button("保存").disabled, "Plan rebind did not freeze the entire editor");
  check(rebindRequest.previousPlanRevisionId === "old-ui-plan" && rebindRequest.planRevisionId === "new-ui-plan" && rebindRequest.sourceRevisionId === workspace.chapters[0].candidate.id, "Workspace submit changed the frozen source");
  releaseRebind(); await tick(); await tick(); await tick();
  check(document.body.textContent.includes("关联已成功，但刷新失败") && document.querySelector('.chapter-plan-rebind [role="status"]')?.textContent.includes("正文保留"), "A committed rebind was reported as failed when reload failed");
  check(!document.querySelector('[aria-label="场景 1 正文"]').disabled, "Editor remained locked after confirmed commit");
  check(!document.body.textContent.includes("正在更新章纲关联"), "Completed rebind retained an in-progress status");
  failRebindRefresh = false;
  const readsBeforeRecovery = workspaceReads;
  await click("重新读取作品");
  check(workspaceReads > readsBeforeRecovery && document.body.textContent.includes("作品已重新读取") && document.querySelector('.chapter-plan-rebind [role="status"]')?.textContent.includes(`作品版本 ${workspace.revision}`) && !document.querySelector('[aria-label="场景 1 正文"]').disabled, "Read-only recovery did not reload the committed version");
  const settings = document.querySelector<HTMLDetailsElement>('.chapter-settings');
  check(settings?.closest('.chapter-page-actions') && !document.querySelector('.fact-proposal-form,.writer-runtime-preview,.chapter-manual-tools') && !document.body.textContent.includes("记录本章新事实") && !document.body.textContent.includes("查看生成记录"), "Chapter settings kept technical author forms or failed to move into its heading");
  settings.querySelector('summary').click(); await tick();
  check(!settings.open && !document.querySelector('.chapter-review-dirty') && document.querySelector('.chapter-save-state')?.textContent === "正文已保存", "Closing chapter settings changed saved prose or introduced fake dirty state");
  chromeSnapshot.chapterControls = { body: document.querySelector('.chapter-save-state').textContent, settings: "页头 · 章节设置", technicalForms: false, manualToolbox: false };
  workspace = { ...workspace, revision: workspace.revision + 1, chapters: workspace.chapters.map(item => ({ ...item, candidate: structuredClone({ ...revision, id: "merge-base", chapterId: chapter.id, sceneGroups: scenes.map(scene => ({ ...scene, sceneId: scene.sceneId ?? scene.id })) }) })) };
  await render({ view: "chapters", agentRunning: false, refreshToken: "merge-base" });
  await input("场景 1 正文", "作者独立场景一");
  workspace = { ...workspace, revision: workspace.revision + 1, chapters: workspace.chapters.map(item => ({ ...item, candidate: { ...item.candidate, id: "merge-remote", sceneGroups: item.candidate.sceneGroups.map((scene, index) => ({ ...scene, id: scene.id + "-new", sceneId: scene.sceneId ?? scene.id, body: index === 1 ? "后台独立场景二" : scene.body })) } })) };
  await render({ view: "chapters", agentRunning: false, refreshToken: "merge-remote" });
  check(document.querySelector('[aria-label="场景 1 正文"]').value === "作者独立场景一" && document.querySelector('[aria-label="场景 2 正文"]').value === "后台独立场景二", "Independent chapter scenes did not merge");
  check(document.body.textContent.includes("当前正文仍未保存") && !document.querySelector('.editor-conflict'), "Chapter merge falsely saved or retained a conflict");
  const mergedScenes = workspace.chapters[0].candidate.sceneGroups;
  workspace = { ...workspace, revision: workspace.revision + 1, chapters: workspace.chapters.map(item => ({ ...item, candidate: { ...item.candidate, id: "structure-insert", sceneGroups: [...item.candidate.sceneGroups, { id: "inserted", sceneId: "inserted", body: "后台新增场景", locked: false }] } })) };
  await render({ view: "chapters", agentRunning: false, refreshToken: "structure-insert" });
  check(document.querySelector('[aria-label="场景 3 正文"]').value === "后台新增场景" && document.querySelector('[aria-label="场景 1 正文"]').value === "作者独立场景一" && !document.querySelector('.editor-conflict'), "Structural insertion lost independent local prose");
  workspace = { ...workspace, revision: workspace.revision + 1, chapters: workspace.chapters.map(item => ({ ...item, candidate: { ...item.candidate, id: "structure-reorder", sceneGroups: [...item.candidate.sceneGroups].reverse() } })) };
  await render({ view: "chapters", agentRunning: false, refreshToken: "structure-reorder" });
  check(document.querySelector('[aria-label="场景 3 正文"]').value === "作者独立场景一" && !document.querySelector('.editor-conflict'), "Scene reorder lost local prose identity");
  workspace = { ...workspace, revision: workspace.revision + 1, chapters: workspace.chapters.map(item => ({ ...item, candidate: { ...item.candidate, id: "merge-remote", sceneGroups: mergedScenes } })) };
  await render({ view: "chapters", agentRunning: false, refreshToken: "structure-delete" });
  check(!document.querySelector('[aria-label="场景 3 正文"]') && document.querySelector('[aria-label="场景 1 正文"]').value === "作者独立场景一" && !document.querySelector('.editor-conflict'), "Unedited scene deletion lost independent prose");
  failSave = true; await click("保存"); await tick();
  check(lastCheckpointExpected.expectedRevision === workspace.revision && lastCheckpointExpected.expectedCandidateRevisionId === "merge-remote", "Merged editor retained stale save baseline");
  check(saveResult.sceneGroups[0].body === "作者独立场景一" && saveResult.sceneGroups[1].body === "后台独立场景二" && saveResult.sceneGroups[0].id.endsWith("-new"), "Merged save payload lost a scene or latest group identity");
  failSave = false; releaseSave = null; await click("保存");
  check(releaseSave, "Concurrent chapter save was not held");
  const titleDuringSave = document.querySelector('[aria-label="章节标题"]').value;
  workspace = { ...workspace, revision: workspace.revision + 1, chapters: workspace.chapters.map(item => ({ ...item, candidate: { ...item.candidate, id: "merge-during-save", title: "后台独立标题" } })) };
  await render({ view: "chapters", agentRunning: false, refreshToken: "merge-during-save" });
  check(document.querySelector('[aria-label="章节标题"]').value === titleDuringSave, "Background merge mutated an in-flight save draft");
  failPendingSave = true; releaseSave(); await tick(); await tick();
  await render({ view: "chapters", agentRunning: false, refreshToken: "merge-after-save-failure" });
  check(document.querySelector('[aria-label="章节标题"]').value === "后台独立标题" && document.querySelector('[aria-label="场景 1 正文"]').value === "作者独立场景一", "Explicit refresh after failed save could not reconcile draft");
  workspace = { ...workspace, revision: workspace.revision + 1, chapters: workspace.chapters.map(item => ({ ...item, candidate: { ...item.candidate, id: "resolve-local", sceneGroups: item.candidate.sceneGroups.map((scene, index) => ({ ...scene, body: index === 0 ? "后台冲突正文" : scene.body })) } })) };
  await render({ view: "chapters", agentRunning: false, refreshToken: "resolve-local" });
  const beforeResolution = checkpointCalls;
  workspace = { ...workspace, revision: workspace.revision + 1, chapters: workspace.chapters.map(item => ({ ...item, candidate: { ...item.candidate, id: "resolve-local-newer" } })) };
  await click("冲突处保留我的修改"); await tick();
  check(document.querySelector('.editor-conflict') && document.body.textContent.includes("服务器版本再次变化") && checkpointCalls === beforeResolution, "Stale resolution overwrote a newer candidate");
  await click("冲突处保留我的修改"); await tick();
  check(!document.querySelector('.editor-conflict') && document.querySelector('[aria-label="场景 1 正文"]').value === "作者独立场景一" && checkpointCalls === beforeResolution, "Local conflict resolution saved or lost prose");
  workspace = { ...workspace, revision: workspace.revision + 1, chapters: workspace.chapters.map(item => ({ ...item, candidate: { ...item.candidate, id: "resolve-remote", sceneGroups: item.candidate.sceneGroups.map((scene, index) => ({ ...scene, body: index === 0 ? "选择服务器正文" : scene.body })) } })) };
  await render({ view: "chapters", agentRunning: false, refreshToken: "resolve-remote" });
  await click("冲突处使用服务器内容"); await tick();
  check(!document.querySelector('.editor-conflict') && document.querySelector('[aria-label="场景 1 正文"]').value === "选择服务器正文" && checkpointCalls === beforeResolution, "Remote conflict resolution saved or lost selected prose");
  await input("场景 1 正文", "作者独立场景一");
  workspace = { ...workspace, revision: workspace.revision + 1, chapters: workspace.chapters.map(item => ({ ...item, candidate: { ...item.candidate, id: "merge-source-changed", sourcePlanRefs: { localOnly: false, lineageStatus: "current", planRevisionId: "new-plan" }, sceneGroups: item.candidate.sceneGroups.map((scene, index) => ({ ...scene, body: index === 1 ? "来源改变后的场景二" : scene.body })) } })) };
  await render({ view: "chapters", agentRunning: false, refreshToken: "merge-source-changed" });
  check(document.querySelector('.editor-conflict') && document.querySelector('[aria-label="场景 2 正文"]').value === "后台独立场景二", "Changed plan source silently rebased local chapter");
  await input("场景 1 正文", "保留我的本地编辑");
  workspace = { ...workspace, revision: workspace.revision + 1, chapters: workspace.chapters.map(item => ({ ...item, candidate: { ...item.candidate, id: "late-conflict-candidate", sceneGroups: item.candidate.sceneGroups.map((scene, index) => ({ ...scene, body: index === 0 ? "冲突的后台场景一" : scene.body })) } })) };
  await render({ view: "chapters", agentRunning: false, refreshToken: "late-conflict" }); await tick();
  check(button("以最新版为基线保存本地稿"), "Conflict save action missing");
  const beforeLateSave = checkpointCalls;
  holdWorkspaceRead = true; await click("以最新版为基线保存本地稿");
  check(releaseWorkspaceRead, "Conflict baseline read was not held");
  releaseWorkspaceRead("foreign-project"); await tick(); await tick();
  check(checkpointCalls === beforeLateSave && document.body.textContent.includes("读取到的作品归属不一致"), "Foreign workspace became a save baseline");
  holdWorkspaceRead = true; releaseWorkspaceRead = null; await click("以最新版为基线保存本地稿");
  check(releaseWorkspaceRead, "Conflict retry baseline read was not held");
  root.unmount(); releaseWorkspaceRead(); await tick(); await tick();
  check(checkpointCalls === beforeLateSave, "Unmounted conflict editor initiated a late save");
  const recoveryKey = `novel-codex:chapter-editor-draft:${JSON.stringify(["fixture-author", "ui-test", chapter.id])}`;
  check(JSON.parse(localStorage.getItem(recoveryKey)).sceneGroups[0].body === "保留我的本地编辑", "Chapter draft was not persisted before unmount");
  root = createRoot(document.getElementById("root"));
  await render({ view: "chapters", agentRunning: false, refreshToken: "recover-chapter" });
  check(document.querySelector('[aria-label="场景 1 正文"]').value === "保留我的本地编辑" && document.querySelector('.editor-conflict'), "Reopen lost chapter draft or silently rebased its old candidate");
  check(checkpointCalls === beforeLateSave, "Chapter recovery submitted a save");
  const recoveryBeforeProjectSwitch = localStorage.getItem(recoveryKey);
  await render({ projectId: "another-project", view: "chapters", refreshToken: "foreign-project-read" });
  check(localStorage.getItem(recoveryKey) === recoveryBeforeProjectSwitch && localStorage.getItem(`novel-codex:chapter-editor-draft:${JSON.stringify(["fixture-author", "another-project", chapter.id])}`) === null, "Old project response wrote into another project's recovery scope");
  await render({ view: "chapters", refreshToken: "restore-original-project" });
  check(document.querySelector('[aria-label="场景 1 正文"]').value === "保留我的本地编辑", "Project round trip lost the original draft");
  await render({ authorKey: "another-author", view: "chapters", refreshToken: "another-author" });
  check(document.querySelector('[aria-label="场景 1 正文"]').value === "冲突的后台场景一", "Chapter recovery crossed author identity");
  await render({ view: "chapters", refreshToken: "original-author" });
  check(document.querySelector('[aria-label="场景 1 正文"]').value === "保留我的本地编辑", "Switching back lost the original author's draft");
  await click("放弃本地稿并载入服务器版本");
  check(localStorage.getItem(recoveryKey) === null, "Explicit discard left a restorable draft");
  await input("场景 1 正文", "原版本上的重开草稿");
  root.unmount(); root = createRoot(document.getElementById("root"));
  let discardRecovered;
  await render({ view: "chapters", refreshToken: "recover-same-revision", onDiscardHandlerChange: handler => { discardRecovered = handler; } });
  check(document.querySelector('[aria-label="场景 1 正文"]').value === "原版本上的重开草稿" && !document.querySelector('.editor-conflict'), "Same-version draft recovery invented a conflict");
  check(discardRecovered?.(), "Recovered chapter could not be explicitly discarded"); await tick();
  check(localStorage.getItem(recoveryKey) === null, "Discard handler did not clear recovery state");
  chapterHistory.set(workspace.chapters[0].candidate.id, structuredClone(workspace.chapters[0].candidate));
  await input("场景 1 正文", "历史核对保留的本地正文");
  root.unmount();
  workspace = { ...workspace, revision: workspace.revision + 1, chapters: workspace.chapters.map(item => ({ ...item, candidate: { ...item.candidate, id: "history-latest", sceneGroups: item.candidate.sceneGroups.map((scene, index) => ({ ...scene, body: index === 1 ? "重开期间后台修改" : scene.body })) } })) };
  historyOverride = "wrong-chapter";
  root = createRoot(document.getElementById("root"));
  await render({ view: "chapters", refreshToken: "history-wrong-owner" }); await tick();
  check(document.querySelector('.editor-conflict') && document.body.textContent.includes("历史章节归属不一致"), "Foreign chapter history was accepted");
  historyOverride = null;
  await click("重新核对历史版本"); await tick(); await tick();
  check(!document.querySelector('.editor-conflict') && document.querySelector('[aria-label="场景 1 正文"]').value === "历史核对保留的本地正文" && document.querySelector('[aria-label="场景 2 正文"]').value === "重开期间后台修改", "Historical baseline did not merge reopened draft");
  check(checkpointCalls === beforeLateSave, "Historical recovery unexpectedly saved");
  chapterHistory.set(workspace.chapters[0].candidate.id, structuredClone(workspace.chapters[0].candidate));
  root.unmount();
  workspace = { ...workspace, revision: workspace.revision + 1, chapters: workspace.chapters.map(item => ({ ...item, candidate: { ...item.candidate, id: "history-held", title: "后台新标题" } })) };
  holdHistory = true; root = createRoot(document.getElementById("root"));
  await render({ view: "chapters", refreshToken: "history-held" });
  check(releaseHistory, "History response was not held");
  root.unmount(); releaseHistory(); await tick();
  check(JSON.parse(localStorage.getItem(recoveryKey)).baseCandidateRevisionId === "history-latest" && checkpointCalls === beforeLateSave, "Late history mutated an unmounted draft");
  root = createRoot(document.getElementById("root"));
  root.unmount(); localStorage.setItem(recoveryKey, "{invalid"); root = createRoot(document.getElementById("root"));
  await render({ view: "chapters", refreshToken: "corrupt-recovery" });
  check(document.body.textContent.includes("本地章节草稿无法读取") && localStorage.getItem(recoveryKey) === "{invalid", "Corrupt recovery data was silently deleted or overwritten");
  root.unmount();
  return { passed: true, retrySnapshot, chromeSnapshot };
}
run().then((result) => { window.testResult = result; }).catch((error) => { window.testResult = { passed: false, error: (window.testResult?.error ?? error.stack) + "\nDOM:" + document.body.innerText }; });
