import { novelFixtureMedia } from "../helpers/novel-edition";
import { emptyCreationIssueSummary } from "../helpers/creation-issues";
import React from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import "../../../src/styles.css";
import { saveAuthor } from "../helpers/author-session";
import { chapterRuntimeReads } from "../helpers/chapter-runtime";
import { operationHeading, operationPresentation } from "../../../src/operation-presentation";
import { operationSummary } from "../../../src/conversation-value";

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
const task = (id, goal, extra = {}) => ({ id, projectId: projectA, threadId: threadA, kind: "generation", goal, status: "running", version: 22, controlRevision: 2,
  deliveryStatus: "partial", deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], evidence: [], pendingActionIds: [], unsettledAttemptIds: [] }, resumeMode: null, resumeBlockReason: null, pendingContinuation: false,
  steps: [{ id: `${id}-step`, operation: "generateChapter", status: "running", target: { chapterId: "chapter-1", chapterNumber: 7 }, instruction: null,
    attempts: [], changes: [], currentAttemptId: null, readingProgress: null, outputRefs: [] }], ...extra });
const initialTasks = () => [task("original-work", "继续三百万字长篇，保存前一百章"), task("revision-work", "只修订第七章时间线"),
  task("paused-work", "已暂停的创作", { status: "paused" }), task("foreign-thread", "另一对话任务", { threadId: "thread-b" }), task("foreign-project", "另一作品任务", { projectId: "project-b" })];
let tasks = initialTasks(), root, freshTransform = work => work, readGate = null, pauseGate = null, pauseTransform = work => work, nativeFailure = null, formalFailure = null;
const calls = { reads: [], pause: [], refresh: 0, newRoot: 0, models: 0 };
const api = window.novelCodex = { multimedia: novelFixtureMedia,
  app: { onCloseRequested: () => () => {} },
  runtime: { ...chapterRuntimeReads(projectFor), status: async () => ({ state: "online", mode: "local", error: null }), listProjects: async () => ({ projects }),
    getState: async id => { if (formalFailure) throw Error(formalFailure); return structuredClone(state(id)); },
    getCreativeWorkspace: async id => ({ ...await chapterRuntimeReads(projectFor).getCreativeWorkspace(id), chapters: [{ ...savedChapter, candidate: null, accepted: null, working: null, workingSelection: null }] }),
    getWriterPreview: async () => ({ target: { position: { readingChapter: 1, sceneOrder: 1 } }, plannedEffects: [], relevantText: [], coverage: { complete: true, missingScope: [], truncationReasons: [] } }),
    listGenerationRuns: async () => ({ runs: [] }), getTaskFinalReply: async () => null,
    listWorkTasks: async id => { calls.refresh++; return { tasks: structuredClone(tasks.filter(work => work.projectId === id)), activeCount: tasks.filter(work => work.projectId === id && work.status === "running").length, nextCursor: null }; },
    getWorkTask: async (projectId, id) => { calls.reads.push({ projectId, id }); if (readGate) return readGate.promise;
      return freshTransform(structuredClone(tasks.find(work => work.id === id))); },
    getHistoricalKnowledgeState: async (id, snapshotId) => ({ projectId: id, snapshotId, entries: [], coverage: { complete: true, truncatedEntries: false, pendingSources: [] } }) },
  agent: { status: async () => ({ configured: true, state: "offline", mode: "novel-agent-engine", provider: "fixture", model: "fixture", error: "模型暂不可用" }),
    listConnections: async () => ({ connections: [] }), recoveryReport: async () => [],
    listExecutions: async () => [], readMessage: async () => { throw Error("message receipt not found"); }, beginInterlude: async () => { throw Error("unexpected interlude"); }, returnInterlude: async () => { throw Error("unexpected interlude return"); }, listMessages: async () => [], listThreads: async projectId => ({ threads: [{ id: threadA, projectId, name: "原长篇对话", status: "idle", isPinned: false, isArchived: false },
      { id: "thread-b", projectId, name: "另一对话", status: "idle", isPinned: false, isArchived: false }], nextCursor: null }),
    readConversation: async (projectId, id) => ({ thread: { id, projectId, name: "原长篇对话", status: "idle" }, turns: [] }),
    listProposedActions: async () => ({ actions: [] }), setWorkspaceContext: async () => {}, onEvent: () => () => {}, onUserInputRequest: () => () => {},
    sendPrompt: async () => { calls.newRoot++; throw Error("Pause created a new root"); }, generateChapter: async () => { calls.models++; throw Error("Pause called a model"); },
    controlWorkTask: async (projectId, id, command, expectedControlRevision, expectedThreadId) => { calls.pause.push({ projectId, id, command, expectedControlRevision, expectedThreadId });
      check(tasks.find(work => work.id === id).controlRevision === expectedControlRevision && tasks.find(work => work.id === id).threadId === expectedThreadId, "UI control omitted fresh version/thread");
      if (pauseGate) return pauseGate.promise; if (nativeFailure) throw Error(nativeFailure);
      const work = tasks.find(item => item.id === id); work.version++; work.controlRevision++;
      if (command === "pauseAfterStep") work.pauseAfterStep = { controlRevision: work.controlRevision, remainingAttempts: 1 };
      else { work.status = "paused"; work.pauseAfterStep = null; }
      return pauseTransform(structuredClone(work)); },
  },
};
const rail = () => document.querySelector<HTMLElement>(".conversation-rail");
const menus = () => document.querySelector<HTMLElement>('[role="menu"][aria-label="选择要暂停的任务"]');
const choice = goal => menus()?.querySelector<HTMLButtonElement>(`[aria-label="暂停：${goal}"]`);
const originalChoice = () => choice(tasks.find(work => work.id === "original-work").goal);
const open = async () => { rail().querySelector<HTMLButtonElement>('[aria-label="对话操作"]').click(); await tick();
  const entry = document.querySelector<HTMLButtonElement>('[role="menuitem"][aria-label="暂停创作任务…"]'); check(entry && !entry.disabled, "pause entry unavailable while model offline"); entry.click();
  await until(() => menus(), "pause choices missing"); };
const mount = async (projectId = projectA, threadId = threadA) => {
  if (root) { root.unmount(); await tick(); }
  localStorage.setItem("novel-codex-active-project", projectId);
  localStorage.setItem(`novel-codex:selected-conversation:${JSON.stringify(["fixture@example.invalid", projectId])}`, threadId);
  root = createRoot(document.getElementById("root")); root.render(<App />);
  await until(() => document.querySelector(".project-switcher")?.textContent.includes(projectFor(projectId).title) && rail()?.textContent.includes(tasks.find(work => work.projectId === projectId && work.threadId === threadId)?.goal), "persisted Work not restored"); await tick(); await tick();
};
const cases = [];
(async () => { try {
  const source = { id: "source", label: "相关正文（第 1—7 章）", sectionPath: "/relevantText" };
  const page = operationPresentation({ tool: "read_source_content", arguments: { sourceId: "source" }, status: "completed",
    result: { structuredContent: { sourceId: "source", sectionPath: "/relevantText", chapterNumbers: [1, 2, 3, 4, 5, 6, 7], returnedChapterNumbers: [6], contentRead: true } } }, [source]);
  check(operationHeading(page) === "读取相关正文（第 6 章）片段", "catalog scope leaked into returned page scope");
  const uncertain = operationPresentation({ tool: "read_source_content", arguments: { sourceId: "source" }, status: "completed",
    result: { structuredContent: { sourceId: "source", sectionPath: "/relevantText", chapterNumbers: [1, 2, 3, 4, 5, 6, 7] } } }, [source]);
  check(operationHeading(uncertain) === "读取相关正文片段", "missing returned identities claimed full catalog read");
  const index = operationPresentation({ tool: "read_source_content", arguments: { sourceId: "source", view: "index" }, status: "completed",
    result: { structuredContent: { sourceId: "source", sectionPath: "/relevantText", representation: "sourceIndex", coverage: { contentRead: false } } } }, [source]);
  check(index.outcome !== "read" && operationSummary([{ id: "index", title: operationHeading(index), kind: "tool", status: "completed", operation: index }]) === "已执行：查看相关正文目录", "index metadata was certified as content reading");
  const foreign = operationPresentation({ tool: "read_source_content", arguments: { sourceId: "source" }, status: "completed",
    result: { structuredContent: { sourceId: "foreign", sectionPath: "/relevantText", returnedChapterNumbers: [9], contentRead: true } } }, [source]);
  check(foreign.outcome !== "read" && !operationHeading(foreign).includes("第 9 章"), "foreign page claimed an authorized chapter read");
  const reportRead = operationPresentation({ tool: "read_revision_outcome", arguments: { writerTaskId: "writer-one", chapterNumber: 9 }, status: "completed",
    result: { structuredContent: { revisionId: "report-one", authority: "candidateAnalysis", assessment: { writerTaskId: "writer-one", outcomes: [{ id: "acceptance-1" }] } } } });
  check(reportRead.category === "read" && operationHeading(reportRead) === "读取章节检查报告", "report tool was technical or guessed a chapter from the request");
  const numberedReport = operationPresentation({ tool: "read_revision_outcome", arguments: { writerTaskId: "writer-one" }, status: "completed",
    result: { structuredContent: { chapterNumber: 6 } } });
  check(operationHeading(numberedReport) === "读取第 6 章 · 章节检查报告", "returned report chapter was not shown accurately");
  for (const assessment of [null, { writerTaskId: "foreign-writer", outcomes: [{ id: "acceptance-1" }] }, { writerTaskId: "writer-one", taskSourceRef: true }]) {
    const unprovedReport = operationPresentation({ tool: "read_revision_outcome", arguments: { writerTaskId: "writer-one" }, status: "completed",
      result: { structuredContent: { revisionId: "report-one", authority: "candidateAnalysis", assessment } } });
    check(unprovedReport.outcome !== "read", "missing, foreign or projected report was certified as content read");
  }
  cases.push("returned reading scope");
  const pauseItem = { tool: "pause_generation_run", arguments: { runId: "original-run", expectedVersion: 42 }, status: "completed" };
  const pauseReceipt = { authority: "committedGenerationControl", runId: "original-run", version: 43, status: "paused", sourceTaskId: "task-current" };
  const committedPause = operationPresentation({ ...pauseItem, result: { structuredContent: { receipt: pauseReceipt } } });
  check(committedPause.category === "arrange" && operationHeading(committedPause) === "已暂停连续创作", "real model pause receipt was hidden or not named clearly");
  for (const receipt of [null, { ...pauseReceipt, runId: "foreign-run" }, { ...pauseReceipt, version: 42 }, { ...pauseReceipt, status: "running" }, { ...pauseReceipt, authority: "authorComment" }]) {
    const unproved = operationPresentation({ ...pauseItem, result: { structuredContent: { receipt } } });
    check(unproved.outcome !== "paused" && !operationHeading(unproved).startsWith("已暂停"), "unproved model pause was announced as paused");
    check(operationSummary([{ id: "pause", title: operationHeading(unproved), kind: "tool", status: "completed", operation: unproved }]).includes("暂停状态待核对"), "unproved pause lost its pending status");
  }
  cases.push("model pause receipt");
  localStorage.clear(); saveAuthor({ name: "恢复作者", email: "fixture@example.invalid", passwordSalt: "fixture", passwordHash: "fixture" });
  const report = (verdict, extra = {}) => ({ writerTaskId: "writer-one", chapterId: "chapter-1", draftRevisionId: "draft-one",
    chapterAcceptance: { requirements: [{ id: "acceptance-1", text: "承接前章的时间顺序" }], length: { requirement: "完整章节，约三千字", minCharacters: 2500, maxCharacters: 3500 } },
    reportRevisionId: verdict === "pending" ? null : "report-one", current: verdict !== "pending" && verdict !== "stale", verdict,
    length: { characterCount: 3100, minCharacters: 2500, maxCharacters: 3500, satisfied: true, countUnit: "Unicode characters excluding whitespace; punctuation included; body only" },
    checks: verdict === "pending" ? [] : [{ id: "acceptance-1", status: verdict === "passed" ? "supported" : verdict === "insufficientEvidence" ? "insufficientEvidence" : "unmet", reason: "前后行动时间已实际核对。", issueId: null }], ...extra });
  for (const [verdict, expected, state] of [["pending", "正文已保存，待独立检查", "completed"], ["notMet", "检查未通过，正在修订", "running"],
    ["notMet", "检查未通过，待修订", "completed"], ["insufficientEvidence", "检查依据不足，待补充核对", "completed"],
    ["stale", "检查尚需重新核对", "completed"], ["passed", "当前稿检查通过", "completed"]]) {
    tasks = initialTasks(); tasks[0].steps[0].status = state;
    tasks[0].steps[0].attempts = [{ id: "writer-one", threadId: threadA, turnId: "writer-turn", status: "completed", error: null, startedAt: 1, completedAt: 2 }];
    tasks[0].deliveryReport.chapterAcceptanceReports = [report(verdict)]; await mount();
    const progress = rail().querySelector<HTMLElement>('[aria-label="章节检查进展"]');
    check(progress?.textContent.includes(expected), `${verdict}/${state} acceptance state missing`);
    check(!progress.querySelector<HTMLDetailsElement>("details").open && !progress.innerHTML.includes("writer-one") && !progress.innerHTML.includes("report-one"), "technical checks opened by default or leaked identities");
    if (verdict !== "passed") check(!progress.textContent.includes("当前稿检查通过"), "saving or failed/stale check was certified as passed");
  }
  tasks = initialTasks(); tasks[0].steps[0].attempts = [{ id: "writer-one", threadId: threadA, turnId: "writer-turn", status: "completed", error: null, startedAt: 1, completedAt: 2 }];
  tasks[0].steps.push({ ...tasks[0].steps[0], id: "new-writer-step", status: "completed", attempts: [{ ...tasks[0].steps[0].attempts[0], id: "writer-two" }] });
  tasks[0].deliveryReport.chapterAcceptanceReports = [report("pending", { writerTaskId: "writer-two", draftRevisionId: "draft-two" }), report("passed", { current: false })];
  await mount(); const latestAcceptance = rail().querySelector<HTMLElement>('[aria-label="章节检查进展"]');
  check(latestAcceptance.querySelectorAll("section").length === 1 && latestAcceptance.textContent.includes("待独立检查") && !latestAcceptance.textContent.includes("当前稿检查通过"), "older report arrival order certified the new writer draft");
  const directPending = report("pending", { writerTaskId: "writer-two" });
  directPending.chapterAcceptance.verification = { method: "direct", reason: "主模型核对简单修改" };
  tasks[0].deliveryReport.chapterAcceptanceReports = [directPending]; await mount();
  check(rail().textContent.includes("正文已保存，待核对") && !rail().textContent.includes("待独立检查"), "direct verification was falsely presented as independent review");
  tasks[0].deliveryReport.chapterAcceptanceReports = [report("passed", { writerTaskId: "foreign-writer" })]; await mount();
  check(!rail().querySelector('[aria-label="章节检查进展"]'), "report from another Work writer was certified");
  tasks = initialTasks(); delete tasks[0].deliveryReport.chapterAcceptanceReports; await mount();
  check(rail().textContent.includes("章节检查记录无法核对") && !rail().textContent.includes("当前稿检查通过"), "malformed current protocol was replaced by a successful or empty status");
  cases.push("independent acceptance states"); tasks = initialTasks();
  await mount(); await open();
  check(choice(tasks[1].goal) && !choice(tasks[2].goal) && !choice(tasks[3].goal) && !choice(tasks[4].goal), "foreign or paused Work entered choices");
  check(!menus().textContent.includes("original-work") && calls.pause.length === 0, "menu leaked technical identity or auto paused"); cases.push("restart offline choices");
  readGate = deferred(); const beforeReads = calls.reads.length, selected = originalChoice(); selected.click(); selected.click();
  await until(() => calls.reads.length === beforeReads + 1, "pause lookup was not single flight"); check(calls.pause.length === 0, "pause skipped fresh lookup");
  const pause = deferred(); pauseGate = pause; readGate.resolve(structuredClone(tasks[0])); readGate = null;
  await until(() => calls.pause.length === 1, "original Work not paused"); check(calls.pause[0].id === "original-work" && calls.pause[0].command === "pause", "another Work was paused");
  tasks[0].status = "paused"; tasks[0].version++; tasks[0].controlRevision++; pause.resolve(structuredClone(tasks[0])); pauseGate = null;
  await until(() => rail().textContent.includes("已确认暂停任务"), "pause receipt not shown"); cases.push("explicit original Work");
  await mount(); await open(); check(!originalChoice() && choice(tasks[1].goal), "restart lost persisted paused state"); cases.push("persisted paused receipt");

  tasks = initialTasks(); freshTransform = work => ({ ...work, version: work.version + 100 });
  await mount(); await open(); const beforeProgressPause = calls.pause.length; originalChoice().click();
  await until(() => rail().textContent.includes("已确认暂停任务"), "progress update invalidated author pause");
  check(calls.pause.length === beforeProgressPause + 1, "progress pause did not reach Native"); cases.push("progress does not invalidate pause");

  tasks = initialTasks(); freshTransform = work => work; await mount(); await open();
  const liveMenu = menus(), liveButton = originalChoice(), livePauseCount = calls.pause.length;
  liveButton.focus(); tasks[0].version += 10; tasks[0].goal += "（已保存新进度）";
  await until(() => originalChoice(), "Polling did not update the open pause menu");
  check(menus() === liveMenu && document.activeElement === liveButton, "Progress refresh closed the menu or lost keyboard focus");
  originalChoice().click();
  await until(() => calls.pause.length === livePauseCount + 1 && rail().textContent.includes("已确认暂停任务"), "Live menu progress prevented pause");
  cases.push("open menu survives progress");

  tasks = initialTasks(); await mount(); await open(); const staleButton = originalChoice(), staleCount = calls.pause.length;
  tasks[0].controlRevision++;
  await until(() => !menus(), "Changed author control left stale menu open");
  staleButton.click(); await tick();
  check(calls.pause.length === staleCount, "Stale detached menu issued a control");
  cases.push("open menu invalidates changed control");

  tasks = initialTasks(); freshTransform = work => work; await mount(); await open();
  const beforeDrain = calls.pause.length;
  const drainChoice = menus().querySelector<HTMLButtonElement>(`[aria-label="本步结束后暂停：${tasks[0].goal}"]`);
  check(drainChoice, "Scheduled pause entry missing"); drainChoice.click(); drainChoice.click();
  await until(() => rail().textContent.includes("等待当前步骤结束"), "Scheduled pause status missing");
  check(calls.pause.length === beforeDrain + 1 && calls.pause.at(-1).command === "pauseAfterStep" && tasks[0].status === "running", "Scheduled pause stopped execution or dispatched twice");
  check(!rail().textContent.includes("已确认暂停任务"), "Pending barrier was falsely reported as paused");
  await open(); check(!menus().querySelector(`[aria-label="本步结束后暂停：${tasks[0].goal}"]`) && originalChoice(), "Pending barrier must still allow immediate pause");
  originalChoice().click();
  await until(() => tasks[0].status === "paused" && rail().textContent.includes("已确认暂停任务"), "Immediate pause could not override scheduled pause");
  check(calls.pause.length === beforeDrain + 2, "Immediate pause replayed another control");
  cases.push("scheduled pause and immediate override");

  for (const [name, transform] of [["control revision changed", work => ({ ...work, controlRevision: work.controlRevision + 1 })], ["status changed", work => ({ ...work, status: "completed" })],
    ["foreign receipt", work => ({ ...work, threadId: "thread-b" })]]) {
    tasks = initialTasks(); freshTransform = transform; await mount(); await open(); const before = calls.pause.length; originalChoice().click();
    await until(() => rail().textContent.includes(name === "foreign receipt" ? "任务来源已变化" : "原任务状态已变化"), `${name} passed`);
    check(calls.pause.length === before, `${name} paused stale Work`); cases.push(name);
  }
  freshTransform = work => work; tasks = initialTasks(); nativeFailure = "Native拒绝：暂停状态未生效"; await mount(); await open();
  const beforeRefresh = calls.refresh; originalChoice().click(); await until(() => rail().textContent.includes(nativeFailure), "Native failure hidden");
  await until(() => calls.refresh > beforeRefresh, "failed pause did not refresh persisted state"); check(!rail().textContent.includes("已确认暂停任务"), "failed pause showed success"); cases.push("Native failure refresh"); nativeFailure = null;
  for (const transform of [work => ({ ...work, status: "running" }), work => ({ ...work, threadId: "thread-b" }), work => ({ ...work, controlRevision: 2 })]) {
    tasks = initialTasks(); pauseTransform = transform; await mount(); await open(); originalChoice().click();
    await until(() => rail().textContent.includes("任务状态回执无法核对"), "unproved pause receipt accepted"); check(!rail().textContent.includes("已确认暂停任务"), "invalid receipt showed success");
  }
  cases.push("invalid pause receipt"); pauseTransform = work => work;
  tasks = initialTasks(); await mount(); await open(); const beforeFailedRefreshPause = calls.pause.length;
  formalFailure = "作品读取暂时失败"; originalChoice().click();
  await until(() => rail().textContent.includes("操作已返回成功，但作品界面刷新失败"), "successful pause refresh failure was hidden");
  check(calls.pause.length === beforeFailedRefreshPause + 1 && tasks[0].status === "paused", "refresh failure replayed pause or lost its receipt");
  formalFailure = null; cases.push("successful pause refresh failure");

  tasks = initialTasks(); readGate = deferred(); await mount(); await open(); const raceReadCount = calls.reads.length, beforePause = calls.pause.length;
  originalChoice().click(); await until(() => calls.reads.length === raceReadCount + 1, "race read absent"); const oldRead = readGate; readGate = null;
  oldRead.resolve(structuredClone(tasks[0])); document.querySelector<HTMLButtonElement>('[aria-label="切换对话：另一对话"]').click();
  await until(() => rail().dataset.conversationId === "thread-b", "thread not changed"); await tick(); check(calls.pause.length === beforePause && !rail().textContent.includes("原任务状态已变化"), "live navigation race paused previous Work"); cases.push("live thread race");
  readGate = deferred(); await mount(); await open(); originalChoice().click(); const oldProjectRead = readGate; readGate = null;
  await mount("project-b", threadA); oldProjectRead.reject(Error("旧作品迟到读取错误")); await tick(); await tick();
  check(calls.pause.length === beforePause && !rail().textContent.includes("旧作品迟到读取错误"), "late project result polluted new project"); cases.push("late project read");
  tasks = initialTasks(); await mount(); await open(); pauseGate = deferred(); const latePause = pauseGate; originalChoice().click(); await until(() => calls.pause.length === beforePause + 1, "late pause not started");
  document.querySelector<HTMLButtonElement>('[aria-label="切换对话：另一对话"]').click(); await until(() => rail().dataset.conversationId === "thread-b", "pending pause navigation blocked");
  tasks[0].status = "paused"; tasks[0].version++; tasks[0].controlRevision++; latePause.resolve(structuredClone(tasks[0])); pauseGate = null; await tick(); await tick();
  check(!rail().textContent.includes("已确认暂停任务") && !rail().textContent.includes("原任务状态已变化"), "late receipt appeared in another conversation"); cases.push("late pause receipt");
  window.testResult = { passed: true, cases, calls };
} catch (error) { window.testResult = { passed: false, error: String(error), calls, dom: document.body.innerHTML }; }
finally { root?.unmount(); } })();
