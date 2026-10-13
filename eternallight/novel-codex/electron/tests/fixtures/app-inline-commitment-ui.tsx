import { novelFixtureMedia } from "../helpers/novel-edition";
import { emptyCreationIssueSummary } from "../helpers/creation-issues";
import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import { saveAuthor } from "../helpers/author-session";

const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (value, message) => { if (!value) throw Error(message); };
const until = async (predicate, message) => { for (let i = 0; i < 180 && !predicate(); i++) await tick(); check(predicate(), message); };
const text = () => document.body.innerText;
const button = (scope, label) => [...scope.querySelectorAll<HTMLButtonElement>("button")].find(node => node.textContent === label);
window.addEventListener("error", event => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
saveAuthor({ name: "建议作者", email: "fixture@example.invalid", passwordSalt: "fixture", passwordHash: "fixture" });
const project = { id: "project-last-lamp", title: "就地选择测试作品", logline: "", updated_at: "" };
const chapters = [
  { id: "chapter-one", number: 1, title: "另一章的正文", status: "accepted", summary: "", passages: [] },
  { id: "chapter-seven", number: 7, title: "白塔的钟声", status: "accepted", summary: "", passages: [] },
];
const bodies = { "chapter-one": "这一章没有钟声。", "chapter-seven": "白塔只亮四十七秒。" };
const revisions = Object.fromEntries(chapters.map(chapter => [chapter.id, { id: `source-${chapter.id}`, title: chapter.title, version: 1,
  body: bodies[chapter.id], sceneGroups: [{ id: `scene-${chapter.id}`, sceneId: null, body: bodies[chapter.id], locked: false }],
  sourcePlanRefs: { localOnly: true, lineageStatus: "current" } }]));
let projectRevision = 7, snapshotId = "head-seven";
const runtimeState = () => ({ project, canon: { characters: [], locations: [], relationships: [], events: [], plotThreads: [], worldRules: [], notes: [] },
  narrativeIr: { version: "1", scenes: [], anchors: [] }, novelEdition: { chapters } });
const workspace = () => ({ project, revision: projectRevision, headSnapshotId: snapshotId, sourceDigest: "a".repeat(64), index: { status: "ready" },
  artifacts: Object.fromEntries(["brief", "outline", "plan", "identity"].map(kind => [kind, { candidate: null, accepted: null, working: null, locked: false, workingSelection: null }])),
  chapters: chapters.map(chapter => ({ ...chapter, candidate: revisions[chapter.id], accepted: revisions[chapter.id], working: null, workingSelection: null })),
  tasks: [], factProposals: [], reviews: [], commits: [], planningReviews: [], impacts: [], workingBaselines: [], continuationIntents: [],
  modeling: chapters.map(chapter => ({ objectId: `draft-${chapter.id}`, sourceRevisionId: revisions[chapter.id].id, status: "model_pending", batchId: null })),
  chapterPlanBindings: [] });
const proposal = { id: "bell", kind: "foreshadowing", summary: "第七章：钟声只持续四十七秒", fulfillmentCriteria: "后文解释钟声持续时间的原因",
  relatedEntityIds: [], plotThreadId: null, characterArcId: null, targetWindow: { kind: "open" }, status: "open", decisionReason: null,
  setupEvidenceRefs: [{ objectId: "draft-chapter-seven", revisionId: "source-chapter-seven", quote: "白塔只亮四十七秒。", start: 0, end: 9 }], fulfillmentEvidenceRefs: [] };
const reportPayload = { chapterId: "chapter-seven", sourceRevisionId: "source-chapter-seven", examinedRanges: [{ start: 0, end: 9 }], candidates: [proposal] };
const report = () => ({ revisionId: "report-seven", snapshotId, projectRevision, authority: "candidateProposalBatch", reportCurrent: true, sourceCurrent: true,
  established: [{ proposalId: "bell", commitmentId: "independent-bell" }], payload: structuredClone(reportPayload),
  coverage: { semanticCompleteness: "unknown", modelReportedRanges: true, literaryFulfillmentVerified: false, globalCompletenessClaimed: false } });
let detail = { projectId: project.id, objectId: "commitment-independent-bell", revisionId: "bell-candidate", snapshotId,
  payload: { ...structuredClone(proposal), id: "independent-bell" }, sourceCurrent: true, authorControls: { projectRevision, undoAvailable: false },
  confirmationState: "candidate", confirmationCommitId: null, interpretation: "sourceBoundNarrativeClaim", literaryFulfillmentVerified: false,
  extractionOrigin: { chapterId: "chapter-seven", reportRevisionId: "report-seven", proposalId: "bell" } };
const task = { id: "work-extract-seven", projectId: project.id, threadId: "history-thread", goal: "提取第七章的伏笔建议", kind: "direct", status: "completed",
  version: 1, error: null, resumeMode: null, resumeBlockReason: null, deliveryStatus: "ready_for_review",
  deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], evidence: [{ stepId: "step-extract-seven", attemptId: "attempt-extract-seven", kind: "commitmentExtraction", state: "recorded", referenceId: "report-seven" }],
    pendingActionIds: [], unsettledAttemptIds: [] },
  steps: [{ id: "step-extract-seven", operation: "extractCommitments", currentAttemptId: "attempt-extract-seven", status: "completed",
    target: { chapterId: "chapter-seven", chapterNumber: 7, sourceRevisionId: "source-chapter-seven" }, expectedOutputKind: "commitmentExtraction", instruction: null,
    attempts: [{ id: "attempt-extract-seven", threadId: "history-thread", turnId: "turn-extract-seven", status: "completed", startedAt: 100, completedAt: 101, attemptNumber: 1, error: null }],
    changes: [], outputRefs: [], readingProgress: null }] };
localStorage.setItem(`novel-codex:selected-conversation:${JSON.stringify(["fixture@example.invalid", project.id])}`, "history-thread");
const reads = [], writes = [], visits = [], isolatedNavigations = [];
const threads = new Map([
  ["history-thread", { id: "history-thread", projectId: project.id, name: "原建议对话", status: "idle", createdAt: 1, updatedAt: 1, isPinned: false, isArchived: false }],
  ["another-thread", { id: "another-thread", projectId: project.id, name: "另一对话", status: "idle", createdAt: 2, updatedAt: 2, isPinned: false, isArchived: false }],
]);
let modelCalls = 0, controlCalls = 0, createdThreads = 0, holdProof = false, releaseProof;
const forbidden = kind => async (...args) => { writes.push({ kind, args }); throw Error(`Unexpected ${kind}`); };
const api = { multimedia: novelFixtureMedia, app: { onCloseRequested: () => () => {} }, runtime: {
  status: async () => ({ state: "online", mode: "local", error: null }), listProjects: async () => ({ projects: [project] }),
  getState: async id => { check(id === project.id, "State escaped its project"); return structuredClone(runtimeState()); },
  getCreativeWorkspace: async id => { check(id === project.id, "Workspace escaped its project"); return structuredClone(workspace()); },
  getWorkCatalog: async () => ({ revision: projectRevision, volumes: [], placements: [] }),
  listChapterStructureChanges: async projectId => ({ projectId, items: [], nextCursor: null }),
  listPlanningImpacts: async () => ({ data: [], nextCursor: null, counts: { open: 0 } }), listGenerationRuns: async () => ({ runs: [] }),
  listWorkTasks: async id => { check(id === project.id, "Task list escaped its project"); return { tasks: [structuredClone(task)], activeCount: 0, nextCursor: null }; },
  getWorkTask: async (id, taskId) => {
    check(id === project.id && taskId === task.id, "Task identity changed");
    const proof = structuredClone(task);
    if (holdProof) { holdProof = false; return new Promise(resolve => { releaseProof = () => resolve(proof); }); }
    return proof;
  },
  getTaskFinalReply: async (id, taskId) => ({ projectId: id, taskId, state: "recorded", recordedAt: 200,
    binding: { threadId: "history-thread", turnId: "turn-extract-seven", itemId: "answer", intentRevision: 0, truncated: false,
      text: "已阅读第七章并保存钟声建议。你可以在这次任务下选择保留或调整。" }, sourceDigest: "digest", receiptDigest: "receipt" }),
  getWriterPreview: async () => ({ target: { position: {} }, plannedEffects: [], relevantText: [], coverage: { complete: true, missingScope: [], truncationReasons: [] } }),
  getIdentityCatalog: async () => ({ entries: [], ambiguousNames: [], identityRevisionId: null, identityDecisions: [], truncated: false, ambiguityScanComplete: true }),
  ...Object.fromEntries(["Search", "Entity", "Relation", "Knowledge", "SnapshotKnowledge", "Fact"].map(kind => [`get${kind}Audit`, async () => ({ job: null })])),
  getCommitmentExtraction: async (id, chapterId, reportRevisionId = null) => {
    reads.push({ chapterId, reportRevisionId });
    check(id === project.id && chapterId === "chapter-seven" && reportRevisionId === "report-seven", "App read another chapter or substituted a latest report");
    return report();
  },
  getCommitment: async (id, commitmentId) => {
    check(id === project.id && commitmentId === "independent-bell", "App substituted another commitment"); return structuredClone(detail);
  },
  listCommitments: async () => ({ data: [], snapshotId, nextCursor: null }),
  establishCommitment: forbidden("establish"), createCommitmentReview: forbidden("review"), acceptReview: forbidden("accept"),
  saveCommitment: async (id, commitmentId, request) => {
    check(id === project.id && commitmentId === "independent-bell" && request.payload.id === commitmentId, "Save escaped its independent commitment");
    check(request.expectedRevision === projectRevision && request.baseSnapshotId === snapshotId && request.expectedCandidateRevisionId === detail.revisionId
      && request.idempotencyKey, "Save lost its original baseline or CAS");
    check(JSON.stringify(request.extractionOrigin) === JSON.stringify(detail.extractionOrigin)
      && request.payload.setupEvidenceRefs[0].revisionId === "source-chapter-seven", "Editing rebound another chapter/report source");
    writes.push({ kind: "save", projectId: id, commitmentId, request: structuredClone(request) });
    projectRevision++; snapshotId = "head-eight";
    detail = { ...detail, payload: structuredClone(request.payload), revisionId: "bell-edited", snapshotId,
      authorControls: { projectRevision, undoAvailable: false }, confirmationState: "candidate", confirmationCommitId: null };
    return { revisionId: detail.revisionId, snapshotId };
  },
}, agent: {
  status: async () => ({ configured: true, state: "ready", mode: "novel-agent-engine", provider: "fixture", model: "fixture", error: null }),
  listConnections: async () => ({ connections: [] }), recoveryReport: async () => [], listProposedActions: async () => ({ actions: [] }), setWorkspaceContext: async () => {},
  readConversation: async (projectId, id) => { check(projectId === project.id && threads.has(id), "Conversation read escaped its project"); return { thread: structuredClone(threads.get(id)), turns: [] }; },
  listExecutions: async () => [], readMessage: async () => { throw Error("message receipt not found"); }, beginInterlude: async () => { throw Error("unexpected interlude"); }, returnInterlude: async () => { throw Error("unexpected interlude return"); }, listMessages: async () => [], listThreads: async () => ({ threads: [...threads.values()].map(thread => structuredClone(thread)), nextCursor: null }),
  createThread: async (projectId, options) => {
    check(projectId === project.id && !Object.keys(options).length, "Creating a conversation changed project or forked unexpectedly");
    createdThreads++;
    const thread = { id: `created-thread-${createdThreads}`, projectId, name: "新建独立对话", status: "idle", createdAt: 3, updatedAt: 3, isPinned: false, isArchived: false };
    threads.set(thread.id, thread); return { thread: structuredClone(thread) };
  },
  onEvent: () => () => {}, onUserInputRequest: () => () => {},
  sendPrompt: async () => { modelCalls++; throw Error("Unexpected model call"); },
  resumeWorkTask: async () => { controlCalls++; }, controlWorkTask: async () => { controlCalls++; },
} };
window.novelCodex = api;
createRoot(document.getElementById("root")).render(<React.StrictMode><App /></React.StrictMode>);
const card = () => document.querySelector<HTMLElement>('.conversation-rail [data-commitment-report="report-seven"]');
const modal = () => document.querySelector<HTMLDialogElement>('dialog.commitment-management-dialog[open]');
const navigateCharacters = async () => {
  const navigation = [...document.querySelectorAll<HTMLButtonElement>(".nav-item")].find(node => node.textContent.includes("角色"));
  check(navigation, "Character navigation absent"); navigation.click();
  await until(() => document.querySelector('[data-view="characters"]') && card(), "Generic conversation rail absent in characters view");
  visits.push("characters");
};
function changeValue(node: HTMLTextAreaElement, value: string) {
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(node, value);
  node.dispatchEvent(new Event("input", { bubbles: true }));
}
async function verifyDelayedNavigation(kind: "switch" | "create") {
  await until(() => button(card(), "调整后再决定") && !button(card(), "调整后再决定").disabled, "Delayed navigation source is not actionable");
  holdProof = true; releaseProof = null;
  button(card(), "调整后再决定").click();
  await until(() => releaseProof, "App navigation delivery proof did not suspend");
  check(document.querySelector('[data-view="characters"]') && !modal(), "Navigation happened before its delivery proof was verified");
  const beforeWrites = writes.length;
  if (kind === "switch") document.querySelector<HTMLButtonElement>('[aria-label="切换对话：另一对话"]').click();
  else document.querySelector<HTMLButtonElement>('[aria-label="新建对话"]').click();
  const selected = kind === "switch" ? "another-thread" : "created-thread-1";
  await until(() => document.querySelector('.conversation-rail')?.getAttribute("data-conversation-id") === selected && !card(), "New conversation did not replace the old source card");
  releaseProof();
  for (let i = 0; i < 10; i++) await tick();
  check(document.querySelector('[data-view="characters"]') && !modal() && !document.querySelector('.commitment-editor')
    && document.querySelector('.conversation-rail')?.getAttribute("data-conversation-id") === selected,
    "Late proof opened the previous conversation's editor or changed the new conversation's view");
  check(writes.length === beforeWrites && detail.confirmationState === "candidate", "Late navigation initiated further commitment writes");
  isolatedNavigations.push(kind);
  document.querySelector<HTMLButtonElement>('[aria-label="切换对话：原建议对话"]').click();
  await until(() => card()?.textContent.includes(proposal.summary), "Returning to the original source did not restore its exact report");
  check(!modal() && !document.querySelector('.commitment-editor'), "Returning to the original conversation replayed its abandoned navigation");
}

(async () => {
  try {
    await until(() => card()?.textContent.includes(proposal.summary), "Initial exact task report did not load");
    check(writes.length === 0 && !modal(), "Mount automatically wrote or opened an editor");
    await navigateCharacters();
    await until(() => button(card(), "调整后再决定") && !button(card(), "调整后再决定").disabled, "Generic rail has no actionable exact suggestion");
    check(document.querySelectorAll('.conversation-rail [aria-label="创作对话"]').length === 1, "Generic rail duplicated the timeline");
    button(card(), "调整后再决定").click();
    await until(() => modal() && modal().querySelector('[aria-label="承诺摘要"]'), "Generic suggestion did not open the actual editor modal");
    check(document.querySelector('[data-view="chapters"]') && document.querySelector<HTMLInputElement>('[aria-label="章节标题"]').value === chapters[1].title,
      "Adjustment did not navigate the source chapter");
    check(document.querySelectorAll('dialog.commitment-management-dialog[open]').length === 1
      && document.querySelectorAll('.commitment-editor').length === 1, "Multiple modal/editor owners mounted");
    check(modal().querySelector<HTMLTextAreaElement>('[aria-label="承诺摘要"]').value === proposal.summary
      && card()?.textContent.includes(proposal.summary) && card().closest('[data-work-task-id]')?.getAttribute("data-work-task-id") === task.id,
      "The editor or original task card lost its exact report");
    check(writes.length === 0 && detail.confirmationState === "candidate", "Opening adjustment wrote or auto-confirmed an established candidate");
    visits.push("chapter-seven:editor");
    window.testScreenshot = "app-inline-commitment-editor";
    const summary = modal().querySelector<HTMLTextAreaElement>('[aria-label="承诺摘要"]');
    changeValue(summary, "第七章：钟声在税册到期前停止");
    await until(() => !button(modal(), "保存承诺候选").disabled, "Explicit local edit was not registered");
    check(document.querySelector<HTMLButtonElement>('[aria-label="关闭伏笔调整"]').disabled, "Dirty editor could close and discard input silently");
    button(modal(), "保存承诺候选").click();
    await until(() => writes.length === 1 && text().includes("已保存新的候选修订")
      && !document.querySelector<HTMLButtonElement>('[aria-label="关闭伏笔调整"]').disabled, "Candidate save did not finish or release its input guard");
    check(writes[0].kind === "save" && detail.payload.summary === "第七章：钟声在税册到期前停止"
      && detail.confirmationState === "candidate" && detail.confirmationCommitId === null, "Editing silently accepted the candidate");
    check(card()?.textContent.includes(proposal.summary), "Editing overwrote immutable task report text");
    document.querySelector<HTMLButtonElement>('[aria-label="关闭伏笔调整"]').click();
    await until(() => !modal() && !document.querySelector('.commitment-editor'), "Saved editor did not close");
    await navigateCharacters();
    const firstChapter = document.querySelector<HTMLButtonElement>('[data-directory-chapter-id="chapter-one"]');
    check(firstChapter, "First chapter directory entry absent");
    const tree = firstChapter.closest("details"); if (tree && !tree.open) tree.querySelector<HTMLElement>("summary").click();
    firstChapter.click();
    await until(() => document.querySelector<HTMLInputElement>('[aria-label="章节标题"]')?.value === chapters[0].title, "Another chapter could not be opened after adjustment");
    for (let i = 0; i < 8; i++) await tick();
    check(!modal() && !document.querySelector('.commitment-editor') && card()?.textContent.includes(proposal.summary),
      "Consumed external edit request reopened or changed chapter-bound suggestions");
    check(reads.length > 0 && reads.every(read => read.chapterId === "chapter-seven" && read.reportRevisionId === "report-seven"),
      "Reports leaked to another chapter or displayed an unbound latest report");
    check(writes.length === 1 && modelCalls === 0 && controlCalls === 0, "Navigation or close emitted additional writes/model/task controls");
    visits.push("chapter-one:no-reopen");
    await navigateCharacters();
    await verifyDelayedNavigation("switch");
    await verifyDelayedNavigation("create");
    check(writes.length === 1 && createdThreads === 1 && modelCalls === 0 && controlCalls === 0, "Async navigation isolation caused extra commitment/model/task writes");
    window.testResult = { passed: true, writes: writes.map(write => write.kind), modelCalls, controlCalls, reportReads: reads.length, visits,
      confirmationState: detail.confirmationState, confirmationCommitId: detail.confirmationCommitId, isolatedNavigations, createdThreads };
  } catch (error) { window.testResult = { passed: false, error: error.stack, dom: text(), reads, writes }; }
})();
