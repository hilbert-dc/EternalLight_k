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
  buttons(label)[0].click(); await tick();
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
const editContexts = [];
const calls = { sends: [], creates: [], reads: [], resumes: [], writes: 0, interrupts: 0, metadata: [], taskReads: 0 };
let onEvent, failNext = true, failCreate = false, holdNext = false, releaseHeld, showRecordedTasks = false;
const histories = new Map();
const activeExecutions = new Map();
let snapshotRace=false;
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
      return { tasks: showRecordedTasks ? [...threads.values()].filter(thread => ["thread-a", "thread-new-1"].includes(thread.id)).map(thread => recordedTask(thread.id, thread.id === "thread-a" ? "A" : "B")) : [], nextCursor: null, activeCount: 0 };
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
    status: async () => ({ configured: true, state: "ready", mode: "novel-agent-engine", provider: "fixture", model: "fixture", error: null }),
    listConnections: async () => ({ connections: [] }), recoveryReport: async () => [{ runId: "pending-save", planId: "pending-plan", threadId: "thread-a", projectId: projects[0].id, operation: "generateChapter", runtimeStatus: "failed", reconciledAt: null, requests: [{ requestId: "pending-write", turnId: "pending-turn", kind: "write", operation: "save_draft_checkpoint", state: "accepted", receiptRef: null, hasIdempotencyKey: true, recovery: "manual-reconciliation" }] }], listProposedActions: async () => ({ actions: [] }),
    setWorkspaceContext: async context => { editContexts.push(context); }, onEvent: listener => { onEvent = listener; return () => {}; }, onUserInputRequest: () => () => {},
    listExecutions: async projectId => {
      const snapshots=structuredClone([...activeExecutions.values()].filter(item=>item.projectId===projectId));
      if(snapshotRace && snapshots.length){snapshotRace=false;await new Promise(resolve=>setTimeout(resolve,25));
        const current=activeExecutions.get(snapshots[0].executionId);
        const event={method:"item/agentMessage/delta",params:{threadId:current.threadId,turnId:current.turnId,itemId:"snapshot-race",delta:"读取快照期间的新进度"},
          hostExecution:{id:current.executionId,requestId:current.requestId,projectId,threadId:current.threadId,sequence:++current.cursor}};
        current.events.push(event);onEvent?.(event);
      }
      return snapshots;
    }, readMessage: async () => { throw Error("message receipt not found"); }, beginInterlude: async () => { throw Error("unexpected interlude"); }, returnInterlude: async () => { throw Error("unexpected interlude return"); }, listMessages: async () => [], listThreads: async (projectId, archived = false) => ({ threads: [...threads.values()].filter(thread => thread.projectId === projectId && thread.isArchived === archived).map(thread => structuredClone(thread)), nextCursor: null }),
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
      calls.sends.push(structuredClone(args)); const [projectId, prompt, , focus, threadId] = args;
      const thread = getThread(projectId, threadId); check(!thread.isArchived, "Sent to archived thread");
      const turnId = "turn-" + calls.sends.length;
      onEvent?.({method: "agent/execution-request", params: {requestId: turnId, projectId}});
      onEvent?.({method: "agent/message-updated", params: {messageId: turnId, projectId, threadId, status: "queued"}});
      let sequence=0;
      const emit = event => { const value={...event,hostExecution:{id:turnId,requestId:turnId,projectId,threadId,sequence:sequence=Math.max(sequence,activeExecutions.get(turnId)?.cursor??0)+1}};
        const snapshot=activeExecutions.get(turnId);if(snapshot){snapshot.events.push(structuredClone(value));snapshot.cursor=sequence;}
        onEvent?.(value); };
      const turns = histories.get(threadId) ?? []; histories.set(threadId, turns);
      const turn = { id: turnId, status: "inProgress", startedAt: 1, completedAt: null, items: [{ id: turnId + "-author", type: "userMessage", status: "completed", text: prompt, phase: null }] };
      activeExecutions.set(turnId, {executionId: turnId, requestId: turnId, projectId, threadId, executionThreadId: threadId, taskId: null, turnId, prompt, events:[],cursor:0,truncated:false, workflow: null, questions: []});
      turns.push(turn); updateThread(projectId, threadId, { lastTurnId: turnId, preview: prompt, status: "active" });
      emit({ method: "turn/started", params: { threadId, turnId, turn: { id: turnId, status: "inProgress" } } });
      if (failNext) {
        failNext = false; turn.status = "failed"; updateThread(projectId, threadId, { status: "idle" });
        emit({ method: "turn/completed", params: { threadId, turnId, turn: { id: turnId, status: "failed", error: { message: "A 的可重试失败" }, items: [] } } });
        activeExecutions.delete(turnId);
        emit({method: "agent/execution-ended", params: {projectId, threadId, requestId: turnId, failure: {message: "A 的可重试失败", code: null}}});
        throw Error("A 的可重试失败");
      }
      if (holdNext) { holdNext = false; await new Promise(resolve => { releaseHeld = resolve; }); }
      const message = "仅属于 " + threadId + " 的结果 " + turnId;
      turn.status = "completed"; turn.completedAt = 2; turn.items.push({ id: turnId + "-assistant", type: "agentMessage", status: "completed", text: message, phase: "final_answer" });
      updateThread(projectId, threadId, { status: "idle" });
      emit({ method: "item/completed", params: { threadId, turnId, item: { id: turnId + "-assistant", type: "agentMessage", text: message, phase: "final_answer" } } });
      emit({ method: "turn/completed", params: { threadId, turnId, turn: { id: turnId, status: "completed", items: turn.items } } });
      activeExecutions.delete(turnId);
      emit({method: "agent/execution-ended", params: {projectId, threadId, requestId: turnId, failure: null}});
      return { mode: "advisory", projectId, message, proposedActions: [], candidateResults: [] };
    },
  },
};
let root = createRoot(document.getElementById("root"));
(async () => { try {
  localStorage.clear(); saveAuthor({ name: "多对话作者", email: "multi@example.invalid", passwordSalt: "fixture", passwordHash: "fixture" }); root.render(<App />);
  await until(() => composer() && document.querySelector(".draft-editor") && row(threadA.id), "Startup with conversation A");
  await select(threadA);
  await input("A 的问题草稿"); await quote(document.querySelector<HTMLTextAreaElement>(".draft-editor"), 0, 3);
  check(cards().length === 1, "A quote missing");
  await click("新建对话"); await until(() => calls.creates.length === 1 && row("thread-new-1"), "Empty conversation persisted");
  const threadB = threads.get("thread-new-1");
  await until(() => row(threadB.id)?.querySelector('[aria-current="true"]'), "New conversation selected");
  check(composer().value === "" && !cards().length, "A draft or quotes leaked to B");
  await input("B 的独立草稿"); await select(threadA);
  check(composer().value === "A 的问题草稿" && cards().length === 1, "A draft and source not restored");
  showRecordedTasks = true;
  for(const extra of [{isComposing:true},{keyCode:229},{repeat:true},{shiftKey:true},{altKey:true}]) {
    composer().dispatchEvent(new KeyboardEvent("keydown",{key:"Enter",bubbles:true,cancelable:true,...extra}));
    await tick();check(calls.sends.length===0 && composer().value==="A 的问题草稿","Composition or repeated Enter submitted/consumed the draft");
  }
  composer().dispatchEvent(new KeyboardEvent("keydown",{key:"Enter",bubbles:true,cancelable:true}));
  await until(() => document.querySelector(".composer-retry"), "A failure available");
  await until(() => transcript().includes("A 独占任务记录"), "A own task is visible");
  check(calls.sends[0][4] === threadA.id && calls.sends[0][3].excerpts[0].text === "守钟人", "A send lost explicit thread or frozen quote");
  await select(threadB); check(composer().value === "B 的独立草稿" && !document.querySelector(".composer-retry") && !transcript().includes("A 独占任务记录"), "B received A failure or task, or lost draft");
  await select(threadA); await until(() => document.querySelector(".composer-retry"), "A failure restored");
  document.querySelector<HTMLButtonElement>(".composer-retry")!.click();
  await until(() => calls.sends.length === 2 && transcript().includes("仅属于 thread-a 的结果 turn-2") && !buttons("停止当前执行").length, "A retry complete");
  check(calls.sends[1][4] === threadA.id && JSON.stringify(calls.sends[1][3]) === JSON.stringify(calls.sends[0][3]), "Retry consumed wrong thread or source");
  await input("A 持续运行的问题"); holdNext = true; await click("发送创作指令");
  await until(() => releaseHeld && buttons("停止当前执行").length, "A running");
  await input("重载期间保留的 A 草稿");
  const dispatchedBeforeReload = calls.sends.length;
  snapshotRace=true;
  root.unmount(); await tick(); root = createRoot(document.getElementById("root")); root.render(<App />);
  await until(() => row(threadA.id)?.querySelector('[aria-current="true"]') && buttons("停止当前执行").length, "Reload must restore the live execution from Host");
  await until(()=>transcript().includes("读取快照期间的新进度"),"Reload lost the event delivered after snapshot capture");
  check(calls.sends.length === dispatchedBeforeReload && composer().value === "重载期间保留的 A 草稿", "Reload replayed execution or lost the draft");
  await select(threadB);
  check(!buttons("停止当前执行").length && !transcript().includes("A 持续运行的问题"), "B shows A active task or stop control");
  check(!buttons("发送创作指令")[0].disabled, "A incorrectly blocks B send");
  await input("B 并行提问"); await click("发送创作指令");
  await until(() => transcript().includes("仅属于 thread-new-1 的结果 turn-4"), "B completes while A remains active");
  check(threads.get(threadA.id).status === "active", "B completion changed A");
  await input("B 运行期间的草稿"); releaseHeld();
  await until(() => threads.get(threadA.id).status === "idle" && !buttons("发送创作指令")[0].disabled, "A completed in background");
  check(!transcript().includes("仅属于 thread-a 的结果 turn-3") && composer().value === "B 运行期间的草稿", "A result contaminated B");
  await select(threadA); await until(() => transcript().includes("仅属于 thread-a 的结果 turn-3"), "A result restored on return");
  check(calls.interrupts === 0, "Switching cancelled A");
  check(!document.querySelector(".conversation-recovery"), "Historical save records displaced the conversation");
  await click("对话操作"); await click("保存记录");
  check(document.querySelectorAll(".conversation-recovery").length === 1 && document.querySelector<HTMLDialogElement>(".conversation-recovery").open && document.querySelectorAll('[data-recovery-run="pending-save"]').length === 1, "Saving history could not be opened exactly once");
  await click("关闭保存记录");
  await click("管理对话：" + threadB.name); await click("重命名");
  await change(document.querySelector('[aria-label="对话名称"]'), "人物推敲");
  const rename = document.querySelector<HTMLFormElement>('form[aria-label^="重命名对话："]');
  check(rename, "Accessible rename form missing"); rename.querySelector<HTMLButtonElement>('button[type="submit"]')!.click();
  await until(() => row(threadB.id)?.textContent.includes("人物推敲"), "Renamed conversation");
  await click("管理对话：人物推敲"); await click("置顶"); await until(() => threadB.isPinned, "Pin persisted");
  await change(document.querySelector('[aria-label="搜索对话"]'), "人物");
  check(row(threadB.id) && !row(threadA.id), "Title search not filtering");
  await change(document.querySelector('[aria-label="搜索对话"]'), "");
  await click("管理对话：人物推敲"); await click("归档"); await until(() => !row(threadB.id) && threadB.isArchived, "Archive persisted");
  await click("已归档"); await until(() => row(threadB.id), "Archived conversation discoverable");
  await click("管理对话：人物推敲"); await click("恢复"); await until(() => !threadB.isArchived, "Restore persisted");
  await click("最近"); await until(() => row(threadB.id), "Restored conversation returned");
  await select(threadB); check(composer().value === "B 运行期间的草稿", "Archive cycle lost B draft");
  const editor = document.querySelector<HTMLTextAreaElement>(".draft-editor")!;
  await change(editor, "未保存正文，切换不能覆盖。"); const localValue = editor.value;
  await select(threadA); await select(threadB);
  check(editor.isConnected && editor.value === localValue && calls.writes === 0, "Conversation switch saved or replaced unsaved document");
  await quote(editor); await click("发送创作指令");
  await until(() => calls.sends.length === 5 && transcript().includes("仅属于 thread-new-1 的结果 turn-5") && !buttons("停止当前执行").length, "B unsaved discussion complete");
  check(calls.sends[4][4] === threadB.id && calls.sends[4][3].readOnly === true && calls.sends[4][3].excerpts[0].text === localValue, "B unsaved quote not sent as own read-only context");
  await until(() => transcript().includes("B 独占任务记录"), "B own task is visible");
  check(!transcript().includes("A 独占任务记录"), "B displays unrelated A task record");
  check(editor.value === localValue && calls.writes === 0, "Unsaved discussion wrote document");
  await input("留在人物对话的草稿"); await quote(editor, 0, 5);
  for (const [theme, width] of [["light", 1400], ["dark", 1400], ["light", 1050]]) {
    document.documentElement.dataset.theme = theme; window.testViewport = { width, height: 1000 };
    await until(() => !window.testViewport, "Viewport applied"); await tick();
    const navigator = row(threadB.id).closest("nav, section");
    const box = navigator.getBoundingClientRect(); check(box.width > 0 && box.left >= 0 && box.right <= innerWidth, "Conversation navigator outside viewport");
    window.testScreenshot = `multi-conversation-${theme}-${width}`; await until(() => !window.testScreenshot, "Screenshot captured");
  }
  editor.focus(); editor.setSelectionRange(0, 5); editor.dispatchEvent(new Event("select", { bubbles: true }));
  await until(() => buttons("新建对话讨论")[0], "Selection can start independent conversation");
  failCreate = true; await click("新建对话讨论");
  await until(() => buttons("新建对话讨论")[0] && !buttons("新建对话讨论")[0].disabled, "Failed creation keeps selectable source");
  check(calls.creates.length === 1 && row(threadB.id)?.querySelector('[aria-current="true"]') && cards().length === 1, "Failed selection conversation consumed draft or changed owner");
  await click("新建对话讨论");
  await until(() => calls.creates.length === 2 && row("thread-new-2")?.querySelector('[aria-current="true"]') && cards().length === 1, "New conversation owns selected quote");
  check(cards()[0].textContent.includes(localValue.slice(0, 5)) && !transcript().includes("仅属于 thread-new-1 的结果"), "New quote conversation inherited unrelated history");
  await select(threadB);
  check(composer().value === "留在人物对话的草稿" && cards().length === 1, "Selection new conversation consumed B draft");
  root.unmount(); await tick(); root = createRoot(document.getElementById("root")); root.render(<App />);
  await until(() => row(threadB.id)?.querySelector('[aria-current="true"]') && composer(), "Reload restores selected conversation");
  check(composer().value === "留在人物对话的草稿" && cards().length === 1, "Reload lost conversation-local draft or frozen quote");
  check(row(threadB.id).textContent.includes("人物推敲"), "Reload lost title");
  await until(() => transcript().includes("仅属于 thread-new-1 的结果 turn-5"), "Reload restores persisted conversation messages");
  buttons("从此回合新建分支对话").at(-1)!.click(); await tick();
  await until(() => calls.creates.length === 3 && row("thread-new-3")?.querySelector('[aria-current="true"]'), "Message branch selected");
  check(calls.creates[2].options.sourceThreadId === threadB.id && calls.creates[2].options.lastTurnId === "turn-5", "Fork did not freeze the selected turn boundary");
  await until(() => transcript().includes("仅属于 thread-new-1 的结果 turn-5"), "Fork inherits only explicit source history");
  check(!composer().value && !cards().length, "Fork inherited unsent draft or quote");
  const beforeCompare = calls.sends.length;
  await click("对照两种方案"); await until(() => document.querySelectorAll('.conversation-comparison-columns .conversation-reader').length === 2, "Compare both persisted conversations");
  check(calls.sends.length === beforeCompare && document.querySelector('.conversation-comparison').textContent.includes("turn-5"), "Compare regenerated or lost saved content");
  await click("关闭对照");
  await select(threadB);
  check(composer().value === "留在人物对话的草稿" && cards().length === 1, "Fork consumed source conversation draft");
  await click("发送创作指令");
  await until(() => calls.sends.length === 6 && transcript().includes("仅属于 thread-new-1 的结果 turn-6"), "Reloaded B sends a new turn");
  check(calls.sends[5][4] === threadB.id, "Reloaded conversation sent to wrong thread");
  check(transcript().includes("B 运行期间的草稿") && transcript().includes("仅属于 thread-new-1 的结果 turn-5"), "Sending after reload discarded persisted author or assistant history");
  check(threadB.name === "人物推敲" && threadB.isPinned && !threadB.isArchived && calls.metadata.length >= 4, "Metadata not durable in fixture reads");
  await until(() => !buttons("停止当前执行").length, "Previous quoted discussion completed");
  check(cards().length === 0, "Sent excerpt was not consumed");
  const reopenedEditor = document.querySelector<HTMLTextAreaElement>(".draft-editor");
  check(reopenedEditor?.value === localValue && editContexts.at(-1)?.chapterScope?.chapterId === chapter.id, "Reopened draft did not report its chapter scope");
  await input("讨论已保存章节，不包含本地草稿");
  await click("发送创作指令");
  await until(() => calls.sends.length === 7 && transcript().includes("仅属于 thread-new-1 的结果 turn-7"), "Ordinary chapter discussion was blocked by a local draft");
  check(calls.sends[6][3].chapterId === chapter.id && !calls.sends[6][3].excerpts?.length && reopenedEditor.value === localValue && calls.writes === 0, "Ordinary message captured or saved the unsaved draft");
  await until(() => !buttons("停止当前执行").length, "Ordinary draft discussion finished");
  failNext = true; await input("可重试的章节提问"); await click("发送创作指令");
  await until(() => document.querySelector<HTMLButtonElement>(".composer-retry") && !document.querySelector<HTMLButtonElement>(".composer-retry").disabled, "Dirty chapter discussion retry remained disabled");
  document.querySelector<HTMLButtonElement>(".composer-retry").click();
  await until(() => calls.sends.length === 9 && transcript().includes("仅属于 thread-new-1 的结果 turn-9"), "Dirty chapter discussion could not retry");
  check(reopenedEditor.value === localValue && calls.writes === 0, "Retry changed the local chapter draft");
  check(calls.taskReads <= 100, "Task refresh budget exceeded");
  window.testResult = { passed: true, created: calls.creates.length, sends: calls.sends.length, writes: calls.writes, interrupts: calls.interrupts, taskReads: calls.taskReads, draftIsolation: true, unsavedPreserved: true, executionOwnership: true, metadataRestored: true };
} catch (error) { window.testResult = { passed: false, error: error.stack ?? String(error), dom: document.body.textContent.slice(-5000) }; } })();
