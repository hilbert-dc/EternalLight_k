import "../../../src/styles.css";
import React from "react";
import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import { clearAuthor, saveAuthor } from "../../../src/author-session";
import { RevisionOutcomeDetails } from "../../../src/RevisionOutcomeDetails";
import type { NovelCodexApi, RevisionOutcome, RevisionOutcomeDecision, RevisionRepairPreparation } from "../../../src/types";

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
const writer = `task-${"a".repeat(32)}`;
const criteria = [{ id: "keep_letter", criterion: "藏好密函", groupIds: ["letter"] }];
const decision = (choice: RevisionOutcomeDecision["choice"] = "repairDraft", id = "repair-1"): RevisionOutcomeDecision => ({
  id, writerTaskId: writer, assessmentRevisionId: "assessment-2", draftRevisionId: "draft-2", knowledgeBatchId: "batch-2",
  choice, note: "建议：先核对守卫的动机；不要直接写作。", createdAt: 100,
});
const report = (state: RevisionOutcome["state"] = "current", decisions = [decision()]): RevisionOutcome => ({
  revisionId: "assessment-2", authority: "candidateAnalysis", state, decisions,
  assessment: { writerTaskId: writer, chapterId: "chapter-2", originalRevisionId: "original-1", draftRevisionId: "draft-2", knowledgeBatchId: "batch-2",
    outcomes: [{ id: "keep_letter", status: "supported", reason: "正文中保留了密函。", evidence: [{ groupId: "letter", quote: "他藏好密函。", start: 0, end: 6 }] }] },
});
const reads: ReturnType<typeof deferred<RevisionOutcome | null>>[] = [];
const preparations: RevisionRepairPreparation[] = [];
const api = { runtime: trap({ getRevisionOutcome: () => {
  const read = deferred<RevisionOutcome | null>(); reads.push(read); return read.promise;
} }, "panel.runtime") } as NovelCodexApi;
function render(projectId = "project-a", version = "1:2", disabled = false) {
  flushSync(() => root.render(<RevisionOutcomeDetails api={api} projectId={projectId} writerTaskId={writer} chapterId="chapter-2"
    desiredOutcomes={criteria} version={version} disabled={disabled} onPrepareRepair={value => preparations.push(value)} />));
}
async function load(value: RevisionOutcome | null, action = () => button("重新读取修改结果")!.click()) {
  const n = reads.length; action(); await wait(() => reads.length === n + 1, "Report read missing");
  reads[n].resolve(value); await tick(); await tick();
}
const snapshots: Record<string, unknown> = {};
void (async () => {
  render(); await tick(); equal(reads.length, 0, "Collapsed report fetched eagerly");
  await load(report(), () => document.querySelector<HTMLElement>(".revision-outcome-details summary")!.click());
  check(button("讨论下一次改稿") && preparations.length === 0, "Repair receipt automatically prepared or missing entry");
  snapshots.entry = { text: button("讨论下一次改稿")!.textContent, disabled: button("讨论下一次改稿")!.disabled };
  for (const foreign of [report("stale"), report("current", [decision("defer", "newer"), decision()]),
    report("current", [decision("acceptRevision", "newer"), decision()]),
    ...["writerTaskId", "assessmentRevisionId", "draftRevisionId", "knowledgeBatchId"].map(field => report("current", [{ ...decision(), [field]: "foreign" }]))]) {
    await load(foreign); check(!button("讨论下一次改稿"), "Old or foreign repair decision offered preparation");
  }
  await load(report()); render("project-a", "1:2", true);
  check(button("讨论下一次改稿")!.disabled, "Running/dirty disabled control remained enabled");
  render(); await click("讨论下一次改稿"); render("project-a", "1:2", true);
  reads.at(-1)!.resolve(report()); await tick(); equal(preparations.length, 0, "Disabled late preparation escaped");
  render(); await click("讨论下一次改稿"); reads.at(-1)!.resolve(report("stale")); await tick();
  check(preparations.length === 0 && button("讨论下一次改稿")!.disabled, "Stale preflight accepted");
  for (const field of ["draftRevisionId", "knowledgeBatchId"] as const) {
    await load(report()); await click("讨论下一次改稿");
    const changed = report(); changed.assessment[field] = "changed"; changed.decisions[0][field] = "changed";
    reads.at(-1)!.resolve(changed); await tick(); equal(preparations.length, 0, "Changed actual source escaped");
  }
  await load(report()); await click("讨论下一次改稿"); const late = reads.at(-1)!;
  await load(report(), () => render("project-b")); late.resolve(report()); await tick();
  equal(preparations.length, 0, "Prior project late preparation escaped");
  await click("讨论下一次改稿"); const closed = reads.at(-1)!;
  document.querySelector<HTMLElement>(".revision-outcome-details summary")!.click(); await tick();
  closed.resolve(report()); await tick(); equal(preparations.length, 0, "Closed report prepared a discussion");
  await load(report(), () => document.querySelector<HTMLElement>(".revision-outcome-details summary")!.click());
  await click("讨论下一次改稿"); reads.at(-1)!.resolve(report()); await tick();
  equal(preparations, [{ projectId: "project-b", source: { writerTaskId: writer, assessmentRevisionId: "assessment-2", decisionId: "repair-1" },
    chapterId: "chapter-2", draftRevisionId: "draft-2", knowledgeBatchId: "batch-2", note: decision().note }], "Preparation changed source identity");
  flushSync(() => root.render(null));

  // Exercise the entire actual App → workspace → task → details → decision chain.
  const project = { id: "project-a", title: "作者改稿讨论测试", logline: "", updated_at: "" };
  const otherProject = { ...project, id: "project-b", title: "另一个作品" };
  const chapter = { id: "chapter-2", number: 2, title: "密函", passages: [] };
  const edition = { project, canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] },
    narrativeIr: { version: "1.0", scenes: [], anchors: [] }, novelEdition: { chapters: [chapter] } };
  const workspace = { project, revision: 7, headSnapshotId: "snapshot-7", sourceDigest: "source-7", index: { status: "ready" },
    artifacts: Object.fromEntries(["brief", "outline", "plan"].map(kind => [kind, { candidate: null, accepted: null, working: null, workingSelection: null, locked: false }])),
    chapters: [{ ...chapter, candidate: { id: "draft-2", version: 1, body: "他藏好密函。", sceneGroups: [{ id: "letter", body: "他藏好密函。", locked: false }] }, accepted: null, working: null, workingSelection: null }],
    tasks: [], commits: [], modeling: [], reviews: [], factProposals: [], planningReviews: [], workingBaselines: [], continuationIntents: [], chapterPlanBindings: [] };
  const work = { id: "work-1", projectId: project.id, threadId: "thread-1", goal: "不可作为隐藏指令的原Writer目标", kind: "conversation", status: "completed", version: 2,
    deliveryStatus: "ready_for_review", deliveryReport: { evidence: [], pendingActionIds: [], unsettledAttemptIds: [] }, error: null, resumeBlockReason: null,
    steps: [{ id: "step-1", operation: "generateChapter", status: "completed", target: { chapterId: chapter.id, desiredOutcomes: criteria },
      attempts: [{ id: writer, status: "completed" }], changes: [], readingProgress: null, sourceTaskId: null }] };
  let currentReport = report();
  let delayedRead: ReturnType<typeof deferred<RevisionOutcome | null>> | null = null;
  let reportReads = 0;
  const sends: unknown[][] = [], steers: unknown[][] = [];
  let sendReply: ReturnType<typeof deferred<any>> | null = null;
  let agentEvent: ((event: unknown) => void) | null = null;
  const runtime = trap({ status: async () => ({ state: "online", mode: "local", error: null }), listProjects: async () => ({ projects: [project, otherProject] }),
    getState: async (id: string) => ({ ...edition, project: id === project.id ? project : otherProject }),
    getCreativeWorkspace: async (id: string) => structuredClone({ ...workspace, project: id === project.id ? project : otherProject }),
    listGenerationRuns: async () => ({ runs: [] }), listPlanningImpacts: async () => ({ data: [], nextCursor: null, counts: { total: 0, open: 0, retained: 0, lockedConflict: 0 } }),
    listWorkTasks: async () => ({ tasks: [work], activeCount: 0, nextCursor: null }),
    getRevisionOutcome: async () => { reportReads++; return delayedRead ? delayedRead.promise : structuredClone(currentReport); },
    getWriterPreview: async (projectId, projectRevision, readingChapter, sceneOrder, povCharacterId, storyTime) => ({ purpose: "writer", taskId: null,
      target: { projectId, projectRevision, position: { readingChapter, sceneOrder, povCharacterId, storyTime } }, plannedEffects: [], relevantText: [],
      coverage: { complete: false, missingScope: [], truncationReasons: [], scannedScope: { factsIncluded: 0 } }, budgetsUsed: { byteSize: 0, byteLimit: 60000 } }),
    ...Object.fromEntries(["Search", "Entity", "Relation", "Knowledge", "SnapshotKnowledge", "Fact"].map(kind => [`get${kind}Audit`, async () => ({ job: null })])),
  }, "app.runtime");
  const agent = trap({ onEvent: (listener: (event: unknown) => void) => { agentEvent = listener; return () => { agentEvent = null; }; }, onUserInputRequest: () => () => {},
    status: async () => ({ configured: true, state: "idle", mode: "novel-agent-engine", error: null }), listConnections: async () => ({ connections: [] }),
    recoveryReport: async () => [], listThreads: async () => ({ threads: [], nextCursor: null }), setModelingContext: async () => ({}), listProposedActions: async () => ({ actions: [] }),
    sendPrompt: (...args: unknown[]) => { sends.push(args); sendReply = deferred(); return sendReply.promise; },
    steer: async (...args: unknown[]) => { steers.push(args); return { steered: true }; },
  }, "app.agent");
  window.novelCodex = { app: { onCloseRequested: () => () => {} }, runtime, agent } as NovelCodexApi;
  saveAuthor({ name: "测试作者", email: "author@example.test", passwordSalt: "local-salt", passwordHash: "local-hash" });
  localStorage.setItem("novel-codex-active-project", project.id);
  flushSync(() => root.render(<App />)); await wait(() => button("编辑"), "App failed to load"); await click("编辑");
  await wait(() => document.querySelector(".revision-outcome-details"), "App report missing");
  document.querySelector<HTMLElement>(".work-task-panel summary")!.click();
  document.querySelector<HTMLElement>(".revision-outcome-details summary")!.click();
  await wait(() => button("讨论下一次改稿"), "App repair entry missing");
  const prepared = () => document.querySelector('[aria-label="已准备改稿讨论"]');
  const composer = () => document.querySelector<HTMLTextAreaElement>('[aria-label="创作指令"]')!;
  const prepare = async () => { await click("讨论下一次改稿"); await wait(prepared, "App did not prepare composer"); };
  const complete = async () => {
    agentEvent?.({ method: "turn/completed", params: { turn: { id: "local-turn", status: "completed" } } });
    sendReply!.resolve({ mode: "novel-agent-engine", message: "仅讨论改稿方向", projectId: project.id, proposedActions: [] });
    await tick(); await tick();
  };
  await prepare(); equal([sends.length, steers.length], [0, 0], "Entry started or steered a run");
  check(!document.querySelector('[role="dialog"]'), "Entry opened an execution/budget dialog");
  check(prepared()!.textContent!.includes(decision().note) && !composer().value.includes(decision().note)
    && !composer().value.includes(work.goal), "Note or old Writer goal became an implicit prompt");
  snapshots.prepared = { text: prepared()!.textContent, composer: composer().value, disabled: composer().disabled };
  await input("创作指令", "作者明确编辑后的讨论问题。");
  await click("取消改稿讨论来源"); check(!prepared(), "Cancel retained source");
  equal(composer().value, "作者明确编辑后的讨论问题。", "Cancel erased author edits");
  await click("发送创作指令"); equal(sends[0], [project.id, "作者明确编辑后的讨论问题。", null], "Ordinary send inherited repair source"); await complete();

  // Final preflight rejects stale decisions, assessment, draft and published batch.
  for (const change of ["state", "decision", "revisionId", "draftRevisionId", "knowledgeBatchId"] as const) {
    currentReport = report(); await prepare(); currentReport = report();
    if (change === "state") currentReport.state = "stale";
    else if (change === "decision") currentReport.decisions = [decision("defer", "new-decision"), decision()];
    else if (change === "revisionId") currentReport.revisionId = "new-assessment";
    else { currentReport.assessment[change] = "changed"; currentReport.decisions[0][change] = "changed"; }
    await click("发送创作指令"); equal(sends.length, 1, `Final ${change} check failed`);
    check(prepared()!.querySelector('[role="alert"]'), "Source rejection not visible");
    await click("取消改稿讨论来源");
  }
  currentReport = report(); await prepare();
  const beforeRead = reportReads;
  delayedRead = deferred(); button("发送创作指令")!.click(); button("发送创作指令")!.click(); await tick();
  equal(reportReads, beforeRead + 1, "Double send issued duplicate preflights");
  check(composer().disabled && button("发送创作指令")!.disabled, "Preflight left send editable/enabled");
  await click("取消改稿讨论来源"); delayedRead.resolve(report()); delayedRead = null; await tick();
  equal(sends.length, 1, "Cancelled late preflight sent a prompt");

  // A running task cannot consume this prepared source, even through Enter/steer.
  await prepare(); agentEvent?.({ method: "turn/started", params: { turn: { id: "other-turn" } } }); await tick();
  check(composer().disabled && button("追加创作指令")!.disabled && button("讨论下一次改稿")!.disabled, "Running preparation was steerable");
  composer().dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); await tick();
  equal([sends.length, steers.length], [1, 0], "Running Enter consumed repair source");
  agentEvent?.({ method: "turn/completed", params: { turn: { id: "other-turn", status: "completed" } } }); await tick();
  delayedRead = deferred(); await click("发送创作指令");
  agentEvent?.({ method: "turn/started", params: { turn: { id: "late-turn" } } }); await tick();
  delayedRead.resolve(report()); delayedRead = null; await tick(); equal(sends.length, 1, "Running late preflight sent");
  agentEvent?.({ method: "turn/completed", params: { turn: { id: "late-turn", status: "completed" } } }); await tick();
  await click("取消改稿讨论来源");

  await prepare(); delayedRead = deferred(); await click("发送创作指令");
  // Editing the actual chapter while the read is pending invalidates the send.
  const body = document.querySelector<HTMLTextAreaElement>('[aria-label="场景 1 正文"]')!;
  check(body, "Actual draft editor missing");
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!.call(body, "未保存的正文。");
  body.dispatchEvent(new Event("input", { bubbles: true })); await tick();
  check(composer().disabled && button("讨论下一次改稿")!.disabled, "Dirty source did not block controls");
  delayedRead.resolve(report()); delayedRead = null; await tick(); equal(sends.length, 1, "Dirty late preflight sent");
  await click("取消改稿讨论来源");
  // Remount with clean local fixtures; this also exercises unmount invalidation.
  flushSync(() => root.render(null)); flushSync(() => root.render(<App />));
  await wait(() => button("编辑"), "App remount failed"); await click("编辑");
  await wait(() => document.querySelector(".revision-outcome-details"), "Remount report missing");
  document.querySelector<HTMLElement>(".work-task-panel summary")!.click();
  document.querySelector<HTMLElement>(".revision-outcome-details summary")!.click();
  await wait(() => button("讨论下一次改稿"), "Remount repair missing");
  await prepare(); await input("创作指令", "只讨论守卫动机，先问我需要改哪一段。");
  const expectedSource = { writerTaskId: writer, assessmentRevisionId: "assessment-2", decisionId: "repair-1" };
  composer().dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await wait(() => sends.length === 2, "Editable Enter did not send");
  equal(sends[1], [project.id, "只讨论守卫动机，先问我需要改哪一段。", expectedSource], "App changed editable author prompt or source fields");
  check(!prepared(), "Successful send retained preparation"); await complete();
  await input("创作指令", "普通聊天。"); await click("发送创作指令");
  equal(sends[2], [project.id, "普通聊天。", null], "Next normal chat reused repair source"); await complete();

  await prepare(); await input("创作指令", "发送失败也必须保留来源。"); await click("发送创作指令");
  equal(sends[3], [project.id, "发送失败也必须保留来源。", expectedSource], "Failed attempt lost repair identity");
  sendReply!.reject(new Error("HOST_REPAIR_FAILURE"));
  await wait(() => document.querySelector('[aria-label="本轮改稿讨论来源"]'), "Failed repair source missing");
  await click("重试"); equal(sends.length, 4, "Failure Retry silently sent with null source");
  check(document.body.textContent!.includes("本轮改稿讨论不能作为普通聊天重试"), "Retry did not explain recovery");
  agentEvent?.({ method: "agent/connection", params: { state: "ready" } }); await tick();
  await input("创作指令", "作者明确选择的普通讨论。"); await click("发送创作指令");
  equal(sends.length, 4, "Failure implicitly switched to an ordinary discussion");
  snapshots.failed = { text: document.querySelector('[aria-label="本轮改稿讨论来源"]')!.textContent };
  await click("取消改稿来源，开始普通讨论"); await click("发送创作指令");
  equal(sends[4], [project.id, "作者明确选择的普通讨论。", null], "Explicit ordinary discussion retained failed source"); await complete();

  await prepare(); delayedRead = deferred(); await click("发送创作指令");
  const switcher = document.querySelector<HTMLButtonElement>("button.project-switcher")!;
  check(switcher, "Project switcher missing"); switcher.click(); await tick();
  const projectButton = [...document.querySelectorAll<HTMLButtonElement>(".project-menu button")]
    .find(node => node.querySelector("strong")?.textContent === otherProject.title);
  check(projectButton && !projectButton.disabled, "Other project menu entry missing"); projectButton!.click(); await tick();
  await wait(() => !prepared(), "Project switch retained preparation");
  delayedRead.resolve(report()); delayedRead = null; await tick(); equal(sends.length, 5, "Prior-project late send escaped");
  check(!composer().value, "Project switch retained prepared prompt");

  // Project switching also clears an already sent repair identity and ignores its late Host failure.
  await click("编辑"); await wait(() => document.querySelector(".revision-outcome-details"), "Other project report missing");
  document.querySelector<HTMLElement>(".work-task-panel summary")!.click();
  document.querySelector<HTMLElement>(".revision-outcome-details summary")!.click();
  await wait(() => button("讨论下一次改稿"), "Other project repair entry missing");
  await prepare(); await input("创作指令", "项目切换前的改稿讨论。"); await click("发送创作指令");
  equal(sends[5], [otherProject.id, "项目切换前的改稿讨论。", expectedSource], "Other project source not bound");
  const oldSend = sendReply!;
  agentEvent?.({ method: "agent/error", params: { message: "HOST_PENDING_FAILURE" } }); await tick();
  check(document.querySelector('[aria-label="本轮改稿讨论来源"]'), "Pending failure lost source");
  document.querySelector<HTMLButtonElement>("button.project-switcher")!.click(); await tick();
  [...document.querySelectorAll<HTMLButtonElement>(".project-menu button")].find(node => node.querySelector("strong")?.textContent === project.title)!.click();
  await wait(() => !document.querySelector('[aria-label="本轮改稿讨论来源"]'), "Project switch retained sent identity");
  oldSend.reject(new Error("LATE_OLD_PROJECT_REPAIR_FAILURE")); await tick();
  check(!document.body.textContent!.includes("LATE_OLD_PROJECT_REPAIR_FAILURE"), "Late failure contaminated another project");
  await input("创作指令", "切换后的普通聊天。"); await click("发送创作指令");
  equal(sends[6], [project.id, "切换后的普通聊天。", null], "Project switch leaked sent identity into retry guard"); await complete();
  await click("编辑"); await wait(() => document.querySelector(".revision-outcome-details"), "Final report missing");
  document.querySelector<HTMLElement>(".work-task-panel summary")!.click();
  document.querySelector<HTMLElement>(".revision-outcome-details summary")!.click();
  await wait(() => button("讨论下一次改稿"), "Final repair entry missing");
  await prepare(); delayedRead = deferred(); await click("发送创作指令");
  flushSync(() => root.render(null)); delayedRead.resolve(report()); delayedRead = null; await tick();
  equal(sends.length, 7, "Unmounted late preflight sent");
  clearAuthor(); root.unmount(); check(unexpectedCalls.length === 0, `Unexpected calls: ${unexpectedCalls.join(", ")}`);
  window["testResult"] = { passed: true, snapshots, unexpectedCalls };
})().catch(error => { window["testResult"] = { passed: false, error: `${error.stack}\nDOM: ${document.body.textContent}` }; });
