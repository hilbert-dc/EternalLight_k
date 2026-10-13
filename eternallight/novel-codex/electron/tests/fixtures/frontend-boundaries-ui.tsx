import { useConversationExecution } from "../../../src/conversation/useConversationExecution";
import { RevisionAuthorityStatus } from "../../../src/ChapterDocumentView";
import { PlanningRevisionPreview } from "../../../src/PlanningRevisionPreview";
import { useEditionNavigation } from "../../../src/multimedia/useEditionNavigation";
import { useConversationDirectory } from "../../../src/useConversationDirectory";
import { usePlanningDraft } from "../../../src/usePlanningDraft";
import { useComposerDrafts } from "../../../src/useComposerDrafts";
import React from "react";
import { createRoot } from "react-dom/client";
import { flushSync } from "react-dom";
import { GenerationPermitDialog } from "../../../src/GenerationPermitDialog";
import { useChapterCreation } from "../../../src/useChapterCreation";
import { useReviewChoices } from "../../../src/useReviewChoices";

const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (value, message) => { if (!value) throw new Error(message); };
let execution;
function ExecutionFixture() { execution = useConversationExecution(undefined); return <><RevisionAuthorityStatus accepted={{ id: "revision", version: null }} /><PlanningRevisionPreview kind="brief" payload={null} /></>; }
let choices, rejectGeneration, writes = 0, errors = [];
const review = { projectId: "project-a", intent: "继续创作", targetChapters: 1, expectedProjectRevision: 1,
  expectedBaseSnapshotId: "head", expectedSourceDigest: "digest", revisionPredecessor: null };
let currentReview = review;
let currentProject = "project-a";
const root = createRoot(document.getElementById("root")!);
function Fixture() {
  choices = useReviewChoices();
  return <GenerationPermitDialog blocked={false} generationPermitReview={currentReview} projectId={currentProject}
    setGenerationPermitReview={update => { currentReview = typeof update === "function" ? update(currentReview) : update; }}
    onDismissRevisionContinuation={undefined} generationReadError={false} setGenerationReload={() => {}}
    workspace={{ revision: 1, headSnapshotId: "head", sourceDigest: "digest" } as any} api={undefined} view="brief"
    onGenerateWork={() => { writes++; return new Promise((_, reject) => { rejectGeneration = reject; }); }}
    setError={value => errors.push(value)} />;
}
let creation, creationProject = "project-a", blocked = false, writeLock = false, createWrites = 0;
let readFails = true, resolveCreate = null, holdCreate = false, selected = [];
const creationWorkspace = () => ({ project: { id: creationProject }, revision: 7, chapters: [{ id: "created" }] });
const creationApi = { runtime: {
  materializeStandaloneChapter: async () => { createWrites++; if (holdCreate) return new Promise(resolve => { resolveCreate = resolve; }); return { chapterId: "created" }; },
  getState: async projectId => { if (readFails) throw Error("saved receipt read failed"); return { project: { id: projectId }, novelEdition: { chapters: [{ id: "created" }] } }; },
  getCreativeWorkspace: async projectId => ({ project: { id: projectId }, chapters: [{ id: "created" }] }),
  listPlanningImpacts: async () => ({ data: [], counts: { open: 0 }, nextCursor: null }),
} };
function CreationFixture() {
  creation = useChapterCreation({ api: creationApi as any, projectId: creationProject, view: "chapters", workspace: creationWorkspace() as any,
    request: { projectId: "project-a", requestId: "request-a", volumeId: "volume-a", volumeTitle: "卷 A" },
    canPrepare: !blocked, blocked, onPrepared: () => {}, onFormalChange: async () => {},
    beginWrite: () => { if (writeLock) return null; writeLock = true; return () => { writeLock = false; }; },
    onReadback: value => selected.push({ projectId: creationProject, chapter: value.chapter.id }), onError: value => errors.push(value) });
  return <div>{creation.pending?.error}</div>;
}
let directory, planning, drafts, boundaryProject = "project-a", pinResolve, pinCalls = 0, running = false;
const pendingLists = [];
const thread = projectId => ({ id: "same-id", projectId, name: "registered", isPinned: false, status: "idle" });
const boundaryApi = { agent: {
  listThreads: projectId => new Promise(resolve => pendingLists.push({projectId, resolve})),
  setThreadPinned: async () => { pinCalls++; await new Promise(resolve => { pinResolve = resolve; }); },
  archiveThread: async () => { throw Error("running archive must not reach host"); },
} };
function BoundaryFixture() {
  directory = useConversationDirectory({ api: boundaryApi as any, projectId: boundaryProject, authorKey: "author", enabled: true, configured: true, changing: false,
    isRunning: () => running, onArchived: () => { throw Error("unexpected archive"); } });
  planning = usePlanningDraft(() => {});
  drafts = useComposerDrafts(boundaryProject, "thread", "author");
  return <div>{directory.error}</div>;
}
const excerpt = (id, projectId = "project-a", text = "完整引用") => ({ id, projectId, view: "chapters", label: "正文", text, state: "displayed", locator: "paragraph", snapshotId: null, revisionId: null, objectId: null, start: 0, end: text.length });
let editions, editionProject = "project-a", editionReadResolve, editionLoads = [], deferEditionRead = true;
const edition = projectId => ({ id: "edition", editionId: "edition", projectId, medium: "comic", scopeId: "scope", branchId: "main" });
const editionApi = { multimedia: {
  listEditions: async projectId => ({ editions: [edition(projectId)] }),
  readWorkspace: async projectId => deferEditionRead ? new Promise(resolve => { editionReadResolve = resolve; }) : ({ project: { id: "foreign" }, edition: edition(projectId) }),
} };
function EditionFixture() {
  editions = useEditionNavigation({ api: editionApi as any, projectId: editionProject, authorKey: "author", onBegin: () => {}, onEnd: () => {}, onLoaded: value => editionLoads.push(value) });
  return <div>{editions.context?.project.id}</div>;
}
(async () => { try {
  flushSync(() => root.render(<Fixture />));
  flushSync(() => {
    choices.setFactDecisions(previous => ({ ...previous, first: "confirmed" }));
    choices.setFactDecisions(previous => ({ ...previous, second: "rejected" }));
    choices.setSelectedSceneGroups(["scene"]);
  });
  check(choices.reviewUiSnapshot.factDecisions.first === "confirmed" && choices.reviewUiSnapshot.factDecisions.second === "rejected"
    && choices.reviewUiSnapshot.selectedSceneGroups[0] === "scene", "batched author choices lost an update");
  const saved = structuredClone(choices.reviewUiSnapshot);
  flushSync(() => choices.setFactDecisions({}));
  flushSync(() => choices.restoreReviewUi(saved));
  check(JSON.stringify(choices.reviewUiSnapshot) === JSON.stringify(saved), "review snapshot restoration changed author choices");
  const confirm = [...document.querySelectorAll("button")].find(button => button.textContent === "确认开始创作")!;
  confirm.click(); confirm.click();
  await tick();
  check(writes === 1, "duplicate confirmation dispatched generation twice");
  currentProject = "project-b"; currentReview = null;
  flushSync(() => root.render(<Fixture />));
  rejectGeneration(new Error("late failure from project a"));
  await tick();
  check(errors.length === 0 && !document.querySelector('[role="dialog"]'), "old generation failure crossed project identity");
  flushSync(() => root.render(<CreationFixture />)); await tick();
  check(creation.open && creation.volumeTitle === "卷 A", "chapter request lost source volume");
  blocked = true; flushSync(() => root.render(<CreationFixture />));
  await creation.create("blocked"); check(createWrites === 0, "dirty boundary allowed creation");
  blocked = false; flushSync(() => root.render(<CreationFixture />));
  const firstCreate = creation.create("chapter"); const duplicate = creation.create("chapter");
  await Promise.all([firstCreate, duplicate]); await tick();
  check(createWrites === 1 && creation.pending?.chapterId === "created" && !writeLock, "receipt or duplicate write boundary was lost");
  readFails = false; await creation.recover(); await tick();
  check(createWrites === 1 && selected.length === 1 && creation.pending === null && !creation.open, "read recovery replayed creation or failed to settle navigation");
  holdCreate = true; const lateCreate = creation.create("late chapter"); await tick();
  creationProject = "project-b"; flushSync(() => root.render(<CreationFixture />));
  check(!creation.open && !creation.pending && !creation.volumeTitle, "chapter A creation UI leaked into project B");
  creationProject = "project-a"; flushSync(() => root.render(<CreationFixture />));
  resolveCreate({ chapterId: "late-created" }); await lateCreate; await tick();
  check(selected.length === 1 && !creation.pending && !writeLock, "late ABA creation selected an obsolete chapter or retained its lock");
  flushSync(() => root.render(<BoundaryFixture />)); await tick();
  const oldList = pendingLists.shift();
  boundaryProject = "project-b"; flushSync(() => root.render(<BoundaryFixture />)); await tick();
  boundaryProject = "project-a"; flushSync(() => root.render(<BoundaryFixture />)); await tick();
  oldList.resolve({ threads: [{...thread("project-a"), name: "stale"}] }); await tick();
  check(directory.threads.length === 0, "ABA list response entered current directory");
  pendingLists.at(-1).resolve({ threads: [thread("project-a")] }); await tick();
  const pin = directory.pin(directory.threads[0]); await directory.pin(directory.threads[0]);
  check(pinCalls === 1, "duplicate pin reached Host");
  boundaryProject = "project-b"; flushSync(() => root.render(<BoundaryFixture />)); await tick();
  pinResolve(); await pin; await tick();
  check(directory.threads.length === 0 && !directory.error, "late pin changed another project");
  running = true; await directory.archive(thread("project-b")); await tick();
  check(directory.error?.includes("正在执行"), "running archive was not rejected");
  flushSync(() => { planning.editPlan(0, { title: "作者标题" }); planning.editPlan(0, { goal: "作者目标" }); });
  check(planning.plans[0].title === "作者标题" && planning.plans[0].goal === "作者目标", "atomic planning edits lost a field");
  boundaryProject = "project-a"; flushSync(() => root.render(<BoundaryFixture />));
  const adding = drafts.addExcerpt(excerpt("late"));
  boundaryProject = "project-b"; flushSync(() => root.render(<BoundaryFixture />));
  boundaryProject = "project-a"; flushSync(() => root.render(<BoundaryFixture />));
  check(!await adding && drafts.excerpts.length === 0, "lazy excerpt validation crossed ABA scope");
  check(await drafts.addExcerpt(excerpt("exact")), "valid excerpt rejected"); await tick();
  check(drafts.excerpts[0].text === "完整引用" && drafts.excerptsReady, "validated excerpt changed text or remained unavailable");
  check(!await drafts.addExcerpt(excerpt("huge", "project-a", "界".repeat(12001))), "oversized excerpt bypassed budget");
  check(drafts.excerpts.length === 1, "failed excerpt validation mutated accepted draft");
  flushSync(() => root.render(<EditionFixture />)); await tick();
  const pendingEdition = editions.open(edition("project-a")); await tick();
  editionProject = "project-b"; flushSync(() => root.render(<EditionFixture />)); await tick();
  editionProject = "project-a"; flushSync(() => root.render(<EditionFixture />)); await tick();
  editionReadResolve({ project: { id: "project-a" }, edition: edition("project-a") }); await pendingEdition;
  check(editionLoads.length === 0 && editions.context === null && !editions.opening.current, "late edition response crossed ABA owner or retained navigation lock");
  deferEditionRead = false;
  let rejected = false; try { await editions.open(edition("project-a")); } catch { rejected = true; }
  check(rejected && !editions.opening.current && editionLoads.length === 0, "foreign media receipt entered workspace");
  flushSync(() => root.render(<ExecutionFixture />));
  const controller = execution;
  check(!Object.keys(controller).some(key => key.endsWith("Ref") || key.startsWith("set")), "public execution boundary leaked mutable state");
  flushSync(() => { execution.activateConversation("project-a", "conversation-a"); execution.startCandidate("第一条创作要求"); });
  const firstScope = execution.scope;
  let invalidResumeStarted = false;
  try { execution.resumeWork(undefined, { id: "", projectId: "project-a", threadId: "conversation-a", kind: "advisory", steps: [] }, "project-a", () => true, () => { invalidResumeStarted = true; return {}; }); } catch { /* Invalid origin must fail before changing the execution scope. */ }
  check(!invalidResumeStarted && execution.scope === firstScope, "invalid task origin reset live execution before validation");
  check(execution.isRunning() && execution.isViewing("project-a", "conversation-a"), "execution command lost current conversation");
  flushSync(() => execution.activateConversation("project-a", "conversation-b"));
  check(execution === controller && !execution.isRunning() && execution.isConversationRunning("conversation-a"), "navigation lost background execution or replaced command identity");
  flushSync(() => execution.activateConversation("project-a", "conversation-a"));
  check(execution.scope === firstScope && execution.run.prompt === "第一条创作要求", "conversation restoration lost owned execution");
  flushSync(() => execution.leaveProject());
  check(execution.scope !== firstScope && !execution.isRunning(), "project reset retained current execution scope");
  check(document.body.textContent.includes("版本号未提供") && document.body.textContent.includes("尚未提供规划内容") && !document.body.textContent.includes("vnull"), "nullable receipt invented revision metadata");
  window.testResult = { passed: true, writes, restoredChoices: true, isolatedLateFailure: true, creationWrites: createWrites,
    creationReadRecovery: true, creationScopeIsolation: true };
} catch (error) { window.testResult = { passed: false, error: error.stack }; } })();
