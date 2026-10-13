import { novelFixtureMedia } from "../helpers/novel-edition";
import { emptyCreationIssueSummary } from "../helpers/creation-issues";
import React from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import "../../../src/styles.css";
import { saveAuthor } from "../helpers/author-session";
import { chapterRuntimeReads } from "../helpers/chapter-runtime";

const tick = () => new Promise(resolve => setTimeout(resolve, 20));
let renderError;
let phase = "startup";
window.addEventListener("error", event => { renderError = event.error ?? event.message; });
const check = (value, message) => { if (!value) throw Error(message); };
const until = async (predicate, message) => {
  for (let index = 0; index < 220 && !predicate(); index++) { if (renderError) throw renderError; await tick(); }
  check(predicate(), message);
};
const buttons = label => [...document.querySelectorAll<HTMLButtonElement>("button")].filter(node => node.textContent === label || node.getAttribute("aria-label") === label);
const click = async label => {
  phase = label;
  await until(() => buttons(label)[0] && !buttons(label)[0].disabled, "Unavailable " + label);
  buttons(label)[0].click(); await tick(); if (renderError) throw renderError;
};
const composer = () => document.querySelector<HTMLTextAreaElement>('[aria-label="创作指令"]');
const change = async (element, value) => {
  check(element, "Missing input");
  const prototype = element.tagName === "TEXTAREA" ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  Object.getOwnPropertyDescriptor(prototype, "value").set.call(element, value);
  element.dispatchEvent(new Event("input", { bubbles: true })); await tick();
};
const input = async value => change(composer(), value);
const cards = () => [...document.querySelectorAll(".discussion-excerpt")].filter(node => !node.closest(".conversation-turn"));
const hasIntentPicker = () => Boolean(document.querySelector('[aria-label="发送设置"], .composer-intent-picker'));
const transcript = () => document.querySelector<HTMLElement>(".conversation-rail")?.innerText ?? "";
const row = id => document.querySelector<HTMLElement>(`[data-conversation-row="${id}"]`);
const select = async thread => {
  await click("切换对话：" + thread.name);
  await until(() => row(thread.id)?.querySelector('[aria-current="true"]'), "Conversation selected " + thread.id);
  await tick(); await tick();
};
const quote = async (element, start = 0, end = element.value.length) => {
  element.focus(); element.setSelectionRange(start, end); element.dispatchEvent(new Event("select", { bubbles: true }));
  await until(() => document.querySelector(".selection-discussion-entry button"), "Selection entry");
  document.querySelector<HTMLButtonElement>(".selection-discussion-entry button")!.click(); await tick();
};
const projects = [{ id: "project-last-lamp", title: "末灯", logline: "", updated_at: "" }];
const projectFor = id => projects.find(project => project.id === id);
const chapter = { id: "chapter-01", number: 1, title: "旧钟", status: "draft", summary: "", updated_at: "", passages: [] };
const reads = chapterRuntimeReads(projectFor);
const workspace = async id => ({
  ...await reads.getCreativeWorkspace(id), headSnapshotId: "snapshot-multi",
  chapters: [{ ...chapter, candidate: { id: "draft-multi", title: "旧钟", version: 2, body: "守钟人听见远处的旧钟。", sourcePlanRefs: { localOnly: true, lineageStatus: "current" }, sceneGroups: [{ id: "scene-one", body: "守钟人听见远处的旧钟。", locked: false }] }, accepted: null, working: null, workingSelection: null }],
});
const calls = { sends: [], creates: [], reads: [], resumes: [], writes: 0, interrupts: 0, metadata: [], taskReads: 0 };
let failHeld = false;
let onEvent, failNext = true, failCreate = false, holdNext = false, releaseHeld, showRecordedTasks = false;
const histories = new Map();
const messages = new Map(); const messageWorks = new Map(); let hostSession = "host-a", queuedRequestId = null;
const drainQueue = async () => {
  const item = [...messages.values()].find(item => item.sessionId === hostSession && item.status === "queued" && messageWorks.get(item.trigger.workTaskId)?.status === "completed");
  if (!item) return;
  item.status = "claimed"; queuedRequestId = item.id;
  onEvent?.({ method: "agent/message-started", params: { message: structuredClone(item) } });
  try {
    await window.novelCodex.agent.sendPrompt(item.projectId, item.input.text, null, item.input.focus, item.threadId);
    item.status = "delivered";
  } finally { queuedRequestId = null; onEvent?.({ method: "agent/message-ended", params: { projectId: item.projectId, threadId: item.threadId, messageId: item.id, error: null } }); }
};

const makeThread = (id, name, options = {}) => ({ id, projectId: projects[0].id, name, preview: null, modelProvider: "fixture", status: "idle", createdAt: 1, updatedAt: 1, version: 0, isPinned: false, isArchived: false, ephemeral: false, forkedFromId: null, lastTurnId: null, ...options });
const threadA = makeThread("thread-a", "大纲讨论");
const threads = new Map([[threadA.id, threadA]]);
const recordedTask = (threadId, label) => ({ id: "recorded-" + threadId, projectId: projects[0].id, threadId,
  goal: label + " 独占任务记录", kind: "conversation", status: "completed", deliveryStatus: "discussion_ended", version: 0, error: null,
  deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], evidence: [], pendingActionIds: [], unsettledAttemptIds: [] }, resumeBlockReason: null, resumeMode: null, steps: [] });
const getThread = (projectId, id) => {
  const thread = threads.get(id); check(thread && thread.projectId === projectId, "Thread crossed project boundary"); return thread;
};
const updateThread = (projectId, id, updates) => {
  const thread = getThread(projectId, id); Object.assign(thread, updates, { version: thread.version + 1, updatedAt: thread.updatedAt + 1 });
  calls.metadata.push(structuredClone(thread)); return { thread: structuredClone(thread) };
};
window.novelCodex = { multimedia: novelFixtureMedia,
  app: { onCloseRequested: () => () => {} },
  runtime: {
    ...reads, status: async () => ({ state: "online", mode: "local", error: null }), listProjects: async () => ({ projects }), getCreativeWorkspace: workspace,
    listWorkTasks: async () => {
      check(++calls.taskReads <= 100, "Unbounded task refresh during " + phase);
      return { tasks: showRecordedTasks ? [...threads.values()].filter(thread => ["thread-a", "thread-new-1"].includes(thread.id)).map(thread => recordedTask(thread.id, thread.id === "thread-a" ? "A" : "B")) : [...messageWorks.values()].map(x => structuredClone(x)), nextCursor: null, activeCount: 0 };
    },
    getState: async id => ({ project: projectFor(id), canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1", scenes: [], anchors: [] }, novelEdition: { chapters: [chapter] } }),
    getWriterPreview: async () => ({ target: { position: {} }, plannedEffects: [], relevantText: [], coverage: { complete: true, missingScope: [], truncationReasons: [] } }),
    saveDraftCheckpoint: async () => { calls.writes++; throw Error("Unexpected draft save"); },
    saveArtifact: async () => { calls.writes++; throw Error("Unexpected planning save"); },
    saveContinuationIntent: async () => { calls.writes++; throw Error("Unexpected continuation write"); },
    establishCommitment: async () => { calls.writes++; throw Error("Unexpected write"); },
    createCommitmentReview: async () => { calls.writes++; throw Error("Unexpected write"); },
    acceptReview: async () => { calls.writes++; throw Error("Unexpected write"); },
  },
  agent: {
    listExecutions: async () => [], readMessage: async (projectId, threadId, id) => { const item = messages.get(id); check(item?.projectId === projectId && item.threadId === threadId, "Receipt ownership mismatch"); return structuredClone(item); }, beginInterlude: async () => { throw Error("unexpected interlude"); }, returnInterlude: async () => { throw Error("unexpected interlude return"); }, listMessages: async (projectId, threadId) => [...messages.values()].filter(x => x.projectId === projectId && x.threadId === threadId && !["delivered", "cancelled"].includes(x.status)).map(x => ({ ...structuredClone(x), requiresContinue: x.trigger.kind !== "steer" && x.sessionId !== hostSession })),
    submitMessage: async (projectId, threadId, request) => {
      check(request.trigger.kind === "afterWork", "Queue must bind the whole Work");
      const item = { ...structuredClone(request), id: request.messageId, projectId, threadId, status: "queued", sessionId: hostSession, revision: 0 };
      messages.set(item.id, item); return structuredClone(item);
    },
    changeMessage: async (_project, _thread, id, change) => {
      const item = messages.get(id); check(item && item.revision === change.expectedRevision, "Stale message changed");
      if (change.action === "cancel") item.status = "cancelled";
      if (change.action === "edit") item.input = structuredClone(change.input);
      if (change.action === "continue") { item.sessionId = hostSession; setTimeout(() => { void drainQueue(); }, 50); }
      item.revision++; return structuredClone(item);
    },
    status: async () => ({ configured: true, state: "ready", mode: "novel-agent-engine", provider: "fixture", model: "fixture", error: null }),
    listConnections: async () => ({ connections: [] }), recoveryReport: async () => [{ runId: "pending-save", planId: "pending-plan", threadId: "thread-a", projectId: projects[0].id, operation: "generateChapter", runtimeStatus: "failed", reconciledAt: null, requests: [{ requestId: "pending-write", turnId: "pending-turn", kind: "write", operation: "save_draft_checkpoint", state: "accepted", receiptRef: null, hasIdempotencyKey: true, recovery: "manual-reconciliation" }] }], listProposedActions: async () => ({ actions: [] }),
    setWorkspaceContext: async () => {}, onEvent: listener => { onEvent = listener; return () => {}; }, onUserInputRequest: () => () => {},
    listThreads: async (projectId, archived = false) => ({ threads: [...threads.values()].filter(thread => thread.projectId === projectId && thread.isArchived === archived).map(thread => structuredClone(thread)), nextCursor: null }),
    createThread: async (projectId, options = {}) => {
      if (failCreate) { failCreate = false; throw Error("新建对话可重试失败"); }
      check(projectId === projects[0].id, "Create crossed project"); calls.creates.push(structuredClone({ projectId, options }));
      const thread = makeThread("thread-new-" + calls.creates.length, "新对话 " + calls.creates.length, { forkedFromId: options.sourceThreadId ?? null, lastTurnId: options.lastTurnId ?? null });
      const sourceTurns = options.sourceThreadId ? histories.get(getThread(projectId, options.sourceThreadId).id) ?? [] : [];
      const lastTurnIndex = options.lastTurnId ? sourceTurns.findIndex(turn => turn.id === options.lastTurnId) : -1;
      check(!options.sourceThreadId || lastTurnIndex >= 0, "Fork needs a real source turn");
      threads.set(thread.id, thread); histories.set(thread.id, structuredClone(sourceTurns.slice(0, lastTurnIndex + 1))); return { thread: structuredClone(thread) };
    },
    renameThread: async (projectId, id, name) => updateThread(projectId, id, { name }),
    resumeThread: async (projectId, id) => { calls.resumes.push(id); return { thread: structuredClone(getThread(projectId, id)) }; },
    readThread: async (projectId, id) => { calls.reads.push(id); return { thread: structuredClone(getThread(projectId, id)), turns: structuredClone(histories.get(id) ?? []) }; },
    readConversation: async (projectId, id) => { calls.reads.push(id); return { thread: structuredClone(getThread(projectId, id)), turns: structuredClone(histories.get(id) ?? []) }; },
    setThreadPinned: async (projectId, id, isPinned) => updateThread(projectId, id, { isPinned }),
    archiveThread: async (projectId, id) => { updateThread(projectId, id, { isArchived: true }); return { archived: true }; },
    unarchiveThread: async (projectId, id) => updateThread(projectId, id, { isArchived: false }),
    interruptTurn: async () => { calls.interrupts++; return {}; },
    stop: async () => { calls.interrupts++; return {}; },
    sendPrompt: async (...args) => {
      calls.sends.push(structuredClone(args)); const [projectId, prompt, , focus, requestedThreadId] = args; const threadId = requestedThreadId ?? threadA.id;
      const thread = getThread(projectId, threadId); check(!thread.isArchived, "Sent to archived thread");
      const requestId = queuedRequestId ?? "request-" + calls.sends.length;
      if (!queuedRequestId) onEvent?.({ method: "agent/execution-request", params: { projectId, requestId } });
      onEvent?.({ method: "agent/message-updated", params: { projectId, threadId, messageId: requestId, status: "queued" } });
      const turnId = "turn-" + calls.sends.length;
      const work = { ...recordedTask(threadId, prompt), id: "work-author-" + turnId, controlRevision: 0, status: "running",
        steps: [{ changes: [], outputRefs: [], readingProgress: null, ordinal: 1, title: "解释设定", id: "step-" + turnId, status: "running", target: {}, operation: "advisory", currentAttemptId: "author-" + turnId,
          attempts: [{ id: "author-" + turnId, threadId, turnId, status: "running" }] }] };
      messageWorks.set(work.id, work);
      const turns = histories.get(threadId) ?? []; histories.set(threadId, turns);
      const turn = { id: turnId, status: "inProgress", startedAt: 1, completedAt: null, items: [{ id: turnId + "-author", type: "userMessage", status: "completed", text: prompt, phase: null }] };
      turns.push(turn); updateThread(projectId, threadId, { lastTurnId: turnId, preview: prompt, status: "active" });
      onEvent?.({ method: "agent/workflow", params: { workflow: { task: { id: "author-" + turnId, projectId, operation: "advisory", threadId } } } });
      onEvent?.({ method: "turn/started", params: { threadId, turnId, turn: { id: turnId, status: "inProgress" } } });
      const progress = { id: turnId + "-progress", type: "agentMessage", status: "completed", text: "我先对照现有正文，再围绕你的问题比较可行的方向。", phase: "commentary" };
      turn.items.push(progress); onEvent?.({ method: "item/completed", params: { threadId, turnId, item: progress } });
      if (failNext) {
        failNext = false; turn.status = "failed"; updateThread(projectId, threadId, { status: "idle" });
        onEvent?.({ method: "turn/completed", params: { threadId, turnId, turn: { id: turnId, status: "failed", error: { message: "A 的可重试失败" }, items: [] } } });
        throw Error("A 的可重试失败");
      }
      if (holdNext) { holdNext = false; await new Promise(resolve => { releaseHeld = resolve; }); }
      if (failHeld) { work.status = "blocked"; failHeld = false; onEvent?.({ method: "turn/completed", params: { threadId, turn: { id: turnId, status: "failed", error: { message: "受控失败" } } } }); throw Error("受控失败"); }
      const message = "仅属于 " + threadId + " 的结果 " + turnId;
      turn.status = "completed"; turn.completedAt = 2; turn.items.push({ id: turnId + "-assistant", type: "agentMessage", status: "completed", text: message, phase: "final_answer" });
      updateThread(projectId, threadId, { status: "idle" });
      onEvent?.({ method: "item/completed", params: { threadId, turnId, item: { id: turnId + "-assistant", type: "agentMessage", text: message, phase: "final_answer" } } });
      onEvent?.({ method: "turn/completed", params: { threadId, turnId, turn: { id: turnId, status: "completed", items: turn.items } } });
      work.status = "completed"; work.steps[0].attempts[0].status = "completed";
      if (!queuedRequestId) setTimeout(() => { void drainQueue(); }, 50);
      return { mode: "advisory", projectId, message, proposedActions: [], candidateResults: [] };
    },
  },
};
let root = createRoot(document.getElementById("root"));
(async () => { try {
  localStorage.clear(); saveAuthor({ name: "对话体验作者", email: "multi@example.invalid", passwordSalt: "fixture", passwordHash: "fixture" }); root.render(<App />);
  await until(() => composer() && row(threadA.id), "Startup");
  check(!document.querySelector('.conversation-find, .conversation-compose-options, .conversation-recovery, .conversation-outbox'), "Default conversation exposed auxiliary toolbars or historical audit rows");
  check(!buttons("当前任务结束后发送").length && !hasIntentPicker(), "Default input exposed a queue action or lost default intent");
  window.testScreenshot = "conversation-default"; await until(() => !window.testScreenshot, "Default screenshot");
  await click("对话操作");
  check(document.activeElement?.getAttribute("role") === "menuitem", "Conversation menu did not receive keyboard focus");
  document.activeElement.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); await tick();
  check(!document.querySelector('[role="menu"]') && document.activeElement?.getAttribute("aria-label") === "对话操作", "Menu Escape lost focus");
  failNext = false; holdNext = true;
  await input("先讨论第一种方案"); await click("发送创作指令"); await until(() => calls.sends.length === 1 && releaseHeld, "Running");
  check(calls.sends[0][3].readOnly !== true, "Natural author wording was converted into a hidden execution mode");
  check(!hasIntentPicker() && document.querySelector('.conversation-rail').getAttribute('data-conversation-id') === threadA.id,
    "First turn introduced a semantic mode while creating its conversation");
  await input("再讨论第二种方案");
  const narrowPane = document.querySelector<HTMLElement>(".conversation-rail"), initialPaneStyle = narrowPane.style.cssText;
  narrowPane.style.width = "300px"; narrowPane.style.minWidth = "300px"; narrowPane.style.maxWidth = "300px"; await tick();
  const narrowComposer = document.querySelector<HTMLElement>(".composer-card"), narrowHeader = document.querySelector<HTMLElement>(".conversation-shell-header");
  check(narrowComposer.scrollWidth <= narrowComposer.clientWidth + 2 && narrowHeader.scrollWidth <= narrowHeader.clientWidth + 2, "Input choices or header overflowed a narrow conversation");
  window.testScreenshot = "conversation-queue-narrow"; await until(() => !window.testScreenshot, "Narrow screenshot");
  narrowPane.style.cssText = initialPaneStyle; await tick();
  await click("当前任务结束后发送");
  check(!composer().value && document.querySelector('.conversation-outbox')?.textContent.includes("再讨论第二种方案"), "Queue lost text");
  check(calls.sends.length === 1, "Queue sent while current turn running");
  document.querySelector<HTMLDetailsElement>('.conversation-outbox').open = true;
  await click("编辑");
  await change(document.querySelector('.conversation-outbox textarea'), "尚未保存的编辑草稿");
  [...messages.values()][0].revision++;
  await until(() => document.querySelector('.conversation-outbox [role="alert"]') && buttons("保存修改")[0]?.disabled, "Concurrent revision must fence the open editor");
  check(document.querySelector<HTMLTextAreaElement>('.conversation-outbox textarea').value === "尚未保存的编辑草稿", "Concurrent revision discarded the editing draft");
  await click("取消编辑"); await click("编辑");
  await change(document.querySelector('.conversation-outbox textarea'), "再讨论第二种方案，保留灯塔背景");
  await click("保存修改");
  await until(() => document.querySelector('.conversation-outbox')?.textContent.includes("保留灯塔背景") && !document.querySelector('.conversation-outbox textarea'), "Queue edit did not persist");
  window.testScreenshot = "conversation-queue"; await until(() => !window.testScreenshot, "Screenshot");
  releaseHeld(); await until(() => calls.sends.length === 2 && transcript().includes("结果 turn-2"), "Queue starts once after success");
  check(calls.sends[1][3].readOnly !== true && calls.sends[1][4] === threadA.id && calls.sends[1][1] === "再讨论第二种方案，保留灯塔背景", "Queue lost the original language or thread");
  await until(() => !document.querySelector('.conversation-outbox'), "Accepted queue still displayed");
  const delivered = [...messages.values()][0];
  const unconfirmedKey = `novel-codex:message-submission:${JSON.stringify([projects[0].id, threadA.id])}`;
  localStorage.setItem(unconfirmedKey, JSON.stringify({messageId: delivered.id, body: {input: delivered.input, trigger: delivered.trigger}}));
  await until(() => !localStorage.getItem(unconfirmedKey), "Delivered receipt did not resolve the lost submission acknowledgement");
  check(calls.sends.length === 2, "Receipt recovery replayed paid execution");
  await click("对话操作"); await click("查找消息"); const search = document.querySelector<HTMLInputElement>('[aria-label="查找当前对话消息"]'); await change(search, "仅属于");
  search.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); await tick();
  check(document.activeElement?.textContent?.includes("仅属于"), "Search did not focus match");
  await click("关闭查找");
  check(!document.querySelector('.conversation-find') && document.activeElement?.getAttribute("aria-label") === "对话操作", "Closing search kept its toolbar or lost focus");
  composer().focus(); composer().dispatchEvent(new KeyboardEvent("keydown", { key: "f", ctrlKey: true, bubbles: true })); await tick();
  check(document.activeElement?.getAttribute("aria-label") === "查找当前对话消息", "Conversation find shortcut did not focus search");
  document.activeElement.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); await tick();
  check(!document.querySelector('.conversation-find'), "Search Escape did not restore the quiet reading layout");
  holdNext = true; await input("第三轮"); await click("发送创作指令"); await until(() => calls.sends.length === 3, "Third running");
  await input("重开后手动发送"); await click("当前任务结束后发送");
  hostSession = "host-after-restart"; root.unmount(); await tick(); releaseHeld(); await tick(); root = createRoot(document.getElementById("root")); root.render(<App />);
  await until(() => composer() && document.querySelector('.conversation-outbox'), "Queue restored"); await tick(); await tick();
  check(calls.sends.length === 3, "Reload automatically sent paid request");
  check(!hasIntentPicker(), "Mode not restored");
  document.querySelector<HTMLDetailsElement>('.conversation-outbox').querySelector<HTMLElement>("summary").click(); await tick();
  await click("继续等待"); await until(() => calls.sends.length === 4 && !document.querySelector('.conversation-outbox'), "Manual restored send");
  await until(() => document.querySelector<HTMLButtonElement>('[aria-label="发送创作指令"]'), "Ready after manual send");
  holdNext = true; failHeld = true; await input("会失败的轮次"); await click("发送创作指令"); await until(() => calls.sends.length === 5, "Failed case running");
  await input("失败后保留的下一条"); await click("当前任务结束后发送"); releaseHeld();
  await until(() => document.querySelector('.transcript-error'), "Failure visible"); await tick(); await tick();
  check(calls.sends.length === 5 && document.querySelector('.conversation-outbox')?.textContent.includes("失败后保留的下一条"), "Failure replayed queued model work");
  document.querySelector<HTMLDetailsElement>('.conversation-outbox').querySelector<HTMLElement>("summary").click(); await tick();
  await click("移除"); await until(() => !document.querySelector('.conversation-outbox'), "Remove did not persist");
  const resultTask = { ...recordedTask(threadA.id, "已经保存的正文"), deliveryStatus: "ready_for_review",
    steps: [{ id: "result-step", operation: "generateChapter", status: "completed", currentAttemptId: "result-attempt", target: { chapterId: chapter.id, chapterNumber: 1 },
      attempts: [{ id: "result-attempt", threadId: "result-worker", turnId: "result-turn", status: "completed" }], changes: [], outputRefs: [], readingProgress: null }],
    deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], evidence: [{ stepId: "result-step", attemptId: "result-attempt", kind: "draft", state: "recorded", referenceId: "draft-multi" }], pendingActionIds: [], unsettledAttemptIds: [] } };
  window.novelCodex.runtime.listWorkTasks = async () => ({ tasks: [resultTask], activeCount: 0, nextCursor: null });
  window.novelCodex.runtime.getWorkTask = async () => structuredClone(resultTask);
  root.unmount(); await tick(); root = createRoot(document.getElementById("root")); root.render(<App />);
  await until(() => document.querySelector('.task-result-card') && composer(), "Persisted chapter result");
  await input("保留这条未发送的问题");
  const resultCard = document.querySelector('.task-result-card');
  check(resultCard.querySelectorAll("button").length === 1 && resultCard.textContent.includes("查看第 1 章正文")
    && !/继续讨论|引用到输入框/.test(resultCard.textContent), "Result duplicated the composer or lost its view action");
  check(cards().length === 0 && composer().value === "保留这条未发送的问题", "Result display injected context or overwrote the draft");
  check(!hasIntentPicker() && calls.sends.length === 5, "Result display created a mode or paid work");
  await click("查看第 1 章正文");
  await until(() => document.querySelector('.main-panel')?.getAttribute('data-view') === "chapters", "Result view did not open");
  check(composer().value === "保留这条未发送的问题" && cards().length === 0, "Viewing a result replaced the draft or implicitly quoted content");
  const followup = "这版的结尾还可以更自然吗？";
  await input(followup); await click("发送创作指令");
  await until(() => calls.sends.length === 6 && transcript().includes("结果 turn-6") && !buttons("停止当前执行").length, "Direct result follow-up did not finish");
  const sent = calls.sends.at(-1);
  check(sent[0] === projects[0].id && sent[1] === followup && sent[4] === threadA.id,
    "Result follow-up lost original language or selected conversation");
  check(sent[2] === null && !sent[3]?.excerpts && !sent[3]?.reference && !sent[3]?.readOnly
    && cards().length === 0 && !hasIntentPicker() && calls.writes === 0,
    "Direct follow-up invented a reference, mode, repair source or author write");
  check(histories.get(threadA.id).some(turn => turn.id === "turn-2") && histories.get(threadA.id).at(-1).id === "turn-6",
    "Follow-up replaced previous conversation history");
  messages.set("pending-steer",{id:"pending-steer",projectId:projects[0].id,threadId:threadA.id,status:"sending",sessionId:"author-intent",revision:0,input:{text:"已接收但待核对的追加要求",focus:null,connectionId:null},trigger:{kind:"steer",taskId:"original-task"},error:"追加指令交付尚未确认，不会自动重发"});
  await until(()=>document.querySelector('.conversation-outbox')?.textContent.includes("本轮追加指令"),"Pending steer receipt was not restored in the conversation");
  const steerCard=document.querySelector('.conversation-outbox article');
  check(steerCard.querySelectorAll('button').length===1 && steerCard.querySelector('button').textContent==="核对交付","Steer projection exposed queue mutations");
  check(calls.sends.length===6,"Reading a pending steer triggered another paid request");
  messages.set("pending-steer",{...messages.get("pending-steer"),status:"failed",error:"追加要求尚未发送，已保留；继续原任务时会一并处理"});
  await until(()=>document.querySelector('.conversation-outbox')?.textContent.includes("尚未发送"),"Proven unsent receipt was not projected");
  check(!document.querySelector('.conversation-outbox')?.textContent.includes("待核对："),"Proven unsent input was mislabeled as unknown");
  check(calls.sends.length===6,"Unsent receipt was automatically replayed");
  window.testResult = { passed: true, sends: calls.sends.length };
} catch (error) { window.testResult = { passed: false, error: phase + ": " + error.stack, dom: document.body.textContent.slice(-6000) }; } })();
