import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { ExecutionScopeDialog } from "../../../src/ExecutionScopeDialog";

const root = createRoot(document.getElementById("root"));
const tick = () => new Promise(resolve => setTimeout(resolve, 30));
const check = (condition, message) => { if (!condition) throw new Error(message); };
const wait = async predicate => { for (let n = 0; n < 100 && !predicate(); n++) await tick(); check(predicate(), "UI did not settle"); };
const action = { id: `action-${"a".repeat(32)}`, sourceTaskId: `task-${"b".repeat(32)}`, operation: "generateChapter",
  target: { chapterId: "chapter-2", scope: { type: "selectedScenes", groupIds: ["letter"] } },
  instruction: "只修改密函场景，将烧信改为藏信", status: "proposed", inputRevision: 2, intentRevision: 0 };
const previews = [], confirmations = [], completions = [];
let starts = 0, persisted = null;
const preview = (projectId, request) => ({ previewDigest: "d".repeat(64), projectId, sourceTaskId: action.sourceTaskId,
  workTaskId: "work-local", authorMessageId: "author-message", authorIntent: `${projectId}：将烧信改为藏信，保留其他正文。`,
  request, targets: [{ operation: action.operation, target: action.target }], inputRevision: 2, workVersion: 4,
  remainingRequests: 15, remainingTokens: 199900, deadlineAt: 1790707200 });
const api = {
  previewExecutionScope: (...args) => new Promise((resolve, reject) => previews.push({ args, resolve, reject })),
  confirmExecutionScope: (...args) => new Promise((resolve, reject) => confirmations.push({ args, resolve, reject })),
  readExecutionScope: async () => ({ scope: persisted }),
};
const button = label => [...document.querySelectorAll("button")].find(item => item.getAttribute("aria-label") === label || item.textContent === label);
async function render(projectId, key = projectId) {
  root.render(<ExecutionScopeDialog key={key} projectId={projectId} initialAction={action} actions={[action]} api={api}
    blocked={false} onClose={() => root.unmount()} onStart={() => { starts++; }} onComplete={async result => { completions.push(result); }} onError={() => {}} />);
  await tick();
}
function changeNumber(label, value) {
  const input = document.querySelector(`input[aria-label="${label}"]`);
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(input, value);
  input.dispatchEvent(new Event("input", { bubbles: true }));
}
void (async () => {
  await render("旧作品");
  check(button("确认范围并继续").disabled && previews.length === 0, "Cannot confirm before a read-only preview");
  button("预览范围").click(); await wait(() => previews.length === 1);
  await render("港城");
  previews[0].resolve(preview("旧作品", previews[0].args[2])); await tick();
  check(!document.body.textContent.includes("旧作品："), "Late preview leaked across projects");
  button("预览范围").click(); await wait(() => previews.length === 2);
  previews[1].resolve(preview("港城", previews[1].args[2])); await wait(() => !button("确认范围并继续").disabled);
  changeNumber("累计尝试上限", "10"); await tick();
  check(button("确认范围并继续").disabled && !document.querySelector('[aria-label="冻结范围预览"]'), "Edited attempt limit retained old permission preview");
  button("预览范围").click(); await wait(() => previews.length === 3);
  const frozen = preview("港城", previews[2].args[2]);
  check(frozen.request.maxTaskAttempts === 10, "Changed bound did not reach preview request");
  previews[2].resolve(frozen); await wait(() => !button("确认范围并继续").disabled);
  changeNumber("后继分析轮数", "2"); await tick();
  check(button("确认范围并继续").disabled && !document.querySelector('[aria-label="冻结范围预览"]'), "Edited advisory limit retained an old preview");
  button("预览范围").click(); await wait(() => previews.length === 4);
  const continued = preview("港城", previews[3].args[2]);
  check(continued.request.maxAdvisoryRounds === 2, "Advisory limit did not reach author preview");
  previews[3].resolve(continued); await wait(() => !button("确认范围并继续").disabled);
  const review = { heading: document.querySelector("h2").textContent, author: document.querySelector(".scope-preview p").textContent,
    limits: continued.request, targets: continued.targets, warning: document.querySelector("#scope-description").textContent };
  document.documentElement.dataset.theme = "light";
  window.uiReady = true;
  await new Promise(resolve => setTimeout(resolve, 250));
  const confirm = button("确认范围并继续"); confirm.click(); confirm.click();
  await wait(() => confirmations.length === 1);
  check(starts === 1, "Same-frame confirm started duplicate host executions");
  check(JSON.stringify(confirmations[0].args) === JSON.stringify(["港城", action.sourceTaskId, continued.request, continued.previewDigest]), "Confirmation rewrote frozen author scope");
  persisted = { id: `scope-${continued.previewDigest}`, scope: continued, status: "active", confirmedAt: 100 };
  confirmations[0].reject(new Error("宿主响应丢失"));
  await wait(() => completions.length === 1);
  await wait(() => button("确认范围并继续"));
  check(completions[0].work === null && button("确认范围并继续").disabled, "Unknown execution was incorrectly considered complete or re-authorized");
  check(document.body.textContent.includes("不要重复确认"), "Unknown committed result recovery warning missing");
  check(confirmations.length === 1 && previews.length === 4, "Recovery blindly retried confirmation");
  await render("港城", "reopened");
  button("预览范围").click(); await wait(() => previews.length === 5);
  const reopened = preview("港城", previews[4].args[2]);
  previews[4].resolve(reopened); await wait(() => !button("确认范围并继续").disabled);
  button("确认范围并继续").click(); await wait(() => confirmations.length === 2);
  check(!button("查看执行过程").disabled, "Long execution trapped the author inside the confirmation dialog");
  button("查看执行过程").click(); await tick();
  check(!document.querySelector('[role="dialog"]'), "Author could not return to existing execution controls");
  confirmations[1].resolve({ permit: { ...persisted, scope: reopened }, work: { status: "completed" } });
  await wait(() => completions.length === 2);
  check(completions[1].work.status === "completed", "Closing the dialog lost the owner workflow result");
  window.testResult = { passed: true, review };
})().catch(error => { window.testResult = { passed: false, error: error.stack ?? String(error) }; });
