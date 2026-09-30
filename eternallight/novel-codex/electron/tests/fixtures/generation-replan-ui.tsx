import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { GenerationReplanPanel } from "../../../src/GenerationReplanPanel";

const tick = () => new Promise(resolve => setTimeout(resolve, 50));
const check = (value, message) => { if (!value) throw new Error(message); };
const click = async label => { const button = [...document.querySelectorAll("button")].find(item => item.textContent === label); check(button && !button.disabled, `Unavailable ${label}`); button.click(); await tick(); };
const runId = "generation-" + "a".repeat(32);
let source = "b".repeat(64); let fail = false; let defer = false; let release; let submissions = []; let confirmed = []; let chapterMode = false;
const preview = projectId => ({ run: { id: runId, authorMessageId: "author-message-generation-replan-ui", projectId, version: 7, status: "paused", stage: chapterMode ? "draft" : "outline", activeTaskId: chapterMode ? "failed-attempt" : null, currentChapterId: chapterMode ? "chapter-1" : null, pendingCurrentIntentGeneration: null, maxTaskAttempts: 12, usedTaskAttempts: 2, providerBudget: { maxRequests: 100, maxTokens: 100_000, usedRequests: 7, usedTokens: 12_000, unknownTokens: 2_000, overrun: false, charges: [] }, completedChapters: 0, targetChapters: 1, deadlineAt: 2000000000 }, projectRevision: 2, sourceDigest: source, availableFrom: chapterMode ? ["currentChapter"] : ["brief", "outline"], activeAttemptStatus: chapterMode ? "failed" : null });
const api = { runtime: {
  previewGenerationReplan: async projectId => defer ? new Promise(resolve => { release = resolve; }) : preview(projectId),
  replanGeneration: async (projectId, id, request) => { submissions.push({ projectId, id, request }); if (fail) throw new Error("作品已变化"); return { ...preview(projectId).run, status: "running" }; },
} };
const root = createRoot(document.getElementById("root"));
const render = projectId => root.render(<main style={{ width: "min(640px, 90vw)", margin: "40px auto" }}><GenerationReplanPanel api={api} projectId={projectId} runId={runId} disabled={false} onConfirmed={value => confirmed.push(value)} /></main>);
(async () => {
  render("project"); await tick(); await click("预览重规划");
  check(document.body.textContent.includes("剩余 10 次任务尝试") && document.body.textContent.includes("剩余 93 次请求") && document.body.textContent.includes("保留当前简报"), "Missing decision context");
  const input = document.querySelector("textarea");
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(input, "按新藏信设定重新规划"); input.dispatchEvent(new Event("input", { bubbles: true })); await tick();
  window.uiReady = true; await new Promise(resolve => setTimeout(resolve, 400));
  const snapshot = {
    title: document.querySelector("strong").textContent,
    paragraphs: [...document.querySelectorAll("section p")].map(item => item.textContent),
    options: [...document.querySelectorAll("option")].map(item => ({ label: item.textContent, selected: item.selected })),
    buttons: [...document.querySelectorAll("button")].map(item => ({ label: item.textContent, disabled: item.disabled })),
    instruction: document.querySelector("textarea").value,
  };
  source = "c".repeat(64); fail = true; await click("确认重规划");
  check(submissions[0].request.expectedSourceDigest === "b".repeat(64) && submissions[0].request.expectedVersion === 7, "Submission refreshed preview silently");
  check(document.body.textContent.includes("作品已变化") && !document.body.textContent.includes("确认重规划") && confirmed.length === 0, "Failure retained authority");
  fail = false; await click("预览重规划");
  check(document.querySelector("textarea").value === "按新藏信设定重新规划", "Failure lost author instruction");
  await click("确认重规划"); check(confirmed.length === 1 && submissions[1].request.expectedSourceDigest === source, "Explicit refresh failed");
  chapterMode = true; await click("预览重规划");
  const chapterSnapshot = { range: document.querySelector("select").value, description: [...document.querySelectorAll("section p")].map(item => item.textContent) };
  await click("确认重规划");
  check(submissions[2].request.restartFrom === "currentChapter" && confirmed.length === 2, "Edited chapter could not rebind a terminal attempt");
  defer = true; await click("预览重规划"); check(release, "No pending preview");
  render("other-project"); await tick(); release(preview("project")); await tick();
  check(!document.body.textContent.includes("确认重规划"), "Late preview crossed projects");
  root.unmount(); window.testResult = { passed: true, snapshot, chapterSnapshot };
})().catch(error => { window.testResult = { passed: false, error: error.stack }; });
