import React from "react";
import { createRoot } from "react-dom/client";
import { useWorkTaskList } from "../../../src/useWorkTaskList";
import { WorkTaskPanel } from "../../../src/WorkTaskPanel";
import "../../../src/styles.css";

const tick = () => new Promise((resolve) => setTimeout(resolve, 30));
const check = (condition, message) => { if (!condition) throw new Error(message); };
const requests = [];
const api = { runtime: { listWorkTasks: (projectId, cursor = null) => new Promise((resolve, reject) => {
  requests.push({ projectId, cursor, resolve, reject });
}) } };
const emptyReport = { evidence: [], pendingActionIds: [], unsettledAttemptIds: [] };
const task = (id, status = "paused") => ({ id, goal: id, status, deliveryStatus: "pending", deliveryReport: emptyReport, resumeBlockReason: null, steps: [] });
const page = (id, nextCursor = null, status = "paused") => ({ tasks: id ? [task(id, status)] : [], nextCursor, activeCount: id ? 1 : 0 });
let controls = 0;
function Harness({ projectId, available }) {
  const list = useWorkTaskList(available ? api : undefined, projectId, 0, false);
  return <><button onClick={() => void list.refresh()}>刷新测试任务</button>
    <WorkTaskPanel {...list} readError={list.error} onRetry={() => void list.refresh()} busy={false} interruptionBusy={false}
      agentRunning={false} agentAvailable={true} dirty={false} onUndo={() => { controls += 1; }}
      onControl={() => { controls += 1; }} onLoadMore={() => void list.loadMore()} revision={0} /></>;
}
const root = createRoot(document.getElementById("root"));
const render = async (projectId = "A", available = true) => { root.render(<Harness projectId={projectId} available={available} />); await tick(); await tick(); };
const button = (text) => [...document.querySelectorAll("button")].find((element) => element.textContent === text);
const click = (text) => { const element = button(text); check(element && !element.disabled, `Unavailable ${text}`); element.click(); };
const settle = async (request, result) => { request.resolve(result); await tick(); await tick(); };
async function run() {
  await render();
  check(document.querySelector("summary")?.textContent.includes("读取中"), "Initial load claimed no tasks");
  requests[0].reject(new Error("读取测试失败")); await tick();
  check(document.querySelector('[role="alert"]')?.textContent.includes("不能判断"), "Initial failure hid unknown task state");
  click("重试读取任务"); await tick();
  await settle(requests[1], page("初次读取", "cursor-1"));
  check(document.body.textContent.includes("初次读取"), "Read retry did not recover");
  const beforeMore = requests.length;
  click("加载更多任务"); click("加载更多任务"); await tick();
  check(requests.length === beforeMore + 1, "Same-frame pagination was duplicated");
  click("刷新测试任务"); await tick();
  check(requests.length === beforeMore + 1, "Refresh overlapped pagination");
  await settle(requests.at(-1), page("失效分页内容"));
  check(!document.body.textContent.includes("失效分页内容") && requests.at(-1).cursor === null, "Refresh accepted an obsolete page or failed to coalesce");
  await settle(requests.at(-1), page("当前任务", "cursor-2"));
  click("加载更多任务"); await tick();
  requests.at(-1).reject(new Error("WORK_TASK_STALE")); await tick();
  check(requests.at(-1).cursor === null, "Stale cursor was retried instead of resetting the list");
  await settle(requests.at(-1), page("重读任务"));
  click("刷新测试任务"); await tick();
  requests.at(-1).reject(new Error("服务暂时不可用")); await tick();
  check(document.body.textContent.includes("重读任务") && document.body.textContent.includes("状态可能已变化"), "Refresh failure erased persisted tasks");
  check(button("取消任务")?.disabled && button("继续任务")?.disabled && controls === 0, "Read failure allowed stale task controls");
  click("重试读取任务"); await tick();
  check(button("取消任务")?.disabled, "Retry prematurely removed the failure guard");
  await settle(requests.at(-1), page("恢复任务"));
  check(!button("取消任务").disabled && !document.querySelector('[role="alert"]'), "Successful retry failed to restore controls");

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
  check(document.body.textContent.includes("运行结束"), "Slow running poll never became visible");
  check(document.querySelector(".work-task-row header")?.textContent.includes("执行结束 · 交付待核验"), "Execution completion was misrepresented as goal achievement");
  click("刷新测试任务"); await tick();
  const evidence = [{ stepId: "draft-step", attemptId: "draft-attempt", kind: "draft", referenceId: "old-draft", state: "superseded" }];
  await settle(requests.at(-1), { tasks: [{ ...task("局部交付", "completed"), deliveryStatus: "partial",
    deliveryReport: { evidence, pendingActionIds: ["action-one"], unsettledAttemptIds: ["attempt-one"] },
    steps: [{ id: "draft-step", operation: "generateChapter", target: {}, status: "completed", readingProgress: null, outputRefs: [], attempts: [], changes: [] }] }], activeCount: 0, nextCursor: null });
  check(document.body.textContent.includes("尚未执行") && document.body.textContent.includes("尚未结算")
    && document.body.textContent.includes("后续版本替换"), "Delivery blockers were hidden");
  const deliverySnapshot = document.querySelector(".work-task-row").innerText;
  window.uiReady = true; await tick(); await tick();
  click("刷新测试任务"); await tick();
  await settle(requests.at(-1), { tasks: [{ ...task("缺少实际产物", "completed"),
    deliveryReport: { ...emptyReport, evidence: [{ ...evidence[0], referenceId: null, state: "missing" }] },
    steps: [{ id: "draft-step", operation: "generateChapter", target: {}, status: "completed", readingProgress: null,
      outputRefs: [{ kind: "draft", revisionId: "claimed-draft" }], attempts: [], changes: [] }] }], activeCount: 0, nextCursor: null });
  check(document.body.textContent.includes("未找到符合任务契约") && !document.body.textContent.includes("已保存 1 项候选")
    && !document.body.textContent.includes("证据已核验"), "Display metadata overrode missing canonical evidence");
  click("刷新测试任务"); await tick();
  await settle(requests.at(-1), { tasks: [{ ...task("讨论", "completed"), deliveryStatus: "discussion_ended" }], activeCount: 0, nextCursor: null });
  check(document.querySelector(".work-task-row header")?.textContent.includes("讨论已结束")
    && !document.querySelector(".work-task-row header")?.textContent.includes("待审阅"), "Discussion invented a candidate artifact");
  click("刷新测试任务"); await tick();
  await settle(requests.at(-1), { tasks: [{ ...task("等待结算", "completed"),
    deliveryReport: { ...emptyReport, unsettledAttemptIds: ["ended-attempt"] } }], activeCount: 0, nextCursor: null });
  const beforeSettlementPoll = requests.length;
  await new Promise((resolve) => setTimeout(resolve, 1650));
  check(requests.length === beforeSettlementPoll + 1, "Completed execution stopped polling before settlement");
  await settle(requests.at(-1), { tasks: [{ ...task("结算完成", "completed"), deliveryStatus: "discussion_ended" }], activeCount: 0, nextCursor: null });
  const afterSettlementPoll = requests.length;
  await new Promise((resolve) => setTimeout(resolve, 1650));
  check(requests.length === afterSettlementPoll, "Settled terminal work kept polling");
  click("刷新测试任务"); await tick(); await settle(requests.at(-1), page(null));
  check(!document.querySelector(".work-task-panel"), "Successfully empty list still indicated an error");
  const beforeUnavailable = requests.length;
  await render("A", false);
  check(document.querySelector('[role="alert"]')?.textContent.includes("未提供任务读取接口")
    && requests.length === beforeUnavailable, "Missing bridge was presented as endless loading or an empty task list");
  root.unmount();
  return { passed: true, deliverySnapshot };
}
run().then((result) => { window.testResult = result; }).catch((error) => {
  window.testResult = { passed: false, error: error.stack + "\nDOM:" + document.body.innerText };
});
