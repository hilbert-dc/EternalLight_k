import { novelFixtureMedia, fixtureCreatedNovel } from "../helpers/novel-edition";
import { chapterRuntimeReads } from "../helpers/chapter-runtime";
import React from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import { hashPassword } from "../../../src/author-session";
import { saveAuthor } from "../helpers/author-session";
import { resizeTestViewport, surfaceGeometry, captureTestSurface } from "../helpers/page-geometry";
import "../../../src/styles.css";

const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (condition, message) => { if (!condition) throw Error(message); };
const until = async predicate => { for (let i = 0; i < 180 && !predicate(); i++) await tick(); check(predicate(), `UI did not settle: ${String(predicate)} / ${document.body.textContent.slice(-2000)}`); };
const projects = Array.from({ length: 42 }, (_, i) => ({ id: i ? `project-${i}` : "project-last-lamp", title: `作品 ${i}`, logline: "", updated_at: "" }));
const state = project => ({ project, canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1.0", scenes: [], anchors: [] }, novelEdition: { chapters: [] } });
let modelCalls = 0, createCalls = 0, settleCreate, listener;
window.novelCodex = { multimedia: novelFixtureMedia,
  app: { onCloseRequested: () => () => {} },
  runtime: { ...chapterRuntimeReads(id => projects.find(project => project.id === id)), listWorkTasks: async () => ({ tasks: [], nextCursor: null, activeCount: 0 }),
    ...Object.fromEntries(["Search", "Entity", "Relation", "Knowledge", "SnapshotKnowledge", "Fact"].map(kind => [`get${kind}Audit`, async () => ({ job: null })])), status: async () => ({ state: "online", mode: "local", error: null }), listProjects: async () => ({ projects }),
    getState: async id => state(projects.find(project => project.id === id)),
    createProject: async (_title, _logline, medium, key) => { check(medium === "novel" && typeof key === "string", "Project medium or idempotency identity missing"); createCalls++; await new Promise((resolve, reject) => { settleCreate = { resolve, reject }; }); return fixtureCreatedNovel(projects[0]); } },
  agent: { status: async () => ({ configured: true, state: "ready", mode: "novel-agent-engine", error: null }),
    listConnections: async () => ({ connections: [] }), recoveryReport: async () => [], listProposedActions: async () => ({ actions: [] }),
    setWorkspaceContext: async () => {}, listExecutions: async () => [], readMessage: async () => { throw Error("message receipt not found"); }, beginInterlude: async () => { throw Error("unexpected interlude"); }, returnInterlude: async () => { throw Error("unexpected interlude return"); }, listMessages: async () => [], listThreads: async () => ({ threads: [], nextCursor: null }),
    onEvent: fn => { listener = fn; return () => {}; }, onUserInputRequest: () => () => {},
    sendPrompt: async projectId => { modelCalls++; listener?.({ method: "agent/execution-request", params: { projectId, requestId: "input-request" } }); listener?.({ method: "agent/message-updated", params: { projectId, messageId: "input-request", status: "queued" } }); await tick(); listener?.({ method: "thread/started", params: { thread: { id: "thread" } } }); await tick(); listener?.({ method: "turn/started", params: { threadId: "thread", turn: { id: "turn" } } }); await tick(); listener?.({ method: "turn/completed", params: { threadId: "thread", turn: { id: "turn", status: "completed", items: [] } } }); return { message: "已记录", proposedActions: [] }; } },
};
const root = createRoot(document.getElementById("root"));
const input = () => document.querySelector('textarea[aria-label="创作指令"]');
const fill = (node, value) => { Object.getOwnPropertyDescriptor(node.tagName === "TEXTAREA" ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype, "value").set.call(node, value); node.dispatchEvent(new Event("input", { bubbles: true })); };
const button = text => [...document.querySelectorAll("button")].find(node => node.textContent === text);
const switchProject = async i => {
  await until(() => !document.querySelector(".project-switcher").disabled);
  document.querySelector(".project-switcher").click(); await tick();
  [...document.querySelectorAll(".project-menu-item")].find(node => node.querySelector("strong")?.textContent === projects[i].title).click();
  await tick();
};
const geometry = [];
const openCreation = async () => { const launcher = document.querySelector<HTMLButtonElement>(".project-switcher"); launcher.focus(); launcher.click(); await tick(); const entry = document.querySelector<HTMLButtonElement>(".project-create-menu"); entry.focus(); entry.click(); await until(() => document.querySelector(".project-create-dialog")); };
(async () => { try {
  saveAuthor({ name: "体验作者", email: "author@example.invalid", passwordSalt: "fixture", passwordHash: await hashPassword("fixture-password", "fixture") });
  root.render(<App />); await until(() => input() && !input().disabled);
  fill(input(), "A 的未发送构思"); await tick(); await switchProject(1);
  await until(() => document.querySelector(".project-switcher [title]")?.textContent === projects[1].title);
  check(input().value === "", "A input leaked to B");
  fill(input(), "B 的未发送构思"); await tick(); await switchProject(0);
  await until(() => input().value === "A 的未发送构思");
  await switchProject(1); await until(() => input().value === "B 的未发送构思");
  fill(input(), ""); await tick(); await switchProject(0); await until(() => input().value === "A 的未发送构思");
  document.querySelector('[aria-label="发送创作指令"]').click(); await until(() => modelCalls === 1 && input().value === ""); await tick();
  await switchProject(1); await switchProject(0); check(input().value === "", "sent input revived");
  fill(input(), "x".repeat(32001)); await tick(); await switchProject(1);
  check(document.querySelector(".project-switcher [title]").textContent === projects[0].title && input().value.length === 32001, "oversize draft lost on navigation");
  const sizeNotice = [...document.querySelectorAll('[role="alert"]')].map(node => node.textContent).find(text => text.includes("草稿过长")); check(sizeNotice, "missing size notice");
  fill(input(), "A"); await tick();
  for (let i = 1; i <= 40; i++) { await switchProject(i); await until(() => document.querySelector(".project-switcher [title]")?.textContent === projects[i].title); fill(input(), `构思 ${i}`); await tick(); }
  await switchProject(41); check(document.querySelector(".project-switcher [title]").textContent === projects[40].title && input().value === "构思 40", "capacity discarded draft");
  const capacityNotice = [...document.querySelectorAll('[role="alert"]')].map(node => node.textContent).find(text => text.includes("40 个对话")); check(capacityNotice, "missing capacity notice");
  fill(input(), ""); await tick();
  const originalViewport = { width: 1400, height: 900, theme: document.documentElement.dataset.theme };
  for (const [width, height] of [[1400, 900], [900, 600]]) for (const theme of ["light", "dark"]) {
    await resizeTestViewport(width, height);
    if (document.documentElement.dataset.theme !== theme) { document.querySelector<HTMLButtonElement>(".theme-toggle").click(); await until(() => document.documentElement.dataset.theme === theme); }
    await openCreation();
    const modal = document.querySelector<HTMLFormElement>(".project-create-dialog");
    check(document.activeElement === modal.querySelector("input"), "Creation did not initially focus its title field");
    const controls = [...modal.querySelectorAll<HTMLElement>('button:not(:disabled),input,select,textarea')];
    controls[0].focus(); controls[0].dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", shiftKey: true, bubbles: true, cancelable: true })); check(document.activeElement === controls.at(-1), "Modal Shift-Tab escaped its scope");
    controls.at(-1).dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true, cancelable: true })); check(document.activeElement === controls[0], "Modal Tab escaped its scope");
    geometry.push({ theme, ...await surfaceGeometry(".project-create-dialog", "project creation"), focusTrapped: true });
    await captureTestSurface(`project-creation-${theme}-${width}x${height}`);
    modal.querySelector<HTMLButtonElement>('[aria-label="关闭"]').click(); await until(() => !document.querySelector(".project-dialog"));
    check(document.activeElement === document.querySelector(".project-switcher"), "Closing creation lost its launcher focus");
  }
  await resizeTestViewport(originalViewport.width, originalViewport.height);
  if (document.documentElement.dataset.theme !== originalViewport.theme) { document.querySelector<HTMLButtonElement>(".theme-toggle").click(); await tick(); }
  await openCreation();
  fill(document.querySelector(".project-dialog input"), "一次创建"); await tick();
  const form = document.querySelector(".project-dialog");
  form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })); form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" })); await tick();
  check(createCalls === 1 && document.querySelector(".project-dialog"), "double create or early close");
  settleCreate.reject(Error("受控创建失败")); await until(() => document.querySelector(".project-dialog [role=alert]"));
  check(document.querySelector(".project-dialog input").value === "一次创建", "failed creation discarded title");
  geometry.push(await surfaceGeometry(".project-create-dialog", "failed creation"));
  form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })); await tick(); check(createCalls === 2, "retry blocked");
  settleCreate.resolve(); await until(() => !document.querySelector(".project-dialog"));
  document.querySelector('[aria-label="打开用户设置"]').click(); await until(() => button("退出登录")); button("退出登录").click(); await until(() => document.querySelector(".login-card"));
  fill(document.querySelector('.login-card input[type="password"]'), "fixture-password"); await tick(); document.querySelector(".login-card").dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  await until(() => input()); check(input().value === "", "logout revived draft");
  await switchProject(1); await until(() => document.querySelector(".project-switcher [title]")?.textContent === projects[1].title); check(input().value === "构思 1", "Same-author sign-in lost a preserved draft in another work");
  window.testResult = { passed: true, modelCalls, createCalls, sizeNotice, capacityNotice, retainedTitle: "一次创建", geometry };
} catch (error) { window.testResult = { passed: false, error: error.stack }; } })();
