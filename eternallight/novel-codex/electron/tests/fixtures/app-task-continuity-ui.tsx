import { novelFixtureMedia } from "../helpers/novel-edition";
import { emptyCreationIssueSummary } from "../helpers/creation-issues";
import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import { saveAuthor } from "../helpers/author-session";

const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (value, message) => { if (!value) throw Error(message); };
const until = async (predicate, message) => { for (let i = 0; i < 150 && !predicate(); i++) await tick(); check(predicate(), message); };
window.addEventListener("error", event => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
saveAuthor({ name: "时间线作者", email: "fixture@example.invalid", passwordSalt: "fixture", passwordHash: "fixture" });
const projects = ["project-last-lamp", "project-b", "project-c"].map((id, i) => ({ id, title: `时间线作品${i + 1}`, logline: "", updated_at: "" }));
const chapter = { id: "chapter-one", number: 1, title: "已发布第一章", status: "accepted", summary: "", passages: [] };
const runtimeState = id => ({ project: projects.find(project => project.id === id), canon: { characters: [], locations: [], relationships: [], events: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1", scenes: [], anchors: [] }, novelEdition: { chapters: [chapter] } });
const revision = { id: "published", title: chapter.title, version: 1, body: "已发布的正文。", sceneGroups: [{ id: "scene-one", sceneId: null, body: "已发布的正文。", locked: false }], sourcePlanRefs: { localOnly: true, lineageStatus: "current" } };
const workspace = id => ({ project: runtimeState(id).project, revision: 1, headSnapshotId: "head", sourceDigest: "a".repeat(64), index: { status: "ready" },
  artifacts: Object.fromEntries(["brief", "outline", "plan", "identity"].map(kind => [kind, { candidate: null, accepted: null, working: null, locked: false, workingSelection: null }])),
  chapters: [{ ...chapter, candidate: revision, accepted: revision, working: null, workingSelection: null }], tasks: [], factProposals: [], reviews: [], commits: [], planningReviews: [], impacts: [], workingBaselines: [], continuationIntents: [],
  modeling: [{ objectId: `draft-${chapter.id}`, sourceRevisionId: revision.id, status: "model_pending", batchId: null }], chapterPlanBindings: [] });
const attempt = id => ({ id: `attempt-${id}`, threadId: `thread-${id}`, turnId: `turn-${id}`, status: "completed", startedAt: 100 + id, completedAt: 101 + id, attemptNumber: 1, error: null });
const task = id => ({ id: `work-${id}`, projectId: projects[0].id, threadId: "history-thread", goal: `作者的第${id}轮创作指令`, kind: "direct", status: "completed", version: 1, error: null, resumeMode: null, resumeBlockReason: null,
  deliveryStatus: "ready_for_review", deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], evidence: [{ stepId: `step-${id}`, attemptId: `attempt-${id}`, kind: "draft", state: "recorded", referenceId: revision.id }], pendingActionIds: [], unsettledAttemptIds: [] },
  steps: [{ id: `step-${id}`, operation: "generateChapter", currentAttemptId: `attempt-${id}`, status: "completed", target: { chapterId: chapter.id }, attempts: [attempt(id)], changes: [], outputRefs: [], readingProgress: null }] });
for (const project of projects) localStorage.setItem(`novel-codex:selected-conversation:${JSON.stringify(["fixture@example.invalid", project.id])}`, "history-thread");
const tasks = [4, 3, 2, 1].map(task), reads = [], snapshots = [];
let modelCalls = 0, controlCalls = 0, holdB = false, releaseB, failC = true, createdInB = false, notify;
const api = { multimedia: novelFixtureMedia, app: { onCloseRequested: () => () => {} }, runtime: { listChapterStructureChanges: async (projectId: string) => ({ projectId, items: [], nextCursor: null }),
  getWorkCatalog: async () => ({ revision: 0, volumes: [], placements: [] }),
  status: async () => ({ state: "online", mode: "local", error: null }), listProjects: async () => ({ projects }), getState: async id => runtimeState(id),
  getCreativeWorkspace: async id => structuredClone(workspace(id)), getWorkTask: async (id, taskId) => ({ ...task(Number(taskId.split("-").at(-1))), projectId: id }), listPlanningImpacts: async () => ({ data: [], nextCursor: null, counts: { open: 0 } }), listGenerationRuns: async () => ({ runs: [] }),
  listWorkTasks: async (id, cursor) => {
    reads.push({ id, cursor });
    if (id === projects[1].id && holdB) { holdB = false; await new Promise(resolve => { releaseB = resolve; }); }
    if (id === projects[2].id && failC) throw Error("对话记录读取失败夹具");
    return { tasks: id === projects[0].id ? cursor ? [task(0)] : structuredClone(tasks) : id === projects[1].id && createdInB ? [{ ...task(5), projectId: id }] : [], activeCount: 0, nextCursor: id === projects[0].id && !cursor ? "earlier" : null };
  },
  getTaskFinalReply: async (id, taskId) => ({ projectId: id, taskId, state: "recorded", binding: { threadId: taskId.replace("attempt", "thread"), turnId: taskId.replace("attempt", "turn"), itemId: "answer", intentRevision: 0, truncated: false,
    text: `${taskId}真实保存的测试回复\n\n${"这是该轮保存的结果，仅供夹具检验连续性。\n\n".repeat(taskId === "attempt-5" ? 30 : 12)}` }, sourceDigest: "digest", receiptDigest: "receipt", recordedAt: 200 }),
  getWriterPreview: async () => ({ target: { position: {} }, plannedEffects: [], relevantText: [], coverage: { complete: true, missingScope: [], truncationReasons: [] } }),
  getIdentityCatalog: async () => ({ entries: [], ambiguousNames: [], identityRevisionId: null, identityDecisions: [], truncated: false, ambiguityScanComplete: true }),
  ...Object.fromEntries(["Search", "Entity", "Relation", "Knowledge", "SnapshotKnowledge", "Fact"].map(kind => [`get${kind}Audit`, async () => ({ job: null })])),
}, agent: {
  status: async () => ({ configured: true, state: "ready", mode: "novel-agent-engine", provider: "fixture", model: "fixture", error: null }),
  listConnections: async () => ({ connections: [] }), recoveryReport: async () => [], listProposedActions: async () => ({ actions: [] }), setWorkspaceContext: async () => {},
  readConversation: async (projectId, id) => ({ thread: { id, projectId, name: "作品讨论", status: "idle" }, turns: [] }),
  listExecutions: async () => [], readMessage: async () => { throw Error("message receipt not found"); }, beginInterlude: async () => { throw Error("unexpected interlude"); }, returnInterlude: async () => { throw Error("unexpected interlude return"); }, listMessages: async () => [], listThreads: async id => { if (id === projects[2].id) throw Error("threads unavailable"); return { threads: [], nextCursor: null }; },
  onEvent: listener => { notify = listener; return () => {}; }, onUserInputRequest: () => () => {}, sendPrompt: async () => { modelCalls++; throw Error("Unexpected model call"); },
  resumeWorkTask: async () => { controlCalls++; }, controlWorkTask: async () => { controlCalls++; },
} };
window.novelCodex = api;
createRoot(document.getElementById("root")).render(<App />);
const rail = () => document.querySelector('.conversation-rail-scroll') ?? document.querySelector('.conversation-reading-area');
const navigate = async label => {
  if (label === "作品目录") {
    const row = document.querySelector<HTMLButtonElement>(`[data-directory-chapter-id="${chapter.id}"]`);
    check(row, "Chapter navigation entry is missing");
    const tree = row.closest("details");
    if (tree && !tree.open) tree.querySelector("summary").click();
    row.click();
    await until(() => document.querySelector('[aria-label="章节标题"]'), "Chapter navigation did not open the editor");
  } else [...document.querySelectorAll('.nav-item')].find(item => item.textContent.includes(label)).click();
  await tick(); await tick();
};
const selectProject = async index => { document.querySelector('.project-switcher').click(); await tick(); [...document.querySelectorAll('.project-menu-item')].find(item => item.querySelector('strong')?.textContent === projects[index].title).click(); await tick(); await tick(); };
const rows = () => document.querySelectorAll('.conversation-rail [data-work-task-id]');
const savedReply = () => document.querySelector('[data-work-task-id="work-4"] .saved-task-reply');
(async () => {
  try {
    await until(() => rows().length === 4 && rail().textContent.includes("attempt-4真实保存"), "initial saved history absent");
    check(!document.querySelector('.thread-history-panel') && !document.querySelector('.sidebar-task-section'), "loaded empty thread scaffolding remained");
    await navigate("作品目录"); await until(() => document.querySelector('[aria-label="章节标题"]'), "real workspace did not mount");
    check(!document.querySelector(".work-task-execution"), "Extra execution list returned");
    rail().dispatchEvent(new WheelEvent('wheel', { bubbles: true })); rail().scrollTop = 90; rail().dispatchEvent(new Event('scroll')); await tick();
    for (const label of ["角色", "近章规划", "作品目录"]) {
      await navigate(label); await until(() => rows().length === 4 && rail().textContent.includes("attempt-4真实保存"), `${label} lost saved history`);
      check(!!savedReply(), `${label} lost execution expansion`);
      check(Math.abs(rail().scrollTop - 90) < 3, `${label} lost reading position: ${rail().scrollTop}`);
      check(document.querySelectorAll('.conversation-rail [aria-label="创作对话"]').length === 1, `${label} mounted duplicate timeline`);
      snapshots.push({ view: label, records: rows().length, replyVisible: !!savedReply(), scroll: Math.round(rail().scrollTop) });
    }
    check(reads.filter(read => read.id === projects[0].id).length === 1, "navigation created another task reader");
    await navigate("角色"); [...rail().querySelectorAll('button')].find(item => item.textContent === "查看对应章节正文").click(); await tick(); await tick();
    check(document.querySelector('[aria-label="章节标题"]'), "read-only task navigation missed chapter");
    await navigate("角色"); [...rail().querySelectorAll('button')].find(item => item.textContent === "加载更早对话").click(); await until(() => rows().length === 5, "pagination failed");
    await navigate("角色"); check(rows().length === 5, "navigation discarded loaded earlier page");
    holdB = true; await selectProject(1); await until(() => releaseB, "B delayed read absent");
    check(rail().textContent.includes("正在读取对话与执行记录") && !rail().textContent.includes("作者的第"), "loading leaked prior project history");
    await selectProject(0); await until(() => rows().length === 4, "return project read absent"); releaseB(); await tick(); await tick();
    check(rows().length === 4 && !!savedReply(), "late foreign read cleared current history or expansion");
    await selectProject(2); await until(() => rail().textContent.includes("对话记录读取失败夹具"), "read failure was hidden as empty");
    await until(() => document.querySelector('.conversation-navigator-error'), "thread error collapsed as successful empty");
    failC = false; [...rail().querySelectorAll('button')].find(item => ["重试读取任务", "重新读取任务记录"].includes(item.textContent)).click(); await until(() => !rail().textContent.includes("对话记录读取失败夹具"), "readonly retry failed");
    check(rows().length === 0 && modelCalls === 0 && controlCalls === 0, "read-only page invoked write control/model or leaked history");
    await selectProject(1); await until(() => rows().length === 0 && !rail().textContent.includes("正在读取对话与执行记录"), "empty B project did not settle");
    await navigate("角色");
    createdInB = true;
    const beforeWakeReads = reads.filter(read => read.id === projects[1].id).length;
    notify({ method: "agent/work-updated", params: { projectId: projects[0].id, workTaskId: "work-5", threadId: "history-thread" } });
    notify({ method: "agent/work-updated", params: { projectId: projects[1].id, workTaskId: "work-5" } });
    await tick(); await tick();
    check(reads.filter(read => read.id === projects[1].id).length === beforeWakeReads && rows().length === 0, "foreign or incomplete wake refreshed current project");
    notify({ method: "agent/work-updated", params: { projectId: projects[1].id, workTaskId: "work-5", threadId: "history-thread" } });
    await until(() => rows().length === 1 && rail().textContent.includes("attempt-5真实保存"), "saved wake result required project navigation to become visible");
    check(modelCalls === 0 && controlCalls === 0, "wake display called a model or author control");
    await selectProject(0); await selectProject(1);
    await until(() => rows().length === 1 && rail().textContent.includes("attempt-5真实保存"), "first task was not displayed after an empty project");
    check(!document.querySelector(".work-task-execution"), "First task revived execution details");
    rail().dispatchEvent(new WheelEvent('wheel', { bubbles: true })); rail().scrollTop = 90; rail().dispatchEvent(new Event('scroll')); await tick();
    check(Math.abs(rail().scrollTop - 90) < 3 && rail().scrollHeight - rail().clientHeight - rail().scrollTop > 32, "Single-record fixture did not establish a reading position away from the bottom");
    await navigate("作品目录"); await until(() => document.querySelector('[data-work-task-id="work-5"] .saved-task-reply'), "first task after empty project lost its expansion");
    await until(() => rail().textContent.includes("attempt-5真实保存") && Math.abs(rail().scrollTop - 90) < 3, "first task after empty project lost its reading position after its reply loaded");
    snapshots.push({ view: "first record in new project", records: rows().length, replyVisible: true, scroll: Math.round(rail().scrollTop) });
    check(modelCalls === 0 && controlCalls === 0, "first record navigation executed a task");

    window.testResult = { passed: true, snapshots, modelCalls, controlCalls };
  } catch (error) { window.testResult = { passed: false, error: error.stack + "\nDOM:" + document.body.innerText }; }
})();
