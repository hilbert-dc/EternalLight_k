import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { CreativeWorkspaceView } from "../../../src/CreativeWorkspaceView";
window.addEventListener("error", (event) => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
const tick = () => new Promise((resolve) => setTimeout(resolve, 30));
const check = (condition, message) => { if (!condition) throw new Error(message); };
const waitForFocus = async (label) => {
  const deadline = performance.now() + 2000;
  while (document.activeElement?.getAttribute("aria-label") !== label && performance.now() < deadline) await tick();
  check(document.activeElement?.getAttribute("aria-label") === label, `Focus did not reach ${label}`);
};
const button = (text) => [...document.querySelectorAll("button")].find((item) => item.textContent === text);
const click = async (text) => { const element = button(text); check(element && !element.matches(":disabled"), `Unavailable: ${text}`); element.click(); await tick(); };
const input = async (label, value) => {
  const element = document.querySelector(`textarea[aria-label="${label}"]`);
  check(element && !element.matches(":disabled"), `Unavailable: ${label}`);
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(element, value);
  element.dispatchEvent(new Event("input", { bubbles: true })); await tick();
};
const chapter = { id: "chapter-1", number: 1, title: "第一章", passages: [] };
const scenes = [{ id: "a", sceneId: "original-a", body: "原场景 A", locked: false, baseAcceptedRevisionId: "accepted-1", candidateDraftRevisionId: "draft-1" }, { id: "b", sceneId: "original-b", body: "原场景 B", locked: false }];
const revision = { id: "draft-1", version: 1, body: "原场景 A\n\n---\n\n原场景 B", sceneGroups: scenes };
let workspace = { project: { id: "ui-test" }, revision: 1, headSnapshotId: "snapshot-ui-1", sourceDigest: "a".repeat(64), index: { status: "ready" }, artifacts: Object.fromEntries(["brief", "outline", "plan"].map((kind) => [kind, { candidate: null, accepted: null, working: null, locked: false, workingSelection: null }])), chapters: [{ ...chapter, candidate: revision, accepted: revision, working: null, workingSelection: null }], workingBaselines: [], continuationIntents: [], planningReviews: [], commits: [], tasks: [], impacts: [], modeling: [], factProposals: [], reviews: [], chapterPlanBindings: [] };
let releaseSave, saveResult, selectedForAccept, planningReviewRequest, reviewedModelingBatch, resumedChapter, resumedWorkTask, requestedGenerationIntent, feedbackChoice, retriedRun, releaseRebind, rebindRequest;
let failRebindRefresh = false;
let workTasks = [];
const emptyDeliveryReport = { evidence: [], pendingActionIds: [], unsettledAttemptIds: [] };
let pagedWorkTasks = null;
let staleWorkTaskPage = false;
let auditJob = null;
let entityAuditJob = null;
let relationAuditJob = null;
let knowledgeAuditJob = null;
let snapshotKnowledgeAuditJob = null;
let factAuditJob = null;
let generationRuns = [];
let failGenerationList = false;
let savedFutureIntent;
let savedCurrentRevision;
let failSave = false;
const api = { runtime: {
  getCreativeWorkspace: async () => { if (failRebindRefresh) throw new Error("刷新连接中断"); return structuredClone(workspace); },
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
  getSearchAudit: async () => ({ job: structuredClone(auditJob) }),
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
  getWriterPreview: async (projectId, projectRevision, readingChapter, sceneOrder, povCharacterId, storyTime) => ({ id: "context-ui", purpose: "writer", taskId: null, snapshotId: "snapshot-ui", contentHash: "preview-ui",
    target: { projectId, projectRevision, position: { readingChapter, sceneOrder, povCharacterId, storyTime } },
    plannedEffects: workspace.chapterPlanBindings?.map(binding => ({ source: "plan", value: { planRevisionId: binding.plan_revision_id, chapters: [{ ...binding.writerProjection, chapterPlanId: binding.chapter_plan_id }] } })) ?? [], relevantText: [{ objectId: "draft-previous", sourceRevisionId: "previous-source-1", authority: "candidate", content: { chapterId: "previous", chapterNumber: 0, title: "序章", body: "候选前文" } }],
    coverage: { complete: false, semanticCompleteness: "unknown", missingScope: ["knowledgeModeling"], truncationReasons: [], scannedScope: { factsIncluded: 0, draftsIncluded: 1 } },
    budgetsUsed: { byteLimit: 60000, byteSize: 1000, entryLimit: 24 } }),
  saveDraftCheckpoint: async (_, payload) => { saveResult = payload; if (failSave) throw new Error("磁盘写入失败"); await new Promise((resolve) => { releaseSave = resolve; }); workspace = { ...workspace, revision: 2, chapters: [{ ...workspace.chapters[0], candidate: { ...revision, ...payload, id: "draft-2" } }] }; return { revisionId: "draft-2" }; },
  createReview: async (_, kind, revisionId, proposals, __, modelingBatchId) => {
    if (modelingBatchId) {
      reviewedModelingBatch = modelingBatchId;
      workspace.reviews = [{ id: "review-modeling", artifactKind: "draft", artifactRevisionId: revisionId, projectRevision: workspace.revision, status: "ready", checks: [], selections: {} }];
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
  acceptReview: async (_, __, ___, selections) => { selectedForAccept = selections; },
}, agent: {
  addFutureIntent: async (_projectId, runId, instruction) => { savedFutureIntent = { runId, instruction }; return { intentGeneration: 1 }; },
  reviseCurrentChapter: async (_projectId, runId, instruction) => { savedCurrentRevision = { runId, instruction }; return { intentGeneration: 2, pendingCurrentIntentGeneration: 2 }; },
  resumeWorkTask: async (_projectId, taskId) => { resumedWorkTask = taskId; workTasks = []; },
  controlWorkTask: async () => {},
  interrupt: async () => {},
} };
const props = { api, projectId: "ui-test", view: "chapters", state: { novelEdition: { chapters: [chapter] }, canon: { characters: [] } }, selectedChapterId: chapter.id, agentAvailable: true, agentRunning: false, refreshToken: 0, onSelectChapter() {}, onGeneratePlanning() {}, async onGenerateWork(intent, targetChapters, expectedProjectRevision, expectedBaseSnapshotId, expectedSourceDigest) { requestedGenerationIntent = { intent, targetChapters, expectedProjectRevision, expectedBaseSnapshotId, expectedSourceDigest }; }, async onContinueWork() {}, async onRetryWork(runId) { retriedRun = runId; }, async onResolveFeedback(runId, revisionId, choice, instruction) { feedbackChoice = { runId, revisionId, choice, instruction }; }, async onReviewPlanning(request) { planningReviewRequest = request; }, async onCancelPlanningReview() {}, async onGenerateChapter() {}, async onResumeKnowledge(chapter) { resumedChapter = chapter.id; }, onOpenPlanningTarget() {}, onReturnToFeedback() {}, async onFormalChange() {}, onDirtyChange() {} };
let root = createRoot(document.getElementById("root"));
const render = async (extra = {}) => { root.render(<CreativeWorkspaceView {...props} {...extra} />); await tick(); await tick(); await tick(); };
async function run() {
  let saveEditor, discardEditor, dirtyEditor = false, finishEditorSave;
  const commitmentPayload = { id: "ui-commitment", kind: "suspense", summary: "白塔的秘密", fulfillmentCriteria: "正文解释秘密", relatedEntityIds: [], plotThreadId: null, characterArcId: null,
    setupEvidenceRefs: [{ objectId: "draft-chapter-1", revisionId: "draft-1", quote: "原场景 A", start: 0, end: 5 }], fulfillmentEvidenceRefs: [], status: "open", decisionReason: null, targetWindow: { kind: "open" } };
  let commitmentDetail = { projectId: "ui-test", objectId: "commitment-ui-commitment", revisionId: "commitment-ui-1", snapshotId: "commitment-snapshot-1", payload: commitmentPayload,
    extractionOrigin: { chapterId: "chapter-1", reportRevisionId: "report-ui-1", proposalId: "proposal-ui-1" }, sourceCurrent: true, confirmationState: "candidate", confirmationCommitId: null,
    authorControls: { projectRevision: 1, undoAvailable: false }, interpretation: "sourceBoundNarrativeClaim", literaryFulfillmentVerified: false };
  api.runtime.getCommitmentExtraction = async () => ({ revisionId: "report-ui-1", snapshotId: "snapshot-ui-1", projectRevision: 1, sourceCurrent: true,
    established: [{ proposalId: "proposal-ui-1", commitmentId: "ui-commitment" }], payload: { chapterId: "chapter-1", sourceRevisionId: "draft-1", examinedRanges: [{ start: 0, end: 5 }], candidates: [{ ...commitmentPayload, id: "proposal-ui-1" }] } });
  api.runtime.getCommitment = async () => structuredClone(commitmentDetail);
  api.runtime.saveCommitment = async (_projectId, _id, request) => {
    check(request.expectedRevision === 1 && request.expectedCandidateRevisionId === "commitment-ui-1" && request.baseSnapshotId === "commitment-snapshot-1", "Workspace save did not freeze commitment CAS");
    await new Promise((resolve) => { finishEditorSave = resolve; });
    commitmentDetail = { ...commitmentDetail, payload: request.payload, revisionId: "commitment-ui-2", snapshotId: "commitment-snapshot-2" };
    return { revisionId: commitmentDetail.revisionId, snapshotId: commitmentDetail.snapshotId };
  };
  const editProps = { onDirtyChange: (value) => { dirtyEditor = value; }, onSaveHandlerChange: (handler) => { saveEditor = handler; }, onDiscardHandlerChange: (handler) => { discardEditor = handler; } };
  await render(editProps);
  document.querySelector(".commitment-panel summary").click(); await tick(); await tick();
  await click("打开已建立承诺"); await tick();
  await click("调整这份承诺");
  await input("承诺摘要", "导航保护中的作者输入"); await tick();
  check(dirtyEditor && document.querySelector('textarea[aria-label="场景 1 正文"]')?.disabled, "Commitment editor escaped global protection or allowed concurrent draft edits");
  workspace.modeling = [{ objectId: "draft-chapter-1", status: "model_pending", sourceRevisionId: "draft-1" }];
  await render({ ...editProps, refreshToken: 700 });
  check(document.querySelector('[aria-label="承诺摘要"]').value === "导航保护中的作者输入" && dirtyEditor, "Workspace refresh unmounted dirty commitment editor");
  check(button("继续分析本章")?.disabled && !resumedChapter, "Knowledge resume bypassed the supplemental editor protection");
  const savingEditor = saveEditor(); await tick();
  check(finishEditorSave && discardEditor() === false && dirtyEditor, "In-flight save permitted discard and navigation");
  check(await saveEditor() === false, "In-flight save allowed a second navigation save");
  finishEditorSave();
  check(await savingEditor === true, "Global save failed to persist commitment input"); await tick(); await tick();
  check(!dirtyEditor && commitmentDetail.payload.summary === "导航保护中的作者输入", "Global save did not clear only persisted input");
  await input("承诺摘要", "将放弃的本地修改"); await tick();
  check(dirtyEditor && discardEditor() === true, "Global discard did not accept local-only abandonment"); await tick(); await tick();
  check(!dirtyEditor && !document.querySelector(".commitment-editor") && commitmentDetail.payload.summary === "导航保护中的作者输入", "Discard changed persisted commitment or left stale protection");
  document.querySelector(".commitment-panel summary").click(); await tick();
  workspace.modeling = [];
  await render();
  const diagnostics = document.querySelector('.workspace-diagnostics');
  check(diagnostics && !diagnostics.open && diagnostics.querySelectorAll('.workspace-diagnostic-panels > details').length === 6, "Diagnostics were not grouped and collapsed by default");
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
  check(!document.body.textContent.includes("关系索引校验 · 一致"), "Incomplete relation audit looked verified");
  await click("暂停关系校验");
  check(document.body.textContent.includes("关系索引校验 · 已暂停"), "Relation audit pause did not update UI");
  await click("继续关系校验");
  await click("取消关系校验");
  relationAuditJob = null;
  await click("开始断言与认知索引校验");
  check(document.body.textContent.includes("0 / 9 个建模批次"), "Knowledge audit did not show bounded progress");
  check(!document.body.textContent.includes("断言与认知索引校验 · 一致"), "Incomplete knowledge audit looked verified");
  await click("暂停断言与认知索引校验");
  check(document.body.textContent.includes("断言与认知索引校验 · 已暂停"), "Knowledge audit pause did not update UI");
  await click("继续断言与认知索引校验");
  await click("取消断言与认知索引校验");
  knowledgeAuditJob = { ...knowledgeAuditJob, status: "completed", verifiedConsistent: false,
    audit: { ...knowledgeAuditJob.audit, entities: { ...knowledgeAuditJob.audit.entities, missingClaims: 2, missingNames: 3 }, heads: { ...knowledgeAuditJob.audit.heads, missingHeads: 1, affectedObjectIds: ["chapter-1"] } } };
  await render({ refreshToken: 8 });
  check(document.body.textContent.includes("当前知识头缺失 1 个"), "Missing knowledge head was hidden from author");
  check(document.body.textContent.includes("实体索引缺失 2 条") && document.body.textContent.includes("名称索引缺失 3 条"), "Unified source audit hid entity index gaps");
  check(document.body.textContent.includes("断言与认知索引校验 · 发现不一致"), "Missing knowledge head looked verified");
  knowledgeAuditJob = null;
  await click("开始历史快照知识校验");
  check(document.body.textContent.includes("0 / 13 个快照成员"), "Historical knowledge audit did not show bounded progress");
  check(!document.body.textContent.includes("历史快照知识校验 · 一致"), "Incomplete historical audit looked verified");
  await click("暂停历史快照知识校验");
  check(document.body.textContent.includes("历史快照知识校验 · 已暂停"), "Historical knowledge audit pause did not update UI");
  await click("继续历史快照知识校验");
  await click("取消历史快照知识校验");
  snapshotKnowledgeAuditJob = { ...snapshotKnowledgeAuditJob, status: "completed", verifiedConsistent: false,
    audit: { ...snapshotKnowledgeAuditJob.audit, members: 13, missingCaptures: 1, affectedMembers: ["snapshot-1/draft-1"] } };
  await render({ refreshToken: 9 });
  check(document.body.textContent.includes("历史知识捕获缺失 1 处"), "Missing historical capture was hidden from author");
  check(document.body.textContent.includes("历史快照知识校验 · 发现不一致"), "Missing historical capture looked verified");
  snapshotKnowledgeAuditJob = null;
  await click("开始确认事实链校验");
  check(document.body.textContent.includes("0 / 11 个确认事实"), "Fact audit did not show bounded progress");
  check(!document.body.textContent.includes("确认事实链校验 · 一致"), "Incomplete fact audit looked verified");
  await click("暂停确认事实链校验");
  check(document.body.textContent.includes("确认事实链校验 · 已暂停"), "Fact audit pause did not update UI");
  await click("继续确认事实链校验");
  await click("取消确认事实链校验");
  factAuditJob = { ...factAuditJob, status: "completed", verifiedConsistent: false,
    audit: { ...factAuditJob.audit, facts: 11, invalidLinks: 1, affectedFacts: ["fact-1"] } };
  await render({ refreshToken: 90 });
  check(document.body.textContent.includes("确认事实关联错误 1 条"), "Broken fact link was hidden from author");
  check(document.body.textContent.includes("确认事实链校验 · 发现不一致"), "Broken fact link looked verified");
  factAuditJob = null;
  pagedWorkTasks = {
    first: { tasks: [{ id: "work-paused", goal: "旧的暂停任务", status: "paused", deliveryStatus: "pending", deliveryReport: emptyDeliveryReport, resumeBlockReason: null, steps: [], version: 1 }], nextCursor: "0:1:2:3", activeCount: 1 },
    second: { tasks: [{ id: "work-history", goal: "历史任务", status: "completed", deliveryStatus: "pending", deliveryReport: emptyDeliveryReport, resumeBlockReason: null, steps: [], version: 2 }], nextCursor: null, activeCount: 1 },
  };
  await render({ refreshToken: 10 });
  check(document.body.textContent.includes("1 项执行未结束") && document.body.textContent.includes("旧的暂停任务"), "Paged task list hid the older actionable task or active count");
  check(document.querySelector(".work-task-panel")?.open, "Actionable task list should open when a paused task appears");
  await click("加载更多任务");
  check(document.body.textContent.includes("历史任务") && !button("加载更多任务"), "Task list did not append the next page");
  pagedWorkTasks.first = { tasks: [{ id: "work-new", goal: "状态变更后的任务", status: "running", deliveryStatus: "pending", deliveryReport: emptyDeliveryReport, resumeBlockReason: null, steps: [], version: 1 }], nextCursor: "0:2:2:4", activeCount: 1 };
  staleWorkTaskPage = true;
  await render({ refreshToken: 101 });
  pagedWorkTasks.first = { tasks: [{ id: "work-refreshed", goal: "刷新后的任务", status: "running", deliveryStatus: "pending", deliveryReport: emptyDeliveryReport, resumeBlockReason: null, steps: [], version: 1 }], nextCursor: null, activeCount: 1 };
  await click("加载更多任务");
  check(document.body.textContent.includes("刷新后的任务") && !document.body.textContent.includes("状态变更后的任务") && !button("加载更多任务"), "Stale task cursor did not refresh the first page");
  staleWorkTaskPage = false;
  pagedWorkTasks = null;
  workTasks = [{ id: "work-ui", projectId: "ui-test", threadId: "thread-ui", goal: "续写白塔", kind: "direct", status: "blocked", deliveryStatus: "partial", deliveryReport: {
    evidence: [{ stepId: "step-ui", attemptId: "attempt-ui", kind: "draft", referenceId: "draft-1", state: "recorded" }], pendingActionIds: [], unsettledAttemptIds: [] }, resumeBlockReason: "partialOutput", version: 3, error: null,
    steps: [{ id: "step-ui", operation: "generateChapter", expectedOutputKind: "draft", instruction: null, sourceTaskId: null, target: {}, status: "blocked", inputRevision: 1,
      dependsOnStepId: null, currentAttemptId: "attempt-ui", readingProgress: null, attempts: [{ id: "attempt-ui", threadId: "thread-ui", turnId: "turn-ui", status: "failed", error: "建模中断" }],
      outputRefs: [{ kind: "draft", revisionId: "draft-1" }], changes: [] }] }];
  await render({ refreshToken: 11 });
  check(document.body.textContent.includes("已保存部分产物") && document.body.textContent.includes("建模中断"), "Blocked task hid its partial result or failure reason");
  check(document.querySelector(".work-task-panel")?.open, "Blocked task list should remain visible");
  check(button("继续任务")?.matches(":disabled") && document.body.textContent.includes("先核对产物"), "Partial write offered an unsafe resume action");
  check(!document.querySelector(".work-reading-progress"), "Absent checkpoint invented reading progress");
  const reading = { status: "current", version: 2, pages: 2, acknowledgedCharacters: 240, traversal: "reading" };
  const readingTask = workTasks[0];
  workTasks = [{ ...readingTask, resumeBlockReason: "pendingRecovery" }];
  await render({ refreshToken: "pending-recovery" });
  check(button("继续任务")?.disabled && document.body.textContent.includes("尚未核对的写入结果"), "Unknown write offered a duplicate execution or hid reconciliation guidance");
  for (const status of ["paused", "blocked", "completed", "cancelled"]) {
    workTasks = [{ ...readingTask, status, steps: [{ ...readingTask.steps[0], readingProgress: reading }] }];
    await render({ refreshToken: `reading-${status}` });
    const progress = document.querySelector(".work-reading-progress");
    check(progress?.textContent.includes("已确认 2 页原文 · 240 字符") && progress.textContent.includes("理解完整性尚未验证"), "Task state erased or overstated source traversal");
    if (status === "paused") {
      const panel = document.querySelector(".work-task-panel");
      panel.style.width = "320px";
      panel.scrollIntoView({ block: "start" });
      await tick();
      check(panel.scrollWidth <= panel.clientWidth + 1, "Narrow reading task panel overflowed horizontally");
      window.readingUiReady = true;
      await new Promise((resolve) => setTimeout(resolve, 150));
      window.readingUiReady = false;
      panel.style.width = "";
    }
    if (["completed", "cancelled"].includes(status)) check(!button("继续任务") && !button("取消任务"), "Historical progress re-enabled task controls");
    else check(button("继续任务")?.disabled, "Reading counts bypassed partial-output resume gate");
  }
  for (const traversal of ["acknowledgedEnd", "budgetExhausted"]) {
    workTasks = [{ ...readingTask, steps: [{ ...readingTask.steps[0], readingProgress: { ...reading, traversal } }] }];
    await render({ refreshToken: traversal });
    check(document.querySelector(".work-reading-progress")?.textContent.includes(traversal === "acknowledgedEnd"
      ? "不代表全书或内容理解完成" : "不会重置此预算"), "Traversal end or budget claimed semantic completion or reset");
  }
  for (const status of ["stale", "unavailable"]) {
    workTasks = [{ ...readingTask, steps: [{ ...readingTask.steps[0], readingProgress: { status } }] }];
    await render({ refreshToken: status });
    const progress = document.querySelector(".work-reading-progress");
    check(progress?.dataset.sourceState === status && !progress.textContent.includes("已确认"), "Invalid source exposed certified counts");
    check(!button("取消任务")?.disabled, "Invalid checkpoint disabled safe cancellation");
  }
  workTasks = [{ ...readingTask, status: "running", steps: [{ ...readingTask.steps[0], readingProgress: { ...reading, pages: 1 } }] }];
  await render({ refreshToken: "live-reading", agentRunning: true });
  workTasks[0].steps[0].readingProgress = reading;
  const deadline = performance.now() + 2300;
  while (!document.querySelector(".work-reading-progress")?.textContent.includes("已确认 2 页") && performance.now() < deadline) await tick();
  check(document.querySelector(".work-reading-progress")?.textContent.includes("已确认 2 页"), "Running task progress did not refresh without a new turn");
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
  check(!document.querySelector(".work-task-row") && document.querySelector(".work-task-panel summary")?.textContent.includes("读取中"), "Project switch retained old tasks or claimed an empty task list");
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
  check(button("继续任务")?.matches(":disabled") && document.body.textContent.includes("作品版本已变化"), "Stale task input offered an unsafe resume action");
  workTasks = [{ ...workTasks[0], goal: "自动分析章节知识", resumeBlockReason: "attemptBudget" }];
  await render({ refreshToken: 131 });
  check(button("继续任务")?.matches(":disabled") && document.body.textContent.includes("已用完三次尝试") && !button("取消任务")?.disabled, "Automatic modeling budget did not block retries while preserving cancellation");
  workTasks = [{ ...workTasks[0], steps: [{ ...workTasks[0].steps[0], inputRevision: 1 }], resumeBlockReason: null }];
  await render({ refreshToken: 14 });
  await click("继续任务");
  check(resumedWorkTask === "work-ui", "Task resume control did not use the persisted task ID");
  const resumeWorkTask = api.agent.resumeWorkTask;
  workTasks = [{ ...readingTask, resumeBlockReason: null }];
  let finishTaskControl, controlCalls = 0, lateFormalRefreshes = 0;
  api.agent.resumeWorkTask = async () => {
    controlCalls += 1;
    return new Promise((resolve) => { finishTaskControl = resolve; });
  };
  await render({ refreshToken: 15, onFormalChange: async () => { lateFormalRefreshes += 1; } });
  button("继续任务").click(); button("继续任务").click(); await tick();
  check(controlCalls === 1, "Same-frame resume created duplicate control requests");
  await render({ projectId: "control-project-B", refreshToken: 16 });
  finishTaskControl(); await tick(); await tick();
  check(lateFormalRefreshes === 0 && !button("继续任务")?.disabled, "Late control refreshed another project or left its UI busy");
  api.agent.resumeWorkTask = resumeWorkTask;
  const controlWorkTask = api.agent.controlWorkTask;
  let finishResume, rejectResume, finishInterruption, interruptionCalls = 0;
  api.agent.resumeWorkTask = async () => new Promise((resolve, reject) => { finishResume = resolve; rejectResume = reject; });
  api.agent.controlWorkTask = async (_projectId, _taskId, command) => {
    interruptionCalls += 1;
    await new Promise((resolve) => { finishInterruption = resolve; });
    workTasks = workTasks.map((task) => task.id === _taskId ? { ...task, status: command === "pause" ? "paused" : "cancelled" } : task);
  };
  let saveDuringResume, discardDuringResume;
  await render({ refreshToken: "resume-interrupt", onSaveHandlerChange: (handler) => { saveDuringResume = handler; },
    onDiscardHandlerChange: (handler) => { discardDuringResume = handler; } });
  button("继续任务").click();
  check(discardDuringResume() === false && await saveDuringResume() === false, "Same-frame resumed execution allowed a conflicting save or discard");
  await tick();
  workTasks = [{ ...workTasks[0], status: "running" }];
  await render({ refreshToken: "resume-running", agentRunning: true });
  check(!button("暂停后续步骤")?.disabled && !button("取消任务")?.disabled, "Waiting for resumed execution removed stop controls");
  button("暂停后续步骤").click(); button("暂停后续步骤").click(); await tick();
  check(interruptionCalls === 1 && button("取消任务")?.disabled, "Short control was duplicated or lacked mutual exclusion");
  finishInterruption(); await tick(); await tick();
  check(button("继续任务")?.disabled && !button("取消任务")?.disabled, "Pause released the pending resume slot or blocked cancellation");
  await click("取消任务");
  check(interruptionCalls === 2, "Cancellation could not run before resume settled");
  finishInterruption(); await tick(); await tick();
  rejectResume(new Error("TASK_CANCELLED: resumed attempt interrupted")); await tick(); await tick();
  check(document.querySelector('[role="status"]')?.textContent.includes("已确认取消任务")
    && !document.querySelector(".creative-alert.error"), "Cancelled resume overwrote the confirmed control outcome");

  workTasks = [{ ...readingTask, resumeBlockReason: null }];
  await render({ refreshToken: "resume-first", agentRunning: false });
  await click("继续任务");
  workTasks = [{ ...workTasks[0], status: "running" }];
  await render({ refreshToken: "resume-first-running", agentRunning: true });
  await click("取消任务");
  finishResume(); await tick(); await tick();
  check(button("取消任务")?.disabled, "Resume completion prematurely released an in-flight cancel guard");
  finishInterruption(); await tick(); await tick();

  workTasks = [{ ...readingTask, resumeBlockReason: null }];
  await render({ refreshToken: "resume-error-first", agentRunning: false });
  await click("继续任务");
  workTasks = [{ ...workTasks[0], status: "running" }];
  await render({ refreshToken: "resume-error-first-running", agentRunning: true });
  await click("取消任务");
  rejectResume(new Error("TASK_CANCELLED: exits before cancel receipt")); await tick(); await tick();
  check(button("取消任务")?.disabled, "Execution error released the pending cancellation slot");
  finishInterruption(); await tick(); await tick();
  check(!document.querySelector(".creative-alert.error") && document.querySelector('[role="status"]')?.textContent.includes("已确认取消任务"), "Earlier resume exit survived the matching cancellation receipt");

  workTasks = [{ ...readingTask, resumeBlockReason: null }, { ...readingTask, id: "work-other", goal: "另一项暂停任务", resumeBlockReason: null }];
  await render({ refreshToken: "other-task-cancel", agentRunning: false });
  await click("继续任务");
  workTasks = workTasks.map((task) => task.id === readingTask.id ? { ...task, status: "running" } : task);
  await render({ refreshToken: "other-task-running", agentRunning: true });
  const otherRow = [...document.querySelectorAll(".work-task-row")].find((row) => row.textContent.includes("另一项暂停任务"));
  const otherCancel = [...otherRow.querySelectorAll("button")].find((element) => element.textContent === "取消任务");
  check(!otherCancel.disabled, "Different-task cancellation was blocked by resumed execution");
  otherCancel.click(); await tick(); finishInterruption(); await tick(); await tick();
  workTasks = workTasks.map((task) => task.id === readingTask.id ? { ...task, status: "blocked" } : task);
  rejectResume(new Error("当前任务真实执行失败")); await tick(); await tick();
  check(document.querySelector(".creative-alert.error")?.textContent.includes("当前任务真实执行失败"), "Cancelling another task suppressed the resumed task's real error");

  workTasks = [{ ...readingTask, resumeBlockReason: null }, { ...readingTask, id: "work-other", goal: "另一项暂停任务", resumeBlockReason: null }];
  await render({ refreshToken: "other-cancel-after-error", agentRunning: false });
  await click("继续任务");
  rejectResume(new Error("取消其他任务前已显示的执行失败")); await tick(); await tick();
  const reverseRow = [...document.querySelectorAll(".work-task-row")].find((row) => row.textContent.includes("另一项暂停任务"));
  [...reverseRow.querySelectorAll("button")].find((element) => element.textContent === "取消任务").click(); await tick();
  check(document.querySelector(".creative-alert.error")?.textContent.includes("取消其他任务前已显示的执行失败"), "Starting another task's control erased an existing execution error");
  finishInterruption(); await tick(); await tick();
  check(document.querySelector(".creative-alert.error")?.textContent.includes("取消其他任务前已显示的执行失败")
    && document.querySelector('[role="status"]')?.textContent.includes("已确认取消任务"), "Another task's cancellation receipt erased the prior execution error");

  workTasks = [{ ...readingTask, resumeBlockReason: null }];
  await render({ refreshToken: "resume-failure", agentRunning: false });
  await click("继续任务");
  workTasks = [{ ...workTasks[0], goal: "失败后持久任务仍可核对", status: "blocked" }];
  rejectResume(new Error("实际模型执行失败")); await tick(); await tick();
  check(document.body.textContent.includes("失败后持久任务仍可核对")
    && document.querySelector(".creative-alert.error")?.textContent.includes("实际模型执行失败"), "Failed resume did not reconcile durable state or hid the execution error");

  api.agent.resumeWorkTask = async () => { workTasks = [{ ...workTasks[0], status: "completed" }]; };
  await render({ refreshToken: "post-control-read-error", onFormalChange: async () => { throw new Error("作品读取测试失败"); } });
  await click("继续任务"); await tick();
  check(document.querySelector(".creative-alert.error")?.textContent.includes("操作已返回成功，但作品界面刷新失败")
    && document.querySelector('[role="status"]')?.textContent.includes("继续执行已结束"), "Confirmed execution was presented as a failed mutation after a read error");
  api.agent.resumeWorkTask = resumeWorkTask;
  api.agent.controlWorkTask = controlWorkTask;
  workTasks = [];
  await render({ refreshToken: 0 });
  check(!document.querySelector(".unguided-writer-confirm") && !button("保留并按此意图继续").disabled, "Unguided creation still asks for duplicate confirmation");
  check(document.querySelector('.writer-runtime-preview').textContent.includes("候选稿"), "Writer preview hid the actual source authority");
  workspace.modeling = [{ objectId: "draft-chapter-1", sourceRevisionId: "draft-1", status: "model_pending", batchId: null, entities: [], assertions: [], confirmationState: "candidate" }];
  await render({ refreshToken: 1 });
  await click("继续分析本章");
  check(resumedChapter === chapter.id, "Pending knowledge did not expose a chapter-bound retry");
  workspace.modeling = [{ objectId: "draft-chapter-1", sourceRevisionId: "draft-1", status: "ready", batchId: "modeling-1", entities: [], assertions: [], confirmationState: "candidate" }];
  await render({ refreshToken: 2 });
  await click("审阅知识候选");
  check(reviewedModelingBatch === "modeling-1" && document.body.textContent.includes("hidLetter") && document.querySelector(".fact-review-card"), "Source-bound knowledge candidates did not reach the author review panel");
  check(document.querySelector(".state-property-details")?.textContent.includes("开始时间未知") && document.querySelector(".state-property-details")?.textContent.includes("不等于确认全局世界规则"), "State claim review hid unknown time or granted schema authority");
  const decision = document.querySelector('select[aria-label="hidLetter 的审阅结论"]');
  check(decision, "Knowledge candidate has no explicit author decision control");
  decision.value = "confirmed"; decision.dispatchEvent(new Event("change", { bubbles: true })); await tick();
  const eventCard = [...document.querySelectorAll(".fact-review-card")].find((card) => card.textContent.includes("hideLetterEvent"));
  check(eventCard?.textContent.includes("行动者：linxi") && eventCard.textContent.includes("效果主张：本批次 #1") && eventCard.textContent.includes("确认事件不自动确认关联主张"), "Event review lost participants, source-bound references or independent confirmation");
  check(eventCard.textContent.includes("前提时序：批次 prior-batch #3 区间在事件前适用") && eventCard.textContent.includes("#1 时间未知") && eventCard.textContent.includes("不证明前提真实或因果成立"), "Event temporal assessment lost source identity, uncertainty or claimed causality");
  check(eventCard.textContent.includes("规则条件：来源冲突") && eventCard.textContent.includes("已校验索引筛选，读取 2/2 个待检查批次，范围共 213 批") && eventCard.textContent.includes("批次 batch-counter #1") && eventCard.textContent.includes("不认证世界真相"), "Event conditions lost counterevidence, coverage or candidate-only authority");
  check(!eventCard.textContent.includes("值：") && !eventCard.textContent.includes("认知状态："), "Event review leaked generic value or epistemic controls");
  const eventDecision = eventCard.querySelector('select[aria-label="hideLetterEvent 的审阅结论"]');
  check(eventDecision.value === "", "Related event was implicitly confirmed");
  eventDecision.value = "rejected"; eventDecision.dispatchEvent(new Event("change", { bubbles: true })); await tick();
  await click("采用并推进正式版本");
  check(selectedForAccept?.factDecisions?.["fact-modeling"] === "confirmed" && selectedForAccept.sceneGroupIds.includes("a"), "Knowledge adoption did not carry the explicit decision and selected evidence scene");
  check(selectedForAccept?.factDecisions?.["fact-event"] === "rejected", "Independent event decision was not preserved");
  selectedForAccept = undefined;
  workspace.modeling = []; workspace.reviews = []; workspace.factProposals = [];
  await render({ refreshToken: 2 });
  await input("第 1 章创作意图", "保留我的创作意图");
  root.unmount(); root = createRoot(document.getElementById("root")); await render();
  check(document.querySelector('[aria-label="第 1 章创作意图"]').value === "保留我的创作意图", "Intent lost after unmount");
  workspace.continuationIntents = [{ targetKind: "draft", targetId: chapter.id, workingRevisionId: "draft-1", authorIntent: "重启后恢复的续创意图", status: "ready", taskId: null, updatedAt: "2026-09-13T08:00:00Z" }];
  root.unmount(); root = createRoot(document.getElementById("root")); await render();
  check(document.querySelector('[aria-label="第 1 章创作意图"]').value === "重启后恢复的续创意图", "Persisted continuation intent was overwritten during editor initialization");
  await input("场景 1 正文", "修改后的 A"); await click("＋ 添加场景"); await input("场景 3 正文", "新增 C"); await click("＋ 添加场景"); await waitForFocus("场景 4 正文"); await click("删除场景 4"); await waitForFocus("场景 3 正文"); await click("上移场景 3"); await click("上移场景 2");
  await click("保存章节");
  check([...document.querySelectorAll('[data-scene-id] textarea, [data-scene-id] button')].every((element) => element.matches(":disabled")), "Save did not lock editing");
  check(saveResult.sceneGroups[1].id === "a" && saveResult.sceneGroups[1].sceneId === "original-a" && saveResult.sceneGroups[1].body === "修改后的 A", "Edit/insertion changed existing identity");
  check(Object.keys(saveResult.sceneGroups[1]).sort().join(",") === "body,id,locked,sceneId", "Save leaked read-only Runtime metadata");
  check(saveResult.sceneGroups[2].id === "b" && !["a", "b"].includes(saveResult.sceneGroups[0].id), "Scene move/new identity failed");
  releaseSave();
  const acceptDeadline = performance.now() + 2000;
  while (!selectedForAccept && performance.now() < acceptDeadline) await tick();
  check(selectedForAccept?.noConfirmedChanges === true && JSON.stringify([...selectedForAccept.sceneGroupIds].sort()) === JSON.stringify(saveResult.sceneGroups.map((group) => group.id).sort()), "Saving the chapter did not write every scene");
  await input("第 1 章创作意图", "长".repeat(32001));
  check(document.querySelector('[aria-label="第 1 章创作意图"]').value.length === 32001 && document.body.textContent.includes("创作意图过长"), "Long intent not preserved with feedback");
  await input("第 1 章创作意图", "长".repeat(16001));
  check(document.querySelector('[aria-label="第 1 章创作意图"]').value.length === 16001 && document.body.textContent.includes("48000 UTF-8 字节") && button("保留并按此意图继续").disabled, "Under-character-limit byte overflow was allowed");
  await render({ view: "brief", planningIntentSeed: "这是作者对故事的构想。".repeat(400) });
  check(!document.body.textContent.includes("创作意图过长") && !document.body.textContent.includes("超过本轮文本预算"), "Valid long Chinese planning intent was rejected");
  await render({ view: "brief", planningIntentSeed: "讨论结果" }); await input("规划生成意图", "作者自己的要求"); await render({ view: "outline", planningIntentSeed: "讨论结果" }); await render({ view: "brief", planningIntentSeed: "讨论结果" });
  check(document.querySelector('[aria-label="规划生成意图"]').value === "作者自己的要求", "Intent lost on view switch");
  await render({ view: "brief", planningIntentSeed: "新的讨论结果" });
  check(document.querySelector('[aria-label="规划生成意图"]').value === "新的讨论结果", "New seed ignored");
  const outlineRevision = { id: "outline-review-1", version: 1, source: "author", payload: { nodes: [{ id: "book", kind: "book", position: 0, title: "全书", purpose: "讲述选择", summary: "全书安排", dependencyKeys: [], certainty: "tentative", protection: "free" }, { id: "volume-1", parentId: "book", kind: "volume", position: 0, title: "第一卷", purpose: "建立冲突", summary: "卷安排", dependencyKeys: [], certainty: "tentative", protection: "free" }] } };
  workspace.artifacts.outline.candidate = outlineRevision;
  await render({ view: "outline", planningIntentSeed: "", refreshToken: 1 });
  check(document.body.textContent.includes("规划深审") && document.body.textContent.includes("只生成意见，不修改或采用规划"), "Planning review entry or read-only boundary is missing");
  const reviewIntent = document.querySelector('.planning-review-intent textarea');
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(reviewIntent, "检查第一卷人物选择是否推动主冲突");
  reviewIntent.dispatchEvent(new Event("input", { bubbles: true }));
  document.querySelector('.planning-review-depth input[value="deep"]').click(); await tick();
  await click("开始规划深审");
  check(planningReviewRequest.revisionId === "outline-review-1" && planningReviewRequest.targetNodeId === null && planningReviewRequest.roles.join(",") === "structure,character,experience", "Planning review request lost its frozen target, scope, or independent roles");
  await render();
  await input("场景 2 正文", "失败时保留的正文");
  failSave = true; await click("保存章节");
  check(document.querySelector('[aria-label="场景 2 正文"]').value === "失败时保留的正文", "Failed save discarded local prose");
  failSave = false; await click("保存章节");
  check(Object.keys(saveResult.sceneGroups[1]).sort().join(",") === "body,id,locked,sceneId", "Second save leaked metadata");
  await render({ view: "brief" });
  releaseSave(); await tick(); await tick();
  check(document.querySelector('[aria-label="规划生成意图"]') && !document.querySelector('[data-scene-id]'), "Old save completion polluted another editor");
  check(!document.body.textContent.includes("这一章已保存。"), "Late save status crossed editor lifetimes");
  generationRuns = [{ id: "generation-ui", authorMessageId: "author-message-generation-ui", status: "running", stage: "draft", activeTaskId: "task-ui", pendingCurrentIntentGeneration: null, startChapterNumber: 1, completedChapters: 0, targetChapters: 3, blockedReason: null,
    maxTaskAttempts: 21, usedTaskAttempts: 3, providerBudget: { maxRequests: 208, maxTokens: 4_000_000, usedRequests: 3, usedTokens: 24_000, unknownTokens: 0, overrun: false, charges: [] } }];
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
  generationRuns = [{ ...generationRuns[0], stage: "commitments" }];
  await render({ view: "brief", agentRunning: true, refreshToken: 105 });
  check(document.body.textContent.includes("进行到 提取叙事承诺提案"), "Automatic commitment extraction is not author-readable");
  check(!document.querySelector("#future-intent").matches(":disabled"), "Unrelated future direction was blocked by commitment extraction");
  generationRuns = [{ ...generationRuns[0], stage: "brief" }];
  await render({ view: "brief", agentRunning: true, refreshToken: 101 });
  check(document.querySelector("#future-intent").matches(":disabled"), "Active planning task incorrectly accepted a future-chapter change");
  generationRuns = [{ ...generationRuns[0], stage: "draft", activeTaskId: null }];
  await render({ view: "brief", agentRunning: true, refreshToken: 102 });
  check(document.querySelector("#future-intent").matches(":disabled"), "A task-claim race exposed future intent editing before the active task was bound");
  generationRuns = [{ ...generationRuns[0], startChapterNumber: 4, completedChapters: 3, targetChapters: 6, activeTaskId: "task-ui", stage: "rollingPlan" }];
  await render({ view: "brief", agentRunning: false, refreshToken: 104 });
  check(document.body.textContent.includes("自动创作：0/3 章"), "Continuation progress includes chapters from an earlier run");
  generationRuns = [{ ...generationRuns[0], id: "generation-feedback", status: "blocked", stage: "check", currentChapterId: "chapter-feedback", blockedReason: "scene_feedback_needs_author", feedbackRepairCount: 0, usedTaskAttempts: 7, maxTaskAttempts: 21 }];
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
  check(button("重写本章")?.matches(":disabled") && document.body.textContent.includes("本章重写次数、任务尝试或供应商预算已用尽"), "Repair limit was not shown before offering an unavailable action");
  generationRuns = [{ ...generationRuns[0], blockedReason: "agent_task_failed_after_output", feedbackRepairPrompt: "重写第二场", feedbackRepairCount: 1 }];
  await render({ view: "brief", agentRunning: false, refreshToken: 108 });
  await click("按评估要求重试本章");
  check(retriedRun === "generation-feedback", "A partial feedback repair did not offer the safe retry entry");
  generationRuns = [{ ...generationRuns[0], usageUnsettled: true }];
  await render({ view: "brief", agentRunning: false, refreshToken: "usage-unsettled" });
  check(document.body.textContent.includes("模型用量待核账") && button("按评估要求重试本章")?.disabled, "Unsettled provider usage still offered a retry");
  generationRuns = [{ ...generationRuns[0], usageUnsettled: false }];
  await click("重新核账"); await tick(); await tick();
  check(!button("按评估要求重试本章")?.disabled, "Reconciled usage did not restore the author retry entry");
  generationRuns = [{ ...generationRuns[0], id: "generation-checkpoint", pendingCurrentIntentGeneration: null, feedbackRepairPrompt: null }];
  await render({ view: "brief", agentRunning: false, refreshToken: "checkpoint-retry" });
  const retrySnapshot = { label: button("从已保存候选继续")?.textContent, disabled: button("从已保存候选继续")?.disabled };
  await click("从已保存候选继续");
  check(retriedRun === "generation-checkpoint", "Ordinary partial output could not resume through existing retry");
  await render({ view: "brief", agentRunning: true, refreshToken: "checkpoint-running" });
  check(button("从已保存候选继续")?.disabled, "Checkpoint retry was offered during active execution");
  generationRuns = [];
  await render({ view: "brief", agentRunning: false, refreshToken: 105 });
  await input("规划生成意图", "第四章继续调查白塔记录");
  failGenerationList = true;
  await render({ view: "brief", agentRunning: false, refreshToken: "generation-list-failed" });
  check(button("续写后续章节")?.disabled && document.body.textContent.includes("创作运行状态读取失败"), "Unverified generation state allowed starting another run");
  failGenerationList = false;
  await click("重新读取"); await tick();
  check(button("续写后续章节") && !button("续写后续章节").matches(":disabled"), "Existing chapters hid the continuation entry");
  await click("续写后续章节");
  check(!requestedGenerationIntent && document.querySelector('[role="dialog"]')?.textContent.includes("最多 80 次") && document.querySelector('[role="dialog"]')?.textContent.includes("2,000,000") && document.querySelector('input[name="generation-target-chapters"][value="1"]')?.checked, "Generation did not default to one author-approved chapter with its own budget");
  document.querySelector('input[name="generation-target-chapters"][value="3"]').click(); await tick();
  check(document.querySelector('[role="dialog"]')?.textContent.includes("最多 208 次") && document.querySelector('[role="dialog"]')?.textContent.includes("4,000,000"), "Changing the chapter scope did not update the shared provider budget");
  failGenerationList = true;
  await render({ view: "brief", agentRunning: false, refreshToken: "generation-dialog-read-failed" });
  check(button("确认开始创作")?.disabled && document.querySelector('[role="dialog"]')?.textContent.includes("创作任务状态未能核对"), "Generation confirmation remained actionable after task-state read failed");
  failGenerationList = false;
  await click("核对任务状态"); await tick();
  check(!button("确认开始创作")?.disabled, "A successful state recheck did not restore generation confirmation");
  const reviewedSourceDigest = workspace.sourceDigest;
  workspace.sourceDigest = "c".repeat(64);
  await render({ view: "brief", agentRunning: false, refreshToken: "generation-dialog-source-changed" });
  check(button("确认开始创作")?.disabled && document.querySelector('[role="dialog"]')?.textContent.includes("作品来源已变化"), "Changed candidate sources remained author-approved while the permit dialog was open");
  workspace.sourceDigest = reviewedSourceDigest;
  await render({ view: "brief", agentRunning: false, refreshToken: "generation-dialog-source-restored" });
  check(!button("确认开始创作")?.disabled, "Restored source identity did not restore generation confirmation");
  await click("确认开始创作");
  check(requestedGenerationIntent?.intent === "第四章继续调查白塔记录" && requestedGenerationIntent.targetChapters === 3 && requestedGenerationIntent.expectedProjectRevision === workspace.revision && requestedGenerationIntent.expectedBaseSnapshotId === workspace.headSnapshotId && requestedGenerationIntent.expectedSourceDigest === workspace.sourceDigest, "Continuation did not hand the author's intent, scope, and reviewed sources to the run");
  requestedGenerationIntent = null;
  await render({ view: "chapters", conversationContinuation: { projectId: "ui-test", intent: "下一章让林夕藏好密函", revision: workspace.revision, headSnapshotId: workspace.headSnapshotId, sourceDigest: workspace.sourceDigest }, refreshToken: "conversation-continuation" });
  await click("按刚才的对话续写新章节");
  check(document.querySelector('textarea[aria-label="本轮创作目标"]')?.value === "下一章让林夕藏好密函" && !requestedGenerationIntent, "Conversation instruction was not offered as an author-editable proposal before execution");
  await input("本轮创作目标", " ");
  check(button("确认开始创作")?.disabled, "Blank continuation intent was allowed to start a run");
  await input("本轮创作目标", "下一章让林夕藏好密函，并保留张衡的动机悬念");
  await click("确认开始创作");
  check(requestedGenerationIntent?.intent === "下一章让林夕藏好密函，并保留张衡的动机悬念" && requestedGenerationIntent.targetChapters === 1 && requestedGenerationIntent.expectedProjectRevision === workspace.revision && requestedGenerationIntent.expectedBaseSnapshotId === workspace.headSnapshotId && requestedGenerationIntent.expectedSourceDigest === workspace.sourceDigest, "Conversation continuation did not use the edited author instruction and reviewed sources");
  await render({ view: "chapters", conversationContinuation: { projectId: "ui-test", intent: "过期续写", revision: workspace.revision, headSnapshotId: workspace.headSnapshotId, sourceDigest: "b".repeat(64) }, refreshToken: "conversation-stale" });
  check(!button("按刚才的对话续写新章节"), "An out-of-date conversation was offered as a continuation instruction");
  workspace.artifacts.plan.candidate = { id: "new-ui-plan", payload: { chapters: [{ chapterPlanId: "plan-ui", chapterNumber: 1, publicTitle: "藏信", writerGuidance: "保留密函" }] } };
  workspace.chapterPlanBindings = [{ project_id: "ui-test", chapter_id: "chapter-1", chapter_plan_id: "plan-ui", plan_revision_id: "old-ui-plan", lineageStatus: "historical", writerProjection: { publicTitle: "焚信", writerGuidance: "烧毁密函" } }];
  await render({ view: "chapters", agentRunning: false, refreshToken: "rebind-ui" });
  await click("预览章纲调整");
  document.querySelector('.rebind-acknowledgment input').click(); await tick();
  await click("确认替换章纲关联");
  check(document.querySelector('[aria-label="场景 1 正文"]').disabled && button("保存章节").disabled, "Plan rebind did not freeze the entire editor");
  check(rebindRequest.previousPlanRevisionId === "old-ui-plan" && rebindRequest.planRevisionId === "new-ui-plan" && rebindRequest.sourceRevisionId === workspace.chapters[0].candidate.id, "Workspace submit changed the frozen source");
  releaseRebind(); await tick(); await tick(); await tick();
  check(document.body.textContent.includes("关联已成功，但刷新失败") && document.querySelector('.chapter-plan-rebind [role="status"]')?.textContent.includes("正文保留"), "A committed rebind was reported as failed when reload failed");
  check(!document.querySelector('[aria-label="场景 1 正文"]').disabled, "Editor remained locked after confirmed commit");
  check(!document.body.textContent.includes("正在更新章纲关联"), "Completed rebind retained an in-progress status");
  failRebindRefresh = false;
  await click("重新读取作品");
  check(document.querySelector('.revision-badge')?.textContent === `作品 v${workspace.revision}` && document.body.textContent.includes("作品已重新读取"), "Read-only recovery did not reload the committed version");
  return { passed: true, retrySnapshot };
}
run().then((result) => { window.testResult = result; }).catch((error) => { window.testResult = { passed: false, error: (window.testResult?.error ?? error.stack) + "\nDOM:" + document.body.innerText }; });
