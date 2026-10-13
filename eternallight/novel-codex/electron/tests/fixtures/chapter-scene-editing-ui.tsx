import React from "react";
import { createRoot } from "react-dom/client";
import { CreativeWorkspaceView, revealControl } from "./creative-workspace-harness";
import { chapterRuntimeReads } from "../helpers/chapter-runtime";
import "../../../src/styles.css";

const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (value, message) => { if (!value) throw Error(message); };
const until = async (predicate, message) => { for (let index = 0; index < 150 && !predicate(); index++) await tick(); check(predicate(), message); };
const chapters = [
  { id: "chapter-one", number: 1, title: "同名信件", status: "draft", summary: "", passages: [] },
  { id: "chapter-two", number: 2, title: "第二封信", status: "draft", summary: "", passages: [] },
];
const makeWorkspace = id => ({ project: { id }, revision: 1, headSnapshotId: `snapshot-${id}`, sourceDigest: "a".repeat(64), index: { status: "current" },
  artifacts: Object.fromEntries(["brief", "outline", "plan", "identity", "propertyRules"].map(kind => [kind, { candidate: null, accepted: null, working: null, workingSelection: null, locked: false }])),
  chapters: chapters.map((chapter, index) => ({ ...chapter, candidate: { id: `source-${id}-${index + 1}`, version: 1, title: chapter.title, chapterId: chapter.id, source: "author", body: index ? "第二章原文。" : "相同场景。\n\n---\n\n相同场景。\n\n---\n\n保护的原文。", sceneGroups: index ? [
    { id: "group-other", sceneId: "stable-other", body: "第二章原文。", locked: false },
  ] : [
    { id: "group-one", sceneId: "stable-one", body: "相同场景。", locked: false },
    { id: "group-two", sceneId: "stable-two", body: "相同场景。", locked: false },
    { id: "group-locked", sceneId: "stable-locked", body: "保护的原文。", locked: true },
  ] }, accepted: null, working: null, workingSelection: null })), workingBaselines: [], continuationIntents: [], planningReviews: [], commits: [], tasks: [], impacts: [], modeling: [], factProposals: [], reviews: [], chapterPlanBindings: [] });
const workspaces = { a: makeWorkspace("a"), b: makeWorkspace("b") };
let projectId = "a", selectedChapterId = chapters[0].id, running = false, refreshToken = 0, holdRead = false, releaseRead;
const saves = [], snapshots = [];
let writes = 0, modelCalls = 0, dirty = false;
const api = { runtime: { ...chapterRuntimeReads(id => ({ id })),
  getCreativeWorkspace: async id => { const copy = structuredClone(workspaces[id]); if (holdRead) { holdRead = false; await new Promise(resolve => { releaseRead = () => resolve(); }); } return copy; },
  getCommitmentExtraction: async () => null,
  getWriterPreview: async () => ({ target: { position: {} }, plannedEffects: [], relevantText: [], coverage: { complete: true, missingScope: [], truncationReasons: [] }, budgetsUsed: {} }),
  saveDraftCheckpoint: async (id, payload, expectedRevision, candidateId) => {
    const workspace = workspaces[id], chapter = workspace.chapters.find(item => item.id === payload.chapterId);
    check(expectedRevision === workspace.revision && candidateId === chapter.candidate.id, "Save CAS changed");
    const locked = payload.sceneGroups.find(group => group.id === "group-locked"); check(locked?.locked && locked.body === "保护的原文。", "Scene actions weakened protected content");
    writes++; saves.push(structuredClone(payload)); workspace.headSnapshotId = `saved-snapshot-${id}-${writes}`;
    chapter.candidate = { ...chapter.candidate, ...payload, id: `saved-source-${id}-${writes}`, version: 1 + writes };
    return { revisionId: chapter.candidate.id };
  },
}, agent: {} };
const root = createRoot(document.getElementById("root"));
window.addEventListener("error", event => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
const render = async () => {
  root.render(<div className="workspace"><main className="main-panel chapter-main-panel" style={{ height: 550, width: 640, overflow: "auto" }}>
    <CreativeWorkspaceView api={api} projectId={projectId} view="chapters" state={{ project: { id: projectId }, canon: { characters: [], locations: [], relationships: [], events: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1", scenes: [], anchors: [] }, novelEdition: { chapters } }}
      selectedChapterId={selectedChapterId} refreshToken={refreshToken} taskConversationContainer={null} agentAvailable={false} agentRunning={running}
      onDiscuss={() => { modelCalls++; }} onDirtyChange={value => { dirty = value; }} onFormalChange={async () => {}}
      onSelectChapter={() => {}} onGenerateWork={async () => { modelCalls++; }} onGenerateChapter={async () => { modelCalls++; }}
      onContinueWork={async () => {}} onRetryWork={async () => {}} onResolveFeedback={async () => {}} onReviewPlanning={async () => { modelCalls++; }} onCancelPlanningReview={async () => {}} onOpenPlanningTarget={() => {}} onReturnToFeedback={() => {}} />
  </main></div>);
  await tick(); await tick();
};
const buttons = label => [...document.querySelectorAll<HTMLButtonElement>("button")].filter(button => button.textContent === label || button.getAttribute("aria-label") === label);
const triggers = () => [...document.querySelectorAll<HTMLButtonElement>(".chapter-scene-menu-trigger")];
const editors = () => [...document.querySelectorAll<HTMLTextAreaElement>(".draft-editor")];
const sceneIds = () => [...document.querySelectorAll(".draft-editor-wrap [data-scene-id]")].map(element => element.getAttribute("data-scene-id"));
const menu = () => document.querySelector<HTMLDivElement>('[role="menu"]');
const click = async button => { check(button && !button.matches(":disabled"), "Unavailable scene action"); await revealControl(button); button.scrollIntoView({ block: "nearest" }); await tick(); button.focus({ preventScroll: true }); button.click(); await tick(); };
const open = async index => { await click(triggers()[index]); await until(() => menu()?.getAttribute("aria-label") === `场景 ${index + 1} 操作`, "Scene menu missing"); };
const key = async key => { document.activeElement.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true })); await tick(); };
const escape = async () => { await key("Escape"); check(!menu(), "Escape did not close menu"); };
const change = async (element, value) => { Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(element, value); element.dispatchEvent(new Event("input", { bubbles: true })); await tick(); };
const capture = label => snapshots.push({ label, sceneIds: sceneIds(), editors: editors().map(editor => ({ body: editor.value, readOnly: editor.readOnly, disabled: editor.matches(":disabled") })), actions: menu() ? [...menu().querySelectorAll<HTMLButtonElement>("button")].map(button => ({ label: button.textContent, accessibleName: button.getAttribute("aria-label"), disabled: button.matches(":disabled") })) : null });
(async () => { try {
  await render(); await until(() => triggers().length === 3 && !triggers()[0].matches(":disabled"), "Saved chapter not loaded");
  check(!document.querySelector(".chapter-manual-tools") && !buttons("讨论这个场景").length, "Removed manual tools or duplicated scene discussion remains");
  const editor = editors()[0]; editor.focus(); editor.setSelectionRange(1, 3); editor.dispatchEvent(new Event("select", { bubbles: true }));
  await open(0); check(document.activeElement === buttons("保护场景 1")[0], "Menu did not focus first action");
  await key("ArrowDown"); check(document.activeElement === buttons("下移场景 1")[0], "Arrow navigation did not skip disabled actions");
  await key("Home"); check(document.activeElement === buttons("保护场景 1")[0], "Home focus");
  await key("End"); check(document.activeElement === buttons("下移场景 1")[0], "End focus");
  await key("ArrowUp"); check(document.activeElement === buttons("保护场景 1")[0], "ArrowUp focus");
  await escape(); check(document.activeElement === triggers()[0] && editors()[0] === editor && editor.selectionStart === 1 && editor.selectionEnd === 3, "Closing menu lost editor identity or selection");
  await open(2); check(editors()[2].readOnly && !editors()[2].disabled, "Protected text cannot be selected");
  check(["上移场景 3", "下移场景 3", "删除场景 3"].every(label => buttons(label)[0].disabled), "Protected scene movement or deletion allowed"); capture("protected"); await escape();
  editors()[2].focus(); editors()[2].setSelectionRange(0, 4); check(editors()[2].selectionEnd === 4, "Protected selection unavailable");
  await open(1); check(buttons("下移场景 2")[0].disabled && buttons("删除场景 2")[0].disabled, "Movement/deletion crossed protected boundary"); await escape();
  await open(0); await click(buttons("保护场景 1")[0]); await until(() => editor.readOnly && document.activeElement === editor, "Protect did not return to scene");
  await open(0); check(buttons("下移场景 1")[0].disabled && buttons("取消保护场景 1").length === 1, "Protect state not reflected");
  await click(buttons("取消保护场景 1")[0]); await until(() => !editor.readOnly && document.activeElement === editor, "Unprotect did not restore edit focus");
  await change(editor, "未保存的修改。"); check(dirty, "Edit not dirty"); await click(buttons("保存")[0]);
  await until(() => !dirty, "Save not restored"); check(saves[0].sceneGroups[0].id === "group-one" && saves[0].sceneGroups[0].body === "未保存的修改。", "Save lost exact edited scene");
  await open(0); await click(buttons("下移场景 1")[0]); await until(() => document.activeElement === editor, "Reorder did not focus moved scene");
  check(sceneIds().join(",") === "group-two,group-one,group-locked" && editor === editors()[1], "Equal text reorder used ordinals rather than stable identities");
  await click(buttons("保存")[0]); await until(() => !dirty, "Reorder save"); capture("saved-reorder");
  await open(1); running = true; await render(); check(!menu() && triggers().every(button => button.matches(":disabled")), "Running task left editable menu open"); capture("running");
  running = false; await render(); await open(0); selectedChapterId = chapters[1].id; await render();
  await until(() => sceneIds().join() === "group-other", "Chapter switch missing"); check(!menu(), "Scene menu crossed chapters");
  selectedChapterId = chapters[0].id; await render(); await until(() => triggers().length === 3 && !triggers()[0].matches(":disabled"), "Return chapter missing");
  await open(0); holdRead = true; refreshToken++; await render(); await until(() => releaseRead, "Deferred read missing");
  projectId = "b"; refreshToken++; await render(); await until(() => triggers().length === 3 && !triggers()[0].matches(":disabled"), "Project B missing");
  releaseRead(); await tick(); await tick(); check(!menu() && editors()[0].value === "相同场景。", "Late A read or menu leaked to B");
  projectId = "a"; refreshToken++; await render(); await until(() => triggers().length === 3 && !triggers()[0].matches(":disabled"), "Return A missing");
  check(sceneIds().join(",") === "group-two,group-one,group-locked" && editors()[1].value === "未保存的修改。", "Return A lost saved order or edit");
  for (const theme of ["light", "dark"]) for (const width of [330, 740]) {
    document.documentElement.dataset.theme = theme; window.testViewport = { width: width + 50, height: 900 }; await until(() => !window.testViewport, "Viewport resize hook");
    document.querySelector<HTMLElement>("main")!.style.width = `${width}px`; await tick(); await open(0); capture(`${theme}-${width}`);
    const bounds = document.querySelector("main")!.getBoundingClientRect(), menuBounds = menu().getBoundingClientRect();
    check(Math.abs(bounds.width - width) <= 1, "Requested chapter width not applied");
    check(triggers().every(button => { const box = button.getBoundingClientRect(); return box.width > 0 && box.left >= bounds.left - 1 && box.right <= bounds.right + 1; }), "Scene controls clipped");
    check(menuBounds.left >= 0 && menuBounds.right <= innerWidth && menuBounds.top >= 0 && menuBounds.bottom <= innerHeight, "Scene portal menu outside viewport");
    window.testScreenshot = `chapter-scene-editing-${theme}-${width}`; await until(() => !window.testScreenshot, "Screenshot hook did not finish"); await escape();
  }
  await click(buttons("＋ 添加场景")[0]); await until(() => editors().length === 4 && document.activeElement === editors()[3], "Adding scene did not focus new editor");
  const added = sceneIds()[3]; await open(3); await click(buttons("删除场景 4")[0]); await until(() => editors().length === 3 && document.activeElement === editors()[2], "Deleting scene did not focus surviving neighbor");
  check(!sceneIds().includes(added), "Deleted scene remains");
  await open(2); await click(buttons("取消保护场景 3")[0]);
  for (let count = 3; count > 0; count--) { await open(count - 1); await click(buttons(`删除场景 ${count}`)[0]); await until(() => editors().length === count - 1, "Scene delete not applied"); }
  await until(() => document.activeElement === buttons("＋ 添加场景")[0], "Last scene delete lost focus"); check(dirty, "Deleting final scene did not retain unsaved changes");
  await click(buttons("＋ 添加场景")[0]); await until(() => editors().length === 1 && document.activeElement === editors()[0], "Empty document cannot add a scene");
  check(writes === 2 && modelCalls === 0, "Manual scene editing executed a model or unexpected save");
  window.testResult = { passed: true, snapshots, saves: saves.length, modelCalls };
} catch (error) { window.testResult = { passed: false, error: error.stack ?? String(error), dom: document.body.textContent.slice(-3000) }; } })();
