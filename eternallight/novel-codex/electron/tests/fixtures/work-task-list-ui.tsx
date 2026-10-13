import { emptyCreationIssueSummary } from "../helpers/creation-issues";
import React from "react";
import { createRoot } from "react-dom/client";
import { useWorkTaskList } from "../../../src/useWorkTaskList";
import { WorkTaskPanel } from "../../../src/WorkTaskPanel";
import "../../../src/styles.css";

const tick = () => new Promise((resolve) => setTimeout(resolve, 30));
const check = (condition, message) => { if (!condition) throw new Error(message); };
const requests = [];
const api = { runtime: { getTaskFinalReply: async () => null, listWorkTasks: (projectId, cursor = null) => new Promise((resolve, reject) => {
  requests.push({ projectId, cursor, resolve, reject });
}) } };
const emptyReport = { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], evidence: [], pendingActionIds: [], unsettledAttemptIds: [] };
const task = (id, status = "paused") => ({ id, goal: id, status, deliveryStatus: "pending", deliveryReport: emptyReport, resumeBlockReason: null,
  resumeMode: ["paused", "blocked"].includes(status) ? "continueExecution" : null, steps: [] });
const page = (id, nextCursor = null, status = "paused") => ({ tasks: id ? [task(id, status)] : [], nextCursor, activeCount: id && !["completed", "cancelled"].includes(status) ? 1 : 0 });
let controls = 0;
let observedList;
function Harness({ projectId, available, agentAvailable, dirty, enabled, agentRunning, missingList }) {
  const list = useWorkTaskList(available ? missingList ? { runtime: {} } : api : undefined, projectId, 0, agentRunning, enabled);
  observedList = list;
  return <><button onClick={() => void list.refresh()}>刷新测试任务</button>
    <button onClick={list.poll}>轮询测试任务</button>
    {enabled && <WorkTaskPanel {...list} readError={list.error} onRetry={() => void list.refresh()} busy={false}
      agentRunning={false} dirty={dirty} onUndo={() => { controls += 1; }} onLoadMore={() => void list.loadMore()} revision={0} />}
    <article className="fixture-manuscript">章节正文应位于任务摘要之后。</article></>;
}
const root = createRoot(document.getElementById("root"));
const render = async (projectId = "A", available = true, agentAvailable = true, dirty = false, enabled = true, agentRunning = false, missingList = false) => {
  root.render(<Harness projectId={projectId} available={available} agentAvailable={agentAvailable} dirty={dirty} enabled={enabled} agentRunning={agentRunning} missingList={missingList} />); await tick(); await tick(); };
const button = (text) => [...document.querySelectorAll("button")].find((element) => element.textContent === text);
const click = (text) => { const element = button(text); check(element && !element.disabled, `Unavailable ${text}`); element.click(); };
const settle = async (request, result) => { request.resolve(result); await tick(); await tick(); };
const openHistory = () => {
  for (const execution of document.querySelectorAll(".work-task-execution")) if (!execution.open) execution.querySelector("summary").click();
};
async function run() {
  const lifecycleCases = [];
  await render("", true, true, false, false, true);
  check(requests.length === 0 && !observedList.loaded && !observedList.reading && observedList.error === null
    && !document.querySelector('[role="alert"]') && !document.querySelector('.work-task-panel'), "Disabled empty workspace looked like a failure or successful read");
  click("刷新测试任务"); click("轮询测试任务"); await observedList.loadMore();
  await new Promise(resolve => setTimeout(resolve, 1650));
  check(requests.length === 0, "Disabled reader refreshed, paginated, or polled");
  await render("", false, true, false, false);
  check(observedList.error === null && !observedList.loaded, "Intentionally disabled reader claimed a missing bridge");
  lifecycleCases.push("disabled empty workspace", "disabled actions and polling", "disabled missing bridge");
  await render();
  check(document.querySelector('[role="status"]')?.textContent.includes("正在读取"), "Initial load claimed no tasks");
  requests[0].reject(new Error("读取测试失败")); await tick();
  check(document.querySelector('[role="alert"]')?.textContent.includes("不能判断"), "Initial failure hid unknown task state");
  click("重试读取任务"); await tick();
  await settle(requests[1], page("初次读取", "cursor-1"));
  check(document.body.textContent.includes("初次读取"), "Read retry did not recover");
  const beforeMore = requests.length;
  click("加载更早对话"); click("加载更早对话"); await tick();
  check(requests.length === beforeMore + 1, "Same-frame pagination was duplicated");
  click("刷新测试任务"); await tick();
  check(requests.length === beforeMore + 1, "Refresh overlapped pagination");
  await settle(requests.at(-1), page("失效分页内容"));
  check(!document.body.textContent.includes("失效分页内容") && requests.at(-1).cursor === null, "Refresh accepted an obsolete page or failed to coalesce");
  await settle(requests.at(-1), page("当前任务", "cursor-2"));
  click("加载更早对话"); await tick();
  requests.at(-1).reject(new Error("WORK_TASK_STALE")); await tick();
  check(requests.at(-1).cursor === null, "Stale cursor was retried instead of resetting the list");
  await settle(requests.at(-1), page("重读任务"));
  click("刷新测试任务"); await tick();
  requests.at(-1).reject(new Error("服务暂时不可用")); await tick();
  check(document.body.textContent.includes("重读任务") && document.body.textContent.includes("状态可能已变化"), "Refresh failure erased persisted tasks");
  check(!button("取消任务") && !button("继续任务") && controls === 0, "Read failure allowed stale task controls");
  click("重试读取任务"); await tick();
  check(!button("取消任务"), "Retry prematurely removed the failure guard");
  await settle(requests.at(-1), page("恢复任务"));
  check(!document.querySelector('[role="alert"]'), "Successful retry failed to restore controls");

  const beforeExplicitPoll = requests.length;
  click("轮询测试任务"); click("轮询测试任务"); await tick();
  check(requests.length === beforeExplicitPoll + 1, "Explicit poll overlapped an in-flight read");
  await settle(requests.at(-1), page("轮询完成"));
  check(requests.length === beforeExplicitPoll + 1, "Skipped poll queued an unnecessary follow-up read");

  click("刷新测试任务"); await tick(); const oldA = requests.at(-1);
  await render("B"); const oldB = requests.at(-1);
  check(!document.querySelector(".work-task-row"), "Project B inherited A's task list");
  await render("A"); const newA = requests.at(-1);
  await settle(newA, page("新的 A 生命周期"));
  await settle(oldA, page("旧的 A 生命周期"));
  await settle(oldB, page("旧的 B 生命周期"));
  check(document.body.textContent.includes("新的 A 生命周期") && !document.body.textContent.includes("旧的"), "A-B-A accepted a late response");

  click("刷新测试任务"); await tick();
  await settle(requests.at(-1), page("独立运行任务", null, "running"));
  const beforePoll = requests.length;
  await new Promise((resolve) => setTimeout(resolve, 1650));
  check(requests.length === beforePoll + 1, "Persisted running task did not poll without an active local Agent");
  await new Promise((resolve) => setTimeout(resolve, 1600));
  check(requests.length === beforePoll + 1, "Slow poll overlapped or lost its response authority");
  await settle(requests.at(-1), page("运行结束", null, "completed"));
  openHistory();
  check(document.body.textContent.includes("运行结束"), "Slow running poll never became visible");
  check(document.querySelector(".work-task-row header")?.textContent.includes("保存结果待核对"), "Execution completion was misrepresented as goal achievement");
  click("刷新测试任务"); await tick();
  await settle(requests.at(-1), { tasks: [{ ...task("自动安排", "running"), deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], ...emptyReport, pendingActionIds: ["action-one"] } }], activeCount: 1, nextCursor: null });
  check(document.body.textContent.includes("正在安排后续创作步骤…") && !document.body.textContent.includes("行动提案待确认"), "Automatic candidate scheduling asked for duplicate approval");
  const schedulingSnapshot = document.querySelector(".work-task-row").innerText;
  click("刷新测试任务"); await tick();
  const evidence = [{ stepId: "draft-step", attemptId: "draft-attempt", kind: "draft", referenceId: "old-draft", state: "superseded" }];
  await settle(requests.at(-1), { tasks: [{ ...task("局部交付", "completed"), deliveryStatus: "partial",
    deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], evidence, pendingActionIds: ["action-one"], unsettledAttemptIds: ["attempt-one"] },
    steps: [{ id: "draft-step", operation: "generateChapter", target: {}, status: "completed", readingProgress: null, outputRefs: [], attempts: [], changes: [] }] }], activeCount: 0, nextCursor: null });
  openHistory();
  check(document.body.textContent.includes("尚未执行") && document.body.textContent.includes("用量记录待核对"), "Delivery blockers were hidden");
  const deliverySnapshot = document.querySelector(".work-task-row").innerText;
  window.uiReady = true; await tick(); await tick();
  click("刷新测试任务"); await tick();
  await settle(requests.at(-1), { tasks: [{ ...task("缺少实际产物", "completed"),
    deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], ...emptyReport, evidence: [{ ...evidence[0], referenceId: null, state: "missing" }] },
    steps: [{ id: "draft-step", operation: "generateChapter", target: {}, status: "completed", readingProgress: null,
      outputRefs: [{ kind: "draft", objectId: "draft-test-chapter", revisionId: "claimed-draft" }], attempts: [], changes: [] }] }], activeCount: 0, nextCursor: null });
  openHistory();
  check(document.body.textContent.includes("保存结果待核对") && !document.body.textContent.includes("已保存 1 项候选")
    && !document.body.textContent.includes("证据已核验"), "Display metadata overrode missing canonical evidence");
  click("刷新测试任务"); await tick();
  await settle(requests.at(-1), { tasks: [{ ...task("讨论", "completed"), deliveryStatus: "discussion_ended" }], activeCount: 0, nextCursor: null });
  check(document.querySelector(".work-task-row header")?.textContent.includes("讨论已结束")
    && !document.querySelector(".work-task-row header")?.textContent.includes("待审阅"), "Discussion invented a candidate artifact");
  click("刷新测试任务"); await tick();
  await settle(requests.at(-1), { tasks: [{ ...task("等待结算", "completed"),
    deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], ...emptyReport, unsettledAttemptIds: ["ended-attempt"] } }], activeCount: 0, nextCursor: null });
  const beforeSettlementPoll = requests.length;
  await new Promise((resolve) => setTimeout(resolve, 1650));
  check(requests.length === beforeSettlementPoll + 1, "Completed execution stopped polling before settlement");
  await settle(requests.at(-1), { tasks: [{ ...task("结算完成", "completed"), deliveryStatus: "discussion_ended" }], activeCount: 0, nextCursor: null });
  const afterSettlementPoll = requests.length;
  await new Promise((resolve) => setTimeout(resolve, 1650));
  check(requests.length === afterSettlementPoll, "Settled terminal work kept polling");
  click("刷新测试任务"); await tick();
  await settle(requests.at(-1), { tasks: [{ ...task("恢复已保存结果", "blocked"), resumeMode: "recoverSavedResult" }], activeCount: 1, nextCursor: null });
  check(!document.querySelector(".work-task-controls"), "Task rows duplicated composer recovery controls");
  const recoverySnapshot = document.querySelector(".work-task-row").innerText;
  // Past requests and the actionable current request form one chronological conversation.
  click("刷新测试任务"); await tick();
  const ended = Array.from({ length: 6 }, (_, index) => ({ ...task(`已结束历史 ${index + 1}`, index % 2 ? "cancelled" : "completed"),
    steps: [{ id: `history-step-${index}`, operation: "generateChapter", target: {}, status: "completed", readingProgress: null, outputRefs: [],
      attempts: [{ id: `history-attempt-${index}`, threadId: "thread-a", turnId: `turn-${index}`, startedAt: index + 1, completedAt: index + 2, status: "completed", error: null }],
      changes: [{ operation: "accept", commitId: `history-commit-${index}`, resultRevision: 0, undoneByCommitId: null }] }] }));
  const paused = { ...task("需要保留的暂停任务"), steps: [{ id: "paused-step", operation: "generateChapter", target: {}, status: "queued", readingProgress: null, outputRefs: [], changes: [],
    attempts: [{ id: "paused-attempt", threadId: "thread-a", turnId: "turn-paused", startedAt: 7, completedAt: null, status: "running", error: null }] }] };
  const beforeDensityControls = controls;
  await settle(requests.at(-1), { tasks: [paused, ...ended.slice().reverse()], activeCount: 1, nextCursor: null });
  const rows = [...document.querySelectorAll(".work-task-row")];
  check(rows.length === 7 && rows[0].innerText.includes("已结束历史 1") && rows.at(-1).innerText.includes("需要保留的暂停任务"), "Conversation did not preserve chronological turns");
  check(!document.querySelector(".work-task-execution") && !document.body.innerText.includes("继续任务"), "Process folding hid a current control or expanded old execution details");
  check(controls === beforeDensityControls, "Rendering history mutated a task");
  const undo = [...rows[0].querySelectorAll("button")].find(element => element.textContent === "撤销");
  check(undo && !undo.disabled, "Conversation lost the original undo control");
  undo.click(); check(controls === beforeDensityControls + 1, "Conversation undo stopped working");
  click("刷新测试任务"); await tick(); requests.at(-1).reject(new Error("时间线读取失败")); await tick();
  check(document.querySelector('[role="alert"]').innerText.includes("任务记录暂时无法读取") && document.querySelector(".read-failure-notice pre").textContent.includes("时间线读取失败") && !button("继续任务") && button("撤销").disabled, "Read failure did not disable conversation controls");
  const densitySnapshot = { first: rows[0].querySelector(".user-message p").textContent, last: rows.at(-1).querySelector(".user-message p").textContent,
    chronological: true, turns: rows.length, executionLists: 0, preservedUndo: true, failureVisible: true };
  click("重试读取任务"); await tick(); await settle(requests.at(-1), page("读取恢复"));
  click("刷新测试任务"); await tick(); await settle(requests.at(-1), page(null));
  check(!document.querySelector(".work-task-panel"), "Successfully empty list still indicated an error");
  const beforeUnavailable = requests.length;
  await render("A", false);
  check(document.querySelector(".read-failure-notice pre")?.textContent.includes("未提供任务读取接口")
    && requests.length === beforeUnavailable, "Missing bridge was presented as endless loading or an empty task list");
  lifecycleCases.push("enabled missing bridge");
  await render("A", true, true, false, true, false, true);
  check(observedList.error?.includes("未提供任务读取接口") && !observedList.loaded && !observedList.reading
    && requests.length === beforeUnavailable, "Missing actual task API was hidden or called");
  lifecycleCases.push("enabled missing task API");
  await render(); const disabledRead = requests.at(-1), previousReader = observedList;
  await render("A", true, true, false, false, true);
  await previousReader.refresh(); previousReader.poll(); await previousReader.loadMore();
  await settle(disabledRead, page("禁用后的迟到运行任务", "stale-cursor", "running"));
  check(observedList.tasks.length === 0 && !observedList.loaded && !observedList.reading && observedList.error === null
    && requests.at(-1) === disabledRead, "Disabled lifecycle accepted an in-flight result or an obsolete action");
  lifecycleCases.push("disabled in-flight and stale actions");
  await render();
  check(requests.at(-1) !== disabledRead && observedList.reading && !observedList.loaded
    && observedList.tasks.length === 0, "Re-enabled reader reused an earlier successful lifecycle");
  requests.at(-1).reject(new Error("启用后真实读取失败")); await tick();
  check(observedList.error === "启用后真实读取失败" && document.querySelector(".read-failure-notice pre")?.textContent.includes("启用后真实读取失败"), "Re-enabled reader hid a real read failure");
  click("重试读取任务"); await tick(); await settle(requests.at(-1), page(null));
  check(observedList.loaded && observedList.error === null && !document.querySelector('.work-task-panel'), "Re-enabled read retry failed to recover");
  lifecycleCases.push("re-enabled fresh read", "real failure and retry");
  root.unmount();
  return { passed: true, deliverySnapshot, schedulingSnapshot, recoverySnapshot, densitySnapshot, lifecycleCases };
}
run().then((result) => { window.testResult = result; }).catch((error) => {
  window.testResult = { passed: false, error: error.stack + "\nDOM:" + document.body.innerText };
});
