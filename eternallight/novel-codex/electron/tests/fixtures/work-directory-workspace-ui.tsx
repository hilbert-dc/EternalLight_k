import React from "react";
import { createRoot } from "react-dom/client";
import { useWorkCatalog } from "../../../src/useWorkCatalog";
import { SidebarWorkDirectory } from "../../../src/SidebarWorkDirectory";
import { CreativeWorkspaceView } from "./creative-workspace-harness";
import { chapterRuntimeReads } from "../helpers/chapter-runtime";
import "../../../src/styles.css";
const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (value, message) => { if (!value) throw Error(message); };
const until = async (predicate, message) => { for (let i = 0; i < 150 && !predicate(); i++) await tick(); check(predicate(), message); };
const root = createRoot(document.getElementById("root")!);
const project = { id: "directory-workspace", title: "作品目录", logline: "" };
const source = { outlineRevisionId: "outline-exact", nodeId: "stable-volume" };
const workspace = { project, revision: 1, headSnapshotId: "head", sourceDigest: "a".repeat(64), index: { status: "ready" }, artifacts: Object.fromEntries(["brief", "outline", "plan"].map(kind => [kind, { candidate: null, accepted: null, working: null, workingSelection: null, locked: false }])), chapters: [], tasks: [], reviews: [], modeling: [], factProposals: [], chapterPlanBindings: [], workingBaselines: [], continuationIntents: [], planningReviews: [], impacts: [], commits: [] };
workspace.artifacts.outline.candidate = { id: source.outlineRevisionId, version: 1, payload: { nodes: [{ id: source.nodeId, clientKey: "client-volume", kind: "volume", title: "潮汐卷", purpose: "卷目标", summary: "卷简介", writerGuidance: "写作备注" }] } };
const draftChapter = number => ({ id: `chapter-${number}`, number, title: `现有第 ${number} 章`, status: "draft", summary: "", updated_at: "", passages: [] });
workspace.chapters = [1, 2, 3, 4].map(number => ({ ...draftChapter(number), candidate: null, accepted: null, working: null, workingSelection: null }));
const state = { project, canon: { characters: [], locations: [], relationships: [], events: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1", scenes: [], anchors: [] }, novelEdition: { chapters: [draftChapter(1)] } };
let request = null, refresh = 0;
let catalog = { revision: 0, volumes: [], placements: [] }, selected = "chapter-1", writes = 0, readFailure = false, models = 0;
const calls = [], snapshots = [], creationAttempts = [];
const api = { runtime: { ...chapterRuntimeReads(() => project),
 getWorkCatalog: async () => structuredClone(catalog), getCreativeWorkspace: async () => { if (readFailure) throw Error("章节读取断线"); return structuredClone(workspace); }, getState: async () => structuredClone(state),
 updateWorkCatalog: async (id, request) => { check(id === project.id && request.expectedRevision === catalog.revision && JSON.stringify(request.operation) === JSON.stringify({ type: "createVolume", title: "潮汐卷", source }), "Directory CAS or single-source creation mismatch"); calls.push(structuredClone(request.operation)); writes++; catalog = { revision: 1, volumes: [{ id: "volume-exact", title: "潮汐卷", position: 0, outlineStatus: "linked", source }], placements: [] }; return { catalog: structuredClone(catalog) }; },
 materializeStandaloneChapter: async (id, title, revision, volumeId) => {
  check(id === project.id && volumeId === "volume-exact", "Volume chapter creation lost exact placement authority");
  creationAttempts.push({ title, expectedRevision: revision });
  if (revision !== workspace.revision) throw Error("作品来源已变化，章节未创建");
  const number = Math.max(0, ...workspace.chapters.map(chapter => chapter.number)) + 1;
  const actualTitle = title.trim() || `第 ${number} 章`;
  writes++; calls.push({ chapter: title, volumeId });
  const chapter = { id: "new-chapter", number, title: actualTitle, status: "draft", summary: "", updated_at: "", passages: [] };
  state.novelEdition.chapters.push(chapter); workspace.chapters.push({ ...chapter, candidate: null, accepted: null, working: null, workingSelection: null }); workspace.revision++;
  catalog = { ...catalog, revision: 2, placements: [{ chapterId: chapter.id, volumeId }] }; readFailure = true;
  return { chapterId: chapter.id, chapterNumber: number };
 },
 getCommitmentExtraction: async () => null,
 getWriterPreview: async () => ({ target: { position: {} }, plannedEffects: [], relevantText: [], coverage: { complete: true, missingScope: [], truncationReasons: [] }, budgetsUsed: {} })
 }, agent: {} };
function DirectoryOwner() { const directory = useWorkCatalog(api, project.id, refresh, true); return <SidebarWorkDirectory directory={directory} api={api} projectId={project.id} chapters={state.novelEdition.chapters} selectedChapterId={selected} refreshToken={refresh} enabled busy={false} dirty={false} onSelectChapter={chapter => { selected = chapter.id; void render(); }} onCreateChapter={next => { request = next; void render(); }} />; }
const render = async () => { root.render(<div className="workspace"><aside><DirectoryOwner /></aside><main className="main-panel chapter-main-panel" style={{ width: 740, height: 600, overflow: "auto" }}><CreativeWorkspaceView api={api} projectId={project.id} view="chapters" requestedChapterCreation={request} onChapterCreationPrepared={id => { if (request?.requestId === id) { request = null; void render(); } }} state={state} selectedChapterId={selected} refreshToken={refresh} taskConversationContainer={null} agentAvailable={false} agentRunning={false} onDirtyChange={() => {}} onFormalChange={async () => { refresh++; void render(); }} onSelectChapter={chapter => { selected = chapter.id; void render(); }} onDiscuss={() => {}} onGenerateWork={async () => { models++; }} onGenerateChapter={async () => { models++; }} onContinueWork={async () => {}} onRetryWork={async () => {}} onResolveFeedback={async () => {}} onReviewPlanning={async () => { models++; }} onCancelPlanningReview={async () => {}} onOpenPlanningTarget={() => {}} onReturnToFeedback={() => {}} /></main></div>); await tick(); await tick(); };
const button = text => { const matches = [...document.querySelectorAll<HTMLButtonElement>("button")].filter(item => item.textContent === text || item.getAttribute("aria-label") === text); return matches.find(item => item.getAttribute("role") === "menuitem") ?? matches.find(item => item.checkVisibility()) ?? matches[0]; };
const click = async text => { const item = button(text); const parents = []; for (let parent = item?.parentElement; parent; parent = parent.parentElement) if (parent instanceof HTMLDetailsElement) parents.unshift(parent); for (const details of parents) if (!details.open) { details.querySelector("summary").click(); await tick(); } check(item && !item.disabled && item.checkVisibility(), `Unavailable ${text}`); item.click(); await tick(); };
(async () => { try {
 await render(); await until(() => button("作品目录操作") && document.querySelector(".work-directory-tree"), "Empty directory missing"); check(document.querySelector<HTMLDetailsElement>(".work-directory").open && !catalog.volumes.length && writes === 0, "Unfiled chapters hid directory or auto-created volume");
 await click("作品目录操作"); await click("新增卷"); const form = document.querySelector<HTMLFormElement>(".work-directory-create")!; const option = form.querySelector<HTMLSelectElement>("select")!; option.value = JSON.stringify(source); option.dispatchEvent(new Event("change", { bubbles: true })); await tick();
 check(form.querySelector<HTMLInputElement>("input").value === "潮汐卷" && form.querySelector<HTMLInputElement>("input").readOnly, "Source selection lost exact title or invited fork"); await click("保存新卷"); await until(() => document.querySelector('[data-volume-id="volume-exact"]'), "Saved volume not refreshed"); await click("关闭整理目录"); check(calls[0].source.nodeId === source.nodeId && workspace.artifacts.outline.accepted === null, "Directory adopted outline or lost clientKey source");
 await click("在潮汐卷新增章节"); await until(() => document.querySelector('form[aria-label="在卷中创建章节"]'), "Volume chapter form missing"); await until(() => document.activeElement === document.querySelector('form[aria-label="在卷中创建章节"] input'), "Explicit new chapter did not focus its title"); check(document.querySelector('form[aria-label="在卷中创建章节"]').textContent.includes("潮汐卷"), "Volume creation form claims independent chapter");
 const defaultTitle = document.querySelector<HTMLInputElement>('[aria-label="新章节标题"]')!;
 check(state.novelEdition.chapters.length === 1 && workspace.chapters.length === 4 && defaultTitle.value === "" && defaultTitle.placeholder === "第 5 章", "Default chapter prediction used stale formal state or submitted a predicted title");
 const chapterForm = defaultTitle.closest("form")!;
 chapterForm.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })); chapterForm.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })); await tick();
 await until(() => button("重新读取已创建章节"), "Saved chapter read failure lacked recovery"); check(writes === 2 && models === 0 && state.novelEdition.chapters.length === 2 && creationAttempts.length === 1 && creationAttempts[0].title === "" && workspace.chapters.at(-1).number === 5 && workspace.chapters.at(-1).title === "第 5 章", "Chapter was repeated or generated during read failure"); readFailure = false; await click("重新读取已创建章节"); await until(() => document.querySelector('[aria-label="章节标题"]') && selected === "new-chapter", "Read-only receipt recovery did not open exact chapter");
 check(writes === 2 && models === 0 && catalog.placements[0].volumeId === "volume-exact" && workspace.chapters.find(chapter => chapter.id === "new-chapter").candidate === null && workspace.artifacts.outline.accepted === null, "Recovery repeated write, forged source or published"); check(document.querySelector<HTMLInputElement>('[aria-label="章节标题"]').value === "第 5 章" && creationAttempts.length === 1, "Receipt recovery changed the actual Runtime-default title or repeated creation"); snapshots.push({ predictedNumber: 5, submittedTitle: calls[1].chapter, actualTitle: workspace.chapters.at(-1).title, directory: document.querySelector(".work-directory > summary").textContent, volume: catalog.volumes[0], placement: catalog.placements[0], calls, body: document.querySelector<HTMLTextAreaElement>('[aria-label="场景 1 正文"]').value, writes, models });
 await click("在潮汐卷新增章节"); await until(() => document.querySelector<HTMLInputElement>('[aria-label="新章节标题"]')?.placeholder === "第 6 章", "Second directory request did not use current saved chapter number");
 const customTitle = document.querySelector<HTMLInputElement>('[aria-label="新章节标题"]')!;
 Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(customTitle, "作者手输的章节名"); customTitle.dispatchEvent(new Event("input", { bubbles: true })); await tick();
 customTitle.focus({ preventScroll: true }); customTitle.setSelectionRange(2, 5, "backward");
 workspace.chapters.push({ ...draftChapter(6), candidate: null, accepted: null, working: null, workingSelection: null }); workspace.revision++; refresh++;
 await render(); await until(() => customTitle.placeholder === "第 7 章", "Workspace refresh did not update the predicted number");
 check(document.querySelector('[aria-label="新章节标题"]') === customTitle && customTitle.value === "作者手输的章节名" && customTitle.selectionStart === 2 && customTitle.selectionEnd === 5 && customTitle.selectionDirection === "backward", "Number refresh remounted or cleared the author's custom title");
 workspace.chapters.push({ ...draftChapter(7), candidate: null, accepted: null, working: null, workingSelection: null }); workspace.revision++;
 await click("创建章节"); await until(() => document.body.textContent.includes("作品来源已变化，章节未创建") && !customTitle.disabled, "Stale chapter creation did not settle with visible failure");
 check(writes === 2 && models === 0 && creationAttempts.length === 2 && creationAttempts[1].title === "作者手输的章节名" && document.querySelector('[aria-label="新章节标题"]') === customTitle && customTitle.value === "作者手输的章节名" && catalog.placements.length === 1, "CAS failure wrote a chapter, lost the custom title or moved existing placement");
 snapshots.push({ customTitle: customTitle.value, predictedNumberAfterRefresh: 7, sameInputAndSelection: true, rejectedWithoutWrite: true, writes, models });
 await click("取消新建章节");
 window.testResult = { passed: true, snapshots, writes, models };
} catch (error) { window.testResult = { passed: false, error: error.stack ?? String(error) }; } })();
