import { novelFixtureMedia } from "../helpers/novel-edition";
import { emptyCreationIssueSummary } from "../helpers/creation-issues";
import { chapterRuntimeReads } from "../helpers/chapter-runtime";
import React from "react";
import { createRoot } from "react-dom/client";
import App, { ActionProposalList } from "../../../src/App";
import { clearAuthor, saveAuthor } from "../helpers/author-session";

window.addEventListener("error", (event) => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
const tick = () => new Promise((resolve) => setTimeout(resolve, 50));
const check = (condition, message) => { if (!condition) throw new Error(message); };
const project = { id: "action-proposal-ui", title: "测试作品", logline: "", updated_at: "" };
const state = { project, canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1.0", scenes: [], anchors: [] }, novelEdition: { chapters: [] } };
const proposal = { id: `action-${"a".repeat(32)}`, sourceTaskId: `task-${"b".repeat(32)}`, status: "proposed", operation: "generateBrief", target: { type: "artifact" }, instruction: "把港城的记忆代价写清", inputRevision: 0, intentRevision: 0 };
const declinedProposal = { ...proposal, id: `action-${"c".repeat(32)}`, instruction: "误建议：改写无关章节" };
const thread = { id: "proposal-thread", projectId: project.id, name: "简报建议", preview: null, modelProvider: "fixture",
  status: "idle", createdAt: 100, updatedAt: 100, isPinned: false, ephemeral: false };
const work = { id: "proposal-work", projectId: project.id, threadId: thread.id, goal: "核对港城的记忆代价", kind: "conversation",
  status: "completed", deliveryStatus: "discussion_ended", version: 1, error: null, resumeBlockReason: null, resumeMode: null,
  deliveryReport: { issues: emptyCreationIssueSummary(), chapterAcceptanceReports: [], evidence: [], pendingActionIds: [], unsettledAttemptIds: [] },
  steps: [{ id: "proposal-source", operation: "advisory", expectedOutputKind: null, instruction: null, sourceTaskId: null,
    target: {}, status: "completed", inputRevision: 0, dependsOnStepId: null, currentAttemptId: proposal.sourceTaskId, readingProgress: null,
    attempts: [{ id: proposal.sourceTaskId, attemptNumber: 1, startedAt: 100, completedAt: 101, threadId: thread.id,
      turnId: "proposal-turn", status: "completed", error: null }], outputRefs: [], changes: [] }] };
const approvals = [];
const declines = [];
let pending = [proposal, declinedProposal];
window.novelCodex = { multimedia: novelFixtureMedia,
  app: { onCloseRequested: () => () => {} },
  runtime: { ...chapterRuntimeReads(() => project), listWorkTasks: async () => ({ tasks: [work], nextCursor: null, activeCount: 0 }),
    getTaskFinalReply: async () => null,
    status: async () => ({ state: "online", mode: "local", error: null }),
    listProjects: async () => ({ projects: [project] }),
    getState: async () => state,
  },
  agent: {
    onEvent: () => () => {}, onUserInputRequest: () => () => {},
    status: async () => ({ configured: false, state: "idle", mode: "novel-agent-engine", error: null }),
    listConnections: async () => ({ connections: [] }), recoveryReport: async () => [],
    listExecutions: async () => [], readMessage: async () => { throw Error("message receipt not found"); }, beginInterlude: async () => { throw Error("unexpected interlude"); }, returnInterlude: async () => { throw Error("unexpected interlude return"); }, listMessages: async () => [], listThreads: async () => ({ threads: [thread], currentThreadId: thread.id, nextCursor: null }),
    readConversation: async () => ({ thread, turns: [] }),
    setWorkspaceContext: async () => ({ selected: true }),
    listProposedActions: async () => ({ actions: pending }),
    approveAction: async (...args) => { approvals.push(args); pending = pending.filter((item) => item.id !== args[2]); return {
      action: proposal, taskId: "task-candidate", candidateResults: [{ taskId: "task-candidate", action: { operation: proposal.operation, target: proposal.target } }],
      executionFailure: { message: "后续知识建模待继续，不能据此判断故事事实。", code: "FOLLOWUP_PENDING", sourceTaskId: proposal.sourceTaskId, workTaskId: work.id, stepId: null },
    }; },
    declineAction: async (...args) => { declines.push(args); pending = pending.filter((item) => item.id !== args[2]); return { ...declinedProposal, status: "declined" }; },
  },
};

async function run() {
  localStorage.setItem("novel-codex-active-project", project.id);
  localStorage.setItem(`novel-codex:selected-conversation:${JSON.stringify(["author@example.test", project.id])}`, thread.id);
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
  const namedRoot = createRoot(document.getElementById("root"));
  const namedActions = [
    { ...proposal, id: "local-revision", operation: "generateChapter", target: { chapterNumber: 2,
      scope: { type: "selectedScenes", sourceRevisionId: "revision-letter", groupIds: ["letter"] } }, instruction: "保留其他场景，仅修改密函" },
    { ...proposal, id: "structure-review", operation: "reviewPlanningSeat", target: { artifactKind: "outline", role: "structure" }, instruction: "核对转折依据" },
    { ...proposal, id: "character-review", operation: "reviewPlanningSeat", target: { artifactKind: "plan", role: "character" }, instruction: "核对人物选择" },
    { ...proposal, id: "unknown-operation", operation: "internalFutureOperation", target: {}, instruction: "等待核对操作类别" },
  ];
  namedRoot.render(<ActionProposalList actions={namedActions} busy={false}
    onApprove={value => approvals.push(value)} onDecline={value => declines.push(value)} onScope={() => {}} />);
  await tick();
  const cards = [...document.querySelectorAll(".conversation-proposal")];
  const headings = cards.map(card => card.querySelector("strong"));
  check(headings[0]?.textContent.includes("第 2 章") && headings[0].textContent.includes("选定")
    && headings[0].textContent.includes("场景"), "Selected-scene proposal lost its chapter or exact scope");
  check(headings[1]?.textContent.includes("大纲") && headings[1]?.textContent.includes("结构与因果")
    && headings[2]?.textContent.includes("近章规划") && headings[2]?.textContent.includes("人物与主题"), "Independent review roles remained indistinguishable");
  check(headings.every(heading => heading?.getAttribute("title") === heading?.getAttribute("aria-label")
    && heading?.getAttribute("title")?.includes(heading.textContent.replace(/…$/, ""))), "Readable proposal names lost complete accessible labels");
  check(cards.every((card, index) => card.textContent.includes(namedActions[index].instruction))
    && !headings.some(heading => /internalFutureOperation|generateChapter|reviewPlanningSeat/.test(heading?.textContent ?? "")), "Naming removed actual instructions or exposed internal operation codes");
  check(approvals.length === 1 && declines.length === 1, "Naming triggered another author action");
  namedRoot.unmount();
  window.testResult = { passed: true, activity };
}
void run().catch((error) => { window.testResult = { passed: false, error: error.stack ?? String(error) }; });
