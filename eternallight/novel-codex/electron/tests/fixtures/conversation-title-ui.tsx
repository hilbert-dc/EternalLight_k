import { chapterRuntimeReads } from "../helpers/chapter-runtime";
import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import { conversationTitle } from "../../../src/conversation-title";
import { saveAuthor } from "../helpers/author-session";

const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (condition, message) => { if (!condition) throw Error(message); };
const until = async predicate => { for (let i = 0; i < 160 && !predicate(); i++) await tick(); check(predicate(), "Title UI did not settle"); };
const projects = [{ id: "project-last-lamp", title: "标题体验 A", logline: "", updated_at: "" }, { id: "project-B", title: "标题体验 B", logline: "", updated_at: "" }];
const rawPreview = "Novel Codex 专用工作约束 #Novel Workflow INTERNAL_BOUND_CONTEXT";
const originalPrompt = "请检查这一章的线索";
const makeThread = (id, name = null) => ({ id, name, preview: rawPreview, modelProvider: "fixture", status: "idle", createdAt: null, updatedAt: null, isPinned: false, ephemeral: false });
const old = makeThread("old"), live = makeThread("live"), named = makeThread("named", "已命名的创作对话");
let threads = [old, named], listener, complete, sendCount = 0;
const reads = [], resumes = [];
const state = project => ({ project, canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1.0", scenes: [], anchors: [] }, novelEdition: { chapters: [] } });
const threadFor = (projectId, id) => projectId === projects[1].id ? makeThread(id) : threads.find(t => t.id === id);
window.novelCodex = {
  app: { onCloseRequested: () => () => {} },
  runtime: { ...chapterRuntimeReads(id => projects.find(project => project.id === id)), listWorkTasks: async () => ({ tasks: [], nextCursor: null, activeCount: 0 }), status: async () => ({ state: "online", mode: "local", error: null }),
    listProjects: async () => ({ projects }), getState: async id => state(projects.find(p => p.id === id)) },
  agent: { status: async () => ({ configured: true, state: "ready", mode: "novel-agent-engine", error: null }),
    listConnections: async () => ({ connections: [] }), recoveryReport: async () => [], listProposedActions: async () => ({ actions: [] }), setWorkspaceContext: async () => {},
    listThreads: async projectId => ({ threads: projectId === projects[1].id ? [makeThread("live")] : threads, nextCursor: null }),
    onEvent: fn => { listener = fn; return () => {}; }, onUserInputRequest: () => () => {},
    sendPrompt: async (projectId, prompt) => {
      check(projectId === projects[0].id && prompt === originalPrompt, "Title presentation changed the author request"); sendCount++;
      threads = [live, ...threads];
      listener({ method: "thread/started", params: { thread: live } });
      listener({ method: "turn/started", params: { threadId: live.id, turn: { id: "live-turn" } } });
      await new Promise(resolve => { complete = resolve; });
      listener({ method: "turn/completed", params: { threadId: live.id, turn: { id: "live-turn", status: "completed", items: [] } } });
      return { message: "检查完成", proposedActions: [] };
    },
    readThread: async (projectId, id) => { reads.push({ projectId, id }); return { thread: { ...threadFor(projectId, id) }, turns: [] }; },
    resumeThread: async (projectId, id) => { resumes.push({ projectId, id }); return { thread: { ...threadFor(projectId, id) } }; },
  },
};
saveAuthor({ name: "标题体验作者", email: "title@example.invalid", passwordSalt: "fixture", passwordHash: "fixture" });
const root = createRoot(document.getElementById("root"));
root.render(<App />);
const sidebarTitles = () => [...document.querySelectorAll(".sidebar-task")].map(node => node.title);
const historyTitles = () => [...document.querySelectorAll(".thread-history-copy strong")].map(node => node.textContent);
const titleSurfaces = () => `${document.querySelector(".sidebar")?.outerHTML}${document.querySelector(".thread-history-panel")?.outerHTML}${document.querySelector(".composer-activity")?.outerHTML}`;
const noPreview = () => check(!titleSurfaces().includes(rawPreview) && !titleSurfaces().includes("INTERNAL_BOUND_CONTEXT"), "Model input leaked into an author title, subtitle, tooltip, or action");
const readFirst = () => document.querySelector('.thread-history-row [aria-label^="查看对话详情"]')?.click();
const snapshot = {};
(async () => { try {
  await until(() => document.querySelectorAll(".sidebar-task").length === 2);
  document.querySelector(".thread-history-disclosure > summary").click(); await tick();
  noPreview(); check(sidebarTitles().join("|") === "创作对话|已命名的创作对话", "Unnamed or named titles are incorrect");
  snapshot.initial = { sidebar: sidebarTitles(), history: historyTitles(), subtitle: document.querySelector(".thread-history-copy small").textContent };
  const author = { projectId: projects[0].id, threadId: live.id, prompt: originalPrompt };
  check(conversationTitle(live, projects[1].id, author) === "创作对话" && conversationTitle(old, projects[0].id, author) === "创作对话", "Foreign project/thread acquired another author's title");
  check(conversationTitle(makeThread("emoji", "😀".repeat(100)), projects[0].id, null) === "😀".repeat(72), "Title cap split Unicode characters or failed to bound names");
  const composer = document.querySelector('textarea[aria-label="创作指令"]');
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(composer, originalPrompt);
  composer.dispatchEvent(new Event("input", { bubbles: true })); await tick();
  document.querySelector('[aria-label="发送创作指令"]').click();
  await until(() => sidebarTitles()[0] === originalPrompt); noPreview();
  check(sidebarTitles()[1] === "创作对话" && historyTitles()[0] === originalPrompt, "Current prompt escaped its exact thread or failed in history");
  readFirst(); await until(() => document.querySelector(".thread-detail-header strong")); noPreview();
  check(document.querySelector(".thread-detail-header strong").textContent === originalPrompt, "Unnamed current detail lost the author title");
  snapshot.namingWindow = { sidebar: sidebarTitles(), history: historyTitles(), detail: document.querySelector(".thread-detail-header strong").textContent };
  document.querySelector(".thread-detail-close").click();
  live.name = "线索检查";
  listener({ method: "thread/name/updated", params: { threadId: live.id, name: live.name } });
  await until(() => sidebarTitles()[0] === live.name); noPreview();
  check(historyTitles()[0] === live.name, "Name notification did not replace the temporary title");
  readFirst(); await until(() => document.querySelector(".thread-detail-header strong")?.textContent === live.name);
  snapshot.named = { sidebar: sidebarTitles()[0], history: historyTitles()[0], detail: document.querySelector(".thread-detail-header strong").textContent };
  complete(); await until(() => !document.querySelector('.project-switcher').disabled);
  document.querySelector('.sidebar-task[title="创作对话"]').click();
  await until(() => document.querySelector(".thread-detail-header strong")?.textContent === "创作对话"); noPreview();
  snapshot.historyView = { detail: document.querySelector(".thread-detail-header strong").textContent, current: document.querySelector(".sidebar-task.current").title };
  check(document.querySelector(".sidebar-task.current").title === live.name, "Readonly view changed the execution title");
  document.querySelector(".thread-history-refresh").click(); await tick(); noPreview();
  document.querySelector(".project-switcher").click(); await tick();
  [...document.querySelectorAll(".project-menu-item")].find(node => node.querySelector("strong")?.textContent === projects[1].title).click();
  await until(() => document.querySelector(".project-card strong")?.textContent === projects[1].title && sidebarTitles().length === 1);
  noPreview(); check(sidebarTitles()[0] === "创作对话" && !titleSurfaces().includes(originalPrompt) && !titleSurfaces().includes("线索检查"), "Same thread ID in another project leaked the previous title");
  document.querySelector(".thread-history-disclosure > summary").click(); await tick(); readFirst();
  await until(() => document.querySelector(".thread-detail-header strong")); noPreview();
  check(document.querySelector(".thread-detail-header strong").textContent === "创作对话", "Foreign-project detail acquired the previous author prompt");
  snapshot.otherProject = { sidebar: sidebarTitles(), history: historyTitles(), detail: document.querySelector(".thread-detail-header strong").textContent };
  check(old.preview === rawPreview && live.preview === rawPreview && named.preview === rawPreview, "Presentation rewrote source preview/history");
  check(sendCount === 1 && resumes.length === 0 && reads.at(-1).projectId === projects[1].id, "Title changes triggered model execution or changed navigation identity");
  window.testResult = { passed: true, snapshot };
} catch (error) { window.testResult = { passed: false, error: error.stack + "\nDOM:" + document.body.innerText }; } })();
