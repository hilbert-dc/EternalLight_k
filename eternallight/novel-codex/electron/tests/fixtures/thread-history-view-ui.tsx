import { novelFixtureMedia } from "../helpers/novel-edition";
import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import { chapterRuntimeReads } from "../helpers/chapter-runtime";
import { saveAuthor } from "../helpers/author-session";

const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (condition, message) => { if (!condition) throw Error(message); };
const until = async predicate => { for (let n = 0; n < 150 && !predicate(); n++) await tick(); check(predicate(), "History UI did not settle"); };
const projects = [{ id: "project-last-lamp", title: "历史 A", logline: "", updated_at: "" }, { id: "project-B", title: "历史 B", logline: "", updated_at: "" }];
const thread = id => ({ id, name: id, preview: null, modelProvider: "fixture", status: "idle", createdAt: null, updatedAt: null, isPinned: false, ephemeral: false });
const runtimeState = project => ({ project, canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1.0", scenes: [], anchors: [] }, novelEdition: { chapters: [] } });
const forbidden = { resume: 0, unarchive: 0, write: 0 };
let listener, finishSend, sends = 0, failRead = false, deferRead = false, configured = false;
const reads = [], pending = [];
const detail = (projectId, id) => ({ thread: thread(id), turns: [{ id: `${id}-turn`, status: "inProgress", startedAt: null, completedAt: null,
  items: [{ id: `${id}-public`, type: "agentMessage", phase: "commentary", text: `${projectId}:${id}:公开记录` }] }] });
const forbiddenCall = kind => async () => { forbidden[kind]++; throw Error(`unexpected ${kind}`); };
window.novelCodex = { multimedia: novelFixtureMedia,
  app: { onCloseRequested: () => () => {} },
  runtime: { ...chapterRuntimeReads(id => projects.find(project => project.id === id)),
    status: async () => ({ state: "online", mode: "local", error: null }), listProjects: async () => ({ projects }),
    getState: async id => runtimeState(projects.find(project => project.id === id)), saveBrief: forbiddenCall("write") },
  agent: { status: async () => ({ configured, state: configured ? "ready" : "disconnected", mode: "novel-agent-engine", error: null }), listConnections: async () => ({ connections: [] }),
    recoveryReport: async () => [], listProposedActions: async () => ({ actions: [] }), setWorkspaceContext: async () => {},
    listExecutions: async () => [], readMessage: async () => { throw Error("message receipt not found"); }, beginInterlude: async () => { throw Error("unexpected interlude"); }, returnInterlude: async () => { throw Error("unexpected interlude return"); }, listMessages: async () => [], listThreads: async (_id, archived) => ({ threads: [thread(archived ? "archived-writer" : "writer-A"), thread("writer-B")], nextCursor: null }),
    onEvent: fn => { listener = fn; return () => {}; }, onUserInputRequest: () => () => {},
    resumeThread: forbiddenCall("resume"), unarchiveThread: forbiddenCall("unarchive"),
    readThread: async (projectId, id) => {
      reads.push({ projectId, id });
      if (deferRead) await new Promise(resolve => pending.push({ projectId, id, resolve }));
      if (failRead) throw Error("controlled history failure");
      return detail(projectId, id);
    },
    sendPrompt: async () => {
      sends++;
      listener({ method: "thread/started", params: { thread: thread("current") } });
      listener({ method: "turn/started", params: { threadId: "current", turn: { id: "current-turn" } } });
      await new Promise(resolve => { finishSend = resolve; });
      listener({ method: "turn/completed", params: { threadId: "current", turn: { id: "current-turn", status: "completed", items: [{ id: "answer", type: "agentMessage", phase: "final_answer", text: "当前创作回答" }] } } });
      return { message: "当前创作已结束", proposedActions: [] };
    },
  },
};
saveAuthor({ name: "历史作者", email: "history@example.invalid", passwordSalt: "fixture", passwordHash: "fixture" });
createRoot(document.getElementById("root")).render(<App />);
const button = text => [...document.querySelectorAll<HTMLButtonElement>("button")].find(node => node.textContent === text);
const open = id => { const node = document.querySelector<HTMLButtonElement>(`.sidebar-task[title="${id}"]`); check(node && !node.disabled, "History is blocked by execution"); node.focus({ preventScroll: true }); node.click(); return node; };
const currentComposer = () => document.querySelector<HTMLTextAreaElement>('[aria-label="创作指令"]');
const fill = value => { const node = currentComposer(); Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(node, value); node.dispatchEvent(new Event("input", { bubbles: true })); };
const historyVisible = () => {
  const node = document.querySelector<HTMLElement>(".thread-history-panel");
  if (!node) return false;
  const rect = node.getBoundingClientRect();
  return getComputedStyle(node).display !== "none" && rect.width > 0 && rect.height > 0;
};
const detailText = () => document.querySelector(".thread-detail")?.textContent ?? "";
const switchProject = async project => { document.querySelector<HTMLButtonElement>(".project-switcher").click(); await tick(); [...document.querySelectorAll<HTMLButtonElement>(".project-menu-item")].find(node => node.querySelector("strong")?.textContent === project.title).click(); await until(() => document.querySelector(".project-card strong")?.textContent === project.title); };
(async () => { try {
  await until(() => document.querySelector('.sidebar-task[title="writer-A"]'));
  check(!historyVisible(), "History should stay hidden until requested");
  failRead = true; open("writer-A"); await until(() => button("重新读取对话记录"));
  check(sends === 0 && historyVisible(), "Unconfigured history was hidden or called model");
  failRead = false; button("重新读取对话记录").click(); await until(() => detailText().includes("writer-A:公开记录"));
  check(historyVisible(), "Loaded history is visually hidden");
  for (const page of ["作品目录", "角色", "作品概况"]) {
    document.querySelector<HTMLElement>(page === "作品目录" ? ".sidebar .work-directory > summary" : `.sidebar button[aria-label="${page}"]`).click(); await tick();
    check(!document.querySelector("main .thread-history-panel"), "History leaked into the document page");
    check(historyVisible() && document.querySelectorAll(".conversation-rail .thread-history-panel").length === 1, "History must have one rail owner");
  }
  document.querySelector<HTMLElement>(".sidebar .work-directory > summary").click(); await tick();
  const rail = document.querySelector<HTMLElement>(".conversation-rail");
  rail.style.width = "300px";
  for (const theme of ["light", "dark"]) {
    document.documentElement.dataset.theme = theme; await tick();
    const reader = document.querySelector<HTMLElement>(".conversation-history-scroll");
    check(historyVisible() && reader.scrollWidth <= reader.clientWidth + 1, "History overflows the narrow rail");
  }
  rail.style.removeProperty("width");
  document.documentElement.dataset.theme = "light";
  document.querySelector<HTMLButtonElement>(".thread-detail-close").click(); await tick();
  check(!historyVisible(), "Closed history remains visible");
  configured = true; window.dispatchEvent(new Event("focus")); await tick();

  fill("当前创作指令"); await until(() => !document.querySelector<HTMLButtonElement>('[aria-label="发送创作指令"]').disabled); document.querySelector<HTMLButtonElement>('[aria-label="发送创作指令"]').click();
  await until(() => finishSend && document.querySelector('[aria-label="停止当前执行"]'));
  const composer = currentComposer(); fill("下一条未发送草稿"); await tick(); composer.setSelectionRange(2, 5);
  const liveScroller = document.querySelector(".conversation-rail-scroll");
  const trigger = open("writer-A"); await until(() => detailText().includes("公开记录"));
  check(currentComposer() === composer && composer.value === "下一条未发送草稿" && composer.selectionStart === 2 && composer.selectionEnd === 5, "Readonly history replaced composer or draft/selection");
  check(document.querySelector('[aria-label="停止当前执行"]') && document.querySelector('.conversation-turn:not(.is-earlier)')?.textContent.includes("当前创作指令"), "Readonly history lost running execution or stop");
  check(document.querySelector(".conversation-rail-scroll") === liveScroller && liveScroller.hasAttribute("inert"), "History replaced live scroller or left it interactive");
  check(detailText().includes("进行中") && detailText().includes("只读对话记录") && detailText().includes("不会恢复原任务"), "History fabricated completion or implied resume");
  button("返回当前对话").click(); await tick();
  check(document.querySelector(".conversation-rail-scroll") === liveScroller && !liveScroller.hasAttribute("inert"), "Return did not restore same live scroller");
  check(document.activeElement === trigger, "Close did not return focus");
  failRead = true; open("writer-B"); await until(() => button("重新读取对话记录"));
  check(composer.value === "下一条未发送草稿" && document.querySelector('[aria-label="停止当前执行"]'), "History error altered execution");
  failRead = false; button("重新读取对话记录").click(); await until(() => detailText().includes("writer-B:公开记录"));
  // A late read cannot suppress the real terminal event.
  deferRead = true; open("writer-A"); await until(() => pending.length === 1);
  check(historyVisible(), "Loading history is visually hidden");
  finishSend(); await until(() => document.querySelector(".conversation-answer")?.textContent.includes("当前创作回答"));
  pending.shift().resolve(); await until(() => detailText().includes("writer-A:公开记录"));
  check(composer.value === "下一条未发送草稿", "Completion during history reading replaced draft");
  // Latest selection wins, even when A is selected again.
  open("writer-A"); await until(() => pending.length === 1); const oldA = pending.shift();
  open("writer-B"); await until(() => pending.length === 1); const oldB = pending.shift();
  open("writer-A"); await until(() => pending.length === 1); const newA = pending.shift();
  newA.resolve(); await until(() => detailText().includes("writer-A:公开记录")); oldB.resolve(); oldA.resolve(); await tick();
  check(!detailText().includes("writer-B:公开记录"), "ABA late read overwrote latest selection");
  open("writer-A"); await until(() => pending.length === 1); const closePending = pending.shift();
  document.querySelector<HTMLButtonElement>(".thread-detail-loading button").click(); closePending.resolve(); await tick();
  check(!document.querySelector(".thread-detail") && !historyVisible(), "Closed pending read reopened detail");
  open("writer-A"); await until(() => pending.length === 1); const oldProject = pending.shift();
  await switchProject(projects[1]); await switchProject(projects[0]); oldProject.resolve(); await tick();
  check(!document.querySelector(".thread-detail"), "Project ABA accepted old response");
  deferRead = false; button("已归档").click(); await until(() => document.querySelector('.sidebar-task[title="archived-writer"]'));
  document.querySelector<HTMLDetailsElement>(".thread-history-disclosure").open = true; await tick();
  document.querySelector<HTMLButtonElement>('.thread-history-open[aria-label="查看对话记录：archived-writer"]').click();
  await until(() => detailText().includes("archived-writer:公开记录"));
  check(JSON.stringify(forbidden) === JSON.stringify({ resume: 0, unarchive: 0, write: 0 }) && sends === 1, "Browsing resumed, unarchived, wrote, or called model");
  window.testResult = { passed: true, snapshot: { calls: forbidden, sendCount: sends, readonlyArchived: true, railOnlyAcrossThreePages: true, narrowThemesWithoutOverflow: true, liveScrollerPreserved: true, unconfiguredHistoryVisible: true, visibleLoadingErrorDetail: true, closedHistoryHidden: true, pendingTerminalPreserved: true,
    scopeABA: true, selectionABA: true, stoppedReadDoesNotReopen: true, composerSameNode: true, draftSelectionPreserved: true, focusReturned: true,
    realTurnStatus: "进行中", label: "只读对话记录" } };
} catch (error) { window.testResult = { passed: false, error: error.stack + "\nDOM:" + document.body.innerText, calls: forbidden }; } })();
