import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import { saveAuthor } from "../helpers/author-session";

window.addEventListener("error", event => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
const tick = () => new Promise(resolve => setTimeout(resolve, 30));
const check = (value, message) => { if (!value) throw new Error(message); };
const until = async (predicate, label) => { for (let i = 0; i < 100 && !predicate(); i++) await tick(); check(predicate(), `Event UI did not settle: ${label}`); };
const project = { id: "project-event-ui", title: "事件归属夹具", logline: "仅本地测试", updated_at: "" };
const state = { project, canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1.0", scenes: [], anchors: [] }, novelEdition: { chapters: [] } };
const workspace = { project, revision: 1, headSnapshotId: "ui-snapshot", sourceDigest: "a".repeat(64), index: { status: "ready" },
  artifacts: Object.fromEntries(["brief", "outline", "plan"].map(kind => [kind, { candidate: null, accepted: null, locked: false }])),
  chapters: [], workingBaselines: [], continuationIntents: [], planningReviews: [], commits: [], tasks: [], modeling: [], factProposals: [], reviews: [], chapterPlanBindings: [] };
saveAuthor({ name: "事件夹具作者", email: "event@example.invalid", passwordSalt: "fixture", passwordHash: "fixture" });
let onEvent, release, turnNumber = 0;
const event = (method, params, turnId = `turn-${turnNumber}`) => onEvent({ method, params: { threadId: "event-thread", turnId, ...params } });
const message = (id, text, phase) => ({ id, type: "agentMessage", text, phase });
const currentTurn = () => [...document.querySelectorAll(".conversation-turn")].at(-1);
window.novelCodex = {
  app: { onCloseRequested: () => () => {} },
  runtime: { listChapterStructureChanges: async (projectId: string) => ({ projectId, items: [], nextCursor: null }),
  getWorkCatalog: async () => ({ revision: 0, volumes: [], placements: [] }),
    status: async () => ({ state: "online", mode: "local", error: null }), listProjects: async () => ({ projects: [project] }),
    getState: async () => structuredClone(state), getCreativeWorkspace: async () => structuredClone(workspace),
    listPlanningImpacts: async () => ({ data: [], nextCursor: null, counts: { open: 0 } }), listGenerationRuns: async () => ({ runs: [] }),
    listWorkTasks: async () => ({ tasks: [], activeCount: 0, nextCursor: null }),
    ...Object.fromEntries(["Search", "Entity", "Relation", "Knowledge", "SnapshotKnowledge", "Fact"].map(kind => [`get${kind}Audit`, async () => ({ job: null })])),
  },
  agent: {
    status: async () => ({ configured: true, state: "ready", mode: "novel-agent-engine", error: null, provider: "fixture", model: "fixture-model" }),
    listConnections: async () => ({ connections: [] }), recoveryReport: async () => [], listThreads: async () => ({ threads: [], nextCursor: null }),
    listProposedActions: async () => ({ actions: [] }), setWorkspaceContext: async () => {}, onUserInputRequest: () => () => {},
    onEvent: listener => { onEvent = listener; return () => { onEvent = null; }; },
    sendPrompt: async () => {
      const id = `turn-${++turnNumber}`;
      event("turn/started", { turn: { id } });
      await new Promise(resolve => { release = resolve; });
      return { message: `第${turnNumber}轮最终结论`, proposedActions: [] };
    },
  },
};
const root = createRoot(document.getElementById("root"));
root.render(<App />);
(async () => {
  await until(() => onEvent && document.body.textContent.includes(project.title) && document.querySelector('textarea[aria-label="创作指令"]')?.disabled === false, "initial project");
  await tick();
  const snapshots = [];
  for (let index = 1; index <= 2; index++) {
    release = null;
    const input = document.querySelector('textarea[aria-label="创作指令"]');
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(input, `第${index}轮分析`);
    input.dispatchEvent(new Event("input", { bubbles: true })); await tick();
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await until(() => release, `send ${index}`);
    const tools = ["resolve_story_evidence", "simulate_story_hypothesis", "request_planning_review"];
    for (const name of tools) {
      event("item/started", { item: { id: `${name}-${index}`, type: "mcpToolCall", server: "novel_workspace", tool: name } });
      await tick();
      event("item/completed", { item: { id: `${name}-${index}`, type: "mcpToolCall", server: "novel_workspace", tool: name } });
    }
    event("item/completed", { item: message(`progress-${index}`, "这是过程消息", "commentary") });
    event("item/completed", { item: message(`reply-${index}`, `第${index}轮最终结论`, "final_answer") });
    event("item/completed", { item: message(`tail-${index}`, "这也是过程消息", "commentary") });
    event("turn/completed", { turn: { id: "old-turn", status: "completed", items: [message("old", "错误的旧回答", "final_answer")] } }, "old-turn");
    event("agent/error", { message: "错误的旧报错" }, "old-turn");
    event("thread/status/changed", { status: { type: "idle" } }, "old-turn");
    await tick();
    check(document.querySelector(".conversation-turn.is-running"), "old events completed the current turn");
    check(!document.querySelector(".transcript-error"), "old error contaminated the current turn");
    event("turn/completed", { turn: { id: `turn-${index}`, status: "completed", items: [
      message(`progress-${index}`, "这是过程消息", "commentary"), message(`reply-${index}`, `第${index}轮最终结论`, "final_answer"),
      message(`tail-${index}`, "这也是过程消息", "commentary"),
    ] } });
    release();
    await until(() => currentTurn()?.classList.contains("is-completed"), `completion ${index}`);
    const turn = currentTurn();
    const answer = turn.querySelector(".conversation-answer")?.textContent;
    check(answer === `第${index}轮最终结论`, `wrong final reply: ${answer}`);
    check(!turn.textContent.includes("错误的旧"), "stale progress entered the current transcript");
    const toolTitles = [...turn.querySelectorAll(".conversation-work-copy strong")].map(node => node.textContent).filter(title => ["定位正文依据", "分析故事假设", "安排多视角规划检查"].includes(title));
    check(toolTitles.length === 3 && !turn.textContent.includes("simulate_story_hypothesis"), "tool lifecycle exposed internal names or lost steps");
    snapshots.push({ state: "completed", prompt: turn.querySelector(".user-message p").textContent, answer, toolTitles });
    await until(() => document.querySelector('textarea[aria-label="创作指令"]')?.placeholder.includes("想如何"), `ready ${index}`);
    await tick();
  }
  root.unmount();
  window.testResult = { passed: true, snapshots };
})().catch(error => { window.testResult = { passed: false, error: error.stack }; });
