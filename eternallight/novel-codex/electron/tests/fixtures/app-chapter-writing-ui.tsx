import React from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
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
window.novelCodex = {
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
  agent: { status: async () => ({ configured: true, state: "ready", mode: "novel-agent-engine", provider: "fixture", model: "fixture", error: null }), listConnections: async () => ({ connections: [] }), recoveryReport: async () => [], listThreads: async () => ({ threads: [], nextCursor: null }), listProposedActions: async () => ({ actions: [] }), setWorkspaceContext: async () => {}, onEvent: fn => { listener = fn; return () => {}; }, onUserInputRequest: () => () => {},
    generateChapter: async (projectId, request) => { generations.push({ projectId, ...structuredClone(request) }); activeOperation = "writer"; execution = deferred(); turn++; event("thread/started", { thread: { id: "thread" } }); event("turn/started", { threadId: "thread", turn: { id: `turn-${turn}` } }); return execution.promise; },
    steer: async (projectId, prompt) => { check(activeOperation === "advisory", "当前作品没有可追加指令的对话任务"); steers.push({ projectId, prompt }); return { steered: true }; },
    sendPrompt: async (projectId, prompt, repairSource, conversationFocus) => { const chapter = conversationFocus && workspaces.get(projectId).chapters.find(item => item.id === conversationFocus.chapterId); discussions.push({ projectId, prompt, repairSource, conversationFocus: structuredClone(conversationFocus), body: chapter?.candidate?.body ?? null }); activeOperation = delegateWriter ? "writer" : "advisory"; execution = deferred(); turn++;
      const source = { id: `author-${turn}`, projectId, threadId: "thread", operation: "advisory", target: {} };
      event("agent/workflow", { workflow: { task: source } }); event("thread/started", { thread: { id: "thread" } }); event("turn/started", { threadId: "thread", turn: { id: `turn-${turn}` } });
      if (delegateWriter) { event("agent/candidate-started", { projectId, sourceTaskId: source.id, actionId: `action-${turn}`, operation: "generateChapter", target: { chapterId: chapter.id } }); delegatedWriters.push({ chapterId: chapter.id, intent: prompt });
        event("turn/completed", { threadId: "thread", turn: { id: `turn-${turn}`, status: "completed", items: [] } });
        event("agent/workflow", { workflow: { task: { id: `writer-${turn}`, projectId, threadId: "thread", operation: "generateChapter", target: { chapterId: chapter.id, requestedByTaskId: source.id, requestedActionId: `action-${turn}` } } } });
        event("turn/started", { threadId: "thread", turn: { id: `turn-${turn}` } }); }
      return execution.promise; },
  },
};
const root = createRoot(document.getElementById("root"));
const navigate = async label => { [...document.querySelectorAll(".nav-item")].find(node => node.textContent.includes(label)).click(); await until(() => document.querySelector<HTMLElement>(".main-panel")?.dataset.view === (label === "作品目录" ? "chapters" : "characters"), `Navigation ${label} failed`); await tick(); };
const prepare = async () => { await until(() => button("让 AI 起草本章") && !button("让 AI 起草本章").matches(":disabled"), "Preparation unavailable"); button("让 AI 起草本章").click(); await until(() => document.querySelector(".chapter-writing-mode"), "No chapter mode"); };
const settleFailure = async () => { execution.reject(Error("真实模型测试失败")); await until(() => !document.querySelector('[aria-label="停止当前执行"]'), "Failure did not settle"); await until(() => !document.querySelector('[aria-label="场景 1 正文"]')?.matches(":disabled"), "Failure left chapter locked"); await tick(); };
let phase = "boot";
const finish = async (message = "讨论完成") => {
  event("turn/completed", { threadId: "thread", turn: { id: "turn-" + turn, status: "completed", items: [] } });
  execution.resolve({ message, proposedActions: [] });
  await until(() => !document.querySelector('[aria-label="停止当前执行"]'), "Completion did not settle");
  await tick();
};
const cancelMode = async () => { button("取消写作")?.click(); await tick(); };
(async () => { try {
  saveAuthor({ name: "统一输入作者", email: "one-composer@example.invalid", passwordSalt: "fixture", passwordHash: "fixture" }); root.render(<App />);
  await until(() => stateReads >= 1 && composer() && document.querySelector(".chapter-document"), "App did not initialize"); await navigate("作品目录");
  await until(() => document.querySelector(".chapter-document"), "Chapter did not load");
  phase = "startup refresh retains selected chapter";
  check(stateReads === 1, "Fixture missed the initial selection-before-refresh window");
  button("下一章").click();
  await until(() => document.querySelector<HTMLInputElement>('[aria-label="章节标题"]')?.value === "新章", "Initial second chapter did not load");
  await until(() => stateReads >= 2, "Startup refresh did not run"); await tick();
  check(document.querySelector<HTMLInputElement>('[aria-label="章节标题"]')?.value === "新章", "Same-project startup refresh stole the selected chapter");
  snapshots.push({ case: phase, selectedChapterPreserved: true });
  button("上一章").click(); await until(() => document.querySelector<HTMLInputElement>('[aria-label="章节标题"]')?.value === "本章", "First chapter did not restore");
  check(document.querySelectorAll('.draft-editor-wrap textarea').length === 1 && !document.querySelector(".chapter-manual-tools").open && !document.querySelector(".chapter-versions"), "Default chapter has extra inputs, expanded tools or an empty published comparison");
  for (const theme of ["light", "dark"]) {
    document.documentElement.dataset.theme = theme; await tick();
    const panel = document.querySelector(".main-panel.chapter-main-panel"), prose = document.querySelector(".chapter-document textarea");
    check(CSS.supports("field-sizing", "content") && prose.scrollHeight <= prose.clientHeight + 2 && getComputedStyle(prose).maxHeight === "none", "Actual App retained an internal prose scroll");
    check(getComputedStyle(panel).backgroundColor === getComputedStyle(document.querySelector(".chapter-document")).backgroundColor, "Actual App theme breaks the continuous document surface");
    panel.scrollTop = panel.scrollHeight; await tick(); const footer = document.querySelector(".chapter-save-toolbar").getBoundingClientRect(), bounds = panel.getBoundingClientRect();
    check(footer.top >= bounds.top && footer.bottom <= bounds.bottom + 2 && panel.scrollWidth <= panel.clientWidth + 2, "Actual App save/publish footer is not reachable inside main");
    snapshots.push({ case: "document layout " + theme, mainOnlyScroll: true, continuousSurface: true, footerReachable: true }); panel.scrollTop = 0;
  }
  document.documentElement.dataset.theme = "light";
  phase = "existing chapter ordinary conversation";
  await input("创作指令", "尚未发送的作者要求");
  check(!button("让 AI 起草本章") && !button("讨论本章") && !button("新建章节") && !document.querySelector(".chapter-writing-mode") && composer().value === "尚未发送的作者要求" && generations.length === 0 && discussions.length === 0, "Existing chapter retained a forced writer entry or consumed draft");
  snapshots.push({ case: phase, noForcedWritingMode: true, draftPreserved: true, implicitChapterContext: true });

  phase = "fresh discussion single flight";
  await input("创作指令", "分析本章的事实，不改写正文。"); await input("场景 1 正文", "准备讨论后才修改的正文。");
  holdPreflight = true; preflight = null;
  check(send().disabled && !button("让 AI 起草本章"), "Dirty ordinary discussion bypassed explicit save"); button("保存").click();
  await until(() => preflight, "Fresh saved workspace read was not deferred");
  check(saves.length === 1 && saves[0].body === "准备讨论后才修改的正文。" && discussions.length === 0 && generations.length === 0
    && composer().value === "分析本章的事实，不改写正文。", "Preparation duplicated writes, ran writer or consumed input before saving");
  preflight.resolve(structuredClone(workspaces.get(projects[0].id)));
  await until(() => !send().disabled, "Saved source did not unlock ordinary discussion"); send().click(); send().click();
  await until(() => discussions.length === 1 && composer().value === "", "Fresh discussion did not start");
  check(discussions[0].projectId === projects[0].id && discussions[0].prompt === "分析本章的事实，不改写正文。"
    && discussions[0].repairSource === null && discussions[0].conversationFocus?.chapterId === chapters[0].id
    && discussions[0].conversationFocus?.chapterNumber === 1 && discussions[0].body === saves[0].body && generations.length === 0, "Discussion used wrong source, target or execution route");
  const failedAnalysis = { id: "analysis-failed", taskKind: "model", status: "running", target: { chapterId: chapters[0].id } };
  workspaces.get(projects[0].id).tasks = [failedAnalysis];
  await input("创作指令", "分析结束后再讨论的草稿");
  event("turn/completed", { threadId: "thread", turn: { id: `turn-${turn}`, status: "completed", items: [] } }); await tick();
  check(document.querySelector('[aria-label="停止当前执行"]') && document.querySelector('[aria-label="场景 1 正文"]').matches(":disabled"), "Advisory completion unlocked an unfinished analysis pipeline");
  failedAnalysis.status = "failed"; await settleFailure();
  check(!document.querySelector('[aria-label="场景 1 正文"]').matches(":disabled"), "Finished failed analysis retained a stale Runtime task lock");
  check(composer().value === "分析结束后再讨论的草稿", "Failed discussion overwrote a newer draft");
  snapshots.push({ case: phase, saves: saves.length, discussionCalls: discussions.length, writerCalls: generations.length, conversationFocus: discussions[0].conversationFocus, nextDraftRetained: true });

  phase = "failed prepare preserves input";
  await input("创作指令", "保存成功后再分析"); await input("场景 1 正文", "失败保存仍保留这段作者正文。");
  failNextSave = true; check(send().disabled, "Dirty discussion was sendable before save"); button("保存").click();
  await until(() => !button("保存").disabled && document.body.innerText.includes("正文保存失败，未启动模型"), "Failed save did not settle");
  check(discussions.length === 1 && generations.length === 0 && composer().value === "保存成功后再分析"
    && document.querySelector<HTMLTextAreaElement>('[aria-label="场景 1 正文"]').value === "失败保存仍保留这段作者正文。", "Failed preparation lost input, edited body or started a task");
  snapshots.push({ case: phase, modelNotStarted: true, inputAndBodyRetained: true });
  button("保存").click(); await until(() => !send().disabled, "Retry save did not unlock conversation"); send().click(); await until(() => discussions.length === 2 && composer().value === "", "Retry after failed preparation did not start");
  await input("创作指令", "下一份尚未发送的构想"); await finish();
  check(saves.length === 2 && discussions[1].body === "失败保存仍保留这段作者正文。" && composer().value === "下一份尚未发送的构想", "Discussion retry lost the saved body or cleared a new draft");

  phase = "new chapter natural goal delegation";
  await cancelMode(); await input("创作指令", ""); button("下一章").click();
  await until(() => document.querySelector<HTMLInputElement>('[aria-label="章节标题"]')?.value === "新章", "Next chapter did not load"); await prepare();
  const firstIntent = composer().value;
  check(document.querySelectorAll('.chapter-document [aria-label="让 AI 起草本章"]').length === 1 && !document.querySelector('.chapter-page-toolbar [aria-label="让 AI 起草本章"]') && button("让 AI 起草") && firstIntent.includes("2500") && firstIntent.includes("新章") && delegatedWriters.length === 0 && discussions.length === 2, "Empty chapter lacks editable default or auto-started writing");
  delegateWriter = true; send().click(); send().click(); await until(() => delegatedWriters.length === 1 && composer().value === "", "Exact default did not run writer once");
  check(delegatedWriters[0].chapterId === chapters[1].id && delegatedWriters[0].intent === firstIntent && discussions.length === 3, "Default writer target or instruction changed");
  await settleFailure(); check(composer().value === "", "A task failure must not resubmit the original author goal");
  snapshots.push({ case: phase, naturalGoalDelegated: true, noAutomaticStart: true, noOriginalGoalReplay: true });

  phase = "writer buffers next input";
  await input("创作指令", firstIntent); send().click(); await until(() => delegatedWriters.length === 2 && composer().value === "", "Writer retry did not start");
  await input("创作指令", "本轮结束后再处理的要求");
  composer().dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); await tick();
  check(send()?.disabled && document.querySelector('[aria-label="停止当前执行"]') && steers.length === 0 && delegatedWriters.length === 2
    && composer().value === "本轮结束后再处理的要求", "Writer incorrectly offered steer or consumed next draft");
  await settleFailure(); check(composer().value === "本轮结束后再处理的要求", "Writer failure overwrote buffered input");
  await cancelMode(); await input("创作指令", ""); await prepare(); send().click();
  await until(() => delegatedWriters.length === 3 && composer().value === "", "Second exact default did not run");
  await input("创作指令", "随后又输入"); await input("创作指令", ""); await settleFailure();
  check(composer().value === "", "Failure resurrected an input after author cleared a newer draft");
  snapshots.push({ case: phase, stopAvailable: true, writerSteerCalls: steers.length, nextDraftRetained: true, clearedDraftNotRestored: true });

  phase = "edited default becomes discussion"; delegateWriter = false;
  await cancelMode(); await input("创作指令", ""); await prepare();
  await input("创作指令", "写下一章，接着调查港口的来信，不改当前这一章。"); send().click();
  await until(() => discussions.length === 6 && composer().value === "", "Edited default did not start ordinary discussion");
  check(delegatedWriters.length === 3 && discussions[5].conversationFocus?.chapterId === chapters[1].id && discussions[5].conversationFocus?.chapterNumber === 2
    && discussions[5].prompt === "写下一章，接着调查港口的来信，不改当前这一章。", "Edited intent still ran writer or lost chapter focus");
  await finish();
  snapshots.push({ case: phase, writerCalls: delegatedWriters.length, discussionCalls: discussions.length, conversationFocus: discussions[5].conversationFocus, nextChapterInstructionPreserved: true });

  phase = "delayed preparation project isolation";
  await input("创作指令", ""); await prepare(); holdPreflight = true; preflight = null; send().click();
  await until(() => preflight, "Late writer preparation was not deferred"); const lateRead = preflight;
  check(delegatedWriters.length === 3 && composer().value === firstIntent, "Deferred preparation started writer or consumed original input");
  document.querySelector<HTMLButtonElement>(".project-switcher").click(); await tick();
  [...document.querySelectorAll(".project-menu-item")].find(node => node.textContent.includes(projects[1].title)).click();
  await until(() => document.querySelector(".project-card strong")?.textContent === projects[1].title && !composer().disabled, "Project switch did not settle");
  check(!document.querySelector(".chapter-writing-mode") && composer().value === "", "Project switch retained another chapter mode or draft");
  await input("创作指令", "B作品独立草稿"); lateRead.resolve(structuredClone(workspaces.get(projects[0].id))); await tick(); await tick();
  check(delegatedWriters.length === 3 && discussions.length === 6 && composer().value === "B作品独立草稿"
    && !document.querySelector(".chapter-writing-mode") && !document.querySelector('[aria-label="停止当前执行"]'), "Late preparation executed or changed another scope");
  snapshots.push({ case: phase, lateWriterCalls: 0, isolatedDraft: composer().value });

  await navigate("角色");
  phase = "ordinary advisory steer";
  await input("创作指令", "普通只读讨论"); send().click();
  await until(() => discussions.length === 7 && document.querySelector('[aria-label="停止当前执行"]'), "Ordinary discussion did not start");
  check(discussions[6].conversationFocus?.view === "characters" && !discussions[6].conversationFocus.chapterId, "Non-chapter discussion retained a chapter focus");
  await input("创作指令", "对话中追加问题"); composer().dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await until(() => steers.length === 1 && composer().value === "", "Advisory Enter did not steer");
  event("turn/completed", { threadId: "thread", turn: { id: `turn-${turn}`, status: "completed", items: [] } }); await tick();
  check(document.querySelector('[aria-label="停止当前执行"]') && document.querySelector(".composer-model-picker button").matches(":disabled"), "Completed advisory turn released the unfinished analysis pipeline or model selection");
  await input("创作指令", "分析结束后发送的下一条草稿");
  composer().dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); await tick();
  check(steers.length === 1 && discussions.length === 7 && send().disabled && composer().value === "分析结束后发送的下一条草稿", "Analysis pipeline Enter sent or steered a new instruction");
  await finish();
  check(!send().disabled && composer().value === "分析结束后发送的下一条草稿", "Pipeline completion did not unlock or retain the next draft");

  phase = "success preserves new draft"; delegateWriter = true;
  document.querySelector<HTMLButtonElement>(".project-switcher").click(); await tick();
  [...document.querySelectorAll(".project-menu-item")].find(node => node.textContent.includes(projects[0].title)).click();
  await until(() => document.querySelector(".project-card strong")?.textContent === projects[0].title && !composer().disabled, "Return project did not settle"); await navigate("作品目录");
  await until(() => document.querySelector('[aria-label="章节标题"]') && button("下一章"), "Chapter navigation did not settle");
  if (document.querySelector<HTMLInputElement>('[aria-label="章节标题"]')?.value !== "新章") button("下一章").click();
  await until(() => document.querySelector<HTMLInputElement>('[aria-label="章节标题"]')?.value === "新章", "Empty chapter did not restore");
  await input("创作指令", ""); await prepare(); send().click();
  await until(() => delegatedWriters.length === 4 && composer().value === "", "Successful default writer did not start");
  await input("创作指令", "完成后继续讨论的草稿");
  const chapter = workspaces.get(projects[0].id).chapters[1];
  chapter.candidate = { ...revision("generated", "模型已保存的正文。"), title: "新章" }; workspaces.get(projects[0].id).revision++;
  await finish("正文已保存"); await until(() => !document.querySelector(".chapter-writing-mode"), "Successful writer did not clear chapter mode");
  check(composer().value === "完成后继续讨论的草稿" && document.querySelector<HTMLTextAreaElement>('[aria-label="场景 1 正文"]').value === "模型已保存的正文。", "Successful old result cleared next draft or did not refresh saved body");
  snapshots.push({ case: phase, nextDraftRetained: true, savedBodyVisible: true });
  await navigate("角色"); check(!document.querySelector(".chapter-writing-mode"), "View switch retained chapter mode");
  window.testResult = { passed: true, snapshots, advisoryCalls: discussions.length, saves: saves.length, generations: generations.length, delegatedWriters: delegatedWriters.length, steers: steers.length };
} catch (error) { window.testResult = { passed: false, error: phase + ": " + error.stack, dom: document.body.innerText.slice(0, 9000), counts: { saves: saves.length, generations: generations.length, delegatedWriters: delegatedWriters.length, steers: steers.length, advisoryCalls: discussions.length } }; } })();
