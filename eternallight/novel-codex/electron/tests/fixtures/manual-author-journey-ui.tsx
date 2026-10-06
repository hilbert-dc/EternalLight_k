import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { CreativeWorkspaceView } from "./creative-workspace-harness";

window.addEventListener("error", event => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
const tick = () => new Promise(resolve => setTimeout(resolve, 30));
const check = (value, message) => { if (!value) throw new Error(message); };
const until = async (predicate, message) => { for (let i = 0; i < 90 && !predicate(); i++) await tick(); check(predicate(), message); };
const button = text => [...document.querySelectorAll("button")].find(item => item.textContent === text);
const click = async text => { const item = button(text); check(item && !item.disabled, `Unavailable: ${text}`); item.click(); await tick(); await tick(); };
const input = async (selector, value) => {
  const item = document.querySelector(selector);
  check(item && !item.disabled, `Unavailable: ${selector}`);
  Object.getOwnPropertyDescriptor(item instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype, "value").set.call(item, value);
  item.dispatchEvent(new Event("input", { bubbles: true })); await tick();
};
const project = id => ({ id, title: id, logline: "", updated_at: "" });
const stateFor = (id, chapters) => ({ project: project(id), canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { scenes: [], anchors: [], version: "1.0" }, novelEdition: { chapters } });
const chapter = (id, number, title) => ({ id, number, title, status: "draft", summary: "", updated_at: "", passages: [] });
const revision = (id, title, body) => ({ id, title, body, version: 1, createdAt: null, sourcePlanRefs: { lineageStatus: "current", localOnly: true }, sceneGroups: body ? [{ id: `scene-${id}`, sceneId: null, body, locked: false }] : [] });
const workspaceFor = (id, chapters) => ({ project: project(id), revision: 1, headSnapshotId: "test-head", sourceDigest: "a".repeat(64), index: { status: "ready" }, artifacts: Object.fromEntries(["brief", "outline", "plan"].map(kind => [kind, { candidate: null, accepted: null, working: null, locked: false, workingSelection: null }])), chapters, workingBaselines: [], continuationIntents: [], planningReviews: [], commits: [], tasks: [], impacts: [], modeling: [], factProposals: [], reviews: [], chapterPlanBindings: [] });
let state = stateFor("manual-author", []);
let workspace = workspaceFor("manual-author", []);
let previewFailure = false, createCalls = [], releaseCreate, stateFailure = false, modelCalls = 0;
const documentSnapshots = [], creationSnapshots = [];
const api = { runtime: {
  getWorkCatalog: async () => ({ revision: 0, volumes: [], placements: [] }),
  getCreativeWorkspace: async projectId => {
    const next = structuredClone(workspace);
    if (previewFailure) throw new Error("夹具预览读取中断");
    return next;
  },
  getState: async () => { if (stateFailure) throw new Error("夹具作品读取中断"); return structuredClone(state); },
  listPlanningImpacts: async () => ({ data: [], nextCursor: null, counts: { total: 0, open: 0 } }),
  listGenerationRuns: async () => ({ runs: [] }),
  listWorkTasks: async () => ({ tasks: [], nextCursor: null, activeCount: 0 }),
  ...Object.fromEntries(["Search", "Entity", "Relation", "Knowledge", "SnapshotKnowledge", "Fact"].map(kind => [`get${kind}Audit`, async () => ({ job: null })])),
  getWriterPreview: async () => ({ plannedEffects: [], relevantText: [], target: { position: { readingChapter: 1 } }, coverage: { complete: true, missingScope: [], truncationReasons: [], scannedScope: {} }, budgetsUsed: { byteSize: 0, byteLimit: 60000 } }),
  materializeStandaloneChapter: async (projectId, title, expectedRevision, volumeId) => {
    check(volumeId === null, "Independent chapter request changed its exact directory placement");
    createCalls.push({ projectId, title, expectedRevision, volumeId });
    await new Promise(resolve => { releaseCreate = resolve; });
    const item = chapter(`created-${createCalls.length}`, state.novelEdition.chapters.length + 1, title);
    state.novelEdition.chapters.push(item);
    workspace.chapters.push({ ...item, candidate: null, accepted: null, working: null }); workspace.revision++;
    return { chapterId: item.id, resultRevision: workspace.revision };
  },
  saveDraftCheckpoint: async (_, payload) => {
    const chapter = state.novelEdition.chapters.find(item => item.id === payload.chapterId);
    check(chapter && payload.chapterNumber === chapter.number, "Chapter save omitted or changed the current chapter number");
    const item = workspace.chapters.find(item => item.id === payload.chapterId);
    item.candidate = { ...revision(`saved-${item.id}`, payload.title, payload.body), sceneGroups: payload.sceneGroups }; workspace.revision++;
    return { revisionId: item.candidate.id };
  },
}, agent: {} };
const root = createRoot(document.getElementById("root"));
const documentSnapshot = () => ({ status: document.querySelector('[aria-label="章节发布状态"]')?.textContent, title: document.querySelector<HTMLInputElement>('[aria-label="章节标题"]')?.value, scenes: [...document.querySelectorAll<HTMLTextAreaElement>('.draft-editor-wrap textarea')].map(item => item.value) });
let selectedId = "", running = false, requestedChapterCreation = null;
const requestChapter = requestId => ({ projectId: state.project.id, volumeId: null, volumeTitle: null, requestId });
const renderCreative = async () => {
  root.render(<CreativeWorkspaceView requestedChapterCreation={requestedChapterCreation} onChapterCreationPrepared={requestId => { check(requestId === requestedChapterCreation?.requestId, "Consumed another directory request"); requestedChapterCreation = null; }} taskConversationContainer={null} api={api} projectId={state.project.id} view="chapters" state={state} selectedChapterId={selectedId}
    onSelectChapter={item => { selectedId = item.id; void renderCreative(); }} onGenerateWork={async () => { modelCalls++; }} onContinueWork={async () => { modelCalls++; }} onRetryWork={async () => { modelCalls++; }} onResolveFeedback={async () => {}} onReviewPlanning={async () => {}} onCancelPlanningReview={async () => {}} onGenerateChapter={async () => { modelCalls++; }} onOpenPlanningTarget={() => {}} onReturnToFeedback={() => {}}
    onFormalChange={async () => { state = await api.runtime.getState(); await renderCreative(); }} onDirtyChange={() => {}} agentAvailable={false} agentRunning={running} refreshToken={workspace.revision} />);
  await tick(); await tick(); await tick();
};

(async () => { try {
  state = stateFor("manual-author", []); workspace = workspaceFor("manual-author", []);
  await renderCreative(); await until(() => document.querySelector('[aria-label="新章节标题"]')?.disabled === false, "First chapter form unavailable without a model");
  await input('[aria-label="新章节标题"]', "手写第一章");
  const submit = document.querySelector('form[aria-label="创建独立章节"]');
  submit.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })); submit.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })); await tick();
  check(createCalls.length === 1, "Rapid creation submitted duplicate writes"); releaseCreate();
  await until(() => selectedId === "created-1" && document.querySelector('.chapter-document'), "New first chapter did not become selected");
  check(workspace.chapters[0].accepted === null && modelCalls === 0, "Manual creation adopted prose or required a model");
  check(!button("新建章节"), "Document retained a second chapter creation authority");
  running = true; requestedChapterCreation = requestChapter(1); await renderCreative();
  check(!document.querySelector('[aria-label="新章节标题"]') && createCalls.length === 1, "Running execution consumed a directory creation request");
  requestedChapterCreation = null; running = false; await renderCreative();
  await input('textarea[aria-label="场景 1 正文"]', "手写第一章的候选正文。");
  requestedChapterCreation = requestChapter(2); await renderCreative();
  check(!document.querySelector('[aria-label="新章节标题"]') && createCalls.length === 1, "Dirty document consumed a directory creation request");
  requestedChapterCreation = null; await renderCreative();
  await click("保存"); await until(() => workspace.chapters[0].candidate?.body === "手写第一章的候选正文。", "First chapter save did not settle");
  requestedChapterCreation = requestChapter(3); await renderCreative();
  await until(() => document.querySelector('[aria-label="新章节标题"]')?.disabled === false, "Exact directory request did not open the chapter form");
  await input('[aria-label="新章节标题"]', "手写第二章");
  creationSnapshots.push({ entry: "作品目录创建请求", title: document.querySelector('[aria-label="新章节标题"]').value, firstCandidate: workspace.chapters[0].candidate.body });
  await click("创建独立章节"); stateFailure = true; releaseCreate();
  await until(() => button("重新读取已创建章节"), "Created chapter plus refresh failure had no read-only recovery");
  creationSnapshots.push({ failure: document.querySelector('.creative-alert.warning').textContent, creationCount: createCalls.length });
  check(createCalls.length === 2 && document.querySelector('[aria-label="新章节标题"]').disabled, "Pending receipt allowed another creation");
  stateFailure = false; previewFailure = true; await click("重新读取已创建章节");
  await until(() => document.querySelector('.creative-alert.warning')?.textContent.includes("夹具预览读取中断"), "Creative workspace read failure discarded the successful receipt");
  check(createCalls.length === 2 && selectedId === "created-1" && button("重新读取已创建章节"), "Successful state read but failed creative read opened an unverified chapter or repeated creation");
  previewFailure = false; await click("重新读取已创建章节");
  await until(() => selectedId === "created-2" && document.querySelector('.chapter-document'), "Read-only recovery did not open the created second chapter");
  check(createCalls.length === 2 && workspace.chapters[0].candidate.body === "手写第一章的候选正文。" && workspace.chapters.every(item => item.accepted === null) && modelCalls === 0, "Recovery repeated creation, lost first prose, or adopted content");
  await input('textarea[aria-label="场景 1 正文"]', "第二章独立候选正文。");
  await click("保存");
  await until(() => button("发布章节")?.disabled === false, "Saved chapter has no publication entry");
  check(document.querySelector<HTMLInputElement>('[aria-label="章节标题"]')?.value === "手写第二章" && document.querySelector<HTMLTextAreaElement>('textarea[aria-label="场景 1 正文"]')?.value === "第二章独立候选正文。", "Saved second chapter did not remain the selected editable document");
  check(document.querySelector('[aria-label="章节发布状态"]')?.textContent === "未发布", "Manual save implicitly published the chapter");
  documentSnapshots.push(documentSnapshot());
  window.testResult = { passed: true, documentSnapshots, creationSnapshots, createCalls, modelCalls };
} catch (error) { window.testResult = { passed: false, error: error.stack }; } })();
