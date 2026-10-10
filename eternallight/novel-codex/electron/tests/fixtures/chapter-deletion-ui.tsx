import { novelFixtureMedia } from "../helpers/novel-edition";
import { emptyCreationIssueSummary } from "../helpers/creation-issues";
import React from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import "../../../src/styles.css";
import { saveAuthor } from "../helpers/author-session";
import { chapterRuntimeReads } from "../helpers/chapter-runtime";
import { useChapterDeletion } from "../../../src/useChapterDeletion";

const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (ok, message) => { if (!ok) throw Error(message); };
const until = async (predicate, message) => { for (let i = 0; i < 200 && !predicate(); i++) await tick(); check(predicate(), message); };
const button = label => [...document.querySelectorAll<HTMLButtonElement>("button")].find(node => node.textContent === label || node.getAttribute("aria-label") === label);
const row = id => document.querySelector<HTMLButtonElement>(`[data-directory-chapter-id="${id}"]`);
const prose = () => document.querySelector<HTMLTextAreaElement>('[aria-label="场景 1 正文"]');
const titleInput = () => document.querySelector<HTMLInputElement>('[aria-label="章节标题"]');
const laterChapterWarning = "删除后会自动检查故事衔接，结果显示在对话中。是否调整其他章节，由你决定。";
const edit = async (element, value) => { Object.getOwnPropertyDescriptor(element instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype, "value").set.call(element, value); element.dispatchEvent(new Event("input", { bubbles: true })); await tick(); };
const deferred = () => { let resolve; const promise = new Promise(fn => { resolve = fn; }); return { promise, resolve }; };
const projects = ["project-delete-a", "project-delete-b"].map(id => ({ id, title: id, logline: "", updated_at: "" }));
const reads = chapterRuntimeReads(id => projects.find(project => project.id === id));
const revision = (id, title) => ({ id, title, version: 1, body: "作者已保存的章节正文。", sceneGroups: [{ id: "scene-one", sceneId: null, body: "作者已保存的章节正文。", locked: false }], sourcePlanRefs: { localOnly: true, lineageStatus: "current" } });
const makeWorkspace = project => ({ project, revision: 1, headSnapshotId: "head-original", sourceDigest: "a".repeat(64), index: { status: "ready" },
  artifacts: Object.fromEntries(["brief", "outline", "plan", "identity", "propertyRules"].map(kind => [kind, { candidate: null, accepted: null, working: null, workingSelection: null, locked: false }])),
  chapters: [1, 2, 3, 4].map(number => ({ id: `chapter-${number}`, number, title: `基础章名${number}`, status: "draft", summary: "", updated_at: "", passages: [],
    candidate: revision(`candidate-${number}`, number < 3 ? "同名章节" : `待删除第${number}章`), accepted: revision(`published-${number}`, `已发布第${number}章`), working: null, workingSelection: null })),
  tasks: [], reviews: [], modeling: [], factProposals: [], chapterPlanBindings: [], workingBaselines: [], continuationIntents: [], planningReviews: [], commits: [], impacts: [] });
const workspaces = new Map(projects.map(project => [project.id, makeWorkspace(project)]));
const catalogs = new Map(projects.map(project => [project.id, { revision: 1, volumes: [{ id: "volume-one", title: "第一卷", position: 0, source: null, outlineStatus: "independent" }], placements: [{ chapterId: "chapter-2", volumeId: "volume-one" }] }]));
const calls = [], snapshots = [], renderErrors = []; window.addEventListener("error", event => renderErrors.push(event.message));
let structurePending = false, structureWork = null, structureReads = 0;
let followup = null;
let saves = 0, modelCalls = 0, stateReads = 0, mode = "normal", failRead = false, listener, pendingDelete;
window.novelCodex = { multimedia: novelFixtureMedia, app: { onCloseRequested: () => () => {} }, runtime: {
  ...reads,
  listChapterStructureChanges: async projectId => ({ projectId, items: structurePending ? [{ id: "change-ui", projectId, chapterNumber: 4, assessment: { state: "pending" } }] : [], nextCursor: null }),
  listWorkTasks: async () => { structureReads++; return { tasks: structureWork ? [structureWork] : [], nextCursor: null, activeCount: structureWork && structureWork.status !== "completed" ? 1 : 0 }; },
  getTaskFinalReply: async () => null,
  status: async () => ({ state: "online", mode: "local", error: null }), listProjects: async () => ({ projects }),
  getState: async id => { stateReads++; if (failRead) { failRead = false; throw Error("删除后读取暂时失败"); } return { project: projects.find(project => project.id === id), canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1.0", scenes: [], anchors: [] }, novelEdition: { chapters: structuredClone(workspaces.get(id).chapters.map(({ candidate, accepted, working, workingSelection, ...chapter }) => chapter)) } }; },
  getCreativeWorkspace: async id => structuredClone(workspaces.get(id)), getWorkCatalog: async id => structuredClone(catalogs.get(id)),
  getWriterPreview: async () => ({ target: { position: { readingChapter: 1, sceneOrder: 1 } }, relevantText: [], plannedEffects: [], coverage: { complete: true, missingScope: [], truncationReasons: [] } }),
  saveDraftCheckpoint: async (id, payload) => { saves++; const workspace = workspaces.get(id), chapter = workspace.chapters.find(item => item.id === payload.chapterId); chapter.candidate = { ...revision(`saved-${saves}`, payload.title), body: payload.body, sceneGroups: payload.sceneGroups }; workspace.revision++; return { revisionId: chapter.candidate.id }; },
  deleteChapter: async (projectId, request) => {
    calls.push({ projectId, ...structuredClone(request) }); const workspace = workspaces.get(projectId), catalog = catalogs.get(projectId), chapter = workspace.chapters.find(item => item.id === request.chapterId);
    check(chapter && request.expectedRevision === workspace.revision && request.idempotencyKey, "Delete request lacks exact chapter/current CAS/key");
    if (mode === "present") throw Error("删除请求未执行");
    if (mode === "held") { pendingDelete = deferred(); await pendingDelete.promise; }
    const receipt = { operation: "delete-chapter", projectId, chapterId: chapter.id, chapterNumber: chapter.number, title: chapter.candidate.title,
      baseRevision: workspace.revision, resultRevision: workspace.revision + 1, catalogRevision: catalog.revision + 1, structureChangeId: `change-${calls.length}`, baseSnapshotId: workspace.headSnapshotId, resultSnapshotId: `head-delete-${calls.length}` };
    workspace.chapters = workspace.chapters.filter(item => item.id !== chapter.id); workspace.revision++; workspace.headSnapshotId = receipt.resultSnapshotId; catalog.revision++; catalog.placements = catalog.placements.filter(item => item.chapterId !== chapter.id);
    if (mode === "lost") throw Error("删除回执连接中断"); if (mode === "read-failed") failRead = true; return receipt;
  },
}, agent: {
  status: async () => ({ configured: true, state: "ready", mode: "novel-agent-engine", provider: "fixture", model: "fixture", error: null }), listConnections: async () => ({ connections: [] }), recoveryReport: async () => [], listExecutions: async () => [], readMessage: async () => { throw Error("message receipt not found"); }, beginInterlude: async () => { throw Error("unexpected interlude"); }, returnInterlude: async () => { throw Error("unexpected interlude return"); }, listMessages: async () => [], listThreads: async () => ({ threads: [], nextCursor: null }), listProposedActions: async () => ({ actions: [] }), setWorkspaceContext: async () => {}, onEvent: fn => { listener = fn; return () => {}; }, onUserInputRequest: () => () => {},
  sendPrompt: async (projectId, prompt, repairSource, focus) => { modelCalls++; followup = { projectId, prompt, repairSource, focus }; return { mode: "novel-agent-engine", message: "可以先讨论，不改正文。", projectId, proposedActions: [] }; }, generateChapter: async () => { modelCalls++; throw Error("unexpected writer"); }, generateWork: async () => { modelCalls++; throw Error("unexpected generation"); },
} };
const root = createRoot(document.getElementById("root")); let phase = "startup";
const openMenu = async (id, method = "context") => { const target = row(id); check(target, `Missing row ${id}`); const volume = target.closest<HTMLDetailsElement>("details.work-directory-volume"); if (volume && !volume.open) { volume.open = true; await tick(); } target.scrollIntoView({ block: "nearest" }); await tick(); check(target.checkVisibility(), "Chapter menu target hidden");
  if (method === "hover") button(`第 ${id.slice(-1)} 章操作`).click();
  else if (method === "keyboard") { target.focus({ preventScroll: true }); target.dispatchEvent(new KeyboardEvent("keydown", { key: "F10", shiftKey: true, bubbles: true })); }
  else target.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: target.getBoundingClientRect().left + 10, clientY: target.getBoundingClientRect().bottom }));
  await until(() => button("删除章节"), "Chapter deletion menu absent"); };
const askDelete = async (id, method = "context") => { await openMenu(id, method); button("删除章节").click(); await until(() => document.querySelector(".chapter-delete-dialog") || document.querySelector(".unsaved-dialog"), "Neither delete confirmation nor dirty guard opened"); };
const confirm = () => button("删除本章");
const closeDelete = async () => { document.querySelector<HTMLButtonElement>(".chapter-delete-dialog footer button").click(); await tick(); };
(async () => { try {
  saveAuthor({ name: "删除验收作者", email: "delete@example.invalid", passwordSalt: "fixture", passwordHash: "fixture" }); root.render(<App />);
  await until(() => titleInput()?.value === "同名章节" && row("chapter-4") && stateReads >= 2, "App did not load authoritative chapter data");
  const body = prose(); body.focus(); body.setSelectionRange(2, 5, "backward");
  phase = "exact menu and cancellation"; await askDelete("chapter-3", "hover"); await until(() => confirm() && document.activeElement?.textContent === "取消", "Prepared confirmation did not focus safe cancellation"); check(document.querySelector(".chapter-delete-dialog").textContent.includes("待删除第3章") && calls.length === 0, "Confirmation guessed title or wrote before confirmation");
  check(document.querySelector(".chapter-delete-dialog").innerText.includes(laterChapterWarning), "Middle chapter lacks the retained later-chapter story continuity warning"); await closeDelete();
  check(prose() === body && body.selectionStart === 2 && body.selectionEnd === 5 && body.selectionDirection === "backward", "Cancelled delete replaced editor/caret");
  check(document.activeElement?.getAttribute("aria-label") === "第 3 章操作", `Cancelled dialog failed to return focus to exact menu anchor: ${document.activeElement?.outerHTML.slice(0, 300)}`);
  snapshots.push({ case: phase, exactTitle: true, laterChaptersWarning: laterChapterWarning, initiallyFocusedCancel: true, cancelFocusReturnsToExactAnchor: true, requests: calls.length, sameEditorAndCaret: true });
  phase = "dirty cancellation"; await edit(body, "作者尚未保存的正文，取消删除必须保留。"); await askDelete("chapter-2");
  check(document.querySelector(".unsaved-dialog") && !document.querySelector(".chapter-delete-dialog[open]") && calls.length === 0, "Dirty deletion skipped author decision"); button("留在当前页面").click(); await tick();
  check(prose() === body && body.value.includes("尚未保存") && calls.length === 0 && saves === 0, "Dirty cancellation discarded or saved author text");
  await askDelete("chapter-2"); button("保存并继续").click(); await until(() => saves === 1 && confirm() && !confirm().disabled, "Explicit save-and-continue did not reach fresh deletion confirmation");
  check(calls.length === 0 && document.querySelector(".chapter-delete-dialog").textContent.includes("第 2 章 · 同名章节"), "Dirty save started deletion without fresh exact confirmation");
  snapshots.push({ case: phase, noImplicitSaveOrDelete: true, preservedDraft: true, explicitSaveThenFreshConfirmation: true });
  phase = "unknown failure stays present and requires new confirmation"; mode = "present"; confirm().click();
  await until(() => button("核对删除结果") && !button("核对删除结果").disabled, "Unknown failure lacks read-only reconciliation"); button("核对删除结果").click();
  await until(() => document.body.innerText.includes("本章仍在作品中") && !confirm()?.disabled, "Unknown delete was not reconciled as still present"); check(row("chapter-2") && prose() === body && calls.length === 1, "Failed deletion optimistically removed content");
  const oldKey = calls[0].idempotencyKey; mode = "held"; confirm().click(); confirm()?.click(); await until(() => pendingDelete, "Explicit retry did not start");
  check(calls.length === 2 && calls[1].idempotencyKey !== oldKey && calls[1].chapterId === "chapter-2" && button("核对删除结果").disabled && row("chapter-2"), "Retry changed target/reused failed key/double-wrote or hid chapter early");
  const dialog = document.querySelector<HTMLDialogElement>(".chapter-delete-dialog"); dialog.dispatchEvent(new Event("cancel", { cancelable: true })); await tick();
  check(dialog.open && document.querySelector<HTMLButtonElement>(".chapter-delete-dialog footer button").disabled && document.querySelector<HTMLButtonElement>(".project-switcher").disabled && document.querySelector<HTMLButtonElement>('[aria-label="发送创作指令"]').disabled, "Pending delete allowed cancel, cross-project navigation or model execution"); pendingDelete.resolve();
  await until(() => !row("chapter-2") && !document.querySelector(".chapter-delete-dialog[open]"), "Successful noncurrent delete did not read back");
  check(prose() === body && body.value.includes("尚未保存") && catalogs.get(projects[0].id).placements.length === 0, "Noncurrent delete changed active editor or kept placement");
  snapshots.push({ case: phase, explicitNewKey: true, requests: calls.length, currentEditorPreserved: true, placementRemoved: true });
  phase = "known receipt read failure is read only recovery"; mode = "read-failed"; await askDelete("chapter-3", "keyboard"); confirm().click();
  await until(() => document.body.innerText.includes("删除后读取暂时失败") && button("核对删除结果") && !button("核对删除结果").disabled, "Known delete receipt read failure unavailable"); const afterReceipt = calls.length; button("核对删除结果").click();
  await until(() => !row("chapter-3") && !document.querySelector(".chapter-delete-dialog[open]"), "Read-only delete recovery did not settle"); check(calls.length === afterReceipt, "Known receipt recovery repeated delete");
  snapshots.push({ case: phase, requests: calls.length, recoveryDidNotWrite: true });
  phase = "lost receipt and current chapter removal"; mode = "lost"; await askDelete("chapter-1"); confirm().click();
  await until(() => button("核对删除结果") && !button("核对删除结果").disabled, "Lost receipt lacks explicit reconciliation"); check(row("chapter-1") && titleInput()?.value === "同名章节", "Unknown receipt optimistically removed current chapter"); button("核对删除结果").click();
  await until(() => !row("chapter-1") && titleInput()?.value === "待删除第4章", "Lost receipt did not reconcile deletion and select remaining chapter");
  check(calls.length === 4 && workspaces.get(projects[0].id).chapters.length === 1, "Lost receipt replayed mutation"); snapshots.push({ case: phase, selectedRemainingChapter: 4, requests: calls.length });
  phase = "last chapter removal"; mode = "normal"; await askDelete("chapter-4"); await until(() => confirm() && !confirm().disabled, "Last chapter confirmation did not settle");
  check(document.querySelector(".chapter-delete-dialog").innerText.includes(laterChapterWarning), "Last chapter also requires the automatic continuity-check explanation"); confirm().click(); await until(() => !row("chapter-4") && !document.querySelector(".chapter-document"), "Last chapter kept phantom document");
  check(document.body.innerText.includes("章节") && workspaces.get(projects[0].id).chapters.length === 0 && calls.length === 5 && modelCalls === 0, "Empty work or execution count wrong");
  snapshots.push({ case: phase, automaticCheckExplained: true, emptyWork: true, requests: calls.length, saves, modelCalls });
  phase = "automatic continuity conversation preserves unsent input";
  const composer = document.querySelector<HTMLTextAreaElement>('textarea[aria-label="创作指令"]');
  await edit(composer, "未发送的构思要保留"); composer.focus(); composer.setSelectionRange(2, 5, "backward");
  structurePending = true; listener({ method: "agent/structure-updated", params: { projectId: projects[0].id, completed: false } });
  await until(() => document.querySelector(".chapter-structure-notice"), "Pending check is missing from conversation");
  check(document.querySelector(".conversation-rail .chapter-structure-notice") && !document.querySelector("main .chapter-structure-notice") && modelCalls === 0, "Pending status spawned a model or appeared in the story page");
  const pendingText = document.querySelector(".chapter-structure-notice").textContent;
  listener({ method: "structure/error", params: { projectId: projects[0].id, changeId: "change-ui", message: "A 检查尚未启动" } });
  listener({ method: "structure/error", params: { projectId: projects[0].id, changeId: "change-other", message: "B 检查尚未启动" } });
  await until(() => document.querySelector(".chapter-structure-notice").textContent.includes("A 检查尚未启动") && button("重试检查"), "Second failed event hid first event recovery");
  listener({ method: "agent/structure-updated", params: { projectId: projects[0].id, changeId: "change-other", completed: true } });
  await tick();
  check(document.querySelector(".chapter-structure-notice").textContent.includes("A 检查尚未启动") && button("重试检查"), "Other event completion cleared pending event recovery");
  listener({ method: "structure/error", params: { projectId: projects[1].id, changeId: "change-ui", message: "别的作品的错误" } });
  await tick();
  check(!document.querySelector(".chapter-structure-notice").textContent.includes("别的作品"), "Foreign project error replaced current recovery");
  listener({ method: "agent/structure-updated", params: { projectId: projects[0].id, changeId: "change-ui", completed: true } });
  await until(() => !button("重试检查"), "Exact event completion did not clear its recovery");
  structurePending = false;
  structureWork = { id: "work-auto", projectId: projects[0].id, goal: "检查章节删除后的故事衔接", status: "running", kind: "direct", version: 0, error: null, deliveryStatus: "pending", resumeMode: null, resumeBlockReason: null,
    deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], evidence: [], pendingActionIds: [], unsettledAttemptIds: [] },
    steps: [{ id: "step-auto", operation: "assessChapterStructure", status: "running", target: { type: "project", structureChangeId: "change-ui" }, changes: [], outputRefs: [], readingProgress: null,
      attempts: [{ id: "task-auto", threadId: "thread-auto", turnId: null, status: "running", error: null, startedAt: 1, attemptNumber: 1 }] }] };
  listener({ method: "agent/structure-started", params: { projectId: projects[0].id, taskId: "task-auto", threadId: "thread-auto", operation: "assessChapterStructure" } });
  await until(() => document.querySelector('[data-work-task-id="work-auto"]'), "Automatic task not bound to its conversation record");
  check(document.querySelectorAll('[data-work-task-id="work-auto"] .system-message').length === 1 && !document.querySelector('[data-work-task-id="work-auto"] .user-message'), "Automatic check impersonates user or duplicates label");
  check(document.querySelector('textarea[aria-label="创作指令"]') === composer && composer.value === "未发送的构思要保留" && composer.selectionStart === 2 && composer.selectionEnd === 5, "Automatic start replaced composer or lost draft/caret");
  listener({ method: "turn/started", params: { threadId: "thread-auto", turn: { id: "turn-auto", status: "inProgress" } } });
  listener({ method: "item/started", params: { threadId: "thread-auto", turnId: "turn-auto", item: { id: "read-removed", type: "mcpToolCall", server: "novel_workspace", tool: "read_chapter_structure_source", status: "inProgress" } } });
  await until(() => document.body.textContent.includes("读取删前章节"), "Real automatic execution events were dropped or untranslated");
  structureWork.status = "blocked"; structureWork.error = "检查连接中断"; structureWork.resumeMode = "continueExecution";
  structureWork.steps[0].status = "failed"; structureWork.steps[0].attempts[0].status = "failed"; structureWork.steps[0].attempts[0].error = "检查连接中断";
  listener({ method: "structure/error", params: { projectId: projects[0].id, changeId: "change-ui", taskId: "task-auto", message: "检查连接中断" } });
  await until(() => button("重新读取检查记录"), "Automatic failure did not provide read-only record recovery");
  const readsBefore = structureReads; button("重新读取检查记录").click(); await until(() => structureReads > readsBefore, "Recovery did not reread original Work");
  check(modelCalls === 0 && calls.length === 5 && composer.value === "未发送的构思要保留", "Record recovery repeated deletion/model request or overwrote composer");
  snapshots.push({ case: phase, pendingText, oneSystemTurn: true, multipleFailuresRemainRecoverable: true, liveReadLabel: "读取删前章节", noAuthorImpersonation: true, retainedComposerAndCaret: true, recovery: "重新读取检查记录", noAutomaticStoryWrite: true });
  phase = "completed check continues in ordinary conversation";
  structureWork = null;
  listener({ method: "agent/structure-started", params: { projectId: projects[0].id, taskId: "task-completed-check", threadId: "thread-completed-check", operation: "assessChapterStructure" } });
  listener({ method: "turn/started", params: { threadId: "thread-completed-check", turn: { id: "turn-completed-check", status: "inProgress" } } });
  listener({ method: "item/completed", params: { threadId: "thread-completed-check", turnId: "turn-completed-check", item: { id: "report", type: "agentMessage", phase: "final_answer", text: "删除后第二章的钥匙来源缺少交代。可补回忆或调整进入方式，由你决定。" } } });
  listener({ method: "turn/completed", params: { threadId: "thread-completed-check", turn: { id: "turn-completed-check", status: "completed" } } });
  await until(() => !button("发送创作指令")?.disabled, "Completed check did not allow author follow-up");
  await edit(composer, "先不改，讨论补回忆的方案"); button("发送创作指令").click();
  await until(() => followup !== null, "Ordinary follow-up was not sent");
  check(followup.projectId === projects[0].id && followup.prompt === "先不改，讨论补回忆的方案"
    && followup.repairSource === null && followup.focus.chapterStructureTaskId === "task-completed-check"
    && followup.focus.view === "chapters" && calls.length === 5 && saves === 1, "Follow-up lost exact report or changed story");
  snapshots.push({ case: phase, report: "第二章的钥匙来源缺少交代", authorInput: followup.prompt, exactReportSource: true,
    existingComposer: true, newButtons: false, storyUnchanged: true });
  phase = "late prepare project and API incarnation"; root.unmount(); await tick();
  let control, applied = 0; const stale = deferred();
  const oldApi = { runtime: { getCreativeWorkspace: async () => stale.promise } } as any;
  const freshApi = { runtime: { getCreativeWorkspace: async id => structuredClone(makeWorkspace(projects.find(project => project.id === id))) } } as any;
  function OwnerHarness({ api, projectId }) { control = useChapterDeletion({ api, projectId, enabled: true, isBlocked: () => false, onApplied: () => { applied++; } }); return <p data-delete-owner>{control.state?.title ?? "没有删除请求"}</p>; }
  const ownerRoot = createRoot(document.getElementById("root")); ownerRoot.render(<React.StrictMode><OwnerHarness api={oldApi} projectId={projects[0].id} /></React.StrictMode>); await tick(); control.open("chapter-3"); await tick();
  check(control.state?.busy, "StrictMode replay prevented first prepare");
  ownerRoot.render(<React.StrictMode><OwnerHarness api={freshApi} projectId={projects[1].id} /></React.StrictMode>); await tick(); ownerRoot.render(<React.StrictMode><OwnerHarness api={freshApi} projectId={projects[0].id} /></React.StrictMode>); await tick();
  check(!control.state, "Old project attempt remained visible"); control.open("chapter-4"); await until(() => control.state?.phase === "confirm", "Old pending read blocked new owner"); const freshKey = control.state.input.idempotencyKey;
  stale.resolve(structuredClone(makeWorkspace(projects[0]))); await tick(); await tick(); check(control.state?.title === "待删除第4章" && control.state.input.idempotencyKey === freshKey && applied === 0 && calls.length === 5, "Late A/API prepare overwrote new owner"); control.close();
  snapshots.push({ case: phase, staleIgnored: true, strictModeReadable: true, pendingOldOwnerDoesNotBlockNew: true, zeroApplied: applied === 0 });
  window.testResult = { passed: true, snapshots, requests: calls.length, saves, modelCalls };
} catch (error) { window.testResult = { passed: false, error: `${phase}: ${error.message}; ${renderErrors.join("; ")}`, dom: document.body.innerHTML.slice(-26000), snapshots, requests: calls.length, saves, modelCalls }; } })();
