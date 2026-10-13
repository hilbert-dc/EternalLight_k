import { novelFixtureMedia } from "../helpers/novel-edition";
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
const makeThread = (id, name = null) => ({ id, projectId: projects[0].id, version: 0, isArchived: false, name, preview: rawPreview, modelProvider: "fixture", status: "idle", createdAt: null, updatedAt: null, isPinned: false, ephemeral: false });
const old = makeThread("old"), live = makeThread("live"), named = makeThread("named", "已命名的创作对话");
let threads = [old, named], listener, complete, sendCount = 0;
const reads = [], resumes = [];
const state = project => ({ project, canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1.0", scenes: [], anchors: [] }, novelEdition: { chapters: [] } });
const threadFor = (projectId, id) => projectId === projects[1].id ? { ...makeThread(id), projectId } : threads.find(t => t.id === id);
window.novelCodex = { multimedia: novelFixtureMedia,
  app: { onCloseRequested: () => () => {} },
  runtime: { ...chapterRuntimeReads(id => projects.find(project => project.id === id)), listWorkTasks: async () => ({ tasks: [], nextCursor: null, activeCount: 0 }), status: async () => ({ state: "online", mode: "local", error: null }),
    listProjects: async () => ({ projects }), getState: async id => state(projects.find(p => p.id === id)) },
  agent: { status: async () => ({ configured: true, state: "ready", mode: "novel-agent-engine", error: null }),
    listConnections: async () => ({ connections: [] }), recoveryReport: async () => [], listProposedActions: async () => ({ actions: [] }), setWorkspaceContext: async () => {},
    listExecutions: async () => [], readMessage: async () => { throw Error("message receipt not found"); }, beginInterlude: async () => { throw Error("unexpected interlude"); }, returnInterlude: async () => { throw Error("unexpected interlude return"); }, listMessages: async () => [], listThreads: async projectId => ({ threads: projectId === projects[1].id ? [{ ...makeThread("live"), projectId }] : threads, nextCursor: null }),
    onEvent: fn => { listener = fn; return () => {}; }, onUserInputRequest: () => () => {},
    createThread: async projectId => { live.projectId = projectId; live.updatedAt = 100; threads = [live, ...threads]; return { thread: { ...live } }; },
    sendPrompt: async (projectId, prompt, repair, focus, threadId) => {
      check(projectId === projects[0].id && prompt === originalPrompt, "Title presentation changed the author request"); sendCount++;
      check(threadId === live.id, "Request lost selected conversation");
      listener({ method: "agent/execution-request", params: { projectId, requestId: "title-request" } });
      listener({ method: "agent/message-updated", params: { projectId, messageId: "title-request", status: "queued" } });
      listener({ method: "thread/started", params: { thread: live } });
      listener({ method: "turn/started", params: { threadId: live.id, turn: { id: "live-turn" } } });
      await new Promise(resolve => { complete = resolve; });
      listener({ method: "turn/completed", params: { threadId: live.id, turn: { id: "live-turn", status: "completed", items: [] } } });
      return { message: "检查完成", proposedActions: [] };
    },
    readConversation: async (projectId, id) => { reads.push({ projectId, id }); return { thread: { ...threadFor(projectId, id) }, turns: [] }; },
    resumeThread: async (projectId, id) => { resumes.push({ projectId, id }); return { thread: { ...threadFor(projectId, id) } }; },
  },
};
saveAuthor({ name: "标题体验作者", email: "title@example.invalid", passwordSalt: "fixture", passwordHash: "fixture" });
const root = createRoot(document.getElementById("root"));
root.render(<App />);
const sidebarTitles = () => [...document.querySelectorAll(".conversation-select-button")].map(node => node.title);
const titleSurfaces = () => `${document.querySelector(".sidebar")?.outerHTML}${document.querySelector(".conversation-shell-header")?.outerHTML}`;
const noPreview = () => check(!titleSurfaces().includes(rawPreview) && !titleSurfaces().includes("INTERNAL_BOUND_CONTEXT"), "Model input leaked into an author title, subtitle, tooltip, or action");
const select = async id => { document.querySelector(`[data-conversation-row="${id}"] .conversation-select-button`).click(); await until(() => document.querySelector('.conversation-rail')?.getAttribute('data-conversation-id') === id); await tick(); };
const title = () => document.querySelector(".conversation-shell-header strong")?.textContent;
const snapshot = {};
(async () => { try {
  await until(() => sidebarTitles().length === 2);
  noPreview(); check(sidebarTitles().join("|") === "新对话|已命名的创作对话", "Unnamed or named titles are incorrect");
  snapshot.initial = { sidebar: sidebarTitles() };
  const author = { projectId: projects[0].id, threadId: live.id, prompt: originalPrompt };
  check(conversationTitle(live, projects[1].id, author) === "创作对话" && conversationTitle(old, projects[0].id, author) === "创作对话", "Foreign project/thread acquired another author's title");
  check(conversationTitle(makeThread("emoji", "😀".repeat(100)), projects[0].id, null) === "😀".repeat(72), "Title cap split Unicode characters or failed to bound names");
  document.querySelector('[aria-label="新建对话"]').click(); await until(() => document.querySelector('.conversation-rail')?.getAttribute('data-conversation-id') === live.id);
  const composer = document.querySelector('textarea[aria-label="创作指令"]');
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(composer, originalPrompt);
  composer.dispatchEvent(new Event("input", { bubbles: true })); await tick();
  document.querySelector('[aria-label="发送创作指令"]').click(); await until(() => complete && composer.value === ""); noPreview();
  check(title() === "新对话" && sidebarTitles()[0] === "新对话", "Pending title replaced persisted conversation name with an instruction or internal preview");
  snapshot.namingWindow = { sidebar: sidebarTitles(), detail: title() };
  listener({ method: "thread/name/updated", params: { projectId: projects[1].id, threadId: live.id, threadName: "别的作品名称" } });
  listener({ method: "thread/name/updated", params: { projectId: projects[0].id, threadId: "unknown-thread", threadName: "未知对话" } });
  await tick(); check(title() === "新对话" && sidebarTitles().length === 3, "Foreign or unknown notification changed conversation identity");
  live.name = "线索检查";
  listener({ method: "thread/name/updated", params: { threadId: live.id, threadName: live.name } });
  await until(() => title() === live.name && sidebarTitles()[0] === live.name); noPreview();
  snapshot.named = { sidebar: sidebarTitles()[0], detail: title() };
  // The current interface selects a readable conversation while the original execution remains owned by live.
  await select(old.id); noPreview();
  check(title() === "新对话" && document.querySelector('[data-conversation-row="live"]').classList.contains('is-running'), "Reading another conversation replaced the running execution identity");
  snapshot.historyView = { detail: title(), running: document.querySelector('[data-conversation-row="live"] .conversation-select-button').title };
  complete(); await until(() => !document.querySelector('[data-conversation-row="live"]').classList.contains('is-running'));
  document.querySelector(".project-switcher").click(); await tick();
  [...document.querySelectorAll(".project-menu-item")].find(node => node.querySelector("strong")?.textContent === projects[1].title).click();
  await until(() => document.querySelector(".project-switcher span[title]")?.textContent === projects[1].title && sidebarTitles().length === 1);
  await select("live"); noPreview();
  check(sidebarTitles()[0] === "新对话" && title() === "新对话" && !titleSurfaces().includes(originalPrompt) && !titleSurfaces().includes("线索检查"), "Same thread ID in another project leaked the previous title");
  snapshot.otherProject = { sidebar: sidebarTitles(), detail: title() };
  check(old.preview === rawPreview && live.preview === rawPreview && named.preview === rawPreview, "Presentation rewrote source preview/history");
  check(sendCount === 1 && resumes.length === 0 && reads.at(-1).projectId === projects[1].id, "Title changes triggered model execution or changed navigation identity");
  window.testResult = { passed: true, snapshot };
} catch (error) { window.testResult = { passed: false, error: error.stack + "\nDOM:" + document.body.innerText }; } })();
