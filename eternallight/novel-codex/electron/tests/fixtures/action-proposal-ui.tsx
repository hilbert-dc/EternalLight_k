import React from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import { clearAuthor, saveAuthor } from "../../../src/author-session";

window.addEventListener("error", (event) => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
const tick = () => new Promise((resolve) => setTimeout(resolve, 50));
const check = (condition, message) => { if (!condition) throw new Error(message); };
const project = { id: "action-proposal-ui", title: "测试作品", logline: "", updated_at: "" };
const state = { project, canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1.0", scenes: [], anchors: [] }, novelEdition: { chapters: [] } };
const proposal = { id: `action-${"a".repeat(32)}`, sourceTaskId: `task-${"b".repeat(32)}`, status: "proposed", operation: "generateBrief", target: { type: "artifact" }, instruction: "把港城的记忆代价写清", inputRevision: 0, intentRevision: 0 };
const declinedProposal = { ...proposal, id: `action-${"c".repeat(32)}`, instruction: "误建议：改写无关章节" };
const approvals = [];
const declines = [];
let pending = [proposal, declinedProposal];
window.novelCodex = {
  app: { onCloseRequested: () => () => {} },
  runtime: {
    status: async () => ({ state: "online", mode: "local", error: null }),
    listProjects: async () => ({ projects: [project] }),
    getState: async () => state,
  },
  agent: {
    onEvent: () => () => {}, onUserInputRequest: () => () => {},
    status: async () => ({ configured: false, state: "idle", mode: "novel-agent-engine", error: null }),
    listConnections: async () => ({ connections: [] }), recoveryReport: async () => [],
    setModelingContext: async () => ({ selected: true }),
    listProposedActions: async () => ({ actions: pending }),
    approveAction: async (...args) => { approvals.push(args); pending = pending.filter((item) => item.id !== args[2]); return { action: proposal, taskId: "task-candidate", modeling: { status: "model_pending" }, followup: { status: "pending" } }; },
    declineAction: async (...args) => { declines.push(args); pending = pending.filter((item) => item.id !== args[2]); return { ...declinedProposal, status: "declined" }; },
  },
};

async function run() {
  localStorage.setItem("novel-codex-active-project", project.id);
  saveAuthor({ name: "测试作者", email: "author@example.test", passwordSalt: "test-salt", passwordHash: "test-hash" });
  const root = createRoot(document.getElementById("root"));
  root.render(<App />);
  await tick(); await tick();
  const proposalPanel = document.querySelector('[aria-label="待确认的创作建议"]');
  check(proposalPanel?.textContent.includes(proposal.instruction), "Model proposal was not shown to the author");
  check(approvals.length === 0, "Proposal executed without author confirmation");
  const reject = [...proposalPanel.querySelectorAll("button")].find((button) => button.textContent.includes("拒绝建议") && button.closest("article")?.textContent.includes(declinedProposal.instruction));
  check(reject && !reject.disabled, "Author rejection control is unavailable");
  reject.click();
  await tick(); await tick();
  check(JSON.stringify(declines) === JSON.stringify([[project.id, declinedProposal.sourceTaskId, declinedProposal.id]]), "Rejection did not bind the exact proposal");
  check(approvals.length === 0 && !document.querySelector('[aria-label="待确认的创作建议"]')?.textContent.includes(declinedProposal.instruction), "Rejected proposal still appeared executable");
  check(document.querySelector('[aria-label="待确认的创作建议"]')?.textContent.includes(proposal.instruction), "Independent proposal was removed by rejection");
  check(document.querySelector('.activity-strip')?.textContent.includes("已拒绝候选修改"), "Rejection was not reported to the author");
  const currentPanel = document.querySelector('[aria-label="待确认的创作建议"]');
  const approve = [...currentPanel.querySelectorAll("button")].find((button) => button.textContent.includes("批准生成候选"));
  check(approve && !approve.disabled, "Author approval control is unavailable");
  approve.click();
  await tick(); await tick();
  check(JSON.stringify(approvals) === JSON.stringify([[project.id, proposal.sourceTaskId, proposal.id]]), "Approval did not bind the exact proposal");
  check(!document.querySelector('[aria-label="待确认的创作建议"]'), "Approved proposal remained pending");
  const activity = document.querySelector('.activity-strip')?.textContent?.trim();
  check(activity?.includes("后续知识建模待继续") && !activity.includes("已生成可审阅候选"), "Partial follow-up was incorrectly shown as finished");
  root.unmount(); clearAuthor();
  window.testResult = { passed: true, activity };
}
void run().catch((error) => { window.testResult = { passed: false, error: error.stack ?? String(error) }; });
