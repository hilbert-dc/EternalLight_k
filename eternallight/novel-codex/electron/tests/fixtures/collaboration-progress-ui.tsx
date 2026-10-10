import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { ConversationTimeline } from "../../../src/ConversationTimeline";
import { collaborationProgress, collaborationFromItem } from "../../../src/collaboration-progress";
import { createAgentRun, reduceAgentEvent, summarizeAgentRun } from "../../../src/agent-events";
const root = createRoot(document.getElementById("root")!);
const tick = () => new Promise(resolve => setTimeout(resolve, 50));
const check = (condition, message) => { if (!condition) throw Error(message); };
let run = reduceAgentEvent(createAgentRun("检查本章"), { method: "thread/started", params: { thread: { id: "root" } } });
run = reduceAgentEvent(run, { method: "turn/started", params: { threadId: "root", turn: { id: "turn" } } });
function event(id, tool, states, prompt = null) {
  run = reduceAgentEvent(run, { method: "item/completed", params: { threadId: "root", turnId: "turn", item: {
    id, type: "collabAgentToolCall", tool, prompt, receiverThreadIds: Object.keys(states), agentsStates: states, status: "completed" } } });
}
async function render() {
  root.render(<main style={{ width: 300, padding: 20, margin: "24px auto", background: "#fff" }}>
    <p style={{ fontSize: 14, lineHeight: 1.8 }}>我会分别检查情节结构与人物动机，再汇总修改建议。</p>
    <ConversationTimeline run={run} />
  </main>); await tick();
}
(async () => { try {
  event("spawn-1", "spawnAgent", { a: { status: "running" } }, "情节结构分析");
  event("spawn-2", "spawnAgent", { b: { status: "pendingInit" } }, "人物一致性检查");
  await render();
  check(document.querySelectorAll(".collaboration-row").length === 2, "Expected one row per child");
  check(!document.querySelector(".conversation-operation-details"), "A projected child status must not repeat spawn/wait routing rows");
  check(document.querySelector(".collaboration-status")?.textContent === "进行中", "Spawn completion falsely completed child");
  check(document.querySelector(".conversation-timeline")?.tagName === "SECTION", "Public narrative must not be inside a collapsed timeline");
  const first = document.querySelector<HTMLDetailsElement>(".collaboration-row")!;
  first.querySelector("summary")!.click(); await tick(); check(first.open, "Cannot expand child details");
  event("wait", "wait", { a: { status: "completed", message: "转折成立，建议补充行动动机。" }, b: { status: "running" } });
  await render();
  check(first === document.querySelector(".collaboration-row") && first.open, "Child identity or open state was lost");
  check(first.textContent?.includes("子任务已返回") && first.textContent?.includes("转折成立")
    && !first.textContent?.includes("目标已完成"), "Child return must not claim parent delivery");
  check(document.querySelectorAll(".collaboration-row").length === 2, "Wait created duplicate rows");
  first.querySelector("summary")!.click(); await tick();
  window.testScreenshot = "collaboration-progress";
  while (window.testScreenshot) await tick();
  const bounds = document.querySelector("main")!.getBoundingClientRect();
  for (const row of document.querySelectorAll(".collaboration-row")) check(row.getBoundingClientRect().right <= bounds.right, "Narrow layout overflow");
  event("empty-result", "wait", { c: { status: "completed" } });
  await render();
  const empty = [...document.querySelectorAll<HTMLDetailsElement>(".collaboration-row")].at(-1);
  empty.querySelector<HTMLElement>("summary").click(); await tick();
  check(empty.querySelector(".collaboration-status").textContent === "子任务已返回"
    && empty.innerText.includes("子任务已结束，未返回文字摘要。") && !empty.innerText.includes("任务已完成"),
    "Completion without a report must stay a child lifecycle fact");
  run = reduceAgentEvent(run, { method: "turn/completed", params: { threadId: "root", turn: { id: "turn", status: "completed" } } });
  await render(); check(document.querySelectorAll(".collaboration-status")[1].textContent === "状态未确认", "Parent success fabricated child completion");
  check(document.querySelectorAll(".collaboration-row").length === 3 && !document.querySelector(".conversation-timeline"),
    "Terminal collaboration results were hidden or left an empty duplicate process section");
  const saved = summarizeAgentRun(run, Date.now()).displayExecution;
  check(saved.steps.some(step => step.collaborators?.some(agent => agent.result?.includes("转折成立"))), "Snapshot lost child results");
  const stopped = collaborationProgress({ ...run, state: "cancelled" });
  check(stopped[0].status === "completed" && stopped[1].status === "turnStopped", "Stopping overwrote completion or left child running");
  const failed = collaborationFromItem({ receiverThreadIds: ["failed"], agentsStates: { failed: { status: "errored", message: "服务不可用" } } });
  check(failed[0].status === "errored" && failed[0].result === "服务不可用", "Lost child failure evidence");
  const unknown = collaborationFromItem({ receiverThreadIds: ["unknown"], status: "completed" });
  check(unknown[0].status === "unknown", "Tool success fabricated unknown child completion");
  root.render(<ConversationTimeline run={{ ...run, state: "running", steps: [{ id: "active-route", kind: "interaction",
    title: "正在安排协作", status: "running", collaborators: [{ id: "active-child", status: "running" }] },
    { id: "unknown-route", kind: "interaction", title: "协作身份待确认", status: "completed", collaborators: [] }] }} />); await tick();
  check(document.querySelector(".conversation-operation-details")?.textContent.includes("正在安排协作")
    && document.querySelector(".conversation-operation-details")?.textContent.includes("协作身份待确认"),
    "Active routing and an unidentified child must remain inspectable");
  root.render(<ConversationTimeline run={{ ...run, state: "error", steps: [{ id: "failed-route", kind: "interaction",
    title: "协作连接未完成", status: "failed", detail: "ROUTING_ERROR", collaborators: [{ id: "failed-child", status: "unknown" }] }] }} />); await tick();
  check(document.querySelector(".conversation-operation-details")?.textContent.includes("协作连接未完成")
    && document.body.innerText.includes("未完成"), "Projected child metadata must not hide failed routing");
  window.testResult = { passed: true };
} catch (error) { window.testResult = { passed: false, error: error.stack }; } })();
