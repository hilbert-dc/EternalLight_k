import React, { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import "../../../src/styles.css";
import { saveAuthor } from "../helpers/author-session";
import { chapterRuntimeReads } from "../helpers/chapter-runtime";
import { fixtureNovelEdition } from "../helpers/novel-edition";

const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (value, message) => { if (!value) throw Error(message); };
const until = async (predicate, message) => { for (let i = 0; i < 180 && !predicate(); i++) await tick(); check(predicate(), message); };
const button = label => [...document.querySelectorAll<HTMLButtonElement>("button")].find(item => item.getAttribute("aria-label") === label || item.textContent === label);
const fill = async (node, value) => { check(node, `Missing field ${value}`); const prototype = node instanceof HTMLSelectElement ? HTMLSelectElement.prototype : node instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype; Object.getOwnPropertyDescriptor(prototype, "value").set.call(node, value); node.dispatchEvent(new Event(node instanceof HTMLSelectElement ? "change" : "input", { bubbles: true })); await tick(); };
const click = async element => { check(element && !element.disabled, "Action unavailable"); element.click(); await tick(); };
const novel = { id: "project-last-lamp", title: "原有小说", logline: "", updated_at: "" };
const projects = [novel], editions = new Map([[novel.id, fixtureNovelEdition(novel.id)]]), workspaces = new Map();
const created = [], saved = [], editionReads = [], novelReads = [], editionCreates = [];
const extraEditions = [], editionWorkspaces = new Map(), creationReceipts = new Map();
const workspaceFor = (projectId, editionId) => editionWorkspaces.get(editionId) ?? workspaces.get(projectId);
let models = 0;
const listeners = new Set(), questions = new Set();
const state = { project: novel, canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1.0", scenes: [], anchors: [] }, novelEdition: { chapters: [] } };
window.novelCodex = {
  app: { onCloseRequested: () => () => {} },
  runtime: { ...chapterRuntimeReads(id => { check(id === novel.id, "Media project entered novel chapter domain"); return novel; }),
    status: async () => ({ state: "online", mode: "local", error: null }), listProjects: async () => ({ projects: [...projects] }),
    getState: async id => { check(id === novel.id, "Media project synthesized a novel state"); novelReads.push(id); return structuredClone(state); },
    createProject: async (title, logline, medium, idempotencyKey) => {
      check(["comic", "animation"].includes(medium) && typeof idempotencyKey === "string" && idempotencyKey.length > 0, "Creation lost exact medium/idempotency key");
      created.push({ medium, idempotencyKey }); const project = { id: `project-${medium}`, title, logline, updated_at: "" };
      const edition = { id: `edition-${medium}`, editionId: `edition-${medium}`, projectId: project.id, scopeId: `scope-${medium}`, branchId: `branch-${medium}`, medium, title, canonSnapshotId: `canon-${project.id}`, sources: [] };
      const document = medium === "comic" ? { kind: "comic", title, format: "pages", pages: [] } : { kind: "animation", title, width: 1280, height: 720, frameRate: { numerator: 24, denominator: 1 }, shots: [], timeline: { tracks: [] } };
      projects.push(project); editions.set(project.id, edition); workspaces.set(project.id, { project, edition, revision: 1, sourceDigest: "a".repeat(64), revisionId: null, documentEncoding: null, documentSource: null, headSnapshotId: `head-${project.id}`, document, assets: [], sources: [] });
      return { project, editions: [edition], activeEditionId: edition.id };
    },
  },
  multimedia: {
    sourceCatalog: async (projectId, editionId) => { const w = workspaceFor(projectId, editionId); return { projectId, editionId, branchId: w.edition.branchId, snapshotId: w.headSnapshotId, items: [], offset: 0, total: 0, nextOffset: null }; },
    createEdition: async (projectId, request) => {
      editionCreates.push(structuredClone(request)); check(projectId === "project-comic" && request.medium === "animation", "Wrong project or media edition created");
      const source = workspaces.get(projectId).documentSource;
      check(request.sources.length === 1 && request.sources[0].objectId === source.objectId && request.sources[0].revisionId === source.revisionId && request.sources[0].digest === source.digest && request.sources[0].digest !== workspaces.get(projectId).sourceDigest && request.sources[0].selector.kind === "panel" && request.sources[0].selector.panelId === workspaces.get(projectId).document.pages[0].panels[0].id, "Adaptation source was guessed, latest-header hash or whole-document substitution");
      const known = creationReceipts.get(request.idempotencyKey); if (known) { check(known.fingerprint === JSON.stringify(request), "Creation retry changed frozen source"); return structuredClone(known.edition); }
      const edition = { id: "edition-comic-adaptation", editionId: "edition-comic-adaptation", projectId, scopeId: "scope-comic-adaptation", branchId: "branch-comic-adaptation", medium: "animation", title: request.title, canonSnapshotId: `canon-${projectId}`, sources: structuredClone(request.sources) };
      extraEditions.push(edition); editionWorkspaces.set(edition.id, { project: workspaces.get(projectId).project, edition, revision: 1, sourceDigest: "a".repeat(64), revisionId: null, documentEncoding: null, documentSource: null, headSnapshotId: "adaptation-head", document: { kind: "animation", title: edition.title, width: 1280, height: 720, frameRate: { numerator: 24, denominator: 1 }, shots: [], timeline: { tracks: [] } }, assets: [], sources: [] }); creationReceipts.set(request.idempotencyKey, { fingerprint: JSON.stringify(request), edition });
      throw Error("版本已登记，连接中断，回执尚未收到。");
    },
    listEditions: async projectId => { editionReads.push(projectId); check(editions.has(projectId), "Unowned edition query"); return { editions: structuredClone([editions.get(projectId), ...extraEditions.filter(item => item.projectId === projectId)]) }; },
    readWorkspace: async (projectId, editionId) => { const workspace = workspaceFor(projectId, editionId); check(workspace && workspace.edition.id === editionId, "Workspace read crossed edition identity"); return structuredClone(workspace); },
    saveDocument: async (projectId, editionId, request) => { const workspace = workspaceFor(projectId, editionId); check(workspace.edition.id === editionId && workspace.revisionId === request.expectedRevisionId && request.document.kind === workspace.edition.medium, "Save crossed media or revision boundary"); saved.push({ projectId, editionId }); Object.assign(workspace, { document: structuredClone(request.document), documentEncoding: JSON.stringify(request.document), revisionId: `${editionId}-r${saved.length}`, revision: workspace.revision + 1, sourceDigest: "b".repeat(64) }); workspace.documentSource = { projectId, scopeId: workspace.edition.scopeId, branchId: workspace.edition.branchId, objectId: editionId, revisionId: workspace.revisionId, digest: "c".repeat(64), selector: { kind: "whole" } }; return structuredClone(workspace); },
    readCanon: async projectId => ({ projectId, scopeId: `canon-scope-${projectId}`, branchId: `canon-branch-${projectId}`, snapshotId: `canon-${projectId}`, revisionId: null, document: { title: "共同设定", entries: [] }, documentSource: null }),
    sourceImpacts: async (projectId, editionId) => { const workspace = workspaceFor(projectId, editionId); return { projectId, editionId, branchId: workspace.edition.branchId, revisionId: workspace.revisionId, items: workspace.edition.sources.map(source => ({ source, latest: source, status: "current", targets: [] })), offset: 0, total: workspace.edition.sources.length, nextOffset: null, canon: { pinnedSnapshotId: workspace.edition.canonSnapshotId, currentSnapshotId: workspace.edition.canonSnapshotId, changed: false } }; },
    listConnections: async () => ({ connections: [], capabilities: [] }),
  },
  agent: { status: async () => ({ configured: true, state: "ready", mode: "novel-agent-engine", provider: "fixture", model: "fixture", error: null }), listConnections: async () => ({ connections: [] }), recoveryReport: async () => [], listProposedActions: async () => ({ actions: [] }), setWorkspaceContext: async () => {},
    listExecutions: async () => [], readMessage: async () => { throw Error("message receipt not found"); }, beginInterlude: async () => { throw Error("unexpected interlude"); }, returnInterlude: async () => { throw Error("unexpected interlude return"); }, listMessages: async () => [], listThreads: async (projectId, archived, cursor, scopeId) => { if (projectId !== novel.id) check([editions.get(projectId), ...extraEditions.filter(item => item.projectId === projectId)].some(item => item.scopeId === scopeId), "Media history lost scope"); return { threads: [], nextCursor: null }; },
    onEvent: listener => { listeners.add(listener); return () => listeners.delete(listener); }, onUserInputRequest: listener => { questions.add(listener); return () => questions.delete(listener); },
    sendPrompt: async () => { models++; throw Error("UI editing must not invoke a model"); },
  },
};
let root = createRoot(document.getElementById("root"));
const composer = () => document.querySelector<HTMLTextAreaElement>('[aria-label="创作要求"]');
const openCreate = async () => { await until(() => document.querySelector(".project-switcher:not(:disabled)"), "Project creation navigation not ready"); await click(document.querySelector(".project-switcher")); await click(document.querySelector(".project-create-menu")); await until(() => document.querySelector(".project-create-dialog"), "Creation dialog missing"); };
const create = async (medium, title) => {
  await openCreate(); const form = document.querySelector<HTMLFormElement>(".project-create-dialog");
  check([...form.querySelectorAll("select option")].map(item => item.textContent).join() === "小说,漫画,动画", "Creation forms are not peer entries");
  await fill(form.querySelector("select"), medium); await fill(form.querySelector("input"), title); form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  await until(() => !document.querySelector(".project-create-dialog") && document.querySelector(`.mm-${medium}-editor`) && composer(), `Independent ${medium} workspace did not open`);
};
const captureMedia = async medium => {
  for (const theme of ["light", "dark"]) {
    document.documentElement.dataset.theme = theme; window.testViewport = { width: 900, height: 600 }; await until(() => !window.testViewport && innerWidth === 900, "Media viewport not applied"); await tick();
    const composerBounds = document.querySelector(".media-composer").getBoundingClientRect();
    check(document.documentElement.scrollWidth <= innerWidth + 1 && composerBounds.bottom <= innerHeight + 1 && composerBounds.top >= 0, "Media shell overflowed or hid composer");
    window.testScreenshot = `multimedia-app-${medium}-${theme}-900`; await until(() => !window.testScreenshot, "Media screenshot not captured");
  }
};
const selectProject = async title => { await click(document.querySelector(".project-switcher")); await click([...document.querySelectorAll<HTMLButtonElement>(".project-menu-item")].find(item => item.querySelector("strong")?.textContent === title)); };
window.addEventListener("error", event => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
(async () => { try {
  localStorage.clear(); saveAuthor({ name: "多媒介作者", email: "media@example.invalid", passwordSalt: "fixture", passwordHash: "fixture" }); root.render(<StrictMode><App /></StrictMode>);
  await until(() => document.querySelector('[aria-label="创作指令"]') && editionReads.includes(novel.id), "Novel did not load through current edition contract");
  await create("comic", "独立漫画"); check(!document.querySelector('[aria-label="章节标题"]'), "Comic includes novel editor");
  await fill(composer(), "漫画对话未发送的构想"); await click(button("＋ 添加页面")); await click(button("＋ 添加画格")); await until(() => document.querySelector(".mm-status-row")?.textContent.includes("未保存"), "Comic change not tracked");
  await selectProject("原有小说"); await until(() => document.querySelector(".unsaved-dialog"), "Switch did not guard unsaved comic document"); await click(button("留在当前页面"));
  check(document.querySelector(".mm-comic-editor") && composer().value === "漫画对话未发送的构想", "Staying lost active comic or author draft"); await click(button("保存")); await until(() => saved.length === 1 && document.querySelector(".mm-status-row")?.textContent === "已保存", "Comic durable save receipt missing");
  await captureMedia("comic"); await create("animation", "独立动画"); check(!document.querySelector(".mm-comic-editor") && !document.body.textContent.includes("漫画对话未发送的构想") && composer().value === "", "Animation inherited comic state");
  await fill(composer(), "动画对话未发送的构想"); await click(button("＋ 添加镜头")); await click(button("加入时间轴")); await click(button("保存")); await until(() => saved.length === 2 && document.querySelector(".mm-status-row")?.textContent === "已保存", "Animation save missing");
  await captureMedia("animation"); await selectProject("独立漫画"); await until(() => document.querySelector(".mm-comic-editor") && composer()?.value === "漫画对话未发送的构想", "Comic scope draft did not restore"); check(document.querySelectorAll("[data-panel-id]").length === 1 && !document.querySelector(".mm-animation-editor"), "Comic document did not restore by revision");
  await click(button("新建作品版本")); await until(() => document.querySelector(".edition-create-dialog"), "Edition create dialog missing");
  const editionForm = document.querySelector<HTMLFormElement>(".edition-create-dialog"); check([...editionForm.querySelector("select").querySelectorAll("option")].map(item => item.textContent).join() === "漫画,动画", "Unsupported novel edition was offered");
  await fill(editionForm.querySelector("select"), "animation"); await fill(editionForm.querySelector('input:not([type="checkbox"])'), "雨夜动画版"); await click(editionForm.querySelector('input[type="checkbox"]'));
  editionForm.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })); editionForm.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })); await until(() => editionForm.querySelector('[role="alert"]'), "Unknown creation receipt did not retain dialog"); check(editionCreates.length === 1, "Edition same-frame creation duplicated");
  editionForm.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })); await until(() => !document.querySelector(".edition-create-dialog") && document.querySelector(".mm-animation-editor") && document.querySelector<HTMLSelectElement>('[aria-label="作品版本"]')?.value === "edition-comic-adaptation", "Created adaptation edition did not open");
  check(editionCreates.length === 2 && editionCreates[0].idempotencyKey === editionCreates[1].idempotencyKey && extraEditions.length === 1 && !document.querySelector('.mm-track-clips > button') && document.querySelector(".mm-status-row")?.textContent === "尚未保存" && button("设定与来源 · 1 处改编来源"), "Creation retry duplicated edition or claimed adaptation finished");
  await click(button("设定与来源 · 1 处改编来源")); await until(() => document.querySelector(".mm-sources-panel")?.textContent.includes("引用来源 · 1 处"), "Adaptation lost its real fixed source behind the collapsed entry"); await click(button("设定与来源 · 1 处改编来源")); await fill(composer(), "同项目改编版的草稿");
  await fill(document.querySelector('[aria-label="作品版本"]'), "edition-comic"); await until(() => document.querySelector(".mm-comic-editor") && composer()?.value === "漫画对话未发送的构想", "Same-project original edition lost content or draft");
  await fill(document.querySelector('[aria-label="作品版本"]'), "edition-comic-adaptation"); await until(() => composer()?.value === "同项目改编版的草稿" && document.querySelector(".mm-animation-editor"), "Adaptation scope draft crossed original");
  await selectProject("原有小说"); await until(() => document.querySelector('[aria-label="创作指令"]') && !document.querySelector(".media-application"), "Novel return missing");
  await selectProject("独立漫画"); await until(() => document.querySelector<HTMLSelectElement>('[aria-label="作品版本"]')?.value === "edition-comic-adaptation" && composer()?.value === "同项目改编版的草稿", "Project return ignored remembered edition");
  root.unmount(); root = createRoot(document.getElementById("root")); root.render(<StrictMode><App /></StrictMode>); await until(() => document.querySelector<HTMLSelectElement>('[aria-label="作品版本"]')?.value === "edition-comic-adaptation" && composer()?.value === "同项目改编版的草稿", "App remount chose first edition instead of remembered scope");
  root.unmount(); localStorage.removeItem(`novel-codex:selected-edition:${JSON.stringify(["media@example.invalid", "project-comic"])}`); root = createRoot(document.getElementById("root")); root.render(<StrictMode><App /></StrictMode>); await until(() => document.querySelector(".edition-selection-required"), "Multiple unselected editions silently chose a first entry"); await click(button("雨夜动画版 · 动画")); await until(() => document.querySelector(".mm-animation-editor") && composer()?.value === "同项目改编版的草稿", "Explicit edition selection did not restore exact scope");
  await selectProject("独立动画"); await until(() => document.querySelector(".mm-animation-editor") && composer()?.value === "动画对话未发送的构想", "Animation draft did not restore");
  check(document.querySelector('[aria-label="动画镜头"]')?.textContent.includes("镜头 1") && workspaces.get("project-animation").document.timeline.tracks[0].clips.length === 1 && workspaces.get("project-comic").document.pages[0].panels.length === 1, "Media document scope changed");
  await selectProject("原有小说"); await until(() => document.querySelector('[aria-label="打开用户设置"]'), "Account settings unavailable after returning to novel");
  const owner = "media@example.invalid", otherOwner = "media@example.invalid:other";
  const ownedKeys = [
    `novel-codex:conversation-draft:${JSON.stringify([owner, novel.id, "old-thread"])}`,
    `novel-codex:media-message:${JSON.stringify([owner, "project-comic", "scope-comic", "old-thread"])}`,
    `novel-codex:media-message:${JSON.stringify([owner, "project-comic", "scope-comic", "old-thread"])}:images`,
    `novel-codex:media-editor-draft:${JSON.stringify([owner, "project-comic", "edition-comic"])}`,
    `novel-codex:selected-conversation:${JSON.stringify([owner, novel.id])}`,
    `novel-codex:selected-edition:${JSON.stringify([owner, "project-comic"])}`,
    `novel-codex:selected-conversation:${JSON.stringify([JSON.stringify([owner, "scope-comic"]), "project-comic"])}`,
  ];
  const otherKeys = ownedKeys.map(key => key.replaceAll(owner, otherOwner));
  ownedKeys.forEach(key => localStorage.setItem(key, "owned draft")); otherKeys.forEach(key => localStorage.setItem(key, "other author draft")); localStorage.setItem("saved-work-metadata", "keep");
  await click(button("打开用户设置")); await click(button("删除本机账号")); await click(button("确认删除本机账号")); await until(() => document.querySelector(".login-card"), "Account deletion did not sign out");
  check(ownedKeys.every(key => localStorage.getItem(key) === null) && otherKeys.every(key => localStorage.getItem(key) === "other author draft") && localStorage.getItem("saved-work-metadata") === "keep", "Account deletion left owned recovery state or deleted other author/work data");
  check(!Object.keys(localStorage).some(key => key.startsWith("novel-codex:media-message:") && key.includes(owner) && !key.includes(otherOwner)), "Account deletion retained a naturally saved media draft");
  check(models === 0 && novelReads.every(id => id === novel.id) && created[0].idempotencyKey !== created[1].idempotencyKey, "Creation invoked model or lost operation identity"); root.unmount(); check(listeners.size === 0 && questions.size === 0, "App listeners survived unmount");
  window.testResult = { passed: true, created: created.map(item => item.medium), saved: saved.map(item => item.projectId), models, editionCreates: editionCreates.length, readScopes: [...new Set(editionReads)] };
} catch (error) { window.testResult = { passed: false, error: error.stack ?? String(error), dom: document.body.textContent.slice(-7000) }; } })();
