import { emptyCreationIssueSummary } from "../helpers/creation-issues";
import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { WorkTaskPanel } from "../../../src/WorkTaskPanel";
import { CommitmentTaskResult } from "../../../src/CommitmentTaskResult";
import { createAgentRun } from "../../../src/agent-events";

const tick = () => new Promise(resolve => setTimeout(resolve, 25));
const check = (value, message) => { if (!value) throw Error(message); };
const until = async (predicate, message) => {
  for (let index = 0; index < 120 && !predicate(); index++) await tick();
  check(predicate(), message);
};
const root = createRoot(document.getElementById("root"));
const reads = [], writes = [], opened = [];
const task = (id, chapterNumber, referenceId, deliveryState = "recorded", projectId = "A", threadId = "thread-a") => {
  const attemptId = `${id}-attempt`, stepId = `${id}-step`;
  return { id, projectId, threadId, goal: `提取第 ${chapterNumber} 章的伏笔建议`, kind: "direct", status: "completed",
    version: 1, error: null, resumeMode: null, resumeBlockReason: null, deliveryStatus: "ready_for_review",
    deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], pendingActionIds: [], unsettledAttemptIds: [], evidence: [
      { stepId, attemptId, kind: "commitmentExtraction", referenceId, state: deliveryState },
    ] }, steps: [{ id: stepId, operation: "extractCommitments", target: { chapterId: `chapter-${chapterNumber}`, chapterNumber,
      sourceRevisionId: `source-${chapterNumber}` }, expectedOutputKind: "commitmentExtraction", instruction: null,
      status: "completed", currentAttemptId: attemptId, readingProgress: null, outputRefs: [], changes: [],
      attempts: [{ id: attemptId, attemptNumber: 1, threadId, turnId: `${id}-turn`, status: "completed", error: null,
        startedAt: chapterNumber, completedAt: chapterNumber + 1 }] }] };
};
const oldTask = task("old-seven", 7, "report-seven-old", "superseded");
const currentTask = task("current-seven", 7, "report-seven-current");
const chapterEightTask = task("current-eight", 8, "report-eight-current");
const ninth = task("multi-nine", 9, "report-multi-nine"), tenth = task("multi-ten", 10, "report-multi-ten");
const multiChapterTask = { ...ninth, id: "multi-chapter", goal: "提取第 9—10 章的伏笔建议", steps: [...ninth.steps, ...tenth.steps],
  deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], ...ninth.deliveryReport, evidence: [...ninth.deliveryReport.evidence, ...tenth.deliveryReport.evidence] } };
const otherThreadTask = task("other-thread", 9, "report-other-thread", "recorded", "A", "thread-b");
const otherProjectTask = task("other-project", 10, "report-other-project", "recorded", "B", "thread-a");
const allTasks = [oldTask, currentTask, chapterEightTask, multiChapterTask, otherThreadTask, otherProjectTask];
const proposal = (id, number, summary) => ({ id, kind: "foreshadowing", summary,
  fulfillmentCriteria: "后文交代这一线索的真实含义，并让人物的选择回应此处铺垫。",
  relatedEntityIds: [], plotThreadId: null, characterArcId: null, status: "open", decisionReason: null,
  targetWindow: { kind: "open" }, fulfillmentEvidenceRefs: [], setupEvidenceRefs: [
    { objectId: `draft-chapter-${number}`, revisionId: `source-${number}`, quote: "她在旧灯下停住，信封上的墨迹尚未干透。", start: 0, end: 23 },
  ] });
const summaries = {
  "report-seven-old": ["旧报告：信封上的半枚灯印"],
  "report-seven-current": ["灯印只在税册到期前夜出现", "姐姐留下的灯芯与失踪者有关"],
  "report-eight-current": ["第八章：船夫避开了旧渡口"],
  "report-multi-nine": ["第九章：铜钥匙只能开启内门"],
  "report-multi-ten": ["第十章：灯塔地下藏着旧税册"],
  "report-other-thread": ["另一对话独有的伏笔"],
  "report-other-project": ["另一作品独有的伏笔"],
};
const reports = Object.fromEntries(allTasks.flatMap(work => work.steps.map(step => {
  const referenceId = work.deliveryReport.evidence.find(item => item.stepId === step.id).referenceId, number = step.target.chapterNumber;
  return [referenceId, { revisionId: referenceId, snapshotId: `snapshot-${referenceId}`, projectRevision: 1,
    authority: "candidateProposalBatch", sourceCurrent: true, reportCurrent: referenceId !== "report-seven-old", established: [],
    payload: { chapterId: step.target.chapterId, sourceRevisionId: step.target.sourceRevisionId, examinedRanges: [{ start: 0, end: 23 }],
      candidates: summaries[referenceId].map((summary, index) => proposal(`${referenceId}-${index}`, number, summary)) },
    coverage: { semanticCompleteness: "unknown", modelReportedRanges: true, literaryFulfillmentVerified: false, globalCompletenessClaimed: false } }];
})));
let holdLatest = false;
const pendingLatest = [], latestReads = [];
const api = { runtime: {
  getTaskFinalReply: async (projectId, taskId) => {
    const work = allTasks.find(work => work.steps.some(step => step.currentAttemptId === taskId));
    const step = work.steps.find(step => step.currentAttemptId === taskId);
    return { projectId, taskId, state: "recorded", recordedAt: 1, binding: {
      threadId: work.threadId, turnId: step.attempts[0].turnId,
      text: `已阅读第 ${step.target.chapterNumber} 章并保存伏笔建议。你可以在下面选择希望保留的线索，原文依据可展开查看。`,
    } };
  },
  getCommitmentExtraction: async (projectId, chapterId, reportRevisionId) => {
    reads.push({ projectId, chapterId, reportRevisionId });
    if (!reportRevisionId && holdLatest) {
      latestReads.push({ projectId, chapterId });
      const currentReport = Object.values(reports).find(report => report.reportCurrent && report.payload.chapterId === chapterId);
      return new Promise(resolve => pendingLatest.push(() => resolve(structuredClone(currentReport))));
    }
    check(reportRevisionId && reports[reportRevisionId], "Inline result read latest or an unknown report instead of the exact revision");
    return structuredClone(reports[reportRevisionId]);
  },
  getCommitment: async () => { throw Error("Fixture has no established commitments to inspect"); },
  establishCommitment: async (...args) => { writes.push({ kind: "establish", args }); throw Error("Rendering wrote a commitment"); },
  createCommitmentReview: async (...args) => { writes.push({ kind: "review", args }); throw Error("Rendering created a review"); },
  acceptReview: async (...args) => { writes.push({ kind: "accept", args }); throw Error("Rendering accepted a review"); },
} };
let projectId = "A", threadId = "thread-a", selectedChapter = "chapter-7", refreshToken = 0, tasks = allTasks, useDefaultSlot = false;
const render = async () => {
  const run = { ...createAgentRun(""), state: "completed", threadId: null, turnId: null };
  root.render(<aside className="conversation-rail" data-selected-chapter={selectedChapter} style={{ width: 380 }}>
    <div className="conversation-rail-scroll" style={{ height: 760, maxHeight: 760, overflowY: "auto" }}>
      <WorkTaskPanel api={api} projectId={projectId} tasks={tasks} activeCount={0} nextCursor={null} loadingMore={false} loaded
        reading={false} readError={null} onRetry={() => {}} revision={refreshToken + 1} busy={false}
        agentRunning={false} dirty={false}
        onOpenResult={result => opened.push(result)} onLoadMore={() => {}}
        onFormalChange={async () => {}} onAdjustCommitment={() => { throw Error("Rendering opened an editor"); }}
        conversation={{ threadId, run, renderCurrent: () => null, renderHistory: () => null }}
        renderCommitmentResult={useDefaultSlot ? undefined : result => <CommitmentTaskResult key={`${result.stepId}:${result.attemptId}:${result.referenceId}`}
          api={api} result={result} disabled={false} refreshToken={refreshToken} onFormalChange={async () => {}}
          onAdjust={() => { throw Error("Rendering opened an editor"); }} />} />
    </div>
  </aside>); await tick(); await tick();
};
const card = id => document.querySelector<HTMLElement>(`[data-commitment-report="${id}"]`);
const rail = () => document.querySelector<HTMLElement>(".conversation-rail-scroll");
const ready = async count => until(() => document.querySelectorAll(".commitment-task-result .commitment-task-proposal").length === count
  && !document.body.innerText.includes("正在读取保存的回复"), "Bound inline reports or saved replies did not settle");
const body = () => document.body.textContent;

(async () => {
  try {
    await render(); await ready(6);
    for (const [work, referenceId] of [[oldTask, "report-seven-old"], [currentTask, "report-seven-current"], [chapterEightTask, "report-eight-current"]]) {
      const element = card(referenceId), row = element.closest("[data-work-task-id]");
      check(row?.getAttribute("data-work-task-id") === work.id, "A report was placed under a different task");
      check(!element.closest(".work-task-execution"), "Author choices were hidden inside execution details");
      const answer = row.querySelector(".saved-task-reply");
      check(answer && Boolean(answer.compareDocumentPosition(element) & Node.DOCUMENT_POSITION_FOLLOWING), "Author choices were not after their task reply");
    }
    check(card("report-seven-old").textContent.includes("旧报告：信封上的半枚灯印")
      && !card("report-seven-old").textContent.includes("灯印只在税册"), "The newer report replaced the original task’s suggestions");
    check([...card("report-seven-old").querySelectorAll<HTMLButtonElement>("button")].filter(button => /确认保留|调整后再决定/.test(button.textContent)).every(button => button.disabled),
      "Superseded report allowed a fresh author write");
    check([...card("report-seven-current").querySelectorAll<HTMLButtonElement>("button")].some(button => button.textContent.includes("确认保留") && !button.disabled),
      "A current exact report could not be chosen in the conversation");
    check(!card("report-other-thread") && !card("report-other-project") && !body().includes("另一对话独有") && !body().includes("另一作品独有"),
      "Other conversation or project suggestions leaked into this conversation");
    check(!reads.some(read => read.reportRevisionId === "report-other-thread" || read.reportRevisionId === "report-other-project"), "Hidden foreign tasks still loaded suggestions");
    check(card("report-multi-nine").closest("[data-work-task-id]")?.getAttribute("data-work-task-id") === "multi-chapter"
      && card("report-multi-ten").closest("[data-work-task-id]")?.getAttribute("data-work-task-id") === "multi-chapter"
      && card("report-multi-nine").textContent.includes("铜钥匙") && !card("report-multi-nine").textContent.includes("旧税册")
      && card("report-multi-ten").textContent.includes("旧税册"), "One multi-chapter task mixed chapter-specific reports");
    check(!document.querySelector(".conversation-chapter-results, .commitment-panel"), "The bottom global chapter panel was rendered with task choices");

    selectedChapter = "chapter-42"; await render(); await ready(6);
    check(card("report-seven-current") && card("report-eight-current"), "Selecting another editor chapter changed task-bound choices");
    const evidence = card("report-seven-current").querySelector<HTMLDetailsElement>(".commitment-task-proposal details");
    check(evidence && !evidence.open, "Evidence did not start collapsed");
    evidence.querySelector<HTMLElement>("summary").click(); await until(() => evidence.open, "Evidence did not open");
    rail().dispatchEvent(new WheelEvent("wheel", { deltaY: -80, bubbles: true })); rail().scrollTop = 140;
    rail().dispatchEvent(new Event("scroll")); await tick(); await tick();
    const top = rail().scrollTop;
    refreshToken += 1; tasks = tasks.map(work => ({ ...work, version: work.version + 1 })); await render(); await ready(6); await tick(); await tick();
    check(card("report-seven-current").querySelector<HTMLDetailsElement>(".commitment-task-proposal details").open,
      "Refreshing the reports lost the author’s expanded evidence");
    check(Math.abs(rail().scrollTop - top) < 3, "Refreshing the reports moved the author’s reading position");
    check(writes.length === 0 && opened.length === 0, "Loading, chapter selection or refresh performed a write or navigation");

    tasks = [currentTask]; await render(); await ready(2);
    rail().scrollTop = 0; rail().dispatchEvent(new Event("scroll"));
    window.testViewport = { width: 450, height: 900 };
    for (let index = 0; index < 100 && window.testViewport; index++) await tick();
    window.testScreenshot = "conversation-inline-commitment-choices";
    for (let index = 0; index < 100 && window.testScreenshot; index++) await tick();
    check(!window.testScreenshot, "Visual fixture screenshot did not settle");

    tasks = allTasks; threadId = "thread-b"; await render(); await ready(1);
    check(card("report-other-thread") && !card("report-seven-current") && !card("report-eight-current"), "Switching conversation retained another conversation’s card");
    projectId = "B"; threadId = "thread-a"; await render(); await ready(1);
    check(card("report-other-project") && !card("report-other-thread") && !card("report-seven-current"), "Switching project retained another project’s card");
    check(writes.length === 0, "Changing conversation or project caused an author write");
    projectId = "A"; threadId = "thread-a"; tasks = [currentTask, chapterEightTask]; useDefaultSlot = true;
    await render(); await ready(3);
    const first = [...card("report-seven-current").querySelectorAll<HTMLButtonElement>("button")].find(button => button.textContent.includes("确认保留"));
    const second = [...card("report-eight-current").querySelectorAll<HTMLButtonElement>("button")].find(button => button.textContent.includes("确认保留"));
    check(!first.disabled && !second.disabled, "Current reports did not begin ready for a real author decision");
    holdLatest = true;
    first.click(); second.click();
    await until(() => latestReads.length === 1, "The default host did not begin one author decision"); await tick();
    check(latestReads[0].chapterId === "chapter-7" && pendingLatest.length === 1,
      "Two cards entered confirmation together before the host’s synchronous lock");
    check(first.disabled && (second.disabled || card("report-eight-current").textContent.includes("另一条建议正在处理")),
      "The real default host did not mark competing author interactions unavailable");
    check(writes.length === 0, "A pending source re-read performed an author write");
    threadId = "thread-b"; tasks = allTasks; await render(); await ready(1);
    check(!card("report-seven-current") && card("report-other-thread"), "Pending author decision retained its old conversation card");
    pendingLatest[0](); await tick(); await tick(); await tick();
    check(writes.length === 0, "A late source read continued writing after conversation ownership changed");
    const next = [...card("report-other-thread").querySelectorAll<HTMLButtonElement>("button")].find(button => button.textContent.includes("确认保留"));
    check(next && !next.disabled && latestReads.length === 1, "Leaving the pending decision retained a stale cross-card lock");
    window.testResult = { passed: true, checks: ["reply placement", "exact same-chapter history", "multi-chapter", "superseded readonly", "editor chapter independent", "conversation and project isolation", "refresh reading and disclosure", "no render writes", "default synchronous cross-card lock"] };
  } catch (error) { window.testResult = { passed: false, error: `${error.stack}\nDOM:${document.body.innerHTML}` }; }
})();
