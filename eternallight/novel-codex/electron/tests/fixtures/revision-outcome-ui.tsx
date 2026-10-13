import { novelFixtureMedia } from "../helpers/novel-edition";
import { emptyCreationIssueSummary, revisionOutcomeFixture } from "../helpers/creation-issues";
import { chapterRuntimeReads } from "../helpers/chapter-runtime";
import "../../../src/styles.css";
import React from "react";
import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";
import App, { ActionProposalList } from "../../../src/App";
import { clearAuthor, saveAuthor } from "../helpers/author-session";
import { ExecutionScopeDialog } from "../../../src/ExecutionScopeDialog";
import { WorkTaskPanel } from "../../../src/WorkTaskPanel";
import { RevisionOutcomeDetails } from "../../../src/RevisionOutcomeDetails";
import type { DesiredOutcome, ExecutionScopePreview, NovelCodexApi, ProposedAction, RevisionOutcome, WorkTaskSummary } from "../../../src/types";

const root = createRoot(document.getElementById("root")!);
const tick = () => new Promise(resolve => setTimeout(resolve, 30));
const check = (value: unknown, message: string) => { if (!value) throw new Error(message); };
const wait = async (predicate: () => unknown, message: string) => {
  for (let n = 0; n < 100 && !predicate(); n++) await tick();
  check(predicate(), message);
};
const writerA = `task-${"a".repeat(32)}`, writerB = `task-${"b".repeat(32)}`, writerC = `task-${"c".repeat(32)}`;
const criteria: DesiredOutcome[] = [
  { id: "hide_letter", criterion: "把烧信改为藏信，保留密函。", groupIds: ["letter"] },
  { id: "keep_secret", criterion: "主角不向守卫泄露密函的位置。", groupIds: ["letter", "guard"] },
  { id: "wait_signal", criterion: "主角等待白塔亮灯后再离开。", groupIds: ["guard"] },
];
const chapterAcceptance = { requirements: [{ id: "acceptance-1", text: "保留整章上下文与既有故事规则。" }], length: { requirement: "本次保持原有篇幅。", minCharacters: null, maxCharacters: null } };
const proposal: ProposedAction = {
  id: `action-${"d".repeat(32)}`, sourceTaskId: `task-${"e".repeat(32)}`, operation: "generateChapter",
  target: { chapterId: "chapter-2", chapterNumber: 2, scope: { type: "selectedScenes", sourceRevisionId: "original-2", groupIds: ["letter", "guard"] }, chapterAcceptance, desiredOutcomes: criteria },
  instruction: "只修改密函与守卫场景。", inputRevision: 2, intentRevision: 0, status: "proposed",
};
const pending: { args: string[]; resolve: (value: RevisionOutcome | null) => void; reject: (error: unknown) => void }[] = [];
const unexpectedCalls: string[] = [];
const forbidden = (path: string): object => new Proxy(() => {
  unexpectedCalls.push(path); throw new Error(`Unexpected API call: ${path}`);
}, { get: (_target, property) => forbidden(`${path}.${String(property)}`) });
const runtime = new Proxy({ getTaskFinalReply: async () => null,
  getRevisionOutcome: (...args: string[]) => new Promise<RevisionOutcome | null>((resolve, reject) => pending.push({ args, resolve, reject })) }, {
  get: (target, property) => property in target ? target[property as keyof typeof target] : forbidden(`runtime.${String(property)}`),
});
const api = new Proxy({}, { get: (_target, property) => property === "runtime" ? runtime : forbidden(String(property)) }) as NovelCodexApi;
const receipt = (writerTaskId = writerA, state: RevisionOutcome["state"] = "current"): RevisionOutcome => revisionOutcomeFixture({
  revisionId: "revision-outcome-report-2", authority: "candidateAnalysis", state, decisions: [],
  assessment: { uncoveredRequirements: [], writerTaskId, chapterId: "chapter-2", originalRevisionId: "original-2", draftRevisionId: "draft-2", knowledgeBatchId: "modeling-2",
    // Reverse the first two results to prove mapping uses IDs rather than position.
    outcomes: [
      { id: "keep_secret", status: "unmet", reason: "主角仍向守卫透露藏信位置。", evidence: [{ groupId: "guard", quote: "信就在我的袖里。", start: 0, end: 8 }] },
      { id: "hide_letter", status: "supported", reason: "新正文写明主角藏起密函。", evidence: [{ groupId: "letter", quote: "他藏好密函。🔒", start: 0, end: 7 }] },
      { id: "wait_signal", status: "insufficientEvidence", reason: "允许修改的正文没有亮灯后的行动证据。", evidence: [] },
      { id: "acceptance-1", status: "supported", reason: "未改动的开场仍明确白塔的规则。", evidence: [{ groupId: "opening", quote: "白塔熄灯前，不可出城。", start: 0, end: 11 }] },
    ] },
});
const button = (label: string) => [...document.querySelectorAll<HTMLButtonElement>("button")].find(item => item.textContent === label || item.getAttribute("aria-label") === label)!;
const details = () => document.querySelector<HTMLDetailsElement>(".revision-outcome-details")!;
const text = () => details().textContent ?? "";
const snapshot = () => ({
  summary: details().querySelector("summary")!.textContent,
  criteria: details().querySelector('[aria-label="已批准的修改标准"]')?.textContent,
  chapterCriteria: details().querySelector('[aria-label="已冻结的章节标准"]')?.textContent,
  paragraphs: [...details().querySelectorAll(":scope > p")].map(node => node.textContent),
  status: details().querySelector('[role="status"]')?.textContent ?? null,
  alert: details().querySelector('[role="alert"]')?.textContent ?? null,
  sources: details().querySelector("dl")?.textContent ?? null,
  outcomes: [...details().querySelectorAll('[aria-label="逐项修改结果"] > li')].map(node => node.textContent),
  buttons: [...details().querySelectorAll("button")].map(node => node.textContent),
});
const snapshots: Record<string, unknown> = {};
function render(projectId = "project-a", writerTaskId = writerA, connection: NovelCodexApi | undefined = api, desiredOutcomes = criteria, version = "2:4") {
  flushSync(() => root.render(<RevisionOutcomeDetails api={connection} projectId={projectId} writerTaskId={writerTaskId}
    chapterId="chapter-2" chapterAcceptance={chapterAcceptance} desiredOutcomes={desiredOutcomes} version={version} />));
}
async function request(projectId: string, writerTaskId: string, action: () => void) {
  const count = pending.length;
  action();
  await wait(() => pending.length === count + 1, "Expected a single revision read");
  check(JSON.stringify(pending[count].args) === JSON.stringify([projectId, writerTaskId]), "Read did not bind project and writer Attempt");
  check(!details().querySelector('[aria-label="逐项修改结果"], [role="alert"]'), "Prior report or error remained visible during read");
  return pending[count];
}
async function reopen(projectId = "project-a", writerTaskId = writerA) {
  details().querySelector<HTMLElement>("summary")!.click();
  await wait(() => !details().open && details().children.length === 1, "Close did not hide the report");
  return request(projectId, writerTaskId, () => details().querySelector<HTMLElement>("summary")!.click());
}
async function saved(read: typeof pending[number], value = receipt()) {
  read.resolve(value);
  await wait(() => details().querySelector('[aria-label="逐项修改结果"]'), "Saved assessment did not render");
  check(text().includes("不是创作目标达成认证") && text().includes("不会采用正文或事实"), "Candidate judgment authority was overstated");
}

void (async () => {
  const approvals: ProposedAction[] = [], scopes: ProposedAction[] = [];
  const showProposal = (busy = false) => flushSync(() => root.render(<ActionProposalList actions={[proposal]} busy={busy}
    onApprove={action => approvals.push(action)} onDecline={() => {}} onScope={action => scopes.push(action)} />));
  showProposal(); await tick();
  const proposed = document.querySelector('[aria-label="待批准的修改标准"]')!;
  check(criteria.every(item => proposed.textContent!.includes(item.criterion) && proposed.textContent!.includes(item.id)), "Criteria were hidden before approval");
  check(!proposed.closest("details") && approvals.length === 0 && scopes.length === 0 && pending.length === 0, "Proposal read or executed before Author approval");
  check(proposed.compareDocumentPosition(button("批准生成候选")) & Node.DOCUMENT_POSITION_FOLLOWING, "Criteria came after approval control");
  snapshots.proposal = document.querySelector(".conversation-proposals")!.textContent;
  showProposal(true); check(button("批准生成候选").disabled && proposed.textContent!.includes(criteria[0].criterion), "Busy controls hid the approved target criteria");
  showProposal(); button("批准生成候选").click(); button("设置本轮范围").click();
  check(approvals[0] === proposal && scopes[0] === proposal, "Approval callbacks changed the exact proposal identity");

  const previews: { args: [string, string, unknown]; resolve: (value: ExecutionScopePreview) => void }[] = [];
  const confirms: unknown[][] = [];
  let frozen: ExecutionScopePreview;
  const scopeApi = {
    previewExecutionScope: (...args: [string, string, unknown]) => new Promise<ExecutionScopePreview>(resolve => previews.push({ args, resolve })),
    confirmExecutionScope: async (...args: unknown[]) => { confirms.push(args); return { permit: { id: `scope-${frozen.previewDigest}`, scope: frozen, status: "active", confirmedAt: 100 }, work: null }; },
  } as NovelCodexApi["agent"];
  flushSync(() => root.render(<ExecutionScopeDialog projectId="project-a" initialAction={proposal} actions={[proposal]} api={scopeApi}
    blocked={false} onClose={() => {}} onStart={() => {}} onComplete={async () => {}} onError={() => {}} />));
  await tick();
  check(document.querySelectorAll('[aria-label="待批准的修改标准"]').length === 1 && !button("预览范围").disabled, "Scope choice criteria were not visible");
  button("预览范围").click(); await wait(() => previews.length === 1, "Scope preview was not read");
  frozen = { previewDigest: "f".repeat(64), projectId: "project-a", sourceTaskId: proposal.sourceTaskId, workTaskId: "work-a",
    authorMessageId: "author-message-a", authorIntent: proposal.instruction, request: previews[0].args[2] as ExecutionScopePreview["request"],
    targets: [{ operation: proposal.operation, target: proposal.target }], baseSnapshotId: "snapshot-a", sourceDigest: "digest-a",
    inputRevision: 2, workVersion: 3, remainingRequests: 8, remainingTokens: 12000, deadlineAt: 1790000000 };
  previews[0].resolve(frozen); await wait(() => !button("确认范围并继续").disabled, "Scope did not become reviewable");
  const frozenCriteria = document.querySelector('.scope-preview [aria-label="待批准的修改标准"]')!;
  check(criteria.every(item => frozenCriteria.textContent!.includes(item.criterion)) && !frozenCriteria.closest("details"), "Frozen preview hid or omitted criteria");
  check(JSON.parse(document.querySelector(".scope-preview pre")!.textContent!)[0].target.desiredOutcomes.length === 3, "Exact frozen target lost criteria");
  snapshots.scope = { proposed: document.querySelector('.scope-choice [aria-label="待批准的修改标准"]')!.textContent,
    frozen: frozenCriteria.textContent, exactTarget: frozen.targets };
  button("确认范围并继续").click(); await wait(() => confirms.length === 1, "Scope was not confirmed");
  check(JSON.stringify(confirms[0]) === JSON.stringify(["project-a", proposal.sourceTaskId, frozen.request, frozen.previewDigest]), "Scope confirmation rewrote the frozen target binding");

  const attempt = (id: string, status: string) => ({ id, status, attemptNumber: 1, startedAt: 1, completedAt: status === "running" ? null : 2, threadId: "thread-a", turnId: null, error: null });
  const step: WorkTaskSummary["steps"][number] = { id: "writer-step", operation: "generateChapter", expectedOutputKind: "draft",
    instruction: proposal.instruction, sourceTaskId: null, target: proposal.target, status: "completed", inputRevision: 2,
    dependsOnStepId: null, currentAttemptId: writerA, readingProgress: null, outputRefs: [], changes: [],
    attempts: [attempt(writerA, "completed"), attempt(writerB, "failed"), attempt(writerC, "cancelled"), attempt("running-attempt", "running")] };
  const task: WorkTaskSummary = { id: "work-a", projectId: "project-a", threadId: "thread-a", goal: proposal.instruction,
    kind: "conversation", status: "completed", deliveryStatus: "pending", version: 2, error: null, resumeBlockReason: null, resumeMode: null,
    deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], evidence: [], pendingActionIds: [], unsettledAttemptIds: [] },
    steps: [step, { ...step, id: "no-goal", target: { chapterId: "chapter-2" } }, { ...step, id: "analysis", operation: "assessRevisionOutcome" }] };
  flushSync(() => root.render(<WorkTaskPanel api={api} projectId="project-a" tasks={[task]} activeCount={0} nextCursor={null}
    loadingMore={false} loaded reading={false} readError={null} onRetry={() => {}} revision={4} busy={false}
    agentRunning={false} dirty={false} onUndo={() => {}} onLoadMore={() => {}} />));
  await tick();
  check(document.querySelectorAll(".revision-outcome-details").length === 3 && pending.length === 0, "Work panel assessed a no-goal, non-writer or nonterminal step, or read eagerly");
  check(!document.querySelector(".work-task-execution"), "Revision results returned to a second process list");
  snapshots.workPanel = { judgments: document.querySelectorAll(".revision-outcome-details").length, adjacentToReply: true, executionList: false };
  let read = await request("project-a", writerA, () => details().querySelector<HTMLElement>("summary")!.click());
  read.resolve(null); await wait(() => text().includes("尚无修改结果判断"), "Work panel did not read by terminal writer Attempt");
  const failedWriter = document.querySelectorAll<HTMLDetailsElement>(".revision-outcome-details")[1];
  const beforeFailedRead = pending.length;
  failedWriter.querySelector<HTMLElement>("summary")!.click();
  await wait(() => pending.length === beforeFailedRead + 1, "Failed writer's result was not readable");
  check(JSON.stringify(pending[beforeFailedRead].args) === JSON.stringify(["project-a", writerB]), "Failed writer read used a work/step ID");
  pending[beforeFailedRead].resolve(null);
  await wait(() => failedWriter.textContent!.includes("尚无修改结果判断"), "Failed writer invented an assessment instead of showing null");

  // Keep this component mounted while identities change to exercise response isolation.
  render(); await tick(); check(pending.length === 2 && !details().open, "Mounted collapsed judgment fetched eagerly");
  read = await request("project-a", writerA, () => details().querySelector<HTMLElement>("summary")!.click());
  snapshots.loading = snapshot();
  read.reject(new Error("分析结果暂时不可读")); await wait(() => details().querySelector('[role="alert"]'), "Read error missing");
  snapshots.readError = snapshot();
  read = await request("project-a", writerA, () => button("重试读取修改结果").click());
  await saved(read);
  const rows = [...details().querySelectorAll('[aria-label="逐项修改结果"] > li')];
  check(rows.length === 4 && rows[0].textContent!.includes(`${chapterAcceptance.requirements[0].text} · 有证据支持`)
    && rows[1].textContent!.includes(`${criteria[0].criterion} · 有证据支持`) && rows[2].textContent!.includes(`${criteria[1].criterion} · 未满足`)
    && rows[3].textContent!.includes(`${criteria[2].criterion} · 证据不足`), "Current full acceptance report or ID mapping was lost");
  check(rows[0].textContent!.includes("白塔") && rows[0].textContent!.includes("场景 opening"), "Whole-chapter standards incorrectly limited evidence to selected repair scenes");
  check(rows[1].textContent!.includes("🔒") && rows[1].textContent!.includes("[0, 7)"), "Unicode quote or offset provenance was lost");
  snapshots.current = snapshot();
  const beforePoll = pending.length;
  render("project-a", writerA, api, structuredClone(criteria)); await tick();
  check(pending.length === beforePoll && details().querySelector('[aria-label="逐项修改结果"]'), "Same-content task polling reset or reread the assessment");
  read = await reopen(); await saved(read, receipt(writerA, "stale"));
  check(text().includes("已过期") && !text().includes("当前来源"), "Stale judgment was shown as current"); snapshots.stale = snapshot();
  read = await reopen(); read.resolve(null); await wait(() => text().includes("尚无修改结果判断"), "Missing report was called supported or unmet"); snapshots.missing = snapshot();
  read = await request("project-a", writerA, () => button("重新读取修改结果").click()); await saved(read);

  // Incomplete, duplicate or foreign ID reports must never attach judgments to criteria.
  for (const change of [
    (value: RevisionOutcome) => { value.assessment.writerTaskId = writerB; },
    (value: RevisionOutcome) => { value.assessment.chapterId = "foreign-chapter"; },
    (value: RevisionOutcome) => { value.assessment.outcomes[0].id = "foreign-criterion"; },
    (value: RevisionOutcome) => { value.assessment.outcomes[0].id = "hide_letter"; },
    (value: RevisionOutcome) => { value.assessment.outcomes.pop(); },
    (value: RevisionOutcome) => { value.assessment.outcomes.at(-1)!.id = "acceptance-2"; },
    (value: RevisionOutcome) => { value.assessment.outcomes[1].evidence = []; },
    (value: RevisionOutcome) => { value.assessment.outcomes[1].evidence[0].groupId = "protected-scene"; },
    (value: RevisionOutcome) => { value.authority = "canon" as RevisionOutcome["authority"]; },
  ]) {
    read = await reopen(); const invalid = receipt(); change(invalid); read.resolve(invalid);
    await wait(() => text().includes("不匹配"), "Mismatched report was accepted");
    check(!details().querySelector('[aria-label="逐项修改结果"]'), "Invalid outcome leaked into visible criteria");
  }
  snapshots.mismatch = snapshot();

  const oldProject = await request("project-a", writerA, () => button("重试读取修改结果").click());
  const newProject = await request("project-b", writerA, () => { render("project-b"); check(!text().includes("不匹配"), "Prior project error flashed"); });
  oldProject.resolve(receipt()); await tick();
  check(!details().querySelector('[aria-label="逐项修改结果"]'), "Late project response leaked");
  await saved(newProject);
  const oldTask = await request("project-b", writerB, () => { render("project-b", writerB); check(!text().includes("新正文写明"), "Prior task report flashed"); });
  const newTask = await request("project-b", writerC, () => render("project-b", writerC));
  await saved(newTask, receipt(writerC)); oldTask.reject(new Error("LATE_TASK_ERROR")); await tick();
  check(details().querySelector('[aria-label="逐项修改结果"]') && !text().includes("LATE_TASK_ERROR"), "Late task failure replaced current report");
  const oldWriter = await request("project-b", writerB, () => render("project-b", writerB));
  const currentWriter = await request("project-b", writerC, () => render("project-b", writerC));
  await saved(currentWriter, receipt(writerC)); oldWriter.resolve(receipt(writerB)); await tick();
  check(text().includes(writerC) && !text().includes(writerB), "Late writer success replaced the latest attempt");
  read = await request("project-b", writerC, () => render("project-b", writerC, api, criteria, "3:5"));
  await saved(read, receipt(writerC, "stale")); check(text().includes("已过期"), "Work/source version change kept cached current state");

  const closedRead = await reopen("project-b", writerC);
  details().querySelector<HTMLElement>("summary")!.click(); await wait(() => !details().open, "Details did not close");
  const beforeClosedSwitch = pending.length;
  render("project-c", writerA); await tick(); closedRead.reject(new Error("LATE_CLOSED_ERROR")); await tick();
  check(pending.length === beforeClosedSwitch && details().children.length === 1, "Closed scope switch read or retained late state");
  read = await request("project-c", writerA, () => details().querySelector<HTMLElement>("summary")!.click());
  const literal = receipt(); literal.assessment.outcomes[1].evidence[0].quote = "<img src=x onerror=window.outcomeExecuted=true>";
  literal.assessment.outcomes[1].evidence[0].end = [...literal.assessment.outcomes[1].evidence[0].quote].length;
  await saved(read, literal); check(!details().querySelector("img,script") && !window["outcomeExecuted"], "Evidence was executed as markup");
  const unavailable = new Proxy({}, { get: (_target, property) => property === "runtime" ? undefined : forbidden(String(property)) }) as NovelCodexApi;
  render("project-c", writerA, unavailable); await wait(() => details().querySelector('[role="alert"]'), "Unavailable read API did not expose retry");
  snapshots.unavailable = snapshot();
  const finalRead = await request("project-c", writerA, () => render("project-c", writerA));
  flushSync(() => root.render(null)); finalRead.resolve(receipt()); await tick();

  // Exercise the production approval message with a fully local Author response.
  const project = { id: "project-a", title: "密函测试作品", logline: "", updated_at: "" };
  const approvalMessages: Record<string, string | null> = {};
  saveAuthor({ name: "测试作者", email: "author@example.test", passwordSalt: "local-salt", passwordHash: "local-hash" });
  localStorage.setItem("novel-codex-active-project", project.id);
  const proposalThread = { id: "proposal-thread", projectId: project.id, name: "密函修改", preview: null, modelProvider: "fixture",
    status: "idle", createdAt: 100, updatedAt: 100, isPinned: false, ephemeral: false };
  const proposalWork: WorkTaskSummary = { ...task, id: "proposal-work", threadId: proposalThread.id, deliveryStatus: "discussion_ended",
    steps: [{ ...step, id: "proposal-source", operation: "advisory", target: {}, currentAttemptId: proposal.sourceTaskId,
      attempts: [{ ...attempt(proposal.sourceTaskId, "completed"), threadId: proposalThread.id, turnId: "proposal-turn" }] }] };
  localStorage.setItem(`novel-codex:selected-conversation:${JSON.stringify(["author@example.test", project.id])}`, proposalThread.id);
  for (const status of ["savedFailure", "failure", "recorded"] as const) {
    let available = true;
    const approvedIds: string[][] = [];
    const appRuntime = {
      ...chapterRuntimeReads(() => project),
      listWorkTasks: async () => ({ tasks: [proposalWork], nextCursor: null, activeCount: 0 }),
      getTaskFinalReply: async () => null,
      status: async () => ({ state: "online", mode: "local", error: null }),
      listProjects: async () => ({ projects: [project] }),
      getState: async () => ({ project, canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] },
        narrativeIr: { version: "1.0", scenes: [], anchors: [] }, novelEdition: { chapters: [] } }),
    };
    const appAgent = {
      onEvent: () => () => {}, onUserInputRequest: () => () => {},
      status: async () => ({ configured: false, state: "idle", mode: "novel-agent-engine", error: null }),
      listConnections: async () => ({ connections: [] }), recoveryReport: async () => [],
      listExecutions: async () => [], readMessage: async () => { throw Error("message receipt not found"); }, beginInterlude: async () => { throw Error("unexpected interlude"); }, returnInterlude: async () => { throw Error("unexpected interlude return"); }, listMessages: async () => [], listThreads: async () => ({ threads: [proposalThread], nextCursor: null }),
      readConversation: async () => ({ thread: proposalThread, turns: [] }), setWorkspaceContext: async () => ({ selected: true }),
      listProposedActions: async () => ({ actions: available ? [proposal] : [] }),
      approveAction: async (...args: string[]) => {
        approvedIds.push(args); available = false;
        return { action: proposal, taskId: writerA,
          candidateResults: status === "failure" ? [] : [{ kind: "draft", revisionId: "draft-2" }],
          executionFailure: status === "recorded" ? null : { message: "修改效果检查尚未完成；没有生成检查结论。" } };
      },
    };
    const trap = (target: object, path: string) => new Proxy(target, { get: (value, property) =>
      Object.hasOwn(value, property) ? Reflect.get(value, property) : forbidden(`${path}.${String(property)}`) });
    window.novelCodex = { multimedia: novelFixtureMedia, app: { onCloseRequested: () => () => {} }, runtime: trap(appRuntime, "app.runtime"), agent: trap(appAgent, "app.agent") } as NovelCodexApi;
    flushSync(() => root.render(<App key={status} />));
    await wait(() => document.querySelector('[aria-label="待确认的创作建议"]'), "App did not render the proposal");
    button("批准生成候选").click();
    await wait(() => !document.querySelector('[aria-label="待确认的创作建议"]') && document.querySelector(".activity-strip"), "App approval did not settle");
    const activity = document.querySelector(".activity-strip")!.textContent;
    check(JSON.stringify(approvedIds) === JSON.stringify([[project.id, proposal.sourceTaskId, proposal.id]]), "Approval failed to bind exact action identity");
    if (status !== "recorded") {
      await wait(() => document.querySelector(".activity-strip")!.textContent!.includes("修改效果检查尚未完成"), "Execution failure hid the incomplete requested assessment");
      check(!document.querySelector(".activity-strip")!.textContent!.includes("处理已完成"), "Failed requested assessment was labeled complete");
    } else check(activity!.includes("尚未正式采用"), "Recorded assessment implied Canon adoption");
    approvalMessages[status] = document.querySelector(".activity-strip")!.textContent;
    flushSync(() => root.render(null));
  }
  snapshots.approvalMessages = approvalMessages;
  clearAuthor(); root.unmount();
  check(unexpectedCalls.length === 0, `Read-only UI executed ${unexpectedCalls.join(", ")}`);
  window["testResult"] = { passed: true, snapshots, reads: pending.length, unexpectedCalls };
})().catch(error => { window["testResult"] = { passed: false, error: `${error.stack}\nDOM: ${document.body.textContent}` }; });
