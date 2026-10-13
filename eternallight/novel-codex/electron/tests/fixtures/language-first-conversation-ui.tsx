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
const alternate = { ...structuredClone(proposal), id: "tax", kind: "suspense", summary: "税册里突然出现了主角的名字", fulfillmentCriteria: "后文揭示是谁填入名字", setupEvidenceRefs: [] };
const reportPayload = { chapterId: "chapter-seven", sourceRevisionId: "source-chapter-seven", examinedRanges: [{ start: 0, end: 9 }], candidates: [proposal, alternate] };
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
let modelCalls = 0, completedModelCalls = 0, controlCalls = 0, createdThreads = 0, holdProof = false, releaseProof;
let onEvent; const sends = [];
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
  onEvent: listener => { onEvent = listener; return () => {}; }, onUserInputRequest: () => () => {},
  sendPrompt: async (...args) => {
    modelCalls++; sends.push(structuredClone(args));
    onEvent?.({ method: "agent/execution-request", params: { projectId: args[0], requestId: `language-request-${modelCalls}` } });
    onEvent?.({ method: "agent/message-updated", params: { projectId: args[0], messageId: `language-request-${modelCalls}`, status: "queued" } });
    const threadId = args[4], turnId = `language-turn-${modelCalls}`;
    check(args[0] === project.id && args[2] === null && threadId === "history-thread", "Natural message escaped source conversation");
    onEvent?.({ method: "thread/started", params: { thread: { id: threadId } } }); await tick();
    onEvent?.({ method: "turn/started", params: { threadId, turn: { id: turnId } } }); await tick();
    const item = { id: `answer-${modelCalls}`, type: "agentMessage", phase: "final_answer", text: "模型收到完整作者表达与引用。" };
    onEvent?.({ method: "item/completed", params: { threadId, turnId, item } }); await tick();
    onEvent?.({ method: "turn/completed", params: { threadId, turn: { id: turnId, status: "completed", items: [item] } } });
    completedModelCalls = modelCalls;
    return { mode: "advisory", projectId: project.id, message: item.text, proposedActions: [], candidateResults: [] };
  },
  resumeWorkTask: async () => { controlCalls++; }, controlWorkTask: async () => { controlCalls++; },
} };
window.novelCodex = api;
createRoot(document.getElementById("root")).render(<React.StrictMode><App /></React.StrictMode>);
const card = () => document.querySelector<HTMLElement>('.conversation-rail [data-commitment-report="report-seven"]');
const composer = () => document.querySelector<HTMLTextAreaElement>('[aria-label="创作指令"]');
const attachments = () => [...document.querySelectorAll<HTMLElement>('.discussion-excerpt')].filter(node => !node.closest('.conversation-turn'));
const changeValue = async value => {
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(composer(), value);
  composer().dispatchEvent(new Event("input", { bubbles: true })); await tick();
};
const referenceButton = (proposalId = "bell") => {
  const evidence = card()?.querySelector<HTMLDetailsElement>(`[data-proposal-id="${proposalId}"] details`);
  if (evidence) evidence.open = true;
  return evidence && button(evidence, "引用这条建议");
};
const attach = async (proposalId = "bell") => {
  await until(() => referenceButton(proposalId) && !referenceButton(proposalId).disabled, "Suggestion reference unavailable");
  referenceButton(proposalId).click();
  await until(() => attachments().length === 1, "Exact proposal not attached");
};
const clearAttachments = async () => {
  attachments().forEach(node => node.querySelector<HTMLButtonElement>('button[aria-label^="移除引用"]').click());
  await until(() => attachments().length === 0, "Source removal failed");
};
const settle = async () => { for (let index = 0; index < 12; index++) await tick(); };
const chooseThread = async id => {
  document.querySelector<HTMLButtonElement>(`[aria-label="切换对话：${threads.get(id).name}"]`).click();
  await until(() => document.querySelector('.conversation-rail')?.getAttribute('data-conversation-id') === id, "Thread switch failed");
  await settle();
};
(async () => { try {
  await until(() => card()?.textContent.includes(alternate.summary) && composer(), "Initial exact suggestions did not load");
  check(!document.querySelector('[aria-label="发送设置"], .composer-intent-picker'), "Conversation still requires a semantic mode choice");
  check([...card().querySelectorAll<HTMLDetailsElement>('details')].every(node => !node.open), "Source or formal choices expanded by default");
  check(!/讨论这些建议|引用到输入框|继续讨论/.test(card().textContent), "Suggestions duplicated the conversation input");
  check([...card().querySelectorAll<HTMLButtonElement>('button')].filter(node => /确认保留|调整后再决定|引用这条/.test(node.textContent)).every(node => node.closest('details') && !node.closest('details').open),
    "Formal actions or per-proposal references remain in primary reading flow");
  await attach();
  check(composer().value === "" && document.activeElement === composer() && modelCalls === 0 && writes.length === 0,
    "Reference invented a question, sent a message or saved a decision");
  const captured = JSON.parse(attachments()[0].querySelector('blockquote').textContent);
  check(captured.capture.chapterId === "chapter-seven" && captured.capture.scope === "singleProposal"
    && captured.reportMetadata.revisionId === "report-seven" && captured.reportMetadata.candidateCount === 2 && captured.proposal.id === "bell",
    "Reference substituted the current selected chapter or lost its exact proposal/report");
  await clearAttachments();
  await changeValue("我有另一种设想：把钟声改成预兆。"); await attach();
  check(composer().value === "我有另一种设想：把钟声改成预兆。", "Attaching a source overwrote author expression");
  await clearAttachments(); await changeValue("");

  const prompts = ["还有其他方案吗？", "把前两个合并，先不保存。", "现在按这个方案修改结尾。", "先分析，别续写。"];
  for (const prompt of prompts) {
    await changeValue(prompt);
    const count = modelCalls;
    document.querySelector<HTMLButtonElement>('[aria-label="发送创作指令"]').click();
    await until(() => completedModelCalls === count + 1 && !document.querySelector('[aria-label="停止当前执行"]')
      && document.querySelector('.composer-send') && !composer().disabled, "Natural send failed to finish");
    const sent = sends.at(-1);
    check(sent[1] === prompt && sent[4] === "history-thread" && sent[3].chapterId === "chapter-one"
      && !sent[3].excerpts && !sent[3].reference && sent[3].readOnly !== true,
      "Direct follow-up interpreted intent, rebound the thread to the viewed chapter or implicitly attached content");
    check(!composer().value && attachments().length === 0, "Sent input was not consumed once");
  }
  check(writes.length === 0 && controlCalls === 0, "Frontend itself interpreted language and invoked author writes or task controls");

  const first = card().querySelector<HTMLDetailsElement>('[data-proposal-id="bell"] details');
  first.open = true; await tick();
  await until(() => button(first, "引用这条建议") && !button(first, "引用这条建议").disabled, "Single proposal reference remained busy");
  button(first, "引用这条建议").click(); await until(() => attachments().length === 1, "Single proposal reference missing");
  const single = JSON.parse(attachments()[0].querySelector('blockquote').textContent);
  check(single.capture.scope === "singleProposal" && single.proposal.id === "bell" && !single.reportMetadata.payload.candidates && !single.report,
    "Single-proposal reference included unrelated candidates");
  const exactPrompt = "只调整这条建议的意象，先给我两个方向。";
  await changeValue(exactPrompt);
  document.querySelector<HTMLButtonElement>('[aria-label="发送创作指令"]').click();
  await until(() => completedModelCalls === prompts.length + 1 && !document.querySelector('[aria-label="停止当前执行"]')
    && document.querySelector('.composer-send') && !composer().disabled,
    "Optional exact reference did not send");
  check(sends.at(-1)[1] === exactPrompt && sends.at(-1)[3].excerpts.length === 1
    && sends.at(-1)[3].excerpts[0].revisionId === "report-seven" && sends.at(-1)[4] === "history-thread",
    "Explicit reference lost original author language, exact version or thread");
  await clearAttachments();

  holdProof = true; releaseProof = null; referenceButton().click();
  await until(() => releaseProof, "Source proof did not suspend");
  await changeValue("这句话是在核对期间输入的。"); releaseProof(); await settle();
  check(!attachments().length && composer().value === "这句话是在核对期间输入的。", "Late source changed newer author input");
  await changeValue("");
  holdProof = true; releaseProof = null; referenceButton().click();
  await until(() => releaseProof, "ABA proof did not suspend");
  await chooseThread("another-thread"); await chooseThread("history-thread");
  releaseProof(); await settle();
  check(!attachments().length && !composer().value, "A→B→A revived a stale discussion request");

  holdProof = true; releaseProof = null; referenceButton().click();
  await until(() => releaseProof, "Competing reference proof did not suspend");
  referenceButton("tax").click();
  await until(() => attachments().length === 1, "Latest single reference did not win");
  releaseProof(); await settle();
  check(attachments().length === 1 && JSON.parse(attachments()[0].querySelector('blockquote').textContent).proposal.id === "tax",
    "Older concurrent reference displaced the latest choice");
  check(writes.length === 0 && modelCalls === prompts.length + 1 && controlCalls === 0, "Reference races executed hidden work");
  window.testScreenshot = "language-first-suggestion-reference";
  await until(() => !window.testScreenshot, "Screenshot did not settle");
  window.testResult = { passed: true, prompts: sends.map(call => call[1]), writes: writes.length, controlCalls,
    checks: ["single input", "no duplicate discussion entry", "no implicit author prompt", "exact proposal", "preserved draft", "unquoted multi-turn language", "optional versioned reference", "input race", "ABA isolation", "latest reference wins"] };
} catch (error) { window.testResult = { passed: false, error: error.stack, dom: document.body.textContent.slice(-6000) }; } })();
