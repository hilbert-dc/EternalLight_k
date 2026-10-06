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
const revision = (id, body) => ({ id, title: id, version: 1, body, sourcePlanRefs: { localOnly: true, lineageStatus: "current" }, sceneGroups: [{ id: "scene-one", sceneId: null, body, locked: false }] });
const workspaceFor = project => ({ project, revision: 1, headSnapshotId: "head", sourceDigest: "a".repeat(64), index: { status: "ready" }, artifacts: Object.fromEntries(["brief", "outline", "plan"].map(kind => [kind, { candidate: null, accepted: null, working: null, locked: false, workingSelection: null }])), chapters: chapters.map((chapter, index) => ({ ...chapter, candidate: revision(`${project.id}-${chapter.id}`, "原有正文。".repeat(80)), accepted: null, working: null, workingSelection: null })), tasks: [], reviews: [], modeling: [], factProposals: [], chapterPlanBindings: [], workingBaselines: [], continuationIntents: [], planningReviews: [], commits: [], impacts: [] });
const workspaces = new Map(projects.map(project => [project.id, workspaceFor(project)]));

const catalogs = new Map(projects.map((project, index) => [project.id, { revision: 4, volumes: index ? [{ id: "volume-project-b", title: "另一本作品的卷", position: 0, source: null, outlineStatus: "independent" }] : [
 { id: "volume-old-source", title: "同名卷", position: 0, source: { outlineRevisionId: "outline-old", nodeId: "node-current" }, outlineStatus: "linked" },
 { id: "volume-independent", title: "同名卷", position: 1, source: null, outlineStatus: "independent" },
 { id: "volume-unavailable", title: "同名卷", position: 2, source: { outlineRevisionId: "outline-deleted", nodeId: "node-deleted" }, outlineStatus: "unavailable" },
], placements: [] }]));
workspaces.get(projects[0].id).artifacts.outline.candidate = { id: "outline-current", version: 2, payload: { nodes: [
 { id: "book-current", parentId: null, kind: "book", title: "全书主线", purpose: "全书主线唯一正文", summary: "主线安排", position: 0, certainty: "tentative", protection: "free", dependencyKeys: [] },
 { id: "node-current", parentId: "book-current", kind: "volume", title: "同名卷", purpose: "当前卷纲目的", summary: "当前卷纲安排", position: 0, certainty: "tentative", protection: "free", dependencyKeys: [] },
 { id: "node-second", parentId: "book-current", kind: "volume", title: "同名卷", purpose: "另一个同名卷纲目的", summary: "另一份卷纲安排", position: 1, certainty: "tentative", protection: "free", dependencyKeys: [] },
 { id: "unit-current", parentId: "node-current", kind: "unit", title: "当前单元", purpose: "局部单元安排", summary: "单元发生的事", position: 0, certainty: "tentative", protection: "free", dependencyKeys: [] },
] } };
const readCatalog = id => { const catalog = structuredClone(catalogs.get(id)); const outline = workspaces.get(id).artifacts.outline.candidate ?? workspaces.get(id).artifacts.outline.accepted; for (const volume of catalog.volumes) if (volume.source) { const nodes = (outline?.payload.nodes ?? []).filter(node => node.id === volume.source.nodeId && node.kind === "volume"); volume.outlineStatus = nodes.length === 1 ? "linked" : "unavailable"; if (nodes.length === 1) volume.title = nodes[0].title;
 // Catalog availability is a derived observation; its label can lag the freshly read outline.
 if (volume.id === "volume-old-source") volume.outlineStatus = "unavailable";
 } return catalog; };
const reads = []; let writes = 0, models = 0, versionReads = 0, eventListener, captureVersionDiscussion = false;
const discussionSends = [];
const stateFor = id => ({ project: projects.find(item => item.id === id), canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1", scenes: [], anchors: [] }, novelEdition: { chapters: workspaces.get(id).chapters } });
window.novelCodex = {
 app: { onCloseRequested: () => () => {} },
 runtime: { listChapterStructureChanges: async (projectId: string) => ({ projectId, items: [], nextCursor: null }),
  getWorkCatalog: async id => readCatalog(id),
  getDiffs: async () => { versionReads++; return { diffs: [] }; },
  getIdentityCatalog: async () => ({ entries: [], ambiguousNames: [], identityRevisionId: null, identityDecisions: [], truncated: false, ambiguityScanComplete: true }),
  updateWorkCatalog: async (id, request) => { const catalog = catalogs.get(id); check(id === projects[0].id && request.expectedRevision === catalog.revision && typeof request.idempotencyKey === "string" && JSON.stringify(request.operation) === JSON.stringify({ type: "linkOutline", volumeId: "volume-independent", source: { outlineRevisionId: "outline-saved", nodeId: "node-second" } }), "Association changed exact volume/current source/CAS"); writes++; catalog.revision++; catalog.volumes.find(volume => volume.id === request.operation.volumeId).source = structuredClone(request.operation.source); return { catalog: readCatalog(id) }; },
  status: async () => ({ state: "online", mode: "local", error: null }), listProjects: async () => ({ projects }),
  getState: async id => { reads.push(id); return structuredClone(stateFor(id)); },
  getCreativeWorkspace: async id => { reads.push(id); check(workspaces.has(id), "Unowned project read"); return structuredClone(workspaces.get(id)); },
  listWorkTasks: async () => ({ tasks: [], activeCount: 0, nextCursor: null }), listGenerationRuns: async () => ({ runs: [] }),
  listPlanningImpacts: async () => ({ data: [], nextCursor: null, counts: { total: 0, open: 0 } }),
  getWriterPreview: async () => ({ target: { position: {} }, plannedEffects: [], relevantText: [], coverage: { complete: true, missingScope: [], truncationReasons: [] } }),
  saveArtifact: async (id, kind, payload, expectedRevision, expectedCandidate) => { const workspace = workspaces.get(id); check(kind === "outline" && expectedRevision === workspace.revision && expectedCandidate === workspace.artifacts.outline.candidate.id, "Outline save lost frozen CAS"); writes++; workspace.artifacts.outline.candidate = { id: "outline-saved", version: 3, payload: structuredClone(payload) }; workspace.sourceDigest = "b".repeat(64); return { revisionId: "outline-saved" }; },
  saveDraftCheckpoint: async () => { writes++; throw Error("Unexpected write"); },
 },
 agent: { status: async () => ({ configured: true, state: "ready", mode: "novel-agent-engine", provider: "fixture", model: "fixture", error: null }), listConnections: async () => ({ connections: [] }), recoveryReport: async () => [], listThreads: async () => ({ threads: [], nextCursor: null }), listProposedActions: async () => ({ actions: [] }), setWorkspaceContext: async () => {}, onEvent: listener => { eventListener = listener; return () => { if (eventListener === listener) eventListener = null; }; }, onUserInputRequest: () => () => {}, sendPrompt: async (projectId, prompt, repairSource, focus) => {
   if (!captureVersionDiscussion) { models++; throw Error("Unexpected model call"); }
   discussionSends.push({ projectId, prompt, repairSource, focus }); await tick();
   const threadId = "local-version-discussion", turnId = "local-version-turn";
   eventListener?.({ method: "thread/started", params: { thread: { id: threadId } } });
   eventListener?.({ method: "turn/started", params: { threadId, turn: { id: turnId } } });
   eventListener?.({ method: "turn/completed", params: { threadId, turn: { id: turnId, status: "completed", items: [{ id: "version-reply", type: "agentMessage", phase: "final_answer", text: "仅讨论当前近章规划。" }] } } });
   return { threadId, message: "仅讨论当前近章规划。", proposedActions: [], candidateResults: [] };
  } },
};
const body = () => document.querySelector<HTMLTextAreaElement>('[aria-label="场景 1 正文"]');
const title = () => document.querySelector<HTMLInputElement>('[aria-label="章节标题"]')?.value;
const chooseProject = async index => { document.querySelector<HTMLButtonElement>(".project-switcher").click(); await tick(); document.querySelectorAll<HTMLButtonElement>(".project-menu-item")[index + 1].click(); await tick(); };
const waitChapter = async (project, chapter) => until(() => title() === `${project}-${chapter}`, `Chapter not restored: ${project}/${chapter}`);
const volume = id => document.querySelector<HTMLButtonElement>(`.outline-volume-children button[data-outline-volume-id="${id}"]`);
const detail = () => document.querySelector<HTMLElement>(".outline-focus-heading");
const openVolume = async id => { const toggle = button("展开大纲下的卷"); if (toggle) { toggle.click(); await tick(); } await until(() => volume(id)?.checkVisibility(), `Missing volume ${id}`); volume(id).click(); await until(() => detail()?.dataset.outlineVolumeId === id, `Incorrect volume ${id}`); };
const returnToChapter = async () => { const directory = document.querySelector<HTMLDetailsElement>(".work-directory"); if (!directory.open) directory.querySelector("summary").click(); document.querySelector<HTMLButtonElement>('[data-directory-chapter-id="chapter-01"]').click(); await waitChapter(projects[0].id, chapters[0].id); };
const key = async keyCode => { window.testKey = { keyCode }; await until(() => !window.testKey, "Native navigation key not delivered"); await tick(); };
const volumeScreens = [], bodyNavigation = [], moduleRecords = [];
let renderError = "";
window.addEventListener("error", event => { renderError = event.error?.stack ?? event.message; });
const records = () => document.querySelector<HTMLDivElement>(".module-records-popover:popover-open");
const openRecords = async () => { const trigger = [...document.querySelectorAll<HTMLButtonElement>(".main-panel button")].find(item => item.getAttribute("aria-label") === "查看修改记录" && item.checkVisibility()); check(trigger, "Missing current module records entry"); trigger.focus({ preventScroll: true }); trigger.click(); await until(() => records(), "Module records did not open"); return trigger; };
const closeRecords = async trigger => { records().querySelector<HTMLButtonElement>('[aria-label="关闭修改记录"]').click(); await until(() => !records(), "Module records did not close"); check(document.activeElement === trigger, "Module records did not restore its exact trigger focus"); };
let root;
(async () => { try {
 localStorage.clear(); saveAuthor({ name: "章节作者", email: "fixture@example.invalid", passwordSalt: "fixture", passwordHash: "fixture" });
 root = createRoot(document.getElementById("root")); root.render(<App />);
 await waitChapter(projects[0].id, chapters[0].id);
 check(document.querySelector<HTMLElement>(".main-panel").dataset.view === "chapters", "Default route isn't chapters");
 const expand = button("展开大纲下的卷"); expand.focus(); await key("Space"); check(button("收起大纲下的卷").getAttribute("aria-expanded") === "true", "Native Space did not expand outline navigation"); await key("Tab"); check(document.activeElement === volume("volume-old-source"), "Native Tab did not reach first exact volume"); await key("Return"); await until(() => detail()?.dataset.outlineVolumeId === "volume-old-source", "Native Return did not open exact volume");
 check(document.activeElement === detail().querySelector("h2"), "Volume heading did not receive explicit navigation focus");
 const mountedWorkspace = document.querySelector(".creative-workspace-view"), mountedComposer = composer();
 const reading = () => document.querySelector<HTMLElement>(".planning-reading");
 check(reading().checkVisibility() && reading().textContent.includes("当前卷纲安排") && !reading().textContent.includes("全书主线唯一正文") && !reading().textContent.includes("另一个同名卷纲目的") && composer().checkVisibility(), "Volume did not read its exact current subtree in the same workspace");
 document.querySelector<HTMLButtonElement>('button[aria-label="讨论同名卷"]').click(); await tick(); check(composer().value.includes("outline-current") && composer().value.includes("node-current") && writes === 0 && models === 0, "Volume discussion lost current source or sent automatically"); await input("创作指令", "");
 const manual = document.querySelector<HTMLDetailsElement>(".planning-manual-editor"); manual.querySelector("summary").click(); await tick();
 check(document.querySelectorAll("[data-outline-node-id]").length === 1 && document.querySelector('[data-outline-node-id="node-current"]') && !document.querySelector('[data-outline-node-id="book-current"]') && !document.querySelector('[data-outline-node-id="node-second"]') && !document.querySelector('[data-outline-node-id="unit-current"]'), "Default volume editor did not focus its one exact stage arrangement");
 check(document.querySelector(".content-header h1")?.textContent === "全书大纲" && JSON.stringify([...document.querySelectorAll('[data-outline-reading-node="node-current"] > details > article > dl dt')].map(item => item.textContent)) === JSON.stringify(["本卷推动什么", "本卷事件与变化"]), "Current volume reused whole-story labels or old page name");
 const originalNodes = structuredClone(workspaces.get(projects[0].id).artifacts.outline.candidate.payload.nodes);
 const titleControl = document.querySelector<HTMLInputElement>('[aria-label="同名卷标题"]');
 const titleMore = titleControl.closest<HTMLDetailsElement>(".outline-node-extra"); if (titleMore && !titleMore.open) { titleMore.querySelector("summary").click(); await tick(); }
 check(titleControl.checkVisibility(), "Explicit volume title editor is not reachable");
 await input("同名卷标题", "卷纲改名后的标题");
 const summary = document.querySelector<HTMLTextAreaElement>('[data-outline-node-id="node-current"] > article > label:nth-of-type(2) textarea');
 Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(summary, "卷纲故事安排由作者修改。"); summary.dispatchEvent(new Event("input", { bubbles: true })); await tick();
 summary.focus({ preventScroll: true }); summary.setSelectionRange(3, 8);
 const dirtySummary = summary.value;
 for (const leave of [() => button("返回全书大纲").click(), () => document.querySelector<HTMLButtonElement>(".outline-volume-root > .nav-item").click()]) {
  leave(); await until(() => document.querySelector(".unsaved-dialog"), "Same-page full-outline return skipped the dirty guard");
  check(detail()?.dataset.outlineVolumeId === "volume-old-source" && volume("volume-old-source").getAttribute("aria-current") === "page", "Dirty return cleared the selected volume before a decision");
  button("留在当前页面").click(); await tick();
  check(summary.isConnected && document.querySelector<HTMLTextAreaElement>('[data-outline-node-id="node-current"] > article > label:nth-of-type(2) textarea') === summary && summary.value === dirtySummary && summary.selectionStart === 3 && summary.selectionEnd === 8 && manual.open && writes === 0 && models === 0, "Cancelled outline/version navigation lost the volume draft, DOM, selection or wrote");
 }
 const dirtyVolumeTrigger = await openRecords();
 await until(() => records().querySelector(".planning-writing-settings"), "Volume records did not load");
 check(records().getAttribute("aria-label") === "同名卷 · 修改记录" && !document.querySelector(".unsaved-dialog") && detail()?.dataset.outlineVolumeId === "volume-old-source", "Dirty volume records navigated or lost exact scope");
 check(records().querySelector('[data-outline-reading-node="node-current"]') && !records().querySelector('[data-outline-reading-node="book-current"], [data-outline-reading-node="node-second"]') && !records().textContent.includes(dirtySummary), "Volume records previewed another subtree or unsaved editor authority");
 const volumeSettings = records().querySelector<HTMLDetailsElement>(".planning-writing-settings"); volumeSettings.querySelector("summary").click(); await tick();
 check([...volumeSettings.querySelectorAll<HTMLButtonElement>(".planning-versions-actions button")].every(item => item.disabled) && writes === 0 && models === 0, "Dirty records exposed write authority");
 await closeRecords(dirtyVolumeTrigger);
 check(summary.isConnected && document.querySelector('[data-outline-node-id="node-current"] > article > label:nth-of-type(2) textarea') === summary && summary.value === dirtySummary && summary.selectionStart === 3 && summary.selectionEnd === 8 && manual.open, "Dirty record reading replaced volume draft or selection");
 moduleRecords.push({ module: "本卷卷纲", dirtyReadOnly: true, sameEditorAndSelection: true, exactTriggerFocus: true });
 button("保存").click(); await until(() => workspaces.get(projects[0].id).artifacts.outline.candidate.id === "outline-saved" && reading().textContent.includes("卷纲故事安排由作者修改。") && volume("volume-old-source").textContent.includes("卷纲改名后的标题"), "Outline save/readback did not update the linked catalog title and current reading");
 const savedNodes = workspaces.get(projects[0].id).artifacts.outline.candidate.payload.nodes;
 check(JSON.stringify(savedNodes) === JSON.stringify(originalNodes.map(node => node.id === "node-current" ? { ...node, title: "卷纲改名后的标题", summary: "卷纲故事安排由作者修改。" } : node)) && workspaces.get(projects[0].id).artifacts.outline.accepted === null && writes === 1 && models === 0, "Subtree save changed unrelated outline fields, adopted, or invoked a model");
 check(document.querySelector(".creative-workspace-view") === mountedWorkspace && composer() === mountedComposer && catalogs.get(projects[0].id).volumes[0].source.outlineRevisionId === "outline-old", "Single-authority edit replaced workspace/composer or changed creation audit");
 manual.open = false; await tick();
 button("返回全书大纲").click(); await tick();
 document.querySelector<HTMLButtonElement>('[data-outline-reading-node="node-second"].outline-volume-overview button').click(); await until(() => reading()?.textContent.includes("另一份卷纲安排"), "Unlinked outline node did not open local reading");
 check(!detail()?.dataset.outlineVolumeId && [...document.querySelectorAll<HTMLButtonElement>(".outline-volume-children button")].every(item => item.getAttribute("aria-current") === null && !item.classList.contains("active")) && writes === 1 && models === 0, "Unlinked same-name node guessed a directory volume or wrote");
 bodyNavigation.push({ route: "未关联节点正文入口", node: "node-second", selected: null, writes, models });
 const localTrigger = await openRecords(); await until(() => records().querySelector('.planning-record-content [data-outline-reading-node="node-second"]'), "Local records lost the exact unlinked node");
 check(!records().hasAttribute("aria-modal") && !document.querySelector<HTMLElement>(".main-panel").inert && document.querySelectorAll('.main-panel [aria-label="查看修改记录"]').length === 1, "Local records were modal or had duplicate module entries");
 const readsBeforeToggle = reads.length; localTrigger.focus({ preventScroll: true }); await key("Return"); await until(() => !records(), "Same-entry keyboard activation did not toggle closed"); check(reads.length === readsBeforeToggle && localTrigger.getAttribute("aria-expanded") === "false", "Closing the same entry re-read or left expanded state");
 await openRecords(); await until(() => records().querySelector('.planning-record-content [data-outline-reading-node="node-second"]'), "Local records did not reopen exact source");
 const localReturn = button("返回全书大纲"); localReturn.focus({ preventScroll: true }); await key("Return");
 await until(() => !records() && reading()?.textContent.includes("全书主线唯一正文"), "Keyboard scope change retained local-node records");
 check(!detail() && document.querySelector('[aria-label="查看修改记录"]').getAttribute("aria-expanded") === "false" && document.activeElement !== localTrigger && writes === 1 && models === 0, "Scope teardown restored the old opener, retained node or changed authority");
 moduleRecords.push({ scopeChange: "本地节点键盘返回全书", closed: true, sameEntryToggleZeroRead: true, singleTopEntry: true, writes, models });
 await openVolume("volume-independent");
 check(document.querySelector('[aria-label="关联卷纲"]').textContent.includes("尚未关联") && !reading() && !document.querySelector('[data-outline-node-id="node-current"]')?.checkVisibility(), "Same-name independent volume guessed an outline source");
 const choices = [...document.querySelector<HTMLSelectElement>('[aria-label="选择已有卷纲"]').options].map(option => option.value);
 check(JSON.stringify(choices) === JSON.stringify(["", "node-current", "node-second"]) && writes === 1 && models === 0, "Explicit association lost exact same-name choices or wrote automatically");
 const unlinkedTrigger = await openRecords();
 await until(() => records().querySelector('[role="alert"]'), "Unlinked volume records did not explain missing source");
 check(records().textContent.includes("本卷尚未关联卷纲") && !records().querySelector(".planning-versions-panel,[data-outline-reading-node]"), "Unlinked volume showed whole-book records");
 await closeRecords(unlinkedTrigger); moduleRecords.push({ module: "独立卷", missingSourceExplicit: true, wholeBookPreview: false, writes, models });
 check(volume("volume-independent").getAttribute("aria-current") === "page" && volume("volume-old-source").getAttribute("aria-current") === null, "Same-name navigation selected by title");
 const selectedOutline = document.querySelector<HTMLSelectElement>('[aria-label="选择已有卷纲"]'); selectedOutline.value = "node-second"; selectedOutline.dispatchEvent(new Event("change", { bubbles: true })); await tick();
 button("关联此卷纲").click(); await until(() => reading()?.textContent.includes("另一份卷纲安排") && !document.querySelector('[aria-label="关联卷纲"]'), "Explicit same-name association did not read the selected current node");
 check(writes === 2 && models === 0 && workspaces.get(projects[0].id).artifacts.outline.candidate.id === "outline-saved" && workspaces.get(projects[0].id).artifacts.outline.accepted === null && !reading().textContent.includes("卷纲故事安排由作者修改。"), "Association copied/saved/adopted an outline or selected the same-name wrong node");
 await openVolume("volume-unavailable"); check(document.querySelector('[aria-label="关联卷纲"]').textContent.includes("已不在当前大纲") && !reading() && writes === 2, "Deleted node silently used a current same-name outline");
 const unavailableTrigger = await openRecords();
 await until(() => records().querySelector('.planning-versions-panel [role="alert"]'), "Missing scoped node records did not report its unavailable source");
 check(!records().querySelector('[data-outline-reading-node="book-current"],[data-outline-reading-node="node-current"],[data-outline-reading-node="node-second"]') && [...records().querySelectorAll<HTMLButtonElement>('.planning-versions-actions button')].every(item => item.disabled), "Missing volume substituted another outline or enabled mutation");
 await closeRecords(unavailableTrigger); moduleRecords.push({ module: "失联卷", missingSourceExplicit: true, wholeBookPreview: false, writes, models });
 await openVolume("volume-old-source");
 await openRecords(); volume("volume-independent").click();
 await until(() => detail()?.dataset.outlineVolumeId === "volume-independent" && !records(), "Changing volume did not close frozen records");
 await openVolume("volume-old-source");
 moduleRecords.push({ scopeChange: "卷", closed: true, writes, models });
 for (const theme of ["light", "dark"]) { if (document.documentElement.dataset.theme !== theme) { button(theme === "dark" ? "切换深色主题" : "切换浅色主题").click(); await tick(); } const workspace = document.querySelector<HTMLElement>(".workspace"); workspace.style.setProperty("--nc-sidebar", "160px"); await tick(); const sidebar = document.querySelector<HTMLElement>(".sidebar"); check(sidebar.scrollWidth <= sidebar.clientWidth + 2 && volume("volume-old-source").checkVisibility() && detail().checkVisibility(), `Narrow volume navigation overflow ${theme}`); volumeScreens.push({ theme, selected: volume("volume-old-source").getAttribute("aria-current"), title: detail().querySelector("h2").textContent, reading: reading().textContent, source: "同一当前卷纲；建立时绑定留作审计", keyboard: "Space 展开 → Tab 卷身份 → Return 卷纲聚焦" }); window.testScreenshot = `outline-volume-${theme}-160`; await until(() => !window.testScreenshot, "Volume screenshot not saved"); }
 button("返回全书大纲").click(); await tick(); check(!detail() && document.querySelector(".creative-workspace-view") === mountedWorkspace && reading().checkVisibility() && reading().textContent.includes("全书主线唯一正文") && composer().checkVisibility(), "Return replaced mounted workspace or lost the full outline/conversation");
 document.querySelector<HTMLButtonElement>('[data-outline-reading-node="node-current"].outline-volume-overview button').click();
 await until(() => detail()?.dataset.outlineVolumeId === "volume-old-source" && volume("volume-old-source")?.getAttribute("aria-current") === "page", "Body outline entry did not select its exact linked directory volume");
 check(volume("volume-old-source").classList.contains("active") && [...document.querySelectorAll<HTMLButtonElement>(".outline-volume-children button")].filter(item => item !== volume("volume-old-source")).every(item => item.getAttribute("aria-current") === null && !item.classList.contains("active")) && reading().textContent.includes("卷纲故事安排由作者修改。") && !reading().textContent.includes("另一份卷纲安排") && writes === 2 && models === 0, "Body navigation selected same-name wrong volume, lost current content or wrote");
 bodyNavigation.push({ route: "已关联节点正文入口", node: "node-current", reportedOutlineStatus: readCatalog(projects[0].id).volumes.find(volume => volume.id === "volume-old-source").outlineStatus, selected: volume("volume-old-source").dataset.outlineVolumeId, active: volume("volume-old-source").classList.contains("active"), current: volume("volume-old-source").getAttribute("aria-current"), writes, models });
 window.testScreenshot = "outline-body-navigation-dark-160"; await until(() => !window.testScreenshot, "Body navigation screenshot not saved");
 const focusedManual = document.querySelector<HTMLDetailsElement>(".planning-manual-editor"); focusedManual.open = true; await tick();
 const discardSummary = document.querySelector<HTMLTextAreaElement>('[data-outline-node-id="node-current"] > article > label:nth-of-type(2) textarea');
 Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(discardSummary, "只在本地的待放弃卷纲。"); discardSummary.dispatchEvent(new Event("input", { bubbles: true })); await tick();
 button("返回全书大纲").click(); await until(() => document.querySelector(".unsaved-dialog"), "Explicit discard return skipped the dirty guard");
 button("放弃修改并离开").click(); await until(() => !detail() && !document.querySelector(".unsaved-dialog"), "Discard did not return to the full outline");
 check(workspaces.get(projects[0].id).artifacts.outline.candidate.payload.nodes.find(node => node.id === "node-current").summary === "卷纲故事安排由作者修改。" && writes === 2 && models === 0, "Discard changed the persisted outline or called the model");
 focusedManual.open = false; await tick();
 check(!detail() && reading().textContent.includes("全书主线唯一正文") && [...document.querySelectorAll<HTMLButtonElement>(".outline-volume-children button")].every(item => item.getAttribute("aria-current") === null && !item.classList.contains("active")), "Return to full outline kept a selected volume");
 bodyNavigation.push({ route: "返回全书", selected: null, writes, models }); await returnToChapter();
 await openRecords();
 button("下一章").click(); await waitChapter(projects[0].id, chapters[1].id);
 await until(() => !records(), "Changing chapter did not close frozen records");
 moduleRecords.push({ scopeChange: "章节", closed: true, writes, models });
 body().focus(); body().setSelectionRange(16, 22); body().dispatchEvent(new Event("select", { bubbles: true }));
 const main = document.querySelector<HTMLElement>(".main-panel"); main.scrollTop = 120; main.dispatchEvent(new Event("scroll")); await tick();
 const savedOffset = body().getBoundingClientRect().top - main.getBoundingClientRect().top;
 await input("创作指令", "下一步只讨论节奏");
 await chooseProject(1); await waitChapter(projects[1].id, chapters[0].id);
 check(composer().value === "", "Draft leaked to another project");
 await chooseProject(0); await waitChapter(projects[0].id, chapters[1].id); await tick(); await tick();
 check(body().selectionStart === 16 && body().selectionEnd === 22, "Chapter caret bookmark lost");
 check(Math.abs(body().getBoundingClientRect().top - main.getBoundingClientRect().top - savedOffset) < 5, "Chapter reading position lost");
 check(composer().value === "下一步只讨论节奏", "Project draft wasn't restored");
 await input("场景 1 正文", "尚未保存的正文。".repeat(50));
 const editor = body(); editor.focus(); editor.setSelectionRange(3, 6);
 const dirtyChapterTrigger = await openRecords();
 await until(() => records().textContent.includes("当前章节暂无可对照的修改"), "Chapter records did not settle");
 check(records().getAttribute("aria-label").startsWith("第 2 章 ·") && !document.querySelector(".unsaved-dialog"), "Dirty chapter records left the module or opened leave guard");
 await closeRecords(dirtyChapterTrigger);
 check(document.querySelector<HTMLElement>(".main-panel").dataset.view === "chapters" && body() === editor && editor.value.startsWith("尚未保存") && editor.selectionStart === 3 && editor.selectionEnd === 6 && composer().value === "下一步只讨论节奏" && writes === 2 && models === 0 && versionReads === 2, "Reading records changed the chapter draft, selection, composer or invoked writes/model");
 moduleRecords.push({ module: "第2章", dirtyReadOnly: true, sameEditorAndSelection: true, exactTriggerFocus: true });
 check(!document.querySelector('[title="运行 Story Tests"]'), "Technical story-test trigger remains in author toolbar");
 const navToggle = button("展开大纲下的卷"); if (navToggle) { navToggle.click(); await tick(); } volume("volume-old-source").click(); await until(() => document.querySelector(".unsaved-dialog"), "Volume navigation skipped dirty guard"); button("留在当前页面").click(); await tick(); check(body() === editor && body().selectionStart === 3 && body().selectionEnd === 6 && body().value.startsWith("尚未保存") && !detail(), "Cancelled volume navigation lost draft or editor selection");
 document.querySelector<HTMLButtonElement>(".project-switcher").focus(); document.querySelector<HTMLButtonElement>(".project-switcher").click(); await tick(); button("作品信息").focus(); button("作品信息").click();
 await until(() => document.querySelector("dialog[open]"), "Information didn't open");
 check(body() === editor && body().selectionStart === 3 && body().selectionEnd === 6, "Information replaced the editor or selection");
 check(!document.querySelector(".unsaved-dialog") && composer().value === "下一步只讨论节奏", "Reading metadata triggered leave flow");
 button("关闭作品信息").click(); await tick();
 check(body() === editor && document.activeElement === document.querySelector(".project-switcher"), "Information close lost context/focus");
 await chooseProject(1); await until(() => document.querySelector(".unsaved-dialog"), "Dirty project switch skipped guard");
 button("留在当前页面").click(); await tick(); check(body() === editor, "Staying discarded editor");
 await chooseProject(1); button("放弃修改并离开").click(); await waitChapter(projects[1].id, chapters[0].id);
 await chooseProject(0); await waitChapter(projects[0].id, chapters[1].id); await tick();
 check(!document.querySelector("dialog[open]"), "Project switching opened an unexpected dialog");
 await openVolume("volume-old-source"); await chooseProject(1); await until(() => !detail() && document.querySelector<HTMLButtonElement>('[data-outline-volume-id="volume-project-b"]'), "Previous project volume leaked after switch"); check(!volume("volume-old-source"), "Previous project catalog survived switch"); button("作品目录")?.click();
 // Return to the existing chapter route before exercising missing chapter restoration.
 document.querySelector<HTMLButtonElement>('[data-directory-chapter-id="chapter-01"]').click(); await waitChapter(projects[1].id, chapters[0].id);
 workspaces.get(projects[0].id).chapters.pop();
 await chooseProject(0); await waitChapter(projects[0].id, chapters[0].id);
 // Reopening the app selects the persisted valid chapter; no fabricated chapter ID.
 root.unmount(); await tick(); root = createRoot(document.getElementById("root")); root.render(<App />);
 await waitChapter(projects[0].id, chapters[0].id);
 check(writes === 2 && models === 0 && reads.every(Boolean), "Navigation added writes beyond the explicit outline save/association or invoked a model");
 check(!document.querySelector('.sidebar [aria-label="修改记录"], [role="group"][aria-label="选择查看内容"], .main-panel[data-view="diffs"]'), "Global records route/picker survived module-bound records");
 const writingSettings = [];
 for (const [view, label] of [["brief", "作品概况"], ["outline", "全书大纲"], ["plans", "近章规划"]]) {
  const navigation = view === "outline" ? document.querySelector<HTMLButtonElement>(".outline-volume-root > .nav-item") : [...document.querySelectorAll<HTMLButtonElement>(".sidebar .nav-item")].find(item => item.getAttribute("aria-label") === label); navigation.click();
  await until(() => document.querySelector<HTMLElement>(".main-panel")?.dataset.view === view && [...document.querySelectorAll<HTMLButtonElement>(".main-panel button")].some(item => item.getAttribute("aria-label") === "查看修改记录" && item.checkVisibility()), `Module records entry unavailable: ${renderError}`);
  const trigger = await openRecords();
  await until(() => records().querySelector(".planning-writing-settings"), "Current planning records did not load");
  const settings = records().querySelector<HTMLDetailsElement>(".planning-writing-settings");
  const controls = [...settings.querySelectorAll<HTMLButtonElement>(".planning-versions-actions button")];
  check(!settings.open && controls.every(item => !item.checkVisibility()) && !settings.querySelector(".planning-versions-sources").checkVisibility(), "Low-frequency writing settings escaped the closed disclosure");
  settings.querySelector("summary").click(); await tick();
  check(settings.open && controls.every(item => item.checkVisibility()) && models === 0 && writes === 2, "Opening writing settings lost controls or executed authority work");
  settings.querySelector("summary").click(); await tick();
  writingSettings.push({ module: records().getAttribute("aria-label"), defaultClosed: !settings.open, current: records().querySelector(".planning-versions-current").textContent });
  await closeRecords(trigger);
  check(document.querySelector<HTMLElement>(".main-panel").dataset.view === view && models === 0 && writes === 2, "Reading module records navigated or executed authority work");
 }
 captureVersionDiscussion = true; await input("创作指令", "只讨论这份近章规划，不改正文。");
 await until(() => !send().disabled, "Local planning discussion unavailable"); send().click();
 await until(() => discussionSends.length === 1 && !send().disabled, "Local planning discussion did not settle");
 check(JSON.stringify(discussionSends) === JSON.stringify([{ projectId: projects[0].id, prompt: "只讨论这份近章规划，不改正文。", repairSource: null, focus: { view: "plans" } }]) && writes === 2 && models === 0, "Planning version discussion inherited a chapter/repair source or changed authority");
 window.testResult = { passed: true, snapshot: { default: "作品目录", metadata: "作品名 → 作品信息", chapterRestored: true, caretRestored: true, dirtyGuard: true, volumeNavigation: { sameName: "精确卷身份", source: "同一当前卷纲，精确节点保存读回同步目录", independent: "不猜来源，显式精确关联", unavailable: "明确重新关联", dirtyCancel: "保留正文与选区", outlineDirtyReturn: "返回全书、同页导航先确认；修改记录只读打开；取消保留卷草稿和选区；明确放弃零写入", moduleRecords, projectSwitch: "清除旧卷详情", screens: volumeScreens, bodyNavigation }, missingChapter: "回到现有首章", writingSettings, planningVersionDiscussion: { localMockOnly: true, discussionSends: discussionSends.length, focus: discussionSends[0].focus, unexpectedModels: models }, writes, models } };
} catch(error) { window.testResult = { passed: false, error: `${error.stack}\nrender: ${renderError}\nDOM: ${document.body.textContent.slice(0, 4000)}` }; } })();
