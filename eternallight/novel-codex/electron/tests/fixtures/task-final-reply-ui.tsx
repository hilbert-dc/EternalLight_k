import "../../../src/styles.css";
import React from "react";
import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";
import { TaskFinalReplyDetails } from "../../../src/TaskFinalReplyDetails";
import type { NovelCodexApi, TaskFinalReply } from "../../../src/types";

const root = createRoot(document.getElementById("root")!);
const tick = () => new Promise(resolve => setTimeout(resolve, 30));
const check = (value: unknown, message: string) => { if (!value) throw new Error(message); };
const wait = async (predicate: () => unknown, message: string) => {
  for (let n = 0; n < 100 && !predicate(); n++) await tick();
  check(predicate(), message);
};
const pending: { args: string[]; resolve: (reply: TaskFinalReply | null) => void; reject: (error: unknown) => void }[] = [];
const unexpectedCalls: string[] = [];
// Every endpoint except the saved-reply read is a local trap, including agent/model actions.
const forbidden = (path: string): object => new Proxy(() => {
  unexpectedCalls.push(path);
  throw new Error(`Unexpected API call: ${path}`);
}, { get: (_target, property) => forbidden(`${path}.${String(property)}`) });
const runtime = new Proxy({ getTaskFinalReply: (...args: string[]) => new Promise<TaskFinalReply | null>((resolve, reject) => pending.push({ args, resolve, reject })) }, {
  get: (target, property) => property === "getTaskFinalReply" ? target.getTaskFinalReply : forbidden(`runtime.${String(property)}`),
});
const api = new Proxy({}, { get: (_target, property) => property === "runtime" ? runtime : forbidden(String(property)) }) as NovelCodexApi;
const markdown = "# 已保存的最终回复\n\n仅修改 **密函场景**，保留 *其他正文*。\n\n- 保留密函\n- 等待白塔亮灯\n\n| 场景 | 结果 |\n| --- | --- |\n| 密函 | 已修改 |\n\n```text\n他藏好密函。\n```\n\n[审阅说明](https://example.invalid/review)\n\n<script>window.finalReplyExecuted = true</script>";
const reply = (projectId: string, taskId: string, state: TaskFinalReply["state"] = "recorded", text = markdown): TaskFinalReply => ({
  projectId, taskId, state,
  binding: { threadId: "fixture-thread", turnId: "fixture-turn", intentRevision: 3, itemId: state === "absent" ? null : "fixture-item", text: state === "absent" ? "" : text, truncated: state === "truncated" },
  sourceDigest: "fixture-source", receiptDigest: "fixture-receipt", recordedAt: 1790000000,
});
const details = () => document.querySelector("details")!;
const text = () => details().textContent ?? "";
const snapshot = () => ({
  summary: details().querySelector("summary")!.textContent,
  paragraphs: [...details().querySelectorAll(":scope > p, [role='alert'] p")].map(node => node.textContent),
  status: details().querySelector("[role='status']")?.textContent ?? null,
  alert: details().querySelector("[role='alert']")?.textContent ?? null,
  buttons: [...details().querySelectorAll("button")].map(node => node.getAttribute("aria-label") ?? node.textContent),
  markdown: details().querySelector(".conversation-markdown")?.innerHTML ?? null,
});
const snapshots: Record<string, ReturnType<typeof snapshot>> = {};
function render(projectId = "fixture-project-a", taskId = "fixture-task-a", connection: NovelCodexApi | null = api) {
  flushSync(() => root.render(<main style={{ maxWidth: 780, margin: "40px auto" }}>
    <h1>创作任务 · 最终回复</h1>
    <TaskFinalReplyDetails api={connection ?? undefined} projectId={projectId} taskId={taskId} />
  </main>));
}
async function request(projectId: string, taskId: string, action: () => void) {
  const count = pending.length;
  action();
  await wait(() => pending.length === count + 1, `Expected saved-reply read ${count + 1} for ${projectId}/${taskId}`);
  const next = pending[count];
  check(JSON.stringify(next.args) === JSON.stringify([projectId, taskId]), "Saved-reply read used the wrong scope");
  await wait(() => details().querySelector("[role='status']"), "Loading status missing");
  check(!details().querySelector(".conversation-markdown, [role='alert']"), "Stale reply or error remained during loading");
  return next;
}
async function reopen(projectId = "fixture-project-a", taskId = "fixture-task-a") {
  details().querySelector("summary")!.click();
  await wait(() => !details().open && details().children.length === 1, "Collapse did not hide reply state");
  return request(projectId, taskId, () => details().querySelector("summary")!.click());
}
const retry = () => details().querySelector<HTMLButtonElement>("[role='alert'] button")!.click();
async function saved(read: typeof pending[number], receipt: TaskFinalReply) {
  read.resolve(receipt);
  await wait(() => details().querySelector(".conversation-markdown"), "Saved Markdown did not render");
  check(text().includes("回复记录不代表创作目标已达成"), "Receipt was presented as proof of completion");
}

void (async () => {
  render(); await tick();
  check(pending.length === 0 && !details().open, "Collapsed reply read automatically");
  let read = await request("fixture-project-a", "fixture-task-a", () => details().querySelector("summary")!.click());
  snapshots.loading = snapshot();
  read.reject(new Error("保存的回复暂时无法读取"));
  await wait(() => details().querySelector("[role='alert']"), "Read failure missing");
  snapshots.readError = snapshot();
  read = await request("fixture-project-a", "fixture-task-a", retry);
  await saved(read, reply("fixture-project-a", "fixture-task-a"));
  check(details().querySelector("h2")?.textContent === "已保存的最终回复", "Markdown heading missing");
  check(details().querySelector("strong")?.textContent === "密函场景" && details().querySelector("em")?.textContent === "其他正文", "Markdown emphasis missing");
  check(details().querySelectorAll("li").length === 2 && details().querySelectorAll("tbody td").length === 2, "Markdown list or GFM table missing");
  check(details().querySelector("pre code")?.textContent === "他藏好密函。\n", "Saved code block changed");
  const link = details().querySelector("a")!;
  check(link.getAttribute("href") === "https://example.invalid/review" && link.target === "_blank" && link.rel === "noreferrer", "Markdown link rendering changed");
  check(!details().querySelector("script") && !window["finalReplyExecuted"], "Saved HTML executed");
  snapshots.recorded = snapshot();
  window["uiReady"] = true; await new Promise(resolve => setTimeout(resolve, 300)); window["uiReady"] = false;
  const readsBeforeRerender = pending.length;
  render(); await tick();
  check(pending.length === readsBeforeRerender && details().querySelector("h2"), "Same-scope rerender repeated the read or lost the reply");

  read = await reopen(); read.resolve(null);
  await wait(() => text().includes("没有最终回复凭据"), "Missing receipt not distinguished");
  snapshots.noReceipt = snapshot();
  read = await reopen(); read.resolve(reply("fixture-project-a", "fixture-task-a", "absent"));
  await wait(() => text().includes("引擎回合结束，但没有最终文本回复"), "Absent final text not distinguished");
  check(!details().querySelector(".conversation-markdown"), "Absent reply rendered Markdown");
  snapshots.absent = snapshot();
  read = await reopen(); await saved(read, reply("fixture-project-a", "fixture-task-a", "truncated", "## 部分回复\n\n已保存的**开头**。"));
  check(text().includes("不是完整回复"), "Truncated reply lacks partial-content warning");
  snapshots.truncated = snapshot();

  // A mounted open details view must clear the previous project synchronously.
  read = await request("fixture-project-b", "fixture-task-a", () => {
    render("fixture-project-b");
    check(!text().includes("部分回复"), "Previous project's reply flashed after scope change");
  });
  read.resolve(reply("foreign-project", "fixture-task-a", "recorded", "FOREIGN_PROJECT_REPLY"));
  await wait(() => text().includes("回复归属不匹配"), "Foreign project receipt was accepted");
  check(!text().includes("FOREIGN_PROJECT_REPLY"), "Foreign project text leaked");
  snapshots.scopeError = snapshot();
  read = await request("fixture-project-b", "fixture-task-a", retry);
  read.resolve(reply("fixture-project-b", "foreign-task", "recorded", "FOREIGN_TASK_REPLY"));
  await wait(() => text().includes("回复归属不匹配"), "Foreign task receipt was accepted");
  check(!text().includes("FOREIGN_TASK_REPLY"), "Foreign task text leaked");
  read = await request("fixture-project-b", "fixture-task-a", retry); read.reject("unstructured failure");
  await wait(() => text().includes("读取最终回复失败"), "Non-Error read rejection lacked fallback");
  snapshots.unknownError = snapshot();
  read = await request("fixture-project-b", "fixture-task-a", retry);
  await saved(read, reply("fixture-project-b", "fixture-task-a", "recorded", "## 重试后已保存\n\n重试只读取回复。"));

  const beforeUnavailable = pending.length;
  render("fixture-project-b", "fixture-task-a", null);
  await tick();
  check(text().includes("最终回复读取接口不可用"), "Unavailable API error missing");
  retry(); await tick();
  check(pending.length === beforeUnavailable && text().includes("最终回复读取接口不可用"), "Unavailable API retry called a backend");
  snapshots.unavailableApi = snapshot();

  const oldProject = await request("fixture-project-a", "fixture-task-a", () => render());
  const newProject = await request("fixture-project-b", "fixture-task-a", () => render("fixture-project-b"));
  oldProject.resolve(reply("fixture-project-a", "fixture-task-a", "recorded", "LATE_OLD_PROJECT")); await tick();
  check(details().querySelector("[role='status']") && !text().includes("LATE_OLD_PROJECT"), "Late project success replaced current loading state");
  await saved(newProject, reply("fixture-project-b", "fixture-task-a", "recorded", "CURRENT_PROJECT_REPLY"));
  const oldTask = await request("fixture-project-b", "fixture-task-b", () => {
    render("fixture-project-b", "fixture-task-b");
    check(!text().includes("CURRENT_PROJECT_REPLY"), "Previous task reply flashed after task change");
  });
  const newTask = await request("fixture-project-b", "fixture-task-c", () => render("fixture-project-b", "fixture-task-c"));
  await saved(newTask, reply("fixture-project-b", "fixture-task-c", "recorded", "CURRENT_TASK_REPLY"));
  oldTask.reject(new Error("LATE_OLD_TASK_ERROR")); await tick();
  check(text().includes("CURRENT_TASK_REPLY") && !details().querySelector("[role='alert']"), "Late task failure replaced current reply");

  const lateTask = await request("fixture-project-b", "fixture-task-d", () => render("fixture-project-b", "fixture-task-d"));
  const latestTask = await request("fixture-project-b", "fixture-task-e", () => render("fixture-project-b", "fixture-task-e"));
  await saved(latestTask, reply("fixture-project-b", "fixture-task-e", "recorded", "LATEST_TASK_REPLY"));
  lateTask.resolve(reply("fixture-project-b", "fixture-task-d", "recorded", "LATE_OLD_TASK_REPLY")); await tick();
  check(text().includes("LATEST_TASK_REPLY") && !text().includes("LATE_OLD_TASK_REPLY"), "Late task success replaced newer reply");

  // Closing cancels a read; changing project/task while closed must stay lazy.
  const closedRead = await reopen("fixture-project-b", "fixture-task-e");
  details().querySelector("summary")!.click(); await wait(() => !details().querySelector("[role='status']"), "Closed read remained visible");
  const beforeClosedSwitch = pending.length;
  render("fixture-project-c", "fixture-task-f"); await tick();
  closedRead.reject(new Error("LATE_CLOSED_ERROR")); await tick();
  check(pending.length === beforeClosedSwitch && !details().querySelector("[role='alert']"), "Closed scope change read or displayed a late failure");
  read = await request("fixture-project-c", "fixture-task-f", () => details().querySelector("summary")!.click());
  await saved(read, reply("fixture-project-c", "fixture-task-f", "recorded", "## 从存储重新读取\n\n重新打开只读取已保存的最终回复。"));
  snapshots.reopened = snapshot();
  check(unexpectedCalls.length === 0, `Reply view executed an unexpected endpoint: ${unexpectedCalls.join(", ")}`);
  root.unmount();
  window["testResult"] = { passed: true, snapshots, reads: pending.length, unexpectedCalls };
})().catch(error => { window["testResult"] = { passed: false, error: error.stack }; });
