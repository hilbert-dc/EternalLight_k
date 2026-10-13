import { novelFixtureMedia } from "../helpers/novel-edition";
import { emptyCreationIssueSummary } from "../helpers/creation-issues";
import React from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import type { ThreadDetail, WorkTaskSummary } from "../../../src/types";
import "../../../src/styles.css";
import { saveAuthor } from "../helpers/author-session";

const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (value, message) => { if (!value) throw Error(message); };
const until = async (predicate, message) => { for (let i = 0; i < 200 && !predicate(); i++) await tick(); check(predicate(), message); };
const deferred = () => { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; };
const button = text => [...document.querySelectorAll("button")].find(node => node.textContent === text || node.getAttribute("aria-label") === text);
const input = async (label, value) => {
  const element = document.querySelector(`[aria-label="${label}"]`); check(element && !element.matches(":disabled"), `Unavailable ${label}`);
  Object.getOwnPropertyDescriptor(element instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype, "value").set.call(element, value);
  element.dispatchEvent(new Event("input", { bubbles: true })); await tick();
};
const composer = () => document.querySelector<HTMLTextAreaElement>('[aria-label="创作指令"]');
const send = () => document.querySelector<HTMLButtonElement>('[aria-label="发送创作指令"], [aria-label="追加创作指令"]');
const projects = ["project-last-lamp", "project-b"].map(id => ({ id, title: id, logline: "", updated_at: "" }));
const chapters = [{ id: "chapter-01", number: 1, title: "本章", status: "draft", summary: "", updated_at: "", passages: [] }, { id: "chapter-02", number: 2, title: "新章", status: "draft", summary: "", updated_at: "", passages: [] }];
const revision = (id, body) => ({ id, title: "本章", version: 1, body, sourcePlanRefs: { localOnly: true, lineageStatus: "current" }, sceneGroups: [{ id: "scene-one", sceneId: null, body, locked: false }] });
const workspaceFor = project => ({ project, revision: 1, headSnapshotId: "head", sourceDigest: "a".repeat(64), index: { status: "ready" }, artifacts: Object.fromEntries(["brief", "outline", "plan", "identity"].map(kind => [kind, { candidate: null, accepted: null, working: null, locked: false, workingSelection: null }])), chapters: chapters.map((chapter, index) => ({ ...chapter, candidate: index === 0 ? revision("saved", "原有短正文。") : null, accepted: null, working: null, workingSelection: null })), tasks: [], reviews: [], modeling: [], factProposals: [], chapterPlanBindings: [], workingBaselines: [], continuationIntents: [], planningReviews: [], commits: [], impacts: [] });
const workspaces = new Map(projects.map(project => [project.id, workspaceFor(project)]));
const saves = [], generations = [], discussions = [], steers = [], snapshots = [];
let listener, execution, holdPreflight = false, preflight, failNextSave = false, turn = 0, activeOperation = null, stateReads = 0, delegateWriter = false, delegatedWriters = [];
const event = (method, params) => listener({ method, params });
window.novelCodex = { multimedia: novelFixtureMedia,
  app: { onCloseRequested: () => () => {} },
  runtime: { listChapterStructureChanges: async (projectId: string) => ({ projectId, items: [], nextCursor: null }),
  getWorkCatalog: async () => ({ revision: 0, volumes: [], placements: [] }),
    getIdentityCatalog: async () => ({ entries: [], ambiguousNames: [], identityRevisionId: null, identityDecisions: [], truncated: false, ambiguityScanComplete: true }),
    status: async () => ({ state: "online", mode: "local", error: null }), listProjects: async () => ({ projects }),
    getState: async id => { stateReads++; return { project: projects.find(project => project.id === id), canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1.0", scenes: [], anchors: [] }, novelEdition: { chapters } }; },
    getCreativeWorkspace: async id => { const data = structuredClone(workspaces.get(id)); if (holdPreflight) { holdPreflight = false; preflight = deferred(); return preflight.promise; } return data; },
    listWorkTasks: async () => ({ tasks: [], activeCount: 0, nextCursor: null }), listGenerationRuns: async () => ({ runs: [] }),
    listPlanningImpacts: async () => ({ data: [], nextCursor: null, counts: { total: 0, open: 0 } }),
    getWriterPreview: async () => ({ target: { position: { readingChapter: 1, sceneOrder: 1 } }, relevantText: [], plannedEffects: [], coverage: { complete: true, missingScope: [], truncationReasons: [] } }),
    saveDraftCheckpoint: async (id, payload) => { if (failNextSave) { failNextSave = false; throw Error("正文保存失败，未启动模型"); } saves.push(structuredClone(payload)); const workspace = workspaces.get(id); const chapter = workspace.chapters.find(chapter => chapter.id === payload.chapterId); chapter.candidate = { ...revision(`saved-${saves.length}`, payload.body), title: payload.title, sceneGroups: payload.sceneGroups }; workspace.revision++; return { revisionId: chapter.candidate.id }; },
    setWorkingBaseline: async (id, _, chapterId, revisionId) => { const chapter = workspaces.get(id).chapters.find(chapter => chapter.id === chapterId); chapter.workingSelection = { revisionId, selectionRevision: 1 }; chapter.working = chapter.candidate; },
    saveContinuationIntent: async () => ({}),
  },
  agent: { status: async () => ({ configured: true, state: "ready", mode: "novel-agent-engine", provider: "fixture", model: "fixture", error: null }), listConnections: async () => ({ connections: [] }), recoveryReport: async () => [], listExecutions: async () => [], readMessage: async () => { throw Error("message receipt not found"); }, beginInterlude: async () => { throw Error("unexpected interlude"); }, returnInterlude: async () => { throw Error("unexpected interlude return"); }, listMessages: async () => [], listThreads: async () => ({ threads: [], nextCursor: null }), listProposedActions: async () => ({ actions: [] }), setWorkspaceContext: async () => {}, onEvent: fn => { listener = fn; return () => {}; }, onUserInputRequest: () => () => {},
    generateChapter: async (projectId, request) => { generations.push({ projectId, ...structuredClone(request) }); activeOperation = "writer"; execution = deferred(); turn++; event("thread/started", { thread: { id: "thread" } }); event("turn/started", { threadId: "thread", turn: { id: `turn-${turn}` } }); return execution.promise; },
    steer: async (projectId, prompt) => { check(activeOperation === "advisory", "当前作品没有可追加指令的对话任务"); steers.push({ projectId, prompt }); return { steered: true }; },
    sendPrompt: async (projectId, prompt, repairSource, conversationFocus) => { const chapter = conversationFocus && workspaces.get(projectId).chapters.find(item => item.id === conversationFocus.chapterId); discussions.push({ projectId, prompt, repairSource, conversationFocus: structuredClone(conversationFocus), body: chapter?.candidate?.body ?? null }); activeOperation = delegateWriter ? "writer" : "advisory"; execution = deferred(); turn++;
      const source = { id: `author-${turn}`, projectId, threadId: "thread", operation: "advisory", target: {} };
      event("agent/workflow", { workflow: { task: source } }); event("thread/started", { thread: { id: "thread" } }); event("turn/started", { threadId: "thread", turn: { id: `turn-${turn}` } });
      if (delegateWriter) { delegatedWriters.push({ chapterId: chapter.id, intent: prompt });
        event("turn/completed", { threadId: "thread", turn: { id: `turn-${turn}`, status: "completed", items: [] } });
        event("agent/workflow", { workflow: { task: { id: `writer-${turn}`, projectId, threadId: "thread", operation: "generateChapter", target: { chapterId: chapter.id, requestedByTaskId: source.id, requestedActionId: `action-${turn}` } } } });
        event("turn/started", { threadId: "thread", turn: { id: `turn-${turn}` } }); }
      return execution.promise; },
  },
};

const calls = [], cases = [];
const conversations = new Map<string, ThreadDetail>();
let proposalCounter = 0, heldProposal = null, works = [];
const makeProposal = (projectId, prompt, taskId) => ({ proposalId: `proposal-${++proposalCounter}`, projectId, sourceTaskId: taskId, threadId: "root-thread", authorIntent: prompt, targetChapters: 2, expectedRevision: workspaces.get(projectId).revision, sourceDigest: workspaces.get(projectId).sourceDigest, baseSnapshotId: "head", state: "proposed" });
window.novelCodex.runtime.getTaskFinalReply = async () => null;
window.novelCodex.runtime.listWorkTasks = async () => ({ tasks: structuredClone(works), activeCount: 0, nextCursor: null });
window.novelCodex.agent.generateChapter = async () => { throw Error("Direct writer route must not run"); };
window.novelCodex.agent.listThreads = async (projectId, archived = false) => ({
  threads: [...conversations.values()].filter(detail => detail.thread.projectId === projectId && detail.thread.isArchived === archived)
    .map(detail => structuredClone(detail.thread)), nextCursor: null,
});
window.novelCodex.agent.readConversation = async (projectId, threadId) => {
  const detail = conversations.get(threadId);
  if (!detail || detail.thread.projectId !== projectId) throw Error("Conversation history must belong to an actually registered fixture thread");
  return structuredClone(detail);
};
window.novelCodex.agent.sendPrompt = async (projectId, prompt, repair, focus) => {
  calls.push({ kind: "send", projectId, prompt, focus }); const id = `author-${proposalCounter + 1}`;
  let detail = conversations.get("root-thread");
  if (!detail) {
    detail = { thread: { id: "root-thread", projectId, version: 0, name: prompt, preview: prompt, modelProvider: "fixture", status: "idle",
      createdAt: 1, updatedAt: 1, isPinned: false, ephemeral: false, isArchived: false, forkedFromId: null, lastTurnId: null }, turns: [] };
    conversations.set(detail.thread.id, detail);
  }
  check(detail.thread.projectId === projectId && !detail.thread.isArchived, "Send must preserve the original fixture conversation owner");
  detail.turns.push({ id: `turn-${id}`, status: "completed", startedAt: 1, completedAt: 2,
    items: [{ id: `input-${id}`, type: "userMessage", status: "completed", text: prompt, phase: null }] });
  detail.thread.lastTurnId = `turn-${id}`;
  const task = { id, projectId, threadId: "root-thread", operation: "advisory", target: {} };
  event("agent/workflow", { workflow: { task } }); event("thread/started", { thread: { id: "root-thread" } });
  event("turn/started", { threadId: "root-thread", turn: { id: `turn-${id}` } });
  event("turn/completed", { threadId: "root-thread", turn: { id: `turn-${id}`, status: "completed", items: [] } });
  heldProposal = makeProposal(projectId, prompt, id);
  return { message: "已准备两章续写安排，待你批准范围。", workflow: { task }, proposedActions: [], generationHandoff: heldProposal };
};
window.novelCodex.agent.generateWork = async (...args) => {
  calls.push({ kind: "generate", args }); execution = deferred(); return execution.promise;
};
const root = createRoot(document.getElementById("root"));
const navigate = async label => { [...document.querySelectorAll(".nav-item")].find(node => node.textContent.includes(label)).click(); await until(() => document.querySelector<HTMLElement>(".main-panel")?.dataset.view === "characters", "Characters did not open"); };
(async () => { try {
  saveAuthor({ name: "续写安排作者", email: "handoff@example.invalid", passwordSalt: "fixture", passwordHash: "fixture" }); root.render(<App />);
  await until(() => composer() && send() && !composer().disabled, "App did not initialize"); await navigate("角色");
  await input("创作指令", "接着当前人物冲突写两章，各约三千字，不发布。"); send().click();
  await until(() => button("批准生成 2 章"), "Proposal from nonchapter page was not visible");
  const approvalCard = document.querySelector<HTMLElement>(".conversation-generation-handoff");
  check(approvalCard?.innerText.includes("当前未设用量上限，可随时暂停或停止。")
    && approvalCard.innerText.includes("模型用量按实际计费，工作稿不会自动采用或发布。")
    && calls.filter(c => c.kind === "generate").length === 0, "Approval did not explain current unbounded usage before starting generation");
  check(!document.querySelector('.generation-permit-dialog') && !document.querySelector('.conversation-generation-handoff textarea'), "Proposal repeated author goal in a form");
  const draftNode = composer(); await input("创作指令", "下一轮先讨论动机"); button("暂不开始").click(); await tick();
  check(!button("批准生成 2 章") && composer() === draftNode && composer().value === "下一轮先讨论动机" && calls.filter(c => c.kind === "generate").length === 0, "Cancel consumed draft or started creation");
  cases.push({ case: "characters proposal cancel", oneCard: true, zeroGenerate: true, sameDraftDOM: true });
  await input("创作指令", "继续生成两章，保留人物冲突。"); send().click(); await until(() => button("批准生成 2 章"), "Second proposal missing");
  workspaces.get(projects[0].id).revision++; button("批准生成 2 章").click();
  await until(() => document.body.innerText.includes("作品内容已变化"), "Stale source did not surface");
  check(calls.filter(c => c.kind === "generate").length === 0, "Stale proposal called generateWork");
  cases.push({ case: "stale proposal", generateCalls: 0, sourceErrorVisible: true }); button("暂不开始").click(); await tick();
  await input("创作指令", "以当前内容写两章，不发布。"); send().click(); await until(() => button("批准生成 2 章"), "Fresh proposal missing");
  const goal = calls.filter(c => c.kind === "send").at(-1).prompt;
  await input("创作指令", "创作过程中保留的下一条要求"); const sameDraft = composer();
  button("批准生成 2 章").click(); button("批准生成 2 章")?.click();
  await until(() => calls.filter(c => c.kind === "generate").length === 1, "Approved proposal did not create one run");
  const args = calls.find(c => c.kind === "generate").args;
  check(args[0] === projects[0].id && args[1] === goal && args[2] === 2 && args[7] === "proposal-3", "Approval lost original goal/count/proposal identity");
  check(composer() === sameDraft && composer().value === "创作过程中保留的下一条要求", "Approval lost next draft");
  event("turn/started", { threadId: "root-thread", turn: { id: "generation-turn" } });
  event("turn/completed", { threadId: "root-thread", turn: { id: "generation-turn", status: "completed", items: [] } });

  const attempt = { id: "author-3", threadId: "root-thread", turnId: "turn-author-3", status: "completed", error: null, startedAt: 1, completedAt: 2, changes: [] };
  const work = (id, kind, target, attemptId) => ({ id, kind, projectId: projects[0].id, threadId: "root-thread", goal, version: 1,
    status: "completed", error: null, deliveryStatus: "discussion_ended", deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], evidence: [], pendingActionIds: [], unsettledAttemptIds: [] },
    resumeMode: "none", resumeBlockReason: null, steps: [{ id: `step-${id}`, operation: "advisory", expectedOutputKind: "completionReport", status: "completed", target,
      sourceTaskId: null, outputRefs: [], currentAttemptId: attemptId, attempts: [{ ...attempt, id: attemptId }], changes: [], readingProgress: null }] });
  works = [work("author-work", "conversation", {}, attempt.id), work("generation-work", "generation", { workDecision: { workTaskId: "generation-work", originTaskId: attempt.id } }, "decision-task")];
  execution.resolve({ status: "paused", blockedReason: "仅验证精确许可传递" });
  await until(() => !document.querySelector('[aria-label="停止当前执行"]'), "Generation handoff did not settle");
  check(composer().value === "创作过程中保留的下一条要求", "Completion lost draft");
  await until(() => document.querySelectorAll('[data-work-task-id]').length === 2, "Original and generation Work did not load");
  check(document.querySelectorAll('[data-work-task-id] > .user-message').length === 1, "One author goal was duplicated after generation approval");
  cases.push({ case: "exact scope approval", goalUnchanged: true, targetChapters: args[2], proposalId: args[7], generateCalls: 1, draftPreserved: true, authorMessages: 1 });
  await input("创作指令", "按当前故事继续写两章，不发布。"); send().click(); await until(() => button("批准生成 2 章"), "Pre-task failure proposal missing");
  const initialProposal = structuredClone(heldProposal);
  const originalAuthorTurn = conversations.get(initialProposal.threadId).turns.find(item => item.id === `turn-${initialProposal.sourceTaskId}`);
  check(originalAuthorTurn?.items.some(item => item.type === "userMessage" && item.text === initialProposal.authorIntent), "Initial proposal lost its actual original author turn");
  await input("创作指令", "下一轮还未发送的修改要求"); const pendingEditor = composer(); pendingEditor.setSelectionRange(1, 5);
  const beforeUnbound = document.querySelectorAll('[data-conversation-row^="turn:"]').length;
  button("批准生成 2 章").click(); await until(() => calls.filter(c => c.kind === "generate").length === 2, "Original proposal did not begin start request");
  check(document.querySelectorAll('[data-conversation-row^="turn:"]').length === beforeUnbound, "Proposal approval invented a second author turn before a real task");
  const originalAuthorWork: WorkTaskSummary = { id: `work-${initialProposal.sourceTaskId}`, projectId: initialProposal.projectId,
    threadId: initialProposal.threadId, goal: initialProposal.authorIntent, kind: "conversation", version: 1,
    status: "completed", error: null, deliveryStatus: "discussion_ended", deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], evidence: [], pendingActionIds: [], unsettledAttemptIds: [] },
    resumeMode: null, resumeBlockReason: null, pendingContinuation: false,
    steps: [{ id: `step-${initialProposal.sourceTaskId}`, operation: "advisory", expectedOutputKind: "completionReport", instruction: initialProposal.authorIntent,
      sourceTaskId: null, target: {}, status: "completed", inputRevision: initialProposal.expectedRevision, dependsOnStepId: null,
      currentAttemptId: initialProposal.sourceTaskId, readingProgress: null, outputRefs: [], changes: [],
      attempts: [{ id: initialProposal.sourceTaskId, attemptNumber: 1, threadId: initialProposal.threadId, turnId: originalAuthorTurn.id,
        status: "completed", error: null, startedAt: originalAuthorTurn.startedAt, completedAt: originalAuthorTurn.completedAt }] }] };
  const pausedWork: WorkTaskSummary = { ...originalAuthorWork, id: "work-generation-not-started", kind: "generation", status: "paused", deliveryStatus: "pending",
    resumeMode: "continueExecution", steps: [{ ...originalAuthorWork.steps[0], id: "step-generation-not-started", status: "queued", currentAttemptId: null, attempts: [],
      target: { workDecision: { workTaskId: "work-generation-not-started", originTaskId: initialProposal.sourceTaskId, sourceTaskId: null } } }] };
  works.push(originalAuthorWork, pausedWork);
  execution.resolve({ id: "generation-not-started", status: pausedWork.status, resumeFailure: { code: "WORK_RESUME_NOT_STARTED", message: "MCP 首次启动超时", workTaskId: pausedWork.id },
    pausedWork: { id: pausedWork.id, projectId: pausedWork.projectId, threadId: pausedWork.threadId, status: pausedWork.status, version: pausedWork.version } });
  await until(() => document.body.innerText.includes("原任务尚未启动：MCP 首次启动超时") && !document.querySelector('[aria-label="停止当前执行"]'), "Initial generation pre-task proof was treated as success or stayed busy");
  check(composer() === pendingEditor && composer().value === "下一轮还未发送的修改要求" && pendingEditor.selectionStart === 1 && pendingEditor.selectionEnd === 5, "Initial pre-task failure discarded next draft or caret");
  check(pausedWork.steps[0].status === "queued" && pausedWork.steps[0].currentAttemptId === null && !pausedWork.steps[0].attempts.length
    && !document.querySelector('.conversation-final-reply-error') && document.querySelectorAll('[data-work-task-id] > .user-message').length === 2,
    "Initial pre-task failure invented an attempt, reply ownership error, or duplicate author goal");
  check(!button("批准生成 2 章") && calls.filter(c => c.kind === "generate").length === 2, "Consumed permit was restored or silently replayed");
  cases.push({ case: "initial generation host-proven not started", localError: "原任务尚未启动：MCP 首次启动超时", duplicateAuthorTurn: false, consumedPermitRestored: false, draftDOMAndCaretPreserved: true, automaticReplay: false });
  const queuedMessage = { id: "queued-handoff", projectId: projects[0].id, threadId: "root-thread", input: { text: "排队续写两章，不发布。" } };
  event("agent/message-started", { message: queuedMessage }); await tick();
  const queuedTask = { id: "queued-task", projectId: queuedMessage.projectId, threadId: queuedMessage.threadId, operation: "advisory", target: {} };
  event("agent/workflow", { workflow: { task: queuedTask } });
  event("turn/started", { threadId: "root-thread", turn: { id: "queued-turn" } }); await tick();
  event("turn/completed", { threadId: "root-thread", turn: { id: "queued-turn", status: "completed", items: [] } }); await tick();
  const queuedProposal = makeProposal(queuedMessage.projectId, queuedMessage.input.text, queuedTask.id);
  const ended = { projectId: queuedMessage.projectId, threadId: queuedMessage.threadId, messageId: queuedMessage.id, error: null };
  for (const changed of [{ sourceTaskId: "foreign-task" }, { threadId: "foreign-thread" }, { projectId: "foreign-project" }, { authorIntent: "different intent" }, { resumeRun: { id: "run", expectedVersion: 1 } }]) {
    event("agent/message-ended", { ...ended, generationHandoff: { ...queuedProposal, ...changed } }); await tick();
    check(!button("批准生成 2 章"), "Foreign queued completion exposed an approval");
  }
  event("agent/message-ended", { ...ended, generationHandoff: queuedProposal });
  await until(() => button("批准生成 2 章"), "Successful queued completion lost its Native proposal");
  check(calls.filter(c => c.kind === "generate").length === 2, "Queued completion granted approval itself");
  event("agent/message-ended", { ...ended, error: "execution failed", generationHandoff: queuedProposal }); await tick();
  check(!button("批准生成 2 章"), "Failed queued completion exposed an approval");
  for (const theme of ["light", "dark"]) { document.documentElement.dataset.theme = theme; await tick(); check(document.body.scrollWidth <= innerWidth + 2, "Handoff overflows viewport"); }
  window.testResult = { passed: true, cases, calls: calls.map(c => ({ kind: c.kind, ...(c.kind === "generate" ? { count: c.args[2], proposalId: c.args[7] } : { focus: c.focus }) })) };
} catch (error) { window.testResult = { passed: false, error: error.stack, calls, dom: document.body.innerText }; } })();
