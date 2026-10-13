import React, { useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { flushSync } from "react-dom";
import { useWorkspaceEditor } from "../../../src/useWorkspaceEditor";
import { useSupplementalEditor } from "../../../src/supplemental-editor";
import { emptyReviewUi } from "../../../src/review-ui-storage";

const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (value, message) => { if (!value) throw Error(message); };
const until = async (predicate, message) => { for (let i = 0; i < 100 && !predicate(); i++) await tick(); check(predicate(), message); };
const deferred = () => { let resolve; const promise = new Promise(done => { resolve = done; }); return { promise, resolve }; };
const chapter = { id: "chapter", number: 1, title: "章节", passages: [] };
const workspace = projectId => ({ project: { id: projectId }, revision: 1,
  artifacts: Object.fromEntries(["brief", "outline", "plan"].map(kind => [kind, { candidate: null, accepted: null, working: null, locked: false }])),
  chapters: [{ ...chapter, candidate: null, accepted: null }], impacts: [], openImpactCount: 0 });
const page = id => ({ data: [{ id }], counts: { total: 1, open: 1, retained: 0, lockedConflict: 0 }, nextCursor: "next" });
let projectId = "a", editor, error, pagination = deferred();
const makeApi = () => ({ runtime: {
  getCreativeWorkspace: async id => workspace(id),
  listPlanningImpacts: async (id, cursor) => cursor ? pagination.promise : page(id),
} });
let api = makeApi();
const root = createRoot(document.getElementById("root")!);
function Fixture() {
  const supplementalEditor = useSupplementalEditor();
  const [, setError] = useState(null), [, setNotice] = useState(null);
  const [reviewDirty, setReviewDirty] = useState(false), reviewDirtyRef = useRef(false);
  editor = useWorkspaceEditor({ authorKey: "editor-lifecycle", api: api as any, projectId, view: "chapters", state: { novelEdition: { chapters: [chapter] } } as any,
    selectedChapter: chapter, refreshToken: 0, agentRunning: false,
    taskControls: { pending: false, resuming: false, hasPendingRequests: () => false } as any,
    supplementalEditor, publicationPreparing: false, chapterAcceptActivity: null,
    reviewDirty, reviewDirtyRef, reviewUiSnapshot: emptyReviewUi([]), setReviewDirty, feedbackTarget: null,
    setError: value => { error = value; setError(value); }, setNotice, onFormalChange: async () => {}, onDirtyChange: () => {},
    readReviewSessionKey: () => `review:${projectId}`, discardCurrentReviewUi: () => true });
  return <div>{editor.workspace?.project.id}</div>;
}
const render = () => flushSync(() => root.render(<Fixture />));
(async () => { try {
  render(); await until(() => editor.workspace?.project.id === "a" && !editor.loading, "A did not load");
  const more = editor.loadMoreImpacts(); await tick();
  check(editor.busy, "Pagination did not own activity");
  projectId = "b"; render(); await until(() => editor.workspace?.project.id === "b" && !editor.loading, "B did not load");
  const writeB = deferred(); const resultB = editor.perform("保存 B", () => writeB.promise, "B saved"); await tick();
  pagination.resolve(page("foreign-a")); await more; await tick();
  check(editor.workspace.impacts.every(item => item.id === "b"), "A page contaminated B");
  check(editor.busy && editor.isWriting(), "A page released B write activity");
  writeB.resolve(); check(await resultB, "B write did not finish"); await tick();
  check(!editor.busy && !editor.isWriting(), "B write remained locked");

  const writeOld = deferred(); const oldOwner = editor.editorOwner;
  const oldResult = editor.perform("旧连接保存", () => writeOld.promise, "old saved"); await tick();
  const originalApi = api; api = makeApi(); render(); await tick(); await tick();
  api = originalApi; render(); await tick(); await tick();
  check(!editor.isCurrent(oldOwner) && !editor.isWriting(), "API ABA retained old ownership or write lock");
  const writeNew = deferred(); const newResult = editor.perform("新连接保存", () => writeNew.promise, "new saved"); await tick();
  writeOld.resolve(); check(!await oldResult, "Old write claimed a current success"); await tick();
  check(editor.busy && editor.isWriting(), "Old finally released the new connection write");
  writeNew.resolve(); check(await newResult, "New connection could not complete"); await tick();
  check(!editor.busy && !editor.isWriting() && !error, "Editor failed to settle after replacement");
  root.unmount(); window.testResult = { passed: true, isolatedPagination: true, isolatedWriteCompletion: true, apiAba: true };
} catch (reason) { window.testResult = { passed: false, error: reason.stack ?? String(reason) }; } })();
