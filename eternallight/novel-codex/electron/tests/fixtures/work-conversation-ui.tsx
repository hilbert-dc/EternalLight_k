import { emptyCreationIssueSummary, deliveryIssuesFixture } from "../helpers/creation-issues";
import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { WorkTaskPanel } from "../../../src/WorkTaskPanel";
import { createAgentRun, summarizeAgentRun } from "../../../src/agent-events";
import { ConversationTimeline } from "../../../src/ConversationTimeline";

const tick = () => new Promise(resolve => setTimeout(resolve, 25));
const check = (value, message) => { if (!value) throw Error(message); };
const until = async predicate => { for (let i = 0; i < 100 && !predicate(); i++) await tick(); check(predicate(), "Conversation did not settle: " + String(predicate)); };
const calls = [];
let answerState = "recorded", wrongOwner = false, held = false;
const delayed = [];
const receipt = (projectId, taskId, text = "## 第二章已保存\n\n雾中传来了第二个人的脚步。") => ({ projectId, taskId, state: answerState,
  binding: { threadId: wrongOwner ? "wrong-thread" : "thread-a", turnId: "writer-turn", intentRevision: 0, itemId: "answer", text, truncated: answerState === "truncated" },
  sourceDigest: "source", receiptDigest: "receipt", recordedAt: 20 });
const api = { runtime: { getTaskFinalReply: async (projectId, taskId) => {
  calls.push({ projectId, taskId });
  if (held) return new Promise(resolve => delayed.push({ projectId, taskId, resolve }));
  return answerState === "missing" ? null : receipt(projectId, taskId);
} } };
const attempt = (id, turnId, startedAt = 10) => ({ id, threadId: "thread-a", turnId, startedAt, completedAt: startedAt + 1, attemptNumber: 1, status: "completed", error: null });
const step = (id, operation, activeAttempt) => ({ id, operation, target: {}, status: "completed", attempts: [activeAttempt], changes: [], outputRefs: [], readingProgress: null });
const work = { id: "work-a", projectId: "A", threadId: "thread-a", goal: "作者原话", kind: "direct", status: "completed", version: 1,
  error: null, resumeMode: null, resumeBlockReason: null, pendingContinuation: false, deliveryStatus: "ready_for_review", deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], evidence: [], pendingActionIds: [], unsettledAttemptIds: [] },
  steps: [step("knowledge", "modelKnowledge", attempt("knowledge-attempt", "knowledge-turn", 8)),
    { ...step("writer", "advisory", attempt("writer-attempt", "writer-turn")), currentAttemptId: "writer-attempt", target: { workDecision: { workTaskId: "work-a" } } }] };
const summary = (id, turnId, prompt = "作者原话", time = 10000) => ({ id, threadId: "thread-a", turnId, prompt, answer: "实时历史回复", summary: "", state: "completed", mode: "novel-agent-engine", artifactCount: 0, createdAt: time });
const content = run => ({ threadId: "thread-a", run, renderCurrent: ({ taskBound }) => <div className="fixture-current">{!taskBound && <article className="user-message"><p>{run.prompt}</p></article>}<article className="assistant-message">实时回复</article></div>,
  renderHistory: (entry, { taskBound }) => <div className="fixture-earlier">{!taskBound && <article className="user-message"><p>{entry.prompt}</p></article>}{entry.displayExecution && <ConversationTimeline run={{ ...entry, ...entry.displayExecution }} retained={entry.displayExecution} />}<article className="assistant-message">{entry.answer}</article></div> });
const root = createRoot(document.getElementById("root"));
const render = async ({ projectId = "A", tasks = [work], conversation = null, ...extra } = {}) => {
  root.render(<div className="conversation-task-records" style={{ width: 300 }}><WorkTaskPanel api={api} projectId={projectId} tasks={tasks.map(task => ({ ...task, projectId }))} conversation={conversation}
    activeCount={0} nextCursor={null} loadingMore={false} loaded reading={false} readError={null} onRetry={() => {}} revision={1} busy={false}
    agentRunning={false} dirty={false} onUndo={() => { throw Error("Rendering undid content"); }} onLoadMore={() => {}} {...extra} /></div>);
  await tick(); await tick();
};
const visible = () => document.body.innerText;
async function run() {
  const live = { ...createAgentRun("内部分析指令"), state: "completed", threadId: "thread-a", turnId: "writer-turn", startedAt: 10000 };
  await render({ conversation: content(live) });
  check(!visible().includes("内部分析指令") && visible().split("作者原话").length - 1 === 1 && visible().split("实时回复").length - 1 === 1 && calls.length === 0, "Live round duplicated its persisted request or final reply");
  check(!document.querySelector(".work-task-execution") && document.querySelectorAll(".work-task-row").length === 1, "A unified turn invented a task history panel");
  await render({ conversation: content({ ...live, turnId: "knowledge-turn" }) });
  await until(() => document.querySelector(".saved-task-reply .conversation-answer"));
  check(visible().includes("实时回复") && visible().includes("第二章已保存"), "Intermediate reply displaced the final coordinating reply");
  check(!document.querySelector(".fixture-current").closest("details"), "Intermediate public reply was hidden");
  calls.length = 0;
  const attemptBound = { ...live, state: "running", taskId: "writer-attempt" };
  const noTurnWork = { ...work, status: "running", steps: [{ ...work.steps[1], status: "running",
    attempts: [{ ...work.steps[1].attempts[0], turnId: null, status: "running" }] }] };
  await render({ tasks: [noTurnWork], conversation: content(attemptBound) });
  check(visible().split("作者原话").length - 1 === 1 && !visible().includes("内部分析指令"), "Active attempt without persisted turn duplicated author input");
  answerState = "absent";
  await render({ tasks: [{ ...noTurnWork, status: "cancelled", steps: [{ ...noTurnWork.steps[0], status: "cancelled",
    attempts: [{ ...noTurnWork.steps[0].attempts[0], status: "cancelled" }] }] }], conversation: content({ ...attemptBound, state: "cancelled" }) });
  check(visible().split("作者原话").length - 1 === 1 && !visible().includes("内部分析指令")
    && document.querySelectorAll(".fixture-current").length === 1 && calls.length === 1 && calls[0].taskId === "writer-attempt", "Cancellation lost attempt identity or duplicated the author bubble");
  const stopped = summarizeAgentRun({ ...attemptBound, state: "cancelled", answer: "", steps: [
    { id: "read", kind: "tool", title: "读取本章正文", status: "completed" },
    { id: "write", kind: "tool", title: "修改本章正文", status: "cancelled" },
  ] }, 15000);
  await render({ tasks: [{ ...noTurnWork, status: "cancelled", steps: [{ ...noTurnWork.steps[0], status: "cancelled",
    attempts: [{ ...noTurnWork.steps[0].attempts[0], status: "cancelled" }] }] }], conversation: content({ ...createAgentRun(), history: [stopped] }) });
  const retained = document.querySelector(".fixture-earlier .conversation-timeline");
  check(retained && !retained.closest(".work-task-execution") && retained.tagName === "SECTION" && calls.length === 2 && calls.every(call => call.taskId === "writer-attempt")
    && visible().split("作者原话").length - 1 === 1, "Historical cancellation lost its top-level process or reread a final reply");
  retained.querySelector("summary").click(); await tick();
  check(visible().includes("读取本章正文") && visible().includes("已停止"), "Historical cancellation erased completed and stopped steps");
  answerState = "recorded";
  const different = { ...live, turnId: "different-turn", startedAt: 15000 };
  await render({ conversation: content(different) });
  await until(() => visible().includes("第二章已保存"));
  check(calls.at(-1).taskId === "writer-attempt" && !calls.some(call => call.taskId === "knowledge-attempt"), "Knowledge analysis replaced the author's main reply");
  check(document.querySelectorAll(".fixture-current").length === 1 && document.querySelectorAll(".saved-task-reply").length === 1, "Different turn was incorrectly merged");
  const restored = { ...createAgentRun(), history: [{ ...summary("history-a", "writer-turn"), answer: "第二章已保存" }] };
  const readsBeforeHistory = calls.length;
  await render({ conversation: content(restored) });
  check(visible().split("作者原话").length - 1 === 1 && visible().split("第二章已保存").length - 1 === 1
    && calls.length === readsBeforeHistory && !document.querySelector(".saved-task-reply"), "Exact local history duplicated its author input or final reply");
  const position = selector => document.querySelector(selector).getBoundingClientRect().top;
  check(position(".work-task-row > .user-message") < position(".work-task-status")
    && !document.querySelector(".fixture-earlier").closest(".work-task-execution"), "Local conversation history was hidden inside the business workflow");
  const middle = summary("history-middle", "unmatched-turn", "中间的独立讨论", 11000);
  await render({ conversation: content({ ...restored, history: [middle, ...restored.history] }) });
  check(visible().indexOf("作者原话") < visible().indexOf("中间的独立讨论"), "History and work were not chronological");
  check(document.querySelectorAll(".work-task-panel > summary, .work-task-history").length === 0, "Old nested history navigation returned");
  check(document.querySelector(".work-task-panel").scrollWidth <= document.querySelector(".work-task-panel").clientWidth + 1, "300px conversation overflowed horizontally");
  const timelineSnapshot = { turns: document.querySelectorAll(".work-task-row, .fixture-earlier").length - restored.history.length,
    mainReply: "第二章已保存", processCollapsed: !document.querySelector(".work-task-execution"), authorInput: "作者原话", chronological: true, duplicateLiveReply: false };

  const states = [];
  for (const state of ["truncated", "absent", "missing"]) {
    answerState = state;
    await render({ projectId: state });
    await until(() => !visible().includes("正在读取保存的回复"));
    const reply = document.querySelector(".saved-task-reply").innerText;
    check(state === "truncated" ? reply.includes("部分内容") && reply.includes("第二章已保存") : !reply.includes("第二章已保存"), "Absent or partial receipt was misrepresented as a complete reply");
    states.push({ state, visible: reply.replace(/\s+/g, " ") });
  }
  answerState = "recorded"; wrongOwner = true;
  await render({ projectId: "bad-owner" });
  await until(() => visible().includes("回复归属不匹配"));
  check(!visible().includes("第二章已保存"), "Wrong-thread reply leaked into the conversation");
  wrongOwner = false;
  const beforeRetry = calls.length;
  [...document.querySelectorAll("button")].find(button => button.textContent === "重新读取回复").click();
  await until(() => visible().includes("第二章已保存"));
  check(calls.length === beforeRetry + 1, "Read retry replayed or duplicated execution");

  held = true;
  await render({ projectId: "A-held" }); const oldA = delayed.at(-1);
  await render({ projectId: "B-held" }); const oldB = delayed.at(-1);
  await render({ projectId: "A-held" }); const newA = delayed.at(-1);
  newA.resolve(receipt("A-held", "writer-attempt", "新的 A 回复")); await tick();
  oldA.resolve(receipt("A-held", "writer-attempt", "旧的 A 回复"));
  oldB.resolve(receipt("B-held", "writer-attempt", "旧的 B 回复")); await tick(); await tick();
  check(visible().includes("新的 A 回复") && !visible().includes("旧的 A 回复") && !visible().includes("旧的 B 回复"), "Late A-B-A reply changed the new conversation lifetime");
  held = false;
  const analysisWork = { ...work, steps: [step("discussion", "advisory", attempt("discussion-attempt", "discussion-turn")),
    step("analysis", "modelKnowledge", attempt("analysis-attempt", "writer-turn", 12))] };
  await render({ tasks: [analysisWork], conversation: content({ ...live, turnId: "discussion-turn" }) });
  await until(() => document.querySelector(".saved-task-reply .conversation-answer"));
  check(calls.at(-1).taskId === "analysis-attempt" && !document.querySelector(".saved-task-reply").closest("details"),
    "Completed requested analysis stayed hidden behind its earlier advisory reply");
  timelineSnapshot.analysisResult = document.querySelector(".saved-task-reply .conversation-answer").textContent.trim();
  root.unmount();
  return { passed: true, timelineSnapshot, states };
}
async function runDeliveryReview() {
  const deliveryReviewStates = [];
  const readsBeforeReview = calls.length;
  for (const [goalStatus, label, unresolved, authorDecisions] of [
    ["partial", "已保存成果，目标尚未全部完成", ["第三章结尾尚未补齐。"], []],
    ["needsAuthorDecision", "已保存成果，需你决定后续安排", [], ["是否让主角在本章发现寄信人的身份？"]],
    ["fulfilled", "结果已保存", [], []],
  ]) {
    const issueData = deliveryIssuesFixture(unresolved);
    const deliveryReview = { expectedWorkVersion: 9, expectedSourceDigest: "private-source-digest",
      goalStatus, goalAssessment: "已核对本轮保存的正文。", checkedScope: "本轮三个章节的正文与情节衔接。",
      unresolved: issueData.unresolved, issueResolutions: [], authorDecisions, evidence: [] };
    await render({ tasks: [{ ...work, id: `review-${goalStatus}`, steps: [], deliveryReport: { chapterAcceptanceReports: [], ...work.deliveryReport, issues: issueData.issues, deliveryReview } }] });
    const header = document.querySelector(".work-task-status").textContent.trim();
    check(header === label, `Goal review was misrepresented: ${header}`);
    check(!document.querySelector(".work-task-execution"), "A second execution list returned");
    const review = document.querySelector('[aria-label="本轮目标核对"]');
    check(goalStatus === "fulfilled" ? !review : review && review.innerText.includes(deliveryReview.goalAssessment)
      && review.innerText.includes(`检查范围：${deliveryReview.checkedScope}`), "Review summary missing or successful turn unnecessarily expanded");
    for (const item of [...unresolved, ...authorDecisions]) check(visible().includes(item), "Actionable review finding was hidden");
    check(!/goalStatus|authorDecisions|expectedSourceDigest|private-source-digest|needsAuthorDecision|expectedWorkVersion/.test(visible()), "Review leaked technical field names or binding data");
    check(document.querySelector(".work-task-panel").scrollWidth <= document.querySelector(".work-task-panel").clientWidth + 1, "Review overflowed the 300px conversation");
    deliveryReviewStates.push({ goalStatus, header, unresolvedVisible: unresolved.every(item => visible().includes(item)),
      authorDecisionsVisible: authorDecisions.every(item => visible().includes(item)), reviewVisible: Boolean(review) });
  }
  check(calls.length === readsBeforeReview, "Review rendering replayed a task or fetched unrelated replies");
  await render({ tasks: [{ ...work, steps: [], pendingContinuation: true }] });
  check(!document.querySelector(".work-task-controls") && !visible().includes("接回原任务"), "Task card duplicated composer controls");
  root.unmount();
  return { passed: true, deliveryReviewStates };
}
(process.env.NOVEL_DELIVERY_REVIEW_UI === "1" ? runDeliveryReview() : run()).then(result => { window.testResult = result; }).catch(error => { window.testResult = { passed: false, error: error.stack + "\nDOM:" + document.body.innerText }; });
