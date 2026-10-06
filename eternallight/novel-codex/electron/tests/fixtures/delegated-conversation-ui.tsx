import React from "react";
import { createRoot } from "react-dom/client";
import { WorkTaskPanel } from "../../../src/WorkTaskPanel";
import { createAgentRun } from "../../../src/agent-events";
import { ConversationTimeline } from "../../../src/ConversationTimeline";
import "../../../src/styles.css";

const tick = () => new Promise(resolve => setTimeout(resolve, 25));
const check = (ok, message) => { if (!ok) throw Error(message); };
let renderError, phase = "initialization";
window.addEventListener("error", event => { renderError = event.error ?? event.message; });
const until = async (predicate, message) => { for (let i = 0; i < 150 && !predicate(); i++) { if (renderError) throw renderError; await tick(); } check(predicate(), message); };
const attempt = (id, turnId, startedAt) => ({ id, threadId: "discussion-thread", turnId, status: "completed", startedAt, completedAt: startedAt + 1, attemptNumber: 1, error: null });
const advisory = attempt("advisory-attempt", "advisory-turn", 10), writer = attempt("writer-attempt", "writer-turn", 12);
const step = (id, operation, value, sourceTaskId = null, target = {}) => ({ id, operation, expectedOutputKind: ({ advisory: "completionReport", generateOutline: "outline", generateBrief: "brief", modelKnowledge: "knowledgeBatch" })[operation] ?? null, instruction: null, inputRevision: 1, dependsOnStepId: null, currentAttemptId: value.id, status: "completed", sourceTaskId, target, attempts: [value], changes: [], outputRefs: [], readingProgress: null });
const completion = { stepId: "advisory-step", attemptId: advisory.id, kind: "completionReport", state: "recorded", referenceId: advisory.id };
const work = { id: "work-delegated", projectId: "A", threadId: "discussion-thread", goal: "依据简报生成三卷大纲，保存候选不采用", kind: "direct", status: "completed", version: 1, error: null, resumeMode: null, resumeBlockReason: null,
  deliveryStatus: "ready_for_review", deliveryReport: { evidence: [{ stepId: "outline-step", attemptId: writer.id, kind: "outline", state: "recorded", referenceId: "outline-saved" }, completion], pendingActionIds: [], unsettledAttemptIds: [] },
  steps: [step("advisory-step", "advisory", advisory), step("outline-step", "generateOutline", writer, advisory.id)] };
const intermediate = "已登记生成大纲；内部行动编号仅留在原始讨论记录，不代表保存成功。";
const resultText = "三卷大纲候选已保存，尚未采用。";
const mainText = `${resultText}\n\n${"第一卷围绕来信展开，第二卷检验信任，第三卷收束选择。\n\n".repeat(20)}`;
const reads = [], snapshots = [], opened = [], copied = [];
const replyModes = new Map(), bindings = new Map();
let releaseReply;
Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: async text => { copied.push(text); } } });
let childFailed = false;
const api = { runtime: { getTaskFinalReply: async (projectId, taskId) => {
  reads.push({ projectId, taskId }); const selected = bindings.get(taskId) ?? advisory;
  const mode = replyModes.get(taskId) ?? "recorded";
  if (mode === "error") throw Error("原始读取失败，请稍后重新读取");
  if (mode === "deferred") await new Promise(resolve => { releaseReply = resolve; });
  if (mode === "null") return null;
  const isWorker = selected.operation !== "advisory";
  return { projectId: mode === "foreign" ? "foreign-project" : projectId, taskId,
    state: mode === "absent" || mode === "truncated" ? mode : "recorded",
    binding: { threadId: selected.threadId, turnId: selected.turnId, itemId: "final", intentRevision: 0,
      text: mode === "deferred" ? "旧作品迟到原文，不应进入新作品" : mode === "absent" ? "" : isWorker ? childFailed ? "未能保存大纲候选，原内容保持。" : mainText : intermediate,
      truncated: mode === "truncated" || mode === "binding-truncated" }, recordedAt: 30, sourceDigest: "source", receiptDigest: "receipt" };
} } };
const live = { ...createAgentRun(work.goal), state: "completed", taskId: advisory.id, threadId: advisory.threadId, turnId: advisory.turnId, startedAt: 10000, completedAt: 11000, answer: intermediate, steps: [{ id: "read", kind: "tool", title: "读取创作简报", status: "completed" }], summary: "安排完成" };
const content = run => ({ run,
  renderCurrent: ({ taskBound }) => <section className="fixture-current">{!taskBound && <p>{run.prompt}</p>}<ConversationTimeline run={run} /><p className="intermediate-reply">{run.answer}</p>{run.error && <p role="alert">{run.error}</p>}</section>,
  renderHistory: (entry, { taskBound }) => <section className="fixture-history">{!taskBound && <p>{entry.prompt}</p>}<p className="intermediate-reply">{entry.answer}</p>{(entry.error || entry.state === "error") && <p role="alert">原讨论失败，需处理</p>}</section>,
});
const root = createRoot(document.getElementById("root"));
const render = async (task = work, run = live, width = 340, projectId = "A") => {
  for (const item of task.steps) for (const attempt of item.attempts) bindings.set(attempt.id, { ...attempt, operation: item.operation });
  root.render(<div className="conversation-rail-scroll" style={{ width, height: 260, overflowY: "auto", position: "relative" }}><WorkTaskPanel api={api} projectId={projectId} tasks={[task]} conversation={content(run)} activeCount={0} nextCursor={null} loadingMore={false} loaded reading={false} readError={null} onRetry={() => {}} revision={1} busy={false} interruptionBusy={false} agentRunning={false} agentAvailable dirty={false} onControl={() => { throw Error("Rendering executed a task"); }} onLoadMore={() => {}} onOpenResult={result => opened.push(result)} /></div>);
  await tick(); await tick();
};
const flow = () => document.querySelector<HTMLDetailsElement>(".work-task-execution");
const mainReply = () => document.querySelector<HTMLElement>(".saved-task-reply");
const description = () => document.querySelector<HTMLDetailsElement>(".generated-reply-description");
const currentReply = () => document.querySelector<HTMLElement>(".fixture-current, .fixture-history");
const scoped = (name, projectId = "A") => {
  const cloned = structuredClone(work); cloned.id = `work-${name}`; cloned.projectId = projectId;
  const renamed = new Map();
  for (const item of cloned.steps) for (const old of item.attempts) { renamed.set(old.id, `${name}-${old.id}`); old.id = renamed.get(old.id); old.threadId = `${name}-thread`; old.turnId = `${name}-${old.turnId}`; item.currentAttemptId = old.id; }
  for (const item of cloned.steps) if (item.sourceTaskId) item.sourceTaskId = renamed.get(item.sourceTaskId);
  for (const item of cloned.deliveryReport.evidence) { item.attemptId = renamed.get(item.attemptId); if (item.kind === "completionReport") item.referenceId = item.attemptId; }
  const local = { ...live, taskId: cloned.steps[0].currentAttemptId, threadId: `${name}-thread`, turnId: cloned.steps[0].attempts[0].turnId };
  return { task: cloned, run: local };
};
const capture = name => snapshots.push({ name, intermediateInFlow: Boolean(currentReply()?.closest(".work-task-execution")), flowOpen: flow().open, mainResult: mainReply()?.innerText.split("\n").filter(Boolean).slice(0, 3).join(" · ") ?? null, authorCount: document.body.innerText.split(work.goal).length - 1, description: Boolean(description()), descriptionOpen: description()?.open ?? null, delivery: document.querySelector(".work-task-status")?.textContent ?? null });
(async () => { try {
  phase = "successful current delegation"; await render(); await until(() => mainReply()?.textContent.includes(resultText), "Actual saved worker reply not read");
  check(currentReply()?.closest(".work-task-execution") && !flow().open && !document.body.innerText.includes(intermediate) && !mainReply().closest("details"), "Current advisory duplicated final output or hid the worker result");
  check(reads.length === 1 && reads[0].taskId === writer.id, "Collapsed intermediate replayed a reply read");
  check(description() && !description().open && !document.body.innerText.includes(resultText), "Verified generated explanation was not folded");
  const resultButton = document.querySelector<HTMLButtonElement>(".task-result-actions button"); resultButton.click();
  check(opened.length === 1 && opened[0].referenceId === "outline-saved" && opened[0].attemptId === writer.id, "Result entry lost canonical identity"); capture(phase);
  description().querySelector<HTMLElement>(":scope > summary").click(); await tick();
  check(document.body.innerText.includes(resultText), "Folded explanation cannot show complete original reply");
  const copy = [...mainReply().querySelectorAll<HTMLButtonElement>("button")].find(button => button.getAttribute("aria-label")?.includes("复制") || button.textContent.includes("复制"));
  check(copy, "Saved explanation has no full-copy entry"); copy.click(); await tick();
  check(copied.at(-1) === mainText, "Copy changed or omitted original saved explanation");
  flow().querySelector<HTMLElement>(":scope > summary").click(); await tick(); await tick();
  check(document.body.innerText.includes(intermediate) && document.querySelectorAll(".intermediate-reply").length === 1 && reads.length === 1, "Expanded process lost original advisory or duplicated its receipt");
  const scroller = document.querySelector<HTMLElement>(".conversation-rail-scroll"); scroller.dispatchEvent(new WheelEvent("wheel", { deltaY: -30, bubbles: true })); scroller.scrollTop = 90; scroller.dispatchEvent(new Event("scroll")); await tick();
  const top = scroller.scrollTop, originalFlow = flow();
  phase = "same work refresh keeps reading"; await render({ ...work, version: 2 });
  check(flow() === originalFlow && flow().open && description()?.open && Math.abs(scroller.scrollTop - top) < 2 && reads.length === 1, "Task refresh reset details, scroll, or the saved reply"); capture(phase);
  phase = "successful history delegation"; const history = { id: "history-advisory", ...live, createdAt: 11000, artifactCount: 0, displayExecution: undefined };
  await render({ ...work, version: 3 }, { ...createAgentRun(), history: [history] });
  check(document.querySelector(".fixture-history")?.closest(".work-task-execution") && flow().open && description()?.open && document.body.innerText.includes(intermediate) && reads.length === 1 && document.body.innerText.split(resultText).length - 1 === 1, "History revived duplicate advisory, lost expansion, or reread the main reply"); capture(phase);
  phase = "mixed historical replacement stays folded"; const historyTop = scroller.scrollTop, originalDescription = description();
  const brief = attempt("brief-attempt", "brief-turn", 11);
  const replaced = { ...work, version: 4, deliveryStatus: "partial",
    steps: [work.steps[0], step("brief-step", "generateBrief", brief, advisory.id), work.steps[1]],
    deliveryReport: { ...work.deliveryReport, evidence: [{ stepId: "brief-step", attemptId: brief.id, kind: "brief", state: "recorded", referenceId: "original-brief" }, { ...work.deliveryReport.evidence[0], state: "superseded" }, completion] } };
  await render(replaced, { ...createAgentRun(), history: [history] });
  check(document.querySelector(".fixture-history")?.closest(".work-task-execution") && flow().open && description() === originalDescription && description().open
    && document.body.innerText.includes("原产物已被后续版本替换") && document.body.innerText.includes("结果已保存，后续已有更新")
    && Math.abs(scroller.scrollTop - historyTop) < 2 && reads.length === 1, "Replacing a historical output revived prose, reset reading, or claimed partial failure");
  const survivingResult = document.querySelector<HTMLButtonElement>(".task-result-actions button"); survivingResult.click();
  check(opened.at(-1).referenceId === "original-brief" && opened.at(-1).attemptId === brief.id && document.querySelectorAll(".task-result-actions button").length === 1, "Superseded output navigated to a new head"); capture(phase);
  phase = "all historical outputs replaced";
  await render({ ...replaced, version: 5, deliveryReport: { ...replaced.deliveryReport, evidence: replaced.deliveryReport.evidence.map(item => item.kind === "completionReport" ? item : ({ ...item, state: "superseded" })) } }, { ...createAgentRun(), history: [history] });
  check(description() === originalDescription && description().open && flow().open && document.body.innerText.includes("结果已保存，后续已有更新")
    && !document.querySelector(".task-result-actions") && Math.abs(scroller.scrollTop - historyTop) < 2 && reads.length === 1, "Fully replaced history lost its explanation or invented a new navigation target");
  [...description().querySelectorAll<HTMLButtonElement>("button")].find(button => button.textContent.includes("复制")).click(); await tick();
  check(copied.at(-1) === mainText, "Historical replacement copy changed the original saved reply"); capture(phase);
  // New task scopes begin folded; failure/approval/identity exclusions must stay inline.
  const cases = [
    ["pure discussion", { ...work, steps: [work.steps[0]], deliveryReport: { ...work.deliveryReport, evidence: [completion] } }, live],
    ["proposed child only", { ...work, status: "running", steps: [work.steps[0], { ...work.steps[1], status: "queued", attempts: [] }], deliveryReport: { ...work.deliveryReport, evidence: [completion], pendingActionIds: ["proposal"] } }, live],
    ["partial unresolved output", { ...work, deliveryStatus: "partial", deliveryReport: { ...work.deliveryReport, evidence: [...work.deliveryReport.evidence, { stepId: "unresolved-step", attemptId: "unresolved-attempt", kind: "outline", state: "missing", referenceId: null }] } }, live],
    ["partial without replacement", { ...work, deliveryStatus: "partial" }, live],
    ["replacement with missing output", { ...work, deliveryStatus: "partial", deliveryReport: { ...work.deliveryReport, evidence: [{ ...work.deliveryReport.evidence[0], state: "superseded" }, { stepId: "unresolved-step", attemptId: "unresolved-attempt", kind: "brief", state: "missing", referenceId: null }, completion] } }, live],
    ["replacement with pending approval", { ...work, deliveryStatus: "partial", deliveryReport: { ...work.deliveryReport, evidence: [{ ...work.deliveryReport.evidence[0], state: "superseded" }, completion], pendingActionIds: ["pending-action"] } }, live],
    ["replacement with unsettled usage", { ...work, deliveryStatus: "partial", deliveryReport: { ...work.deliveryReport, evidence: [{ ...work.deliveryReport.evidence[0], state: "superseded" }, completion], unsettledAttemptIds: [writer.id] } }, live],
    ["missing saved output evidence", { ...work, deliveryReport: { ...work.deliveryReport, evidence: [completion] } }, live],
    ["failed discussion", { ...work, status: "blocked", steps: [{ ...work.steps[0], status: "blocked", attempts: [{ ...advisory, status: "failed", error: "原讨论失败" }] }] }, { ...live, state: "error", error: "原讨论失败" }],
    ["cancelled discussion", { ...work, status: "cancelled", steps: [{ ...work.steps[0], status: "cancelled", attempts: [{ ...advisory, status: "cancelled" }] }] }, { ...live, state: "cancelled" }],
    ["repair approval", { ...work, steps: [{ ...work.steps[0], target: { revisionRepairSource: { writerTaskId: "original-writer" } } }, work.steps[1]] }, live],
    ["foreign child source", { ...work, steps: [work.steps[0], { ...work.steps[1], sourceTaskId: "different-advisory" }] }, live],
    ["conflicting thread", work, { ...live, threadId: "foreign-thread" }],
    ["conflicting turn", work, { ...live, turnId: "foreign-turn" }],
    ["child failed", { ...work, status: "blocked", error: "大纲未能保存", steps: [work.steps[0], { ...work.steps[1], status: "blocked", attempts: [{ ...writer, status: "failed", error: "大纲未能保存" }] }], deliveryReport: { ...work.deliveryReport, evidence: [completion] } }, live],
  ];
  for (const [name, task, run] of cases) {
    phase = name; childFailed = name === "child failed"; await render({ ...task, id: `work-${name}` }, run); await tick();
    check(currentReply() && !currentReply().closest(".work-task-execution") && currentReply().textContent.includes(intermediate), `${name} hid its original reply`);
    if (name === "child failed") await until(() => mainReply()?.textContent.includes("未能保存大纲候选"), "Failed worker reply not visible");
    if (name === "failed discussion" || name === "child failed") check([...document.querySelectorAll('[role="alert"]')].some(node => !node.closest("details")), `${name} hid its error`);
    if (["pure discussion", "proposed child only", "partial unresolved output", "partial without replacement", "replacement with missing output", "replacement with pending approval", "replacement with unsettled usage", "missing saved output evidence", "failed discussion", "cancelled discussion", "repair approval", "child failed"].includes(name)) check(!description(), `${name} hid its reply under a saved-result summary`);
    check(!document.body.innerText.includes("结果已保存，后续已有更新"), `${name} was misclassified as a successful historical replacement`);
    capture(name);
  }
  // A summary never hides a missing, failed, foreign, or partial saved reply.
  childFailed = false;
  for (const mode of ["error", "null", "absent", "truncated", "binding-truncated", "foreign"]) {
    phase = `saved reply ${mode}`; const selected = scoped(mode); replyModes.set(selected.task.steps[1].currentAttemptId, mode);
    await render(selected.task, selected.run); await until(() => !mainReply()?.textContent.includes("正在读取保存"), "Saved reply did not settle");
    check(!description(), `${mode} was hidden behind a successful explanation`);
    const messages = { error: "原始读取失败", null: "没有保存的最终回复", absent: "没有最终文本回复", truncated: "部分内容", "binding-truncated": resultText, foreign: "回复归属不匹配" };
    check(mainReply()?.innerText.includes(messages[mode]) && !mainReply().closest("details"), `${mode} receipt warning or content disappeared`); capture(phase);
    if (mode === "error") {
      const before = reads.length; replyModes.set(selected.task.steps[1].currentAttemptId, "recorded");
      [...mainReply().querySelectorAll<HTMLButtonElement>("button")].find(button => button.textContent === "重新读取回复").click();
      await until(() => Boolean(description()), "Read-only retry did not restore the saved explanation");
      check(reads.length === before + 1 && !description().open, "Read retry did not remain a single read"); capture("saved reply read-only retry");
    }
  }
  for (const mode of ["error", "absent", "truncated"]) {
    phase = `historical replaced reply ${mode}`; const selected = scoped(`historical-${mode}`);
    selected.task.deliveryStatus = "partial"; selected.task.deliveryReport.evidence[0].state = "superseded";
    replyModes.set(selected.task.steps[1].currentAttemptId, mode); await render(selected.task, selected.run);
    await until(() => !mainReply()?.textContent.includes("正在读取保存"), "Historical saved reply did not settle");
    check(!description() && document.body.innerText.includes("结果已保存，后续已有更新") && mainReply()?.innerText.includes({ error: "原始读取失败", absent: "没有最终文本回复", truncated: "部分内容" }[mode]), "Historical replacement status hid the original receipt problem"); capture(phase);
  }
  phase = "read analysis remains public"; const analysis = scoped("read-analysis");
  analysis.task.steps[1].operation = "modelKnowledge"; analysis.task.steps[1].expectedOutputKind = "knowledgeBatch"; analysis.task.steps[1].target = { chapterId: "chapter-current", chapterNumber: 1, sourceRevisionId: "revision-current" };
  analysis.task.deliveryReport.evidence[0].kind = "knowledgeBatch";
  await render(analysis.task, analysis.run); await until(() => mainReply()?.textContent.includes(resultText), "Read analysis reply missing");
  check(!description() && mainReply().innerText.includes(resultText), "Read analysis was collapsed as generated prose"); capture(phase);
  phase = "local current writer explanation"; const local = scoped("local-current"), localWriter = local.task.steps[1].attempts[0];
  const localRun = { ...local.run, taskId: localWriter.id, turnId: localWriter.turnId, answer: mainText };
  const beforeLocal = reads.length; await render(local.task, localRun);
  check(description() && !description().open && currentReply()?.closest(".generated-reply-description") && !mainReply() && reads.length === beforeLocal, "Current exact writer reply was duplicated or left exposed"); capture(phase);
  description().querySelector<HTMLElement>(":scope > summary").click(); await tick();
  const localDetails = description(), localScroller = document.querySelector<HTMLElement>(".conversation-rail-scroll");
  localScroller.dispatchEvent(new WheelEvent("wheel", { deltaY: -30, bubbles: true })); localScroller.scrollTop = 90; localScroller.dispatchEvent(new Event("scroll")); await tick(); const localTop = localScroller.scrollTop;
  phase = "local history writer explanation";
  const localHistory = { id: "local-history", ...localRun, createdAt: 13000, artifactCount: 1 };
  await render({ ...local.task, version: 2 }, { ...createAgentRun(), history: [localHistory] });
  check(description()?.open && document.querySelector(".fixture-history")?.closest(".generated-reply-description") && reads.length === beforeLocal && Math.abs(localScroller.scrollTop - localTop) < 2, "Current to history reset explanation or reread the reply"); capture(phase);
  phase = "local history replacement stays folded";
  await render({ ...local.task, version: 3, deliveryStatus: "partial", deliveryReport: { ...local.task.deliveryReport, evidence: local.task.deliveryReport.evidence.map(item => item.kind === "completionReport" ? item : ({ ...item, state: "superseded" })) } }, { ...createAgentRun(), history: [localHistory] });
  check(description()?.open && document.querySelector(".fixture-history")?.closest(".generated-reply-description") && document.body.innerText.includes("结果已保存，后续已有更新") && !document.querySelector(".task-result-actions") && reads.length === beforeLocal && Math.abs(localScroller.scrollTop - localTop) < 2, "Local replacement revived prose or fabricated result navigation"); capture(phase);
  const historyCopy = [...description().querySelectorAll<HTMLButtonElement>("button")].find(button => button.getAttribute("aria-label")?.includes("复制") || button.textContent.includes("复制"));
  check(historyCopy, "Historical explanation has no original-copy entry"); historyCopy.click(); await tick();
  check(copied.at(-1) === mainText, "History explanation copy changed full original text");
  phase = "history error remains public";
  await render({ ...local.task, version: 3 }, { ...createAgentRun(), history: [{ ...localHistory, error: "原回复仍有错误" }] });
  check(!description() && document.body.innerText.includes("原讨论失败，需处理") && document.querySelector('[role="alert"]')?.closest("details") == null, "History error hidden by a completed status"); capture(phase);
  phase = "large local reply remains public"; await render({ ...local.task, id: "work-long-local" }, { ...localRun, answer: mainText.repeat(10) });
  check(!description() && document.body.innerText.includes(resultText), "Potentially clipped local reply was hidden"); capture(phase);
  phase = "foreign project cannot fold local reply"; await render(local.task, localRun, 340, "B");
  check(!description() && document.body.innerText.includes(resultText) && !document.querySelector(".task-result-actions"), "Foreign work was treated as this project's verified result"); capture(phase);
  phase = "late saved reply remains isolated"; const late = scoped("late-owner"), next = scoped("next-owner", "B");
  replyModes.set(late.task.steps[1].currentAttemptId, "deferred"); await render(late.task, late.run);
  check(!description() && mainReply()?.innerText.includes("正在读取"), "Unverified loading reply folded prematurely");
  await render(next.task, next.run, 340, "B"); await until(() => Boolean(description()), "New project reply did not settle");
  description().querySelector<HTMLElement>(":scope > summary").click(); await tick(); const newDetails = description();
  releaseReply(); await tick(); await tick();
  check(description() === newDetails && description().open && !document.body.innerText.includes("旧作品迟到原文"), "Late old-project receipt changed new-project explanation"); capture(phase);
  phase = "dark narrow successful result"; childFailed = false; document.documentElement.dataset.theme = "dark"; await render({ ...work, id: "work-dark" }, live, 300); await until(() => mainReply()?.textContent.includes(resultText), "Dark reply missing");
  check(!flow().open && document.querySelector(".work-task-panel").scrollWidth <= 301, "Narrow discussion overflows"); capture(phase);
  root.unmount(); window.testResult = { passed: true, snapshots, reads };
} catch (error) { window.testResult = { passed: false, error: `${phase}: ${error.stack ?? error}`, dom: document.body.innerText }; root.unmount(); } })();
