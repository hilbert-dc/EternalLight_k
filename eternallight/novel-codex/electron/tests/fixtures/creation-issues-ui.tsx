import React from "react";
import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";
import { WorkTaskPanel } from "../../../src/WorkTaskPanel";
import { CreationIssueList, CreationIssueReference, WorkIssueScope } from "../../../src/CreationIssues";
import { operationHeading, operationPresentation } from "../../../src/operation-presentation";
import type { CreationIssue, CreationIssueObservation, CreationIssueSummary, NovelCodexApi, RevisionOutcome, WorkTaskSummary } from "../../../src/types";
import "../../../src/styles.css";

let renderError: unknown = null;
window.addEventListener("error", event => { renderError ??= event.error ?? Error(event.message); });
window.addEventListener("unhandledrejection", event => { renderError ??= event.reason; });
let root = createRoot(document.getElementById("root")!);
const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (value: unknown, message: string) => { if (!value) throw Error(message); };
const until = async (predicate: () => unknown, message: string) => { for (let n = 0; n < 150 && !predicate(); n++) { if (renderError) throw renderError; await tick(); } if (renderError) throw renderError; check(predicate(), message); };
const capture = async (name: string) => { window.testScreenshot = name; await until(() => window.testScreenshot === null, "UI screenshot did not settle"); };
const deferred = () => { let resolve: (value: unknown) => void; const promise = new Promise(r => { resolve = r; }); return { promise, resolve: value => resolve(value) }; };
const calls = { pages: [] as unknown[][], reports: [] as string[][], writes: 0, refresh: 0 };
const cases: string[] = [];
const writerId = `task-${"a".repeat(32)}`, chapterId = "chapter-one";
const quote = "晨钟响过后，他才动身。";
const evidence = { groupId: "scene-dawn", quote, start: 0, end: 11 };
const acceptance = { requirements: [{ id: "frozen-timeline", text: "行动必须发生在晨钟之后。" }], length: { requirement: "完整章节约三千字", minCharacters: 2500, maxCharacters: 3500 } };
const desired = [{ id: "desired-letter", criterion: "保留信件。", groupIds: ["scene-letter"] }];
const observation = (observationId: string, status: CreationIssueObservation["status"] = "unmet", extra = {}): CreationIssueObservation => ({
  issueId: "issue-timeline", observationId, taskId: writerId, reportRevisionId: `report-${observationId}`, draftRevisionId: "draft-current", criterionId: "frozen-timeline", status,
  reason: status === "supported" ? "复核认为行动发生在晨钟之后。" : "独立核对认为先后行动仍有矛盾。", source: "revisionOutcome", current: true, recheckRequired: false, superseded: false, resolutionOf: [], evidence: [evidence], ...extra,
});
const makeIssue = (issueId = "issue-timeline", extra = {}): CreationIssue => ({ issueId, summary: "行动时间仍需核对。", status: "disputed", chapterId,
  currentDraftRevisionId: "draft-current", observationCount: 3, hasMoreObservations: false,
  observations: [observation("observation-negative", "unmet", { issueId }), observation("observation-positive", "supported", { issueId }), observation("observation-delivery", "unmet", { issueId, source: "deliveryReview", reason: "交付核对引用同一问题，等待独立复核。", evidence: [] })], ...extra });
const metadata = (issue: CreationIssue): CreationIssue => ({ ...issue, observations: [], hasMoreObservations: issue.observationCount > 0 });
const summary = (entries: CreationIssue[], totalCount = entries.length): CreationIssueSummary => ({ totalCount,
  openCount: entries.filter(issue => issue.status === "open").length, staleCount: entries.filter(issue => issue.status === "stale").length,
  disputedCount: entries.filter(issue => issue.status === "disputed").length, entries: entries.slice(-2).map(metadata), hasMore: totalCount > Math.min(entries.length, 2) });
let currentIssue = makeIssue();
let report = (): RevisionOutcome => ({ revisionId: "report-current", authority: "candidateAnalysis", state: "current", decisions: [], issues: [structuredClone(currentIssue)],
  assessment: { writerTaskId: writerId, chapterId, originalRevisionId: null, draftRevisionId: "draft-current", knowledgeBatchId: null, uncoveredRequirements: [],
    outcomes: [{ id: "frozen-timeline", status: "unmet", reason: "本次检查没有通过。", evidence: [evidence], issue: { issueId: "issue-timeline", newIssue: null, resolutionOf: [] } },
      { id: "desired-letter", status: "supported", reason: "正文仍保留信件。", evidence: [{ groupId: "scene-letter", quote: "他收好了信。", start: 0, end: 6 }], issue: null }] } });
const nativeViews = async (projectId: string, workId: string, request: { issueId: string | null; cursor: number; limit: number }): Promise<unknown> => {
  check(projectId === "project-one" && workId === "work-one" && request.issueId === "issue-timeline" && request.cursor === 0 && request.limit === 16, "metadata expansion omitted exact issue page request");
  return { issueId: currentIssue.issueId, entries: structuredClone(currentIssue.observations), totalCount: currentIssue.observationCount, nextCursor: null };
};
let pageHandler = nativeViews;
const api = { runtime: { getTaskFinalReply: async () => null, getRevisionOutcome: async (...args: string[]) => { calls.reports.push(args); return structuredClone(report()); },
  getWorkIssues: async (...args: [string, string, unknown]) => { calls.pages.push(structuredClone(args)); return pageHandler(...args); },
  decideRevisionOutcome: async () => { calls.writes++; throw Error("unexpected decision"); } }, agent: { sendPrompt: async () => { calls.writes++; throw Error("unexpected model"); } } } as unknown as NovelCodexApi;
const work = (issue = currentIssue, extra = {}): WorkTaskSummary => ({ id: "work-one", projectId: "project-one", threadId: "thread-one", kind: "conversation", goal: "继续创作并由应用核对当前正文。",
  status: "running", deliveryStatus: "partial", version: 3, error: null, resumeMode: null, resumeBlockReason: null, pendingContinuation: false,
  deliveryReport: { issues: summary([{ ...issue, observations: [], hasMoreObservations: issue.observationCount > 0 }]), chapterAcceptanceReports: [{ writerTaskId: writerId, chapterId, draftRevisionId: "draft-current", chapterAcceptance: acceptance,
    reportRevisionId: "report-current", current: true, verdict: "notMet", length: { characterCount: 3100, minCharacters: 2500, maxCharacters: 3500, satisfied: true, countUnit: "Unicode characters" },
    checks: [{ id: "frozen-timeline", status: "unmet", reason: "本次独立检查没有通过。", issueId: "issue-timeline" }] }], evidence: [], pendingActionIds: [], unsettledAttemptIds: [],
    deliveryReview: { goalStatus: "partial", goalAssessment: "正文已保存，仍有问题需要核对。", checkedScope: "当前章正文", unresolved: [{ issue: { issueId: "issue-timeline", newIssue: null, resolutionOf: [] }, observationId: "observation-negative", reason: "独立检查的问题尚未关闭。" }], issueResolutions: [], authorDecisions: [] } },
  steps: [{ id: "write-step", operation: "generateChapter", expectedOutputKind: "draft", instruction: "续写本章", sourceTaskId: null,
    target: { chapterId, chapterNumber: 7, chapterAcceptance: acceptance, desiredOutcomes: desired }, status: "completed", inputRevision: 1, dependsOnStepId: null,
    currentAttemptId: writerId, readingProgress: null, outputRefs: [], changes: [], attempts: [{ id: writerId, attemptNumber: 1, startedAt: 1, completedAt: 2, threadId: "thread-one", turnId: null, status: "completed", error: null }] }], ...extra });
const button = (label: string, container: ParentNode = document) => [...container.querySelectorAll<HTMLButtonElement>("button")].find(node => node.textContent === label)!;
const issueNode = (issueId = "issue-timeline") => document.querySelector<HTMLDetailsElement>(`[data-creation-issue="${issueId}"]`)!;
const renderWork = (task = work()) => flushSync(() => root.render(<WorkTaskPanel api={api} projectId={task.projectId} tasks={[task]} activeCount={1} nextCursor={null}
  loadingMore={false} loaded reading={false} readError={null} onRetry={() => { calls.refresh++; }} revision={3} busy={false} agentRunning={false} dirty={false} onLoadMore={() => {}} />));
const renderScope = (projection: CreationIssueSummary, version = 3, projectId = "project-one", reference = "issue-timeline") => flushSync(() => root.render(
  <WorkIssueScope api={api} projectId={projectId} taskId="work-one" version={version} summary={projection} onRetry={() => { calls.refresh++; }}>
    {panel => <main style={{ maxWidth: 800 }}><CreationIssueReference issueId={reference} />{panel}</main>}</WorkIssueScope>));
const openIssue = async (issueId = "issue-timeline") => { issueNode(issueId).querySelector<HTMLElement>("summary")!.click(); await until(() => issueNode(issueId).open, "issue did not expand"); };

void (async () => { try {
  const issueTool = { tool: "read_work_issues", status: "completed", arguments: { cursor: 0, chapterNumber: 99 } };
  const issuePage = { entries: [metadata(currentIssue)], totalCount: 1, nextCursor: null };
  const presented = operationPresentation({ ...issueTool, result: { structuredContent: issuePage } });
  check(presented.category === "read" && presented.outcome === "read" && operationHeading(presented) === "读取问题记录", "issue tool was technical, unclassified or guessed a chapter");
  const viewPage = { issueId: "issue-timeline", entries: currentIssue.observations, totalCount: 3, nextCursor: null };
  check(operationPresentation({ ...issueTool, arguments: { issueId: "issue-timeline" }, result: { structuredContent: viewPage } }).outcome === "read", "actual issue view page not acknowledged");
  for (const page of [null, { entries: [], totalCount: 0, nextCursor: null }, { entries: [metadata(currentIssue)] }, { ...viewPage, issueId: "foreign-issue" }, { ...viewPage, entries: [observation("foreign-observation", "unmet", { issueId: "foreign-issue" })] }]) {
    check(operationPresentation({ ...issueTool, arguments: { issueId: "issue-timeline" }, result: { structuredContent: page } }).outcome !== "read", "missing, empty or foreign issue page certified as read");
  }
  check(operationPresentation({ ...issueTool, result: { isError: true, structuredContent: issuePage } }).outcome !== "read", "failed issue tool certified as read");
  cases.push("issue tool real page presentation");
  document.getElementById("root")!.style.cssText = "width:800px;max-width:100%;margin:32px auto;";
  renderWork(); await tick();
  check(document.querySelectorAll("[data-creation-issue]").length === 1 && !issueNode().open && calls.reports.length === 0 && calls.pages.length === 0, "issue records read eagerly or summary repeated");
  await capture("creation issues summary");
  const checkDetails = document.querySelector<HTMLDetailsElement>('[aria-label="章节检查进展"] details')!;
  checkDetails.querySelector<HTMLElement>("summary")!.click(); await tick();
  check(checkDetails.textContent!.includes(acceptance.requirements[0].text), "frozen stable criterion identity was replaced by positional ID");
  button("查看相关问题", checkDetails).click(); await until(() => issueNode().open && issueNode().querySelectorAll(".creation-issue-observations > section").length === 3, "chapter check did not lazily read exact issue views");
  check(calls.pages.length === 1, "metadata-only issue opened without an actual issueId read"); cases.push("metadata expansion reads actual views");
  const revisionDetails = document.querySelector<HTMLDetailsElement>(".revision-outcome-details")!;
  revisionDetails.querySelector<HTMLElement>("summary")!.click(); await until(() => revisionDetails.querySelector('[aria-label="逐项修改结果"]'), "independent report failed current stable issue contract");
  check(document.querySelectorAll("[data-creation-issue]").length === 1, "report created a duplicate issue panel");
  issueNode().open = false; button("查看相关问题", revisionDetails).click(); await until(() => issueNode().open, "report reference did not reopen same issue");
  issueNode().open = false; button("查看相关问题", document.querySelector('[aria-label="本轮目标核对"]')!).click(); await until(() => issueNode().open, "delivery reference did not open same issue");
  check(document.querySelectorAll("[data-creation-issue]").length === 1 && calls.pages.length === 1, "three reports failed shared stable issue identity"); cases.push("one issue across three reports");
  const view = issueNode();
  check(view.querySelectorAll(".creation-issue-observations > section").length === 3 && view.textContent!.includes(quote) && view.textContent!.includes("交付核对"), "independent evidence or source view missing");
  check(view.querySelector("summary")!.textContent!.includes("判断有分歧") && !view.querySelector("summary")!.textContent!.includes("已复核解决"), "positive view overruled Native disputed status");
  check([...view.querySelectorAll<HTMLDetailsElement>("details")].every(node => !node.open), "technical sources opened by default"); cases.push("independent evidence and Native dispute");
  check(view.innerText.includes("正文引用") && !view.innerText.includes("scene-dawn"), "internal scene ID leaked into visible quotation");
  const provenance = view.querySelector<HTMLDetailsElement>(".creation-issue-observations > section > details")!;
  provenance.querySelector<HTMLElement>("summary")!.click(); await tick();
  check(provenance.innerText.includes("scene-dawn") && provenance.innerText.includes("所引场景内字符区间") && provenance.innerText.includes("[0, 11)"), "source details lost exact scene-local evidence interval");
  provenance.open = false; await tick();
  revisionDetails.open = false; checkDetails.open = false; await tick(); await capture("creation issues evidence");
  renderScope(summary([makeIssue("issue-timeline"), makeIssue("issue-other", { status: "open" })])); await tick();
  check(document.querySelectorAll("[data-creation-issue]").length === 2 && [...document.querySelectorAll(".creation-issue > summary strong")].every(node => node.textContent === "行动时间仍需核对。"), "same text was heuristically merged"); cases.push("same text separate identities");
  renderScope(summary([makeIssue("issue-timeline", { chapterId: "章节 一", currentDraftRevisionId: "稿件 当前" })]), 4); await tick();
  check(issueNode() && !document.querySelector('[role="alert"]'), "legal Native object ID with internal space rejected");
  for (const invalid of [makeIssue("问题 标识"), makeIssue("issue-timeline", { currentDraftRevisionId: "灯".repeat(54) })]) {
    renderScope(summary([invalid]), 5); await tick(); check(document.querySelector('[role="alert"]') && !document.querySelector("[data-creation-issue]"), "stable ASCII or Native byte identity boundary ignored");
  }
  cases.push("Native identity validation");
  currentIssue = makeIssue("issue-timeline", { observationCount: 2, observations: [observation("scene-under", "unmet", { criterionId: "_goal" }), observation("scene-hyphen", "supported", { criterionId: "-goal" })] });
  renderScope(summary([currentIssue]), 6); await tick(); await openIssue();
  await until(() => issueNode().querySelectorAll(".creation-issue-observations > section").length === 2 || issueNode().querySelector('[role="alert"]'), "scene criterion viewpoint read did not settle");
  check(!issueNode().querySelector('[role="alert"]') && issueNode().textContent!.includes("_goal") && issueNode().textContent!.includes("-goal"), "current Native leading underscore/hyphen scene criteria were rejected");
  currentIssue = { ...currentIssue, observations: [observation("_invalid-view", "unmet", { criterionId: "_goal" })], observationCount: 1 };
  renderScope(summary([currentIssue]), 7); await tick();
  await until(() => issueNode().querySelector('[role="alert"]'), "scene criterion union also loosened stable observation IDs");
  check(issueNode().querySelectorAll(".creation-issue-observations > section").length === 0, "invalid stable observation was displayed");
  currentIssue = makeIssue(); cases.push("current scene criterion identities");
  const pendingTask = work(makeIssue("issue-timeline", { status: "stale", currentDraftRevisionId: "draft-new" })); pendingTask.version++;
  pendingTask.deliveryReport.chapterAcceptanceReports[0] = { ...pendingTask.deliveryReport.chapterAcceptanceReports[0], verdict: "pending", current: false, reportRevisionId: null, checks: [] };
  renderWork(pendingTask); await tick();
  check(document.querySelector('[aria-label="章节检查进展"]')!.textContent!.includes("正文已保存，待独立检查") && issueNode().querySelector("summary")!.textContent!.includes("待重新核对"), "saving a new draft certified old issue solved");
  pendingTask.version++; pendingTask.deliveryReport.chapterAcceptanceReports[0].verdict = "stale"; renderWork(pendingTask); await tick();
  check(document.querySelector('[aria-label="章节检查进展"]')!.textContent!.includes("检查尚需重新核对"), "stale report certified current draft"); cases.push("saved pending and stale remain unresolved");
  const resolved = makeIssue("issue-timeline", { status: "resolved", observations: [observation("resolved-view", "supported", { resolutionOf: ["observation-negative"] })], observationCount: 1 });
  currentIssue = resolved; renderScope(summary([resolved]), 5); await openIssue(); await until(() => issueNode().textContent!.includes("处理状态以问题记录为准"), "resolution view not read");
  check(issueNode().textContent!.includes("已复核解决") && issueNode().textContent!.includes("处理状态以问题记录为准"), "explicit Native resolution provenance missing");
  root.unmount(); await tick(); root = createRoot(document.getElementById("root")!); renderScope(structuredClone(summary([resolved])), 5); await tick();
  check(issueNode().dataset.creationIssue === "issue-timeline" && issueNode().querySelector("summary")!.textContent!.includes("已复核解决"), "restart changed persisted identity or status"); cases.push("explicit resolution and persisted identity");
  currentIssue = makeIssue();

  const allIssues = Array.from({ length: 9 }, (_, index) => makeIssue(`issue-${index}`, { summary: `需要核对的问题 ${index}`, status: "open" }));
  const recentIssues = summary(allIssues);
  let gate = deferred(); pageHandler = async (projectId, workId, request: { issueId: string | null; cursor: number; limit: number }) => {
    check(projectId === "project-one" && workId === "work-one" && request.limit === 16 && Object.keys(request).length === 3, "issue page omitted current object contract");
    if (request.issueId !== null) { check(request.issueId === "issue-0" && request.cursor === 0, "paged reference read unrelated issue views"); return { issueId: "issue-0", entries: allIssues[0].observations, totalCount: 3, nextCursor: null }; }
    if (request.cursor === 0) return gate.promise; check(request.cursor === 44, "issue keyset was treated as offset"); return { entries: allIssues.slice(4).map(metadata), totalCount: 9, nextCursor: null };
  };
  renderScope(recentIssues, 6, "project-one", "issue-0"); await tick();
  check(document.querySelectorAll("[data-creation-issue]").length === 2 && button("查看更多问题"), "bounded issue summary pretended full history"); cases.push("bounded views and numeric keyset");
  const beforePages = calls.pages.length; button("查看相关问题").click(); button("查看相关问题").click();
  await until(() => calls.pages.length === beforePages + 1, "paged reference did not singleflight");
  gate.resolve({ entries: allIssues.slice(0, 4).map(metadata), totalCount: 9, nextCursor: 44 }); await until(() => issueNode("issue-0")?.open && issueNode("issue-0").querySelectorAll(".creation-issue-observations > section").length === 3, "paged stable ID reference not opened with actual views");
  button("查看更多问题").click(); await until(() => document.querySelectorAll("[data-creation-issue]").length === 9, "issue keyset continuation did not append exact IDs");
  check(calls.pages.length === beforePages + 3 && !button("查看更多问题"), "issue page replayed or invented extra history"); cases.push("paged issue reference singleflight");

  const allViews = Array.from({ length: 10 }, (_, index) => observation(`view-${index}`, index === 9 ? "supported" : "unmet", index === 2 ? { current: false, recheckRequired: true } : index === 3 ? { current: false, superseded: true } : {}));
  const longIssue = makeIssue("issue-timeline", { observations: allViews.slice(2), observationCount: 10, hasMoreObservations: true });
  let foreign = false; pageHandler = async (_project, _work, request: { issueId: string | null; cursor: number; limit: number }) => {
    check(request.issueId === "issue-timeline" && request.limit === 16, "observation read not bound to exact issue");
    if (foreign) return { issueId: "foreign-issue", entries: allViews.slice(0, 5), totalCount: 10, nextCursor: 55 };
    if (request.cursor === 0) return { issueId: "issue-timeline", entries: allViews.slice(0, 5), totalCount: 10, nextCursor: 55 };
    check(request.cursor === 55, "view keyset was treated as offset"); return { issueId: "issue-timeline", entries: allViews.slice(5), totalCount: 10, nextCursor: null };
  };
  flushSync(() => root.render(<CreationIssueList issues={[longIssue]} />)); await openIssue();
  check(issueNode().textContent!.includes("仅显示最近 8 条观点，共 10 条"), "bounded views posed as full report history");
  check(issueNode().textContent!.includes("这条观点仍需重新核对") && issueNode().textContent!.includes("已由后续检查取代"), "Native per-view freshness and supersession flags were hidden");
  renderScope(summary([longIssue]), 7); await openIssue(); await until(() => issueNode().querySelectorAll(".creation-issue-observations > section").length === 5, "metadata expansion did not read first observation page");
  button("读取更多观点").click(); await until(() => issueNode().querySelectorAll(".creation-issue-observations > section").length === 10, "view keyset did not complete exact history");
  check(issueNode().querySelector("summary")!.textContent!.includes("判断有分歧"), "paged majority guessed resolved"); cases.push("paged observations and provenance");
  foreign = true; renderScope(summary([longIssue]), 8); await tick(); if (!issueNode().open) await openIssue();
  await until(() => issueNode().querySelector('[role="alert"]'), "foreign observation page accepted");
  check(issueNode().querySelectorAll(".creation-issue-observations > section").length === 0, "foreign page was attached to actual problem");
  foreign = false; button("重试读取").click(); await until(() => issueNode().querySelectorAll(".creation-issue-observations > section").length === 5 && !issueNode().querySelector('[role="alert"]'), "view read failure unrecoverable"); cases.push("foreign observation page retry");
  pageHandler = async () => ({ entries: [], totalCount: 9, nextCursor: 99 }); renderScope(recentIssues, 9); await tick(); button("查看更多问题").click();
  await until(() => document.querySelector('[role="alert"]'), "empty nonterminal issue page accepted"); check(document.querySelectorAll("[data-creation-issue]").length === 2, "bad page discarded actual summary"); cases.push("empty nonterminal page rejected");

  gate = deferred(); pageHandler = async () => gate.promise; renderScope(recentIssues, 10); await tick(); button("查看更多问题").click(); await tick();
  const pageCalls = calls.pages.length; const other = makeIssue("other-project-issue", { summary: "另一作品的独立问题。" }); renderScope(summary([other]), 10, "project-two", "other-project-issue");
  gate.resolve({ entries: allIssues.slice(0, 4).map(metadata), totalCount: 9, nextCursor: 44 }); await tick(); await tick();
  check(document.querySelectorAll("[data-creation-issue]").length === 1 && issueNode("other-project-issue") && calls.pages.length === pageCalls && !document.querySelector('[role="alert"]'), "late page crossed current project scope"); cases.push("changed project ignores late page");
  gate = deferred(); pageHandler = async () => gate.promise; renderScope(recentIssues, 11, "project-one", "missing-reference"); await tick(); button("查看相关问题").click(); await tick();
  const unmountCalls = calls.pages.length; root.unmount(); await tick(); root = createRoot(document.getElementById("root")!); renderScope(summary([other]), 12, "project-two", "other-project-issue");
  gate.resolve({ entries: allIssues.slice(0, 4).map(metadata), totalCount: 9, nextCursor: 44 }); await tick(); await tick();
  check(calls.pages.length === unmountCalls && issueNode("other-project-issue"), "unmounted reference continued fetching old project pages"); cases.push("unmounted reference stops page chain");
  renderScope({ ...summary([other]), entries: [{ ...other, status: "pending" }] } as unknown as CreationIssueSummary, 13); await tick();
  check(document.querySelector('[role="alert"]')?.textContent?.includes("问题记录无法核对") && !document.querySelector("[data-creation-issue]"), "unknown state silently became open or solved");
  const refresh = calls.refresh; button("重新读取任务").click(); check(calls.refresh === refresh + 1, "malformed current record had no refresh"); cases.push("malformed summary retry");
  const goodReport = report; report = () => { const value = goodReport(); value.assessment.outcomes[0].issue!.issueId = "foreign-unprojected-issue"; return value; };
  renderWork(); await tick(); const failedDetails = document.querySelector<HTMLDetailsElement>(".revision-outcome-details")!; failedDetails.querySelector<HTMLElement>("summary")!.click();
  await until(() => failedDetails.querySelector('[role="alert"]'), "unknown issue reference accepted in independent report"); check(!failedDetails.querySelector('[aria-label="逐项修改结果"]'), "invalid report facts rendered as verified"); report = goodReport; cases.push("unknown report reference rejected");
  document.documentElement.dataset.theme = "dark"; document.getElementById("root")!.style.width = "300px";
  pageHandler = nativeViews;
  renderScope(summary([makeIssue("issue-timeline", { summary: "核对长篇前后文与当前章节的行动顺序，保留准确的原文证据和检查观点。" })]), 14); await tick(); await openIssue();
  const bounds = issueNode().getBoundingClientRect();
  check(bounds.width <= 300 && issueNode().scrollWidth <= 300 && [...issueNode().querySelectorAll<HTMLDetailsElement>("details")].every(detail => !detail.open), "narrow dark reading leaked technical details or overflowed"); cases.push("dark narrow reading");
  window.testResult = { passed: true, cases, calls };
} catch (error) { window.testResult = { passed: false, error: error instanceof Error ? error.stack : String(error), cases, calls, dom: document.body.innerHTML }; } })();
