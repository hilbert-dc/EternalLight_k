import { novelFixtureMedia } from "../helpers/novel-edition";
import { emptyCreationIssueSummary } from "../helpers/creation-issues";
import React from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import "../../../src/styles.css";
import { saveAuthor } from "../helpers/author-session";
import { chapterRuntimeReads } from "../helpers/chapter-runtime";

let renderError = null;
window.addEventListener("error", event => { renderError ??= event.error ?? Error(event.message); });
window.addEventListener("unhandledrejection", event => { renderError ??= event.reason; });
const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (condition, message) => { if (!condition) throw Error(message); };
const until = async (predicate, message) => { for (let i = 0; i < 200 && !predicate(); i++) { if (renderError) throw renderError; await tick(); } if (renderError) throw renderError; check(predicate(), message); };
const deferred = () => { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; };
const projectA = "project-last-lamp", threadA = "original-thread";
const projects = [{ id: projectA, title: "原长篇作品", logline: "", updated_at: "" }, { id: "project-b", title: "另一作品", logline: "", updated_at: "" }];
const projectFor = id => projects.find(project => project.id === id);
const savedChapter = { id: "chapter-1", number: 1, title: "当前章", status: "draft", updated_at: "", summary: "", passages: [] };
const state = id => ({ project: projectFor(id), canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1", scenes: [], anchors: [] }, novelEdition: { chapters: [savedChapter] } });
const task = (id, goal, extra = {}) => ({ id, projectId: projectA, threadId: threadA, kind: "generation", goal, status: "blocked", version: 22,
  deliveryStatus: "pending", deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], evidence: [], pendingActionIds: [], unsettledAttemptIds: [] }, resumeMode: "continueExecution", resumeBlockReason: null, pendingContinuation: false,
  steps: [{ id: `${id}-step`, operation: "generateChapter", status: "blocked", target: { chapterId: "chapter-1", chapterNumber: 7 }, instruction: null,
    attempts: [{ id: `${id}-attempt`, threadId: threadA, turnId: `${id}-turn`, status: "failed", error: "输出未完成", startedAt: 1, completedAt: 2 }], changes: [], currentAttemptId: `${id}-attempt`, readingProgress: null, outputRefs: [] }], ...extra });
const initialTasks = () => [task("original-work", "继续三百万字长篇，保存前一百章"), task("revision-work", "只修订第七章时间线"),
  task("unknown-write", "写入结果尚未核对的任务", { resumeBlockReason: "pendingRecovery" }), task("stale-source", "来源已变化的任务", { resumeBlockReason: "inputChanged" }),
  task("foreign-thread", "另一对话任务", { threadId: "thread-b" }), task("foreign-project", "另一作品任务", { projectId: "project-b" })];
let tasks = initialTasks(), root, freshTransform = work => work, readGate = null, resumeGate = null;
const calls = { reads: [], resume: [], recover: [], newRoot: 0, models: 0 };
const api = window.novelCodex = { multimedia: novelFixtureMedia,
  app: { onCloseRequested: () => () => {} },
  runtime: { ...chapterRuntimeReads(projectFor), status: async () => ({ state: "online", mode: "local", error: null }), listProjects: async () => ({ projects }),
    getState: async id => structuredClone(state(id)),
    getCreativeWorkspace: async id => ({ ...await chapterRuntimeReads(projectFor).getCreativeWorkspace(id), chapters: [{ ...savedChapter, candidate: null, accepted: null, working: null, workingSelection: null }] }),
    getWriterPreview: async () => ({ target: { position: { readingChapter: 1, sceneOrder: 1 } }, plannedEffects: [], relevantText: [], coverage: { complete: true, missingScope: [], truncationReasons: [] } }),
    listGenerationRuns: async () => ({ runs: [] }),
    getTaskFinalReply: async (projectId, taskId) => null,
    listWorkTasks: async id => ({ tasks: structuredClone(tasks.filter(work => work.projectId === id)), activeCount: tasks.filter(work => work.projectId === id && work.status === "blocked").length, nextCursor: null }),
    getWorkTask: async (projectId, id) => { calls.reads.push({ projectId, id }); if (readGate) return readGate.promise;
      return freshTransform(structuredClone(tasks.find(work => work.id === id))); },
    getHistoricalKnowledgeState: async (id, snapshotId) => ({ projectId: id, snapshotId, entries: [], coverage: { complete: true, truncatedEntries: false, pendingSources: [] } }) },
  agent: { status: async () => ({ configured: true, state: "ready", mode: "novel-agent-engine", provider: "fixture", model: "fixture", error: null }),
    listConnections: async () => ({ connections: [] }), recoveryReport: async () => [],
    listExecutions: async () => [], readMessage: async () => { throw Error("message receipt not found"); }, beginInterlude: async () => { throw Error("unexpected interlude"); }, returnInterlude: async () => { throw Error("unexpected interlude return"); }, listMessages: async () => [], listThreads: async projectId => ({ threads: [{ id: threadA, projectId, name: "原长篇对话", status: "idle", isPinned: false, isArchived: false },
      { id: "thread-b", projectId, name: "另一对话", status: "idle", isPinned: false, isArchived: false }], nextCursor: null }),
    readConversation: async (projectId, id) => ({ thread: { id, projectId, name: "原长篇对话", status: "idle" }, turns: [] }),
    listProposedActions: async () => ({ actions: [] }), setWorkspaceContext: async () => {}, onEvent: () => () => {}, onUserInputRequest: () => () => {},
    sendPrompt: async () => { calls.newRoot++; throw Error("Recovery created a new root"); },
    generateChapter: async () => { calls.models++; throw Error("Unexpected model generation"); },
    resumeWorkTask: async (projectId, id) => { calls.resume.push({ projectId, id }); return resumeGate ? resumeGate.promise : Promise.reject(Error("Native拒绝：原任务来源已变化")); },
    recoverWorkTask: async (projectId, id) => { calls.recover.push({ projectId, id }); const work = tasks.find(item => item.id === id); work.status = "completed"; work.resumeMode = null; return structuredClone(work); },
  },
};
const rail = () => document.querySelector<HTMLElement>(".conversation-rail");
const picker = () => rail()?.querySelector<HTMLButtonElement>('[aria-label="选择要继续的任务"]');
const row = id => rail()?.querySelector<HTMLButtonElement>(`[data-retry-task-id="${id}"]`);
const open = async () => { picker().click(); await until(() => row("original-work"), "original task choice missing"); };
const mount = async (projectId = projectA, threadId = threadA) => {
  if (root) { root.unmount(); await tick(); }
  localStorage.setItem("novel-codex-active-project", projectId);
  localStorage.setItem(`novel-codex:selected-conversation:${JSON.stringify(["fixture@example.invalid", projectId])}`, threadId);
  root = createRoot(document.getElementById("root")); root.render(<App />);
  await until(() => document.querySelector(".project-switcher")?.textContent.includes(projectFor(projectId).title) && rail()?.textContent.includes(tasks.find(work => work.projectId === projectId && work.threadId === threadId)?.goal), "App original tasks not restored");
  await tick(); await tick();
};
const cases = [];
(async () => { try {
  localStorage.clear(); saveAuthor({ name: "恢复作者", email: "fixture@example.invalid", passwordSalt: "fixture", passwordHash: "fixture" });
  await mount(); await open();
  check(row("revision-work") && !row("unknown-write") && !row("stale-source") && !row("foreign-thread") && !row("foreign-project"), "unsafe or foreign tasks entered retry choices");
  check(!document.querySelector(".composer-retry-menu").textContent.includes("original-work"), "internal id exposed as author text");
  check(calls.resume.length === 0 && calls.newRoot === 0, "opening the picker auto selected a task");
  cases.push("restart choices");

  readGate = deferred(); const beforeReads = calls.reads.length;
  const original = row("original-work"); original.click(); original.click();
  await until(() => calls.reads.length === beforeReads + 1, "fresh lookup was not single flight");
  check(calls.resume.length === 0, "selection skipped fresh Native read");
  resumeGate = deferred(); readGate.resolve(structuredClone(tasks[0])); readGate = null;
  await until(() => calls.resume.length === 1, "original Work did not resume");
  check(calls.resume[0].id === "original-work" && calls.resume[0].projectId === projectA, "selection resumed another Work");
  resumeGate.reject(Error("Native拒绝：原任务来源已变化")); resumeGate = null;
  await until(() => rail().textContent.includes("Native拒绝"), "Native rejection was hidden");
  check(calls.newRoot === 0 && calls.models === 0, "Native rejection widened recovery");
  cases.push("explicit original Work");

  for (const [name, transform] of [["version changed", work => ({ ...work, version: work.version + 1 })],
    ["mode changed", work => ({ ...work, resumeMode: "recoverSavedResult" })],
    ["Native blocked", work => ({ ...work, resumeBlockReason: "pendingRecovery" })]]) {
    freshTransform = transform; await mount(); await open(); const before = calls.resume.length;
    row("original-work").click(); await until(() => rail().textContent.includes("原任务状态已变化"), `${name} was accepted`);
    check(calls.resume.length === before && calls.recover.length === 0, `${name} executed a stale choice`);
    cases.push(name);
  }
  freshTransform = work => work;

  tasks = initialTasks(); tasks[1].resumeMode = "recoverSavedResult";
  readGate = deferred(); await mount(); await open(); const raceReadCount = calls.reads.length;
  row("revision-work").click(); await until(() => calls.reads.length === raceReadCount + 1, "saved recovery fresh read did not start");
  const oldLiveRead = readGate; readGate = null;
  const switchThread = document.querySelector<HTMLButtonElement>('[aria-label="切换对话：另一对话"]');
  check(switchThread && !switchThread.disabled, "same-instance thread navigation missing");
  // Queue the old read continuation, then change the live selection in this
  // same JS task, before any awaited tick or new App render can rescue it.
  oldLiveRead.resolve(structuredClone(tasks[1])); switchThread.click();
  await until(() => rail().dataset.conversationId === "thread-b", "same-instance selected thread did not change");
  await tick();
  check(calls.recover.length === 0 && calls.resume.length === 1 && !rail().textContent.includes("原任务状态已变化"), "live selection ref race recovered or polluted the former thread");
  cases.push("live thread ref");
  tasks = initialTasks();

  readGate = deferred(); await mount(); await open(); row("original-work").click();
  const oldThreadRead = readGate; readGate = null;
  await mount(projectA, "thread-b"); oldThreadRead.resolve(structuredClone(tasks[0])); await tick(); await tick();
  check(calls.resume.length === 1 && !rail().textContent.includes("原任务状态已变化"), "late original-thread read resumed or polluted new thread");
  cases.push("late thread read");

  readGate = deferred(); await mount(); await open(); row("original-work").click(); const oldProjectRead = readGate; readGate = null;
  await mount("project-b", threadA); oldProjectRead.reject(Error("旧作品迟到读取错误")); await tick(); await tick();
  check(calls.resume.length === 1 && !rail().textContent.includes("旧作品迟到读取错误"), "late original-project read polluted new project");
  cases.push("late project read");

  tasks = initialTasks(); tasks[1].resumeMode = "recoverSavedResult"; await mount(); await open(); row("revision-work").click();
  await until(() => calls.recover.length === 1, "saved result recovery was not selected");
  check(calls.recover[0].id === "revision-work" && calls.resume.length === 1 && calls.models === 0, "saved recovery executed a model or another Work");
  cases.push("recover saved result");

  tasks = [task("original-work", "继续三百万字长篇，保存前一百章")]; await mount();
  const single = rail().querySelector<HTMLButtonElement>('[aria-label="重试当前任务"]');
  check(single && !picker(), "one candidate lost its direct retry");
  single.click(); await until(() => calls.resume.length === 2, "single original task did not resume directly");
  cases.push("single retry");

  tasks = initialTasks(); await mount(); await open(); const oldRow = row("original-work");
  document.querySelector<HTMLButtonElement>('[aria-label="选择要继续的任务"]').dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); await tick();
  check(!document.querySelector(".composer-retry-menu"), "Escape did not close choices");
  await open(); await mount(projectA, "thread-b"); oldRow.click(); await tick();
  check(!document.querySelector(".composer-retry-menu") && calls.resume.length === 2, "old menu selection survived thread identity");
  cases.push("menu identity");
  window.testResult = { passed: true, cases, calls };
} catch (error) { window.testResult = { passed: false, error: String(error), calls, dom: document.body.innerHTML }; }
finally { root?.unmount(); } })();
