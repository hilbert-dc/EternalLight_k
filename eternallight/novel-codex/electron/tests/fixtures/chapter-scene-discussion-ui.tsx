import React from "react";
import { createRoot } from "react-dom/client";
import { CreativeWorkspaceView, revealControl } from "./creative-workspace-harness";
import { chapterRuntimeReads } from "../helpers/chapter-runtime";
import "../../../src/styles.css";

const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (value, message) => { if (!value) throw Error(message); };
const until = async (predicate, message) => { for (let index = 0; index < 150 && !predicate(); index++) await tick(); check(predicate(), message); };
const chapter = { id: "chapter-one", number: 1, title: "同名信件", status: "draft", summary: "", passages: [] };
const makeWorkspace = id => ({ project: { id }, revision: 1, headSnapshotId: `snapshot-${id}`, sourceDigest: "a".repeat(64), index: { status: "current" },
  artifacts: Object.fromEntries(["brief", "outline", "plan", "identity", "propertyRules"].map(kind => [kind, { candidate: null, accepted: null, working: null, workingSelection: null, locked: false }])),
  chapters: [{ ...chapter, candidate: { id: `source-${id}`, version: 1, title: chapter.title, chapterId: chapter.id, source: "author", body: "相同场景。\n\n---\n\n相同场景。\n\n---\n\n保护的原文。", sceneGroups: [
    { id: "group-one", sceneId: "stable-one", body: "相同场景。", locked: false },
    { id: "group-two", sceneId: "stable-two", body: "相同场景。", locked: false },
    { id: "group-locked", sceneId: "stable-locked", body: "保护的原文。", locked: true },
  ] }, accepted: null, working: null, workingSelection: null }], workingBaselines: [], continuationIntents: [], planningReviews: [], commits: [], tasks: [], impacts: [], modeling: [], factProposals: [], reviews: [], chapterPlanBindings: [] });
const workspaces = { a: makeWorkspace("a"), b: makeWorkspace("b") };
let projectId = "a", running = false, refreshToken = 0, holdRead = false, releaseRead;
const prepared = [], saves = [], snapshots = [];
let writes = 0, modelCalls = 0, dirty = false;
const api = { runtime: { ...chapterRuntimeReads(id => ({ id })),
  getCreativeWorkspace: async id => { const copy = structuredClone(workspaces[id]); if (holdRead) { holdRead = false; await new Promise(resolve => { releaseRead = () => resolve(); }); } return copy; },
  getCommitmentExtraction: async () => null,
  getWriterPreview: async () => ({ target: { position: {} }, plannedEffects: [], relevantText: [], coverage: { complete: true, missingScope: [], truncationReasons: [] }, budgetsUsed: {} }),
  saveDraftCheckpoint: async (id, payload, expectedRevision, candidateId) => {
    const workspace = workspaces[id]; check(expectedRevision === workspace.revision && candidateId === workspace.chapters[0].candidate.id, "Save CAS changed");
    const locked = payload.sceneGroups.find(group => group.id === "group-locked"); check(locked?.locked && locked.body === "保护的原文。", "Read-only discussion weakened protection");
    writes++; saves.push(structuredClone(payload)); workspace.headSnapshotId = `saved-snapshot-${id}-${writes}`;
    workspace.chapters[0].candidate = { ...workspace.chapters[0].candidate, ...payload, id: `saved-source-${id}-${writes}`, version: 1 + writes };
    return { revisionId: workspace.chapters[0].candidate.id };
  },
}, agent: {} };
const root = createRoot(document.getElementById("root"));
window.addEventListener("error", event => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
const render = async () => {
  root.render(<div className="workspace"><main className="main-panel chapter-main-panel" style={{ height: 550, width: 640, overflow: "auto" }}>
    <CreativeWorkspaceView api={api} projectId={projectId} view="chapters" state={{ project: { id: projectId }, canon: { characters: [], locations: [], relationships: [], events: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1", scenes: [], anchors: [] }, novelEdition: { chapters: [chapter] } }}
      selectedChapterId={chapter.id} refreshToken={refreshToken} taskConversationContainer={null} agentAvailable={false} agentRunning={running}
      onDiscuss={(prompt, source) => prepared.push({ prompt, source: structuredClone(source) })} onDirtyChange={value => { dirty = value; }} onFormalChange={async () => {}}
      onSelectChapter={() => {}} onGenerateWork={async () => { modelCalls++; }} onGenerateChapter={async () => { modelCalls++; }}
      onContinueWork={async () => {}} onRetryWork={async () => {}} onResolveFeedback={async () => {}} onReviewPlanning={async () => { modelCalls++; }} onCancelPlanningReview={async () => {}} onOpenPlanningTarget={() => {}} onReturnToFeedback={() => {}} />
  </main></div>);
  await tick(); await tick();
};
const discussions = () => [...document.querySelectorAll<HTMLButtonElement>(".chapter-scene-management button")].filter(button => button.textContent === "讨论这个场景");
const click = async button => { check(button && !button.matches(":disabled"), "Unavailable scene action"); await revealControl(button); button.focus({ preventScroll: true }); button.click(); await tick(); };
const change = async (element, value) => { Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(element, value); element.dispatchEvent(new Event("input", { bubbles: true })); await tick(); };
const capture = label => snapshots.push({ label, actions: discussions().map(button => ({ label: button.textContent, accessibleName: button.getAttribute("aria-label"), disabled: button.matches(":disabled") })), notice: document.querySelector(".scene-discussion-notice")?.textContent ?? null, reference: prepared.at(-1)?.source.focus.reference ?? null });
(async () => { try {
  await render(); await until(() => discussions().length === 3 && !discussions()[0].matches(":disabled"), "Saved chapter not loaded");
  const editor = document.querySelector<HTMLTextAreaElement>('[data-scene-id="group-one"] textarea')!;
  editor.focus(); editor.setSelectionRange(1, 3); editor.dispatchEvent(new Event("select", { bubbles: true }));
  await click(discussions()[0]); await click(discussions()[1]);
  check(prepared[0].source.focus.reference.sceneGroupId === "group-one" && prepared[1].source.focus.reference.sceneGroupId === "group-two", "Equal text confused scene identity");
  check(prepared[0].source.focus.view === "chapters" && prepared[0].source.focus.chapterId === chapter.id && prepared[0].source.focus.chapterNumber === 1, "Missing chapter focus");
  check(prepared[0].source.focus.reference.revisionId === "source-a" && prepared[0].source.focus.reference.snapshotId === "snapshot-a", "Source was guessed");
  check(document.querySelector('[data-scene-id="group-one"] textarea') === editor && editor.selectionStart === 1 && editor.selectionEnd === 3, "Preparing discussion replaced editor/selection");
  await click(discussions()[2]); check(prepared.at(-1).source.focus.reference.sceneGroupId === "group-locked", "Locked scene could not be read"); capture("saved-and-locked");
  await change(editor, "未保存的修改。"); check(dirty && discussions().every(button => button.matches(":disabled")), "Dirty scene discussion was permitted");
  check(document.querySelector(".scene-discussion-notice")?.textContent === "保存后讨论此场景", "Missing save explanation"); capture("unsaved");
  await click([...document.querySelectorAll<HTMLButtonElement>("button")].find(button => button.textContent === "保存"));
  await until(() => !dirty && !discussions()[0].matches(":disabled"), "Saved source not restored"); await click(discussions()[0]);
  check(prepared.at(-1).source.focus.reference.revisionId === "saved-source-a-1" && prepared.at(-1).source.focus.reference.snapshotId === "saved-snapshot-a-1", "Discussion reused pre-save source");
  await click([...document.querySelectorAll<HTMLButtonElement>("button")].find(button => button.textContent === "下移场景 1"));
  check(discussions().every(button => button.matches(":disabled")), "Unsaved reorder discussion allowed");
  await click([...document.querySelectorAll<HTMLButtonElement>("button")].find(button => button.textContent === "保存")); await until(() => !dirty, "Reorder save");
  await click(discussions()[0]); check(prepared.at(-1).source.focus.reference.sceneGroupId === "group-two", "Reorder used ordinal instead of saved group ID"); capture("saved-reorder");
  running = true; await render(); check(discussions().every(button => button.matches(":disabled")), "Running task allowed discussion preparation"); capture("running");
  running = false; await render(); holdRead = true; refreshToken++; await render(); await until(() => releaseRead, "Deferred read missing");
  projectId = "b"; refreshToken++; await render(); await until(() => discussions().length === 3 && !discussions()[0].matches(":disabled"), "Project B missing");
  releaseRead(); await tick(); await click(discussions()[0]); check(prepared.at(-1).source.focus.reference.projectId === "b" && prepared.at(-1).source.focus.reference.revisionId === "source-b", "Late A source leaked to B");
  projectId = "a"; refreshToken++; await render(); await until(() => discussions().length === 3 && !discussions()[0].matches(":disabled"), "Return A missing");
  await click(discussions()[0]); check(prepared.at(-1).source.focus.reference.projectId === "a" && prepared.at(-1).source.focus.reference.revisionId === "saved-source-a-2", "Return A reused stale source");
  for (const theme of ["light", "dark"]) for (const width of [330, 740]) {
    document.documentElement.dataset.theme = theme; document.querySelector<HTMLElement>("main")!.style.width = `${width}px`; await tick(); capture(`${theme}-${width}`);
    const bounds = document.querySelector("main")!.getBoundingClientRect();
    check(Math.abs(bounds.width - width) <= 1, "Requested chapter viewport width was not applied");
    check(discussions().every(button => { const box = button.getBoundingClientRect(); return box.width > 0 && box.left >= bounds.left - 1 && box.right <= bounds.right + 1; }), "Scene controls clipped");
  }
  await revealControl(discussions()[0]);
  document.querySelector(".chapter-scene-management")!.scrollIntoView({ block: "center" }); await tick();
  window.testScreenshot = "chapter-scene-discussion"; await until(() => !window.testScreenshot, "Screenshot hook did not finish");
  check(writes === 2 && modelCalls === 0, "Preparing discussion wrote or executed a model");
  window.testResult = { passed: true, snapshots, prepared, saves: saves.length, modelCalls };
} catch (error) { window.testResult = { passed: false, error: error.stack ?? String(error) }; } })();
