import "../../../src/styles.css";
import React from "react";
import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import { clearAuthor, saveAuthor } from "../helpers/author-session";
import { RevisionOutcomeDetails } from "../../../src/RevisionOutcomeDetails";
import type { NovelCodexApi, RevisionOutcome, RevisionOutcomeDecision, RevisionOutcomeDecisionRequest, RevisionPredecessor } from "../../../src/types";

const root = createRoot(document.getElementById("root")!);
const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (value: unknown, message: string) => { if (!value) throw new Error(message); };
const equal = (actual: unknown, expected: unknown, message: string) => check(JSON.stringify(actual) === JSON.stringify(expected), message);
const wait = async (predicate: () => unknown, message: string) => {
  for (let n = 0; n < 150 && !predicate(); n++) await tick();
  check(predicate(), message);
};
const button = (label: string) => [...document.querySelectorAll<HTMLButtonElement>("button")].find(node => node.textContent === label || node.getAttribute("aria-label") === label);
const click = async (label: string) => { const node = button(label); check(node && !node.disabled, `Unavailable: ${label}`); node!.click(); await tick(); };
const input = async (label: string, value: string) => {
  const node = document.querySelector<HTMLTextAreaElement>(`textarea[aria-label="${label}"]`)!;
  check(node && !node.disabled, `Unavailable input: ${label}`);
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!.call(node, value);
  node.dispatchEvent(new Event("input", { bubbles: true })); await tick();
};
function deferred<T>() {
  let resolve!: (value: T) => void, reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
const unexpectedCalls: string[] = [];
const trap = (target: object, path: string): any => new Proxy(target, { get: (value, property) => {
  if (Object.hasOwn(value, property)) return Reflect.get(value, property);
  return () => { unexpectedCalls.push(`${path}.${String(property)}`); throw new Error(`Unexpected API: ${path}.${String(property)}`); };
} });
const writer = `task-${"a".repeat(32)}`, otherWriter = `task-${"b".repeat(32)}`;
const criteria = [{ id: "keep_letter", criterion: "藏好密函", groupIds: ["letter"] }];
const report = (state: RevisionOutcome["state"] = "current", decisions: RevisionOutcomeDecision[] = []): RevisionOutcome => ({
  revisionId: "assessment-2", authority: "candidateAnalysis", state, decisions,
  assessment: { writerTaskId: writer, chapterId: "chapter-2", originalRevisionId: "original-2", draftRevisionId: "draft-2", knowledgeBatchId: "batch-2",
    outcomes: [{ id: "keep_letter", status: "supported", reason: "正文中保留了密函。", evidence: [{ groupId: "letter", quote: "他藏好密函。", start: 0, end: 6 }] }] },
});
const decision = (choice: RevisionOutcomeDecision["choice"] = "acceptRevision", id = "decision-1"): RevisionOutcomeDecision => ({
  id, writerTaskId: writer, assessmentRevisionId: "assessment-2", draftRevisionId: "draft-2", knowledgeBatchId: "batch-2", choice, note: "", createdAt: 100,
});
const reads: ReturnType<typeof deferred<RevisionOutcome | null>>[] = [];
const writes: { args: [string, string, RevisionOutcomeDecisionRequest]; response: ReturnType<typeof deferred<RevisionOutcomeDecision>> }[] = [];
const preparations: RevisionPredecessor[] = [];
const api = { runtime: trap({
  getRevisionOutcome: () => { const read = deferred<RevisionOutcome | null>(); reads.push(read); return read.promise; },
  decideRevisionOutcome: (...args: [string, string, RevisionOutcomeDecisionRequest]) => {
    const response = deferred<RevisionOutcomeDecision>(); writes.push({ args, response }); return response.promise;
  },
}, "runtime") } as NovelCodexApi;
const text = () => document.querySelector(".revision-outcome-details")?.textContent ?? "";
const snapshots: Record<string, unknown> = {};
const snapshot = () => {
  const panel = document.querySelector(".revision-outcome-decision")!;
  return { statuses: [...panel.querySelectorAll('[role="status"]')].map(node => node.textContent),
    alerts: [...panel.querySelectorAll('[role="alert"]')].map(node => node.textContent),
    buttons: [...panel.querySelectorAll<HTMLButtonElement>("button")].map(node => ({ text: node.textContent, disabled: node.disabled })) };
};
function render(projectId = "project-a", version = "1:2", disabled = false, writerTaskId = writer) {
  flushSync(() => root.render(<RevisionOutcomeDetails api={api} projectId={projectId} writerTaskId={writerTaskId} chapterId="chapter-2"
    desiredOutcomes={criteria} version={version} disabled={disabled} onPrepareContinuation={value => preparations.push(value)} />));
}
async function load(value: RevisionOutcome | null, action = () => button("重新读取修改结果")!.click()) {
  const n = reads.length; action(); await wait(() => reads.length === n + 1, "Report read missing");
  reads[n].resolve(value); await tick(); await tick();
}
const reply = (write: typeof writes[number]) => ({ ...decision(write.args[2].choice, write.args[2].decisionId), note: write.args[2].note });

void (async () => {
  render(); await tick(); equal([reads.length, writes.length], [0, 0], "Collapsed report caused calls");
  const oldAccepted = { ...decision(), assessmentRevisionId: "assessment-1" };
  await load(report("current", [oldAccepted]), () => document.querySelector<HTMLElement>(".revision-outcome-details summary")!.click());
  check(!button("准备后继创作"), "Historical decision attached to a newer assessment");
  for (const field of ["draftRevisionId", "knowledgeBatchId", "writerTaskId"] as const) {
    await load(report("current", [{ ...decision(), [field]: "other-source" }]));
    check(!button("准备后继创作"), `Accepted decision with foreign ${field} offered continuation`);
  }
  await load(report("current", [decision("defer", "latest-defer"), decision()]));
  check(!button("准备后继创作"), "Older accept superseded the latest defer");
  snapshots.latestDefer = snapshot();
  snapshots.styles = {
    label: getComputedStyle(document.querySelector(".revision-outcome-decision label")!).display,
    textareaResize: getComputedStyle(document.querySelector(".revision-outcome-decision textarea")!).resize,
    actions: getComputedStyle(document.querySelector(".revision-decision-actions")!).display,
    actionsWrap: getComputedStyle(document.querySelector(".revision-decision-actions")!).flexWrap,
    inheritedButtons: [...document.querySelectorAll(".revision-outcome-decision button")].every(node => node.classList.contains("secondary-button")),
  };
  const fullHistory = [decision("repairDraft", "latest-repair"), ...Array.from({ length: 19 }, (_, index) => decision("acceptRevision", `older-${index}`))];
  await load(report("current", fullHistory));
  check(!button("准备后继创作") && !document.querySelector('[aria-label="作者决策历史"]'), "Collapsed history authorized old acceptance or rendered entries eagerly");
  const historyReads = reads.length;
  document.querySelector<HTMLElement>(".revision-decision-history summary")!.click(); await tick();
  const historyRows = [...document.querySelectorAll('[aria-label="作者决策历史"] > li')];
  check(historyRows.length === 20 && historyRows[0].textContent!.includes("latest-repair") && historyRows.at(-1)!.textContent!.includes("older-18"), "History cap or newest-first order changed");
  check(reads.length === historyReads && !button("准备后继创作"), "Read-only history issued API calls or authorized a run");
  snapshots.history = { summary: document.querySelector(".revision-decision-history summary")!.textContent,
    count: historyRows.length, newest: historyRows[0].querySelector("strong")!.textContent, oldest: historyRows.at(-1)!.querySelector("strong")!.textContent };
  await click("暂缓"); const historyWrite = writes.at(-1)!;
  historyWrite.response.resolve(reply(historyWrite)); await tick();
  check(document.querySelectorAll('[aria-label="作者决策历史"] > li').length === 20 && !text().includes("older-18"), "Local saved decision did not cap merged history");
  await load(report("current", Array.from({ length: 21 }, (_, index) => decision("defer", `overflow-${index}`))));
  check(button("作为后继创作依据")!.disabled && !document.querySelector(".revision-decision-history"), "Oversized history did not fail closed");
  await load(report());
  await input("修改结果决策备注", "🔒".repeat(1001));
  check(button("作为后继创作依据")!.disabled, "Unicode character note cap not enforced");
  await input("修改结果决策备注", "🔒".repeat(1000));
  const priorWrites = writes.length;
  button("作为后继创作依据")!.click(); button("作为后继创作依据")!.click(); await tick();
  equal(writes.length, priorWrites + 1, "Double click produced two decisions");
  const first = writes.at(-1)!, request = first.args[2];
  equal(first.args.slice(0, 2), ["project-a", writer], "Decision did not bind Author project and writer");
  check(/^[A-Za-z0-9-]{1,128}$/.test(request.decisionId), "Invalid decision identity");
  equal(Object.keys(request).sort(), ["choice", "decisionId", "expectedAssessmentRevisionId", "expectedDecisionId", "note"], "Decision request fields changed");
  equal([request.expectedAssessmentRevisionId, request.expectedDecisionId, request.choice, [...request.note].length], ["assessment-2", null, "acceptRevision", 1000], "Source CAS or note changed");
  snapshots.busy = snapshot(); first.response.resolve(reply(first));
  await wait(() => text().includes("作者决策已保存"), "Decision receipt missing");
  check(button("准备后继创作") && preparations.length === 0, "Decision automatically prepared or ran continuation");
  await load(report("current", [decision()])); snapshots.accepted = snapshot();
  await click("准备后继创作"); reads.at(-1)!.resolve(report("stale", [decision()])); await tick();
  check(preparations.length === 0 && text().includes("最新作者决策已变化"), "Stale preparation escaped");
  check(button("准备后继创作")!.disabled, "Stale source did not lock preparation"); snapshots.stalePreparation = snapshot();

  await load(report("current", [decision("defer", "latest-defer")])); await input("修改结果决策备注", "补足守卫场景");
  await click("还需改稿"); const conflict = writes.at(-1)!;
  equal(conflict.args[2].expectedDecisionId, "latest-defer", "Latest decision CAS lost");
  conflict.response.reject(new Error("TASK_INPUT_STALE：决策已变化")); await tick(); snapshots.conflict = snapshot();
  await click("重试保存原决定"); const replay = writes.at(-1)!;
  equal(replay.args, conflict.args, "Retry changed id or request after uncertain transport"); replay.response.resolve(reply(replay)); await tick();
  check(!button("准备后继创作"), "Repair choice continued automatically");
  await click("暂缓"); const defer = writes.at(-1)!;
  equal(defer.args[2].expectedDecisionId, replay.args[2].decisionId, "Second decision ignored successful local CAS");
  defer.response.resolve(reply(defer)); await tick(); check(!button("准备后继创作"), "Defer offered continuation");

  await click("作为后继创作依据"); const lateSuccess = writes.at(-1)!;
  await load(report(), () => render("project-b", "1:2")); lateSuccess.response.resolve(reply(lateSuccess)); await tick();
  check(!text().includes("作者决策已保存") && !button("准备后继创作"), "Late prior-project success leaked");
  await click("暂缓"); const lateError = writes.at(-1)!;
  await load(report(), () => render("project-b", "2:3")); lateError.response.reject(new Error("LATE_VERSION_ERROR")); await tick();
  check(!text().includes("LATE_VERSION_ERROR") && !button("重试保存原决定"), "Late old-version error leaked");
  await click("暂缓"); const lateWriter = writes.at(-1)!;
  const foreign = report(); foreign.assessment.writerTaskId = otherWriter;
  await load(foreign, () => render("project-b", "2:3", false, otherWriter)); lateWriter.response.resolve(reply(lateWriter)); await tick();
  check(!text().includes("作者决策已保存"), "Late previous-writer receipt leaked");
  await load(report("stale"), () => render("project-b", "3:4"));
  check(button("作为后继创作依据")!.disabled && !button("准备后继创作"), "Stale assessment allowed mutation");
  snapshots.staleReport = snapshot();
  await load(null); check(!document.querySelector(".revision-outcome-decision"), "Missing report offered a decision");
  await load(report("current", [decision()])); await click("准备后继创作");
  const latePreparation = reads.at(-1)!; render("project-b", "3:4", true);
  latePreparation.resolve(report("current", [decision()])); await tick(); check(preparations.length === 0, "Disabled late preparation fired callback");
  render("project-b", "3:4"); await click("准备后继创作"); reads.at(-1)!.resolve(report("current", [decision()])); await tick();
  equal(preparations, [{ writerTaskId: writer, assessmentRevisionId: "assessment-2", decisionId: "decision-1" }], "Explicit preparation changed predecessor");
  await click("暂缓"); const unmounted = writes.at(-1)!; flushSync(() => root.render(null)); unmounted.response.resolve(reply(unmounted)); await tick();

  // Mount the actual App, WorkTaskPanel and existing generation dialog. All runtime calls stay local.
  const project = { id: "project-a", title: "作者后继测试", logline: "", updated_at: "" };
  const chapter = { id: "chapter-2", number: 2, title: "密函", passages: [] };
  const edition = { project, canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] },
    narrativeIr: { version: "1.0", scenes: [], anchors: [] }, novelEdition: { chapters: [chapter] } };
  const source = { revision: 7, headSnapshotId: "snapshot-7", sourceDigest: "source-7" };
  const workspace = { project, ...source, index: { status: "ready" },
    artifacts: Object.fromEntries(["brief", "outline", "plan"].map(kind => [kind, { candidate: null, accepted: null, working: null, workingSelection: null, locked: false }])),
    chapters: [{ ...chapter, candidate: { id: "draft-2", title: chapter.title, version: 1, body: "他藏好密函。", sceneGroups: [{ id: "letter", body: "他藏好密函。", locked: false }] }, accepted: null, working: null, workingSelection: null }],
    tasks: [], commits: [], modeling: [], reviews: [], factProposals: [], planningReviews: [], workingBaselines: [], continuationIntents: [], chapterPlanBindings: [] };
  const intent = "藏好密函，按原作者目标继续后续章节。";
  const work = { id: "work-1", projectId: project.id, threadId: "thread-1", goal: intent, kind: "conversation", status: "completed", version: 2,
    deliveryStatus: "ready_for_review", deliveryReport: { evidence: [], pendingActionIds: [], unsettledAttemptIds: [] }, error: null, resumeBlockReason: null, resumeMode: null,
    steps: [{ id: "step-1", operation: "generateChapter", status: "completed", target: { chapterId: chapter.id, desiredOutcomes: criteria },
      attempts: [{ id: writer, status: "completed", threadId: "thread-1", turnId: "writer-turn", startedAt: 100, completedAt: 101, attemptNumber: 1, error: null }], changes: [], readingProgress: null, sourceTaskId: null }] };
  let currentReport = report("current", [decision()]);
  let delayedRead: ReturnType<typeof deferred<RevisionOutcome | null>> | null = null;
  const generations: unknown[][] = [];
  let failDiscussion = false;
  let agentEvent: ((event: unknown) => void) | null = null;
  const runtime = trap({ listChapterStructureChanges: async (projectId: string) => ({ projectId, items: [], nextCursor: null }), status: async () => ({ state: "online", mode: "local", error: null }), listProjects: async () => ({ projects: [project] }),
    getWorkCatalog: async () => ({ revision: 0, volumes: [], placements: [] }),
    getState: async () => edition, getCreativeWorkspace: async () => structuredClone(workspace), listGenerationRuns: async () => ({ runs: [] }),
    listPlanningImpacts: async () => ({ data: [], nextCursor: null, counts: { total: 0, open: 0, retained: 0, lockedConflict: 0 } }),
    listWorkTasks: async () => ({ tasks: [work], activeCount: 0, nextCursor: null }),
    getTaskFinalReply: async () => null,
    getRevisionOutcome: async () => delayedRead ? delayedRead.promise : structuredClone(currentReport),
    getWriterPreview: async (projectId, projectRevision, readingChapter, sceneOrder, povCharacterId, storyTime) => ({ purpose: "writer", taskId: null,
      target: { projectId, projectRevision, position: { readingChapter, sceneOrder, povCharacterId, storyTime } }, plannedEffects: [], relevantText: [],
      coverage: { complete: false, missingScope: [], truncationReasons: [], scannedScope: { factsIncluded: 0 } }, budgetsUsed: { byteSize: 0, byteLimit: 60000 } }),
    ...Object.fromEntries(["Search", "Entity", "Relation", "Knowledge", "SnapshotKnowledge", "Fact"].map(kind => [`get${kind}Audit`, async () => ({ job: null })])),
  }, "app.runtime");
  const agent = trap({ onEvent: (listener: (event: unknown) => void) => { agentEvent = listener; return () => { agentEvent = null; }; }, onUserInputRequest: () => () => {}, status: async () => ({ configured: true, state: "idle", mode: "novel-agent-engine", error: null }),
    listConnections: async () => ({ connections: [] }), recoveryReport: async () => [], listThreads: async () => ({ threads: [], nextCursor: null }),
    setWorkspaceContext: async () => ({}), listProposedActions: async () => ({ actions: [] }),
    sendPrompt: async (_projectId: string, prompt: string) => {
      agentEvent?.({ method: "turn/started", params: { threadId: "ordinary-thread", turn: { id: "ordinary-turn" } } });
      if (failDiscussion) {
        agentEvent?.({ method: "turn/completed", params: { threadId: "ordinary-thread", turn: { id: "ordinary-turn", status: "failed", error: { message: "本次讨论失败" } } } });
        throw new Error("本次讨论失败");
      }
      agentEvent?.({ method: "turn/completed", params: { threadId: "ordinary-thread", turn: { id: "ordinary-turn", status: "completed" } } });
      return { message: "已核对后续目标，尚未创建新章节。", proposedActions: [], workflow: { ...source }, candidateResults: [] };
    },
    generateWork: async (...args: unknown[]) => {
      generations.push(args);
      const turnId = `local-turn-${generations.length}`;
      agentEvent?.({ method: "turn/started", params: { threadId: "decision-thread", turn: { id: turnId } } });
      agentEvent?.({ method: "turn/completed", params: { threadId: "decision-thread", turn: { id: turnId, status: "completed" } } });
      return { id: "run-1", status: "completed", revisionPredecessor: args[6] };
    },
  }, "app.agent");
  window.novelCodex = { app: { onCloseRequested: () => () => {} }, runtime, agent } as NovelCodexApi;
  saveAuthor({ name: "测试作者", email: "author@example.test", passwordSalt: "local-salt", passwordHash: "local-hash" });
  localStorage.setItem("novel-codex-active-project", project.id);
  flushSync(() => root.render(<App />));
  await wait(() => document.querySelector<HTMLInputElement>('[aria-label="章节标题"]')?.value === chapter.title, "App failed to load the exact chapter editor");
  await wait(() => document.querySelector(".revision-outcome-details"), "App work report missing");
  document.querySelector<HTMLElement>(".work-task-execution > summary")!.click();
  document.querySelector<HTMLElement>(".revision-outcome-details summary")!.click();
  await wait(() => button("准备后继创作"), "App accepted decision entry missing");
  const prepareDialog = async () => { await click("准备后继创作"); await wait(() => document.querySelector(".generation-permit-dialog"), "App did not route preparation to existing dialog"); };
  await prepareDialog(); equal(generations.length, 0, "Preparation started generation");
  equal(document.querySelector<HTMLTextAreaElement>('[aria-label="本轮创作目标"]')!.value, intent, "Original author intent lost");
  const dialog = document.querySelector(".generation-permit-dialog")!;
  snapshots.confirmation = { title: dialog.querySelector("h2")!.textContent, intent,
    limits: [...dialog.querySelectorAll(".generation-permit-limits > div")].map(node => node.textContent),
    scope: [...dialog.querySelectorAll<HTMLInputElement>('input[type="radio"]')].map(node => ({ value: node.value, checked: node.checked })),
    source: dialog.querySelector(".project-dialog-description")!.textContent };
  await click("返回编辑"); equal(generations.length, 0, "Closing confirmation started a run");
  await prepareDialog(); currentReport = report("current", [oldAccepted]);
  await click("确认开始创作"); await wait(() => dialog !== document.querySelector(".generation-permit-dialog") || document.body.textContent!.includes("最新作者决策已变化"), "Changed decision was not rejected");
  equal(generations.length, 0, "Historical acceptance passed final confirmation");
  await click("返回编辑"); currentReport = report("current", [decision()]); await prepareDialog();
  delayedRead = deferred<RevisionOutcome | null>(); await click("确认开始创作");
  await click("返回编辑"); delayedRead.resolve(report("current", [decision()])); await tick(); delayedRead = null;
  equal(generations.length, 0, "Late confirmation after close started generation");
  await prepareDialog(); document.querySelector<HTMLInputElement>('input[name="generation-target-chapters"][value="2"]')!.click(); await tick();
  await input("本轮创作目标", "作者在确认框调整后的目标。");
  await click("确认开始创作"); await wait(() => generations.length === 1, "Explicit confirmation did not generate");
  equal(generations[0], [project.id, "作者在确认框调整后的目标。", 2, 7, "snapshot-7", "source-7", preparations[0]], "App did not append exact predecessor with original bounds");
  await wait(() => !document.querySelector(".generation-permit-dialog"), "Generation confirmation did not close");
  // A normal new run passes a literal null, with the same range and budget dialog.
  const briefNav = [...document.querySelectorAll<HTMLButtonElement>(".sidebar button")].find(node => node.textContent?.includes("作品概况"));
  check(briefNav, "Brief navigation missing"); briefNav!.click();
  await wait(() => document.querySelector(".planning-workspace-header"), "Brief view missing");
  check(!document.querySelector('[aria-label="规划生成意图"]') && !button("续写后续章节"), "Brief retained an independent generation intent entry");
  await input("创作指令", "普通创作目标。"); await click("发送创作指令");
  await wait(() => button("用这次对话续写新章节"), "Conversation did not provide a source-bound continuation");
  check(button("用这次对话续写新章节")!.closest(".conversation-panel") && !document.querySelector(".creative-workspace-view .conversation-continuation-entry"), "Continuation was detached from its current reply");
  failDiscussion = true;
  await input("创作指令", "普通创作目标。"); await click("发送创作指令");
  await wait(() => document.body.textContent?.includes("本次讨论失败") && !document.querySelector('[aria-label="停止当前执行"]'), "Repeated discussion failure did not settle");
  check(!button("用这次对话续写新章节"), "Failed repeat of identical author input reused an old continuation receipt");
  failDiscussion = false;
  await input("创作指令", "普通创作目标。"); await click("发送创作指令");
  await wait(() => button("用这次对话续写新章节"), "Fresh completed discussion did not restore its own continuation");
  await click("用这次对话续写新章节");
  await click("确认开始创作"); await wait(() => generations.length === 2, "Ordinary run did not start");
  equal(generations[1], [project.id, "普通创作目标。", 1, 7, "snapshot-7", "source-7", null], "Ordinary run inherited predecessor");
  clearAuthor(); root.unmount(); check(unexpectedCalls.length === 0, `Unexpected calls: ${unexpectedCalls.join(", ")}`);
  window["testResult"] = { passed: true, snapshots, unexpectedCalls };
})().catch(error => { window["testResult"] = { passed: false, error: `${error.stack}\nDOM: ${document.body.textContent}` }; });
