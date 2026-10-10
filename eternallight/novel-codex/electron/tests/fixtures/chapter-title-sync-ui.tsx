import { novelFixtureMedia } from "../helpers/novel-edition";
import React from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import "../../../src/styles.css";
import { saveAuthor } from "../helpers/author-session";
import { chapterRuntimeReads } from "../helpers/chapter-runtime";

const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (ok, message) => { if (!ok) throw Error(message); };
const until = async (predicate, message) => { for (let i = 0; i < 200 && !predicate(); i++) await tick(); check(predicate(), message); };
const button = label => [...document.querySelectorAll<HTMLButtonElement>("button")].find(node => node.textContent === label || node.getAttribute("aria-label") === label);
const titleInput = () => document.querySelector<HTMLInputElement>('[aria-label="章节标题"]');
const prose = () => document.querySelector<HTMLTextAreaElement>('[aria-label="场景 1 正文"]');
const titleOf = id => document.querySelector(`[data-directory-chapter-id="${id}"] strong`)?.textContent;
const editTitle = async value => { const field = titleInput(); Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(field, value); field.dispatchEvent(new Event("input", { bubbles: true })); await tick(); };
const projects = ["project-title-a", "project-title-b"].map(id => ({ id, title: id, logline: "", updated_at: "" }));
const base = [1, 2, 3].map(number => ({ id: `chapter-${number}`, number, title: `正式底层旧名${number}`, status: "draft", summary: "", updated_at: "", passages: [] }));
const revision = (id, title) => ({ id, title, version: 1, body: "这段保存的正文保持不变。", sceneGroups: [{ id: "scene-one", sceneId: null, body: "这段保存的正文保持不变。", locked: false }], sourcePlanRefs: { localOnly: true, lineageStatus: "current" } });
const workspaceFor = project => ({ project, revision: 1, headSnapshotId: "head", sourceDigest: "a".repeat(64), index: { status: "ready" },
  artifacts: Object.fromEntries(["brief", "outline", "plan", "identity", "propertyRules"].map(kind => [kind, { candidate: null, accepted: null, working: null, workingSelection: null, locked: false }])),
  chapters: base.map((chapter, index) => ({ ...chapter, candidate: index === 0 ? revision("candidate-one", project.id === projects[0].id ? "候选章名" : "B作品候选章名") : null,
    accepted: index < 2 ? revision(`accepted-${index}`, `已发布章名${index + 1}`) : null, working: revision(`working-${index}`, "另一个工作依据名"), workingSelection: null })),
  tasks: [], reviews: [], modeling: [], factProposals: [], chapterPlanBindings: [], workingBaselines: [], continuationIntents: [], planningReviews: [], commits: [], impacts: [] });
const workspaces = new Map(projects.map(project => [project.id, workspaceFor(project)]));
const snapshots = []; const renderErrors = []; window.addEventListener("error", event => renderErrors.push(event.message));
let saves = 0, modelCalls = 0, stateReads = 0, failSave = false, failState = false, holdA = false, listener;
const delayed = [];
window.novelCodex = { multimedia: novelFixtureMedia,
  app: { onCloseRequested: () => () => {} }, runtime: {
    ...chapterRuntimeReads(id => projects.find(project => project.id === id)),
    status: async () => ({ state: "online", mode: "local", error: null }), listProjects: async () => ({ projects }),
    getState: async id => { stateReads++; if (failState) { failState = false; throw Error("共享作品读取失败"); } return { project: projects.find(project => project.id === id), canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1.0", scenes: [], anchors: [] }, novelEdition: { chapters: structuredClone(base) } }; },
    getWriterPreview: async () => ({ target: { position: { readingChapter: 1, sceneOrder: 1 } }, relevantText: [], plannedEffects: [], coverage: { complete: true, missingScope: [], truncationReasons: [] } }),
    getCreativeWorkspace: async id => { const data = structuredClone(workspaces.get(id)); if (holdA && id === projects[0].id) return new Promise(resolve => delayed.push(() => resolve(data))); return data; },
    saveDraftCheckpoint: async (id, payload) => { if (failSave) { failSave = false; throw Error("标题保存失败，正文未改"); } saves++; const workspace = workspaces.get(id), chapter = workspace.chapters.find(row => row.id === payload.chapterId); chapter.candidate = { ...revision(`saved-${saves}`, payload.title), body: payload.body, sceneGroups: payload.sceneGroups }; workspace.revision++; return { revisionId: chapter.candidate.id }; },
  }, agent: {
    status: async () => ({ configured: true, state: "ready", mode: "novel-agent-engine", provider: "fixture", model: "fixture", error: null }), listConnections: async () => ({ connections: [] }), recoveryReport: async () => [], listExecutions: async () => [], readMessage: async () => { throw Error("message receipt not found"); }, beginInterlude: async () => { throw Error("unexpected interlude"); }, returnInterlude: async () => { throw Error("unexpected interlude return"); }, listMessages: async () => [], listThreads: async () => ({ threads: [], nextCursor: null }), listProposedActions: async () => ({ actions: [] }), setWorkspaceContext: async () => {}, onEvent: fn => { listener = fn; return () => {}; }, onUserInputRequest: () => () => {},
    sendPrompt: async () => { modelCalls++; throw Error("unexpected model"); }, generateChapter: async () => { modelCalls++; throw Error("unexpected writer"); }, generateWork: async () => { modelCalls++; throw Error("unexpected generation"); },
  },
};
let root = createRoot(document.getElementById("root")); let phase = "startup";
const selectChapter = async (id, expected) => { document.querySelector<HTMLButtonElement>(`[data-directory-chapter-id="${id}"]`).click(); await until(() => titleInput()?.value === expected, `Chapter ${id} did not load ${expected}`); await tick(); };
const switchProject = async index => { document.querySelector<HTMLButtonElement>(".project-switcher").click(); await tick(); [...document.querySelectorAll<HTMLButtonElement>(".project-menu-item")].find(row => row.textContent.includes(projects[index].title)).click(); await until(() => document.querySelector(".project-card strong")?.textContent === projects[index].title, "Project did not switch"); await tick(); };
const options = async () => { document.querySelector<HTMLButtonElement>(".chapter-switcher-current").click(); await tick(); const names = [...document.querySelectorAll(".chapter-switcher-menu [role=option] b")].map(row => row.textContent); document.querySelector<HTMLButtonElement>(".chapter-switcher-current").click(); await tick(); return names; };
(async () => { try {
  saveAuthor({ name: "标题同步作者", email: "title@example.invalid", passwordSalt: "fixture", passwordHash: "fixture" }); root.render(<App />);
  await until(() => titleInput()?.value === "候选章名" && titleOf("chapter-2") === "已发布章名2" && stateReads >= 2, "Saved title projection did not initialize");
  const names = await options(); check(JSON.stringify(names) === JSON.stringify(["候选章名", "已发布章名2", "正式底层旧名3"]), "Switcher used formal or working title");
  snapshots.push({ case: "saved precedence", directory: base.map(row => titleOf(row.id)), switcher: names, workingIgnored: true });
  phase = "live preview preserves editor";
  const body = prose(); body.focus(); body.setSelectionRange(2, 5, "backward");
  await editTitle("尚未保存本地标题"); check(titleOf("chapter-1") === "尚未保存本地标题" && (await options())[0] === "尚未保存本地标题", "Local title did not update both displays");
  await editTitle(""); check(titleOf("chapter-1") === "" && (await options())[0] === "", "Empty title fell back to saved/base");
  check(prose() === body && body.selectionStart === 2 && body.selectionEnd === 5 && body.selectionDirection === "backward" && saves === 0 && modelCalls === 0, "Preview remounted editor, changed caret or executed");
  snapshots.push({ case: phase, emptyPreview: true, sameEditorAndCaret: true, writes: saves, modelCalls });
  phase = "save failure and discard"; await editTitle("失败后保留的标题"); failSave = true; button("保存").click();
  await until(() => document.body.innerText.includes("标题保存失败，正文未改") && !button("保存").disabled, "Save failure did not settle");
  check(titleInput().value === "失败后保留的标题" && titleOf("chapter-1") === "失败后保留的标题" && saves === 0, "Failed save lost preview");
  button("下一章").click(); await until(() => button("放弃修改并离开"), "Dirty guard absent"); button("留在当前页面").click(); await tick();
  check(prose() === body && titleInput().value === "失败后保留的标题", "Cancelled navigation lost draft");
  button("下一章").click(); await until(() => button("放弃修改并离开"), "Dirty guard absent on discard"); button("放弃修改并离开").click();
  await until(() => titleInput()?.value === "已发布章名2" && titleOf("chapter-1") === "候选章名", "Discard did not restore saved title");
  snapshots.push({ case: phase, failureRetained: true, cancelledNavigationRetained: true, discardedTitle: titleOf("chapter-1"), writes: saves });
  phase = "saved candidate survives navigation and App remount"; await selectChapter("chapter-1", "候选章名"); await editTitle("作者保存的新章名"); failState = true; button("保存").click();
  await until(() => saves === 1 && button("重新读取已保存章节") && !button("重新读取已保存章节").disabled, "Known receipt was not retained after parent read failure");
  check(titleInput().value === "作者保存的新章名", "Known receipt read failure lost local title"); button("重新读取已保存章节").click();
  await until(() => saves === 1 && !button("保存").disabled && document.querySelector(".chapter-save-state")?.textContent.includes("正文已保存"), "Save did not finish");
  check(base[0].title === "正式底层旧名1" && workspaces.get(projects[0].id).chapters[0].accepted.title === "已发布章名1", "Save rewrote formal title");
  [...document.querySelectorAll<HTMLButtonElement>(".nav-item")].find(row => row.textContent.includes("作品概况")).click(); await until(() => document.querySelector<HTMLElement>(".main-panel").dataset.view === "brief", "Did not leave chapters");
  check(titleOf("chapter-1") === "作者保存的新章名", "Leaving editor restored stale base title"); await selectChapter("chapter-1", "作者保存的新章名");
  root.unmount(); await tick(); root = createRoot(document.getElementById("root")); root.render(<App />);
  await until(() => titleInput()?.value === "作者保存的新章名" && titleOf("chapter-1") === "作者保存的新章名", "Remount lost saved title");
  snapshots.push({ case: phase, savedTitle: titleOf("chapter-1"), formalTitle: base[0].title, knownReceiptRecoveryWrites: saves, modelCalls });
  phase = "late workspace project ABA";
  await switchProject(1); await until(() => titleInput()?.value === "B作品候选章名", "Initial B read absent");
  holdA = true; await switchProject(0); await until(() => delayed.length > 0, "No deferred A refresh");
  await switchProject(1); await until(() => titleInput()?.value === "B作品候选章名" && titleOf("chapter-1") === "B作品候选章名", "A titles contaminated B");
  holdA = false; workspaces.get(projects[0].id).chapters[0].candidate.title = "A后续已保存名称"; await switchProject(0);
  await until(() => titleInput()?.value === "A后续已保存名称" && titleOf("chapter-1") === "A后续已保存名称", "Fresh A title absent"); delayed.forEach(resolve => resolve()); await tick(); await tick();
  check(titleInput().value === "A后续已保存名称" && titleOf("chapter-1") === "A后续已保存名称" && (await options())[0] === "A后续已保存名称", "Late A workspace overwrote new incarnation");
  snapshots.push({ case: phase, title: titleOf("chapter-1"), oldResponsesIgnored: true, modelCalls });
  window.testResult = { passed: true, snapshots, saves, modelCalls };
} catch (error) { window.testResult = { passed: false, error: `${phase}: ${error.message}; ${renderErrors.join("; ")}`, dom: document.body.innerHTML.slice(-24000), snapshots, saves, modelCalls }; } })();
