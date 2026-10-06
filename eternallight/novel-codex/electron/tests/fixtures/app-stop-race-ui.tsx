import { chapterRuntimeReads } from "../helpers/chapter-runtime";
import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import { saveAuthor } from "../helpers/author-session";

const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (value, message) => { if (!value) throw Error(message); };
const until = async predicate => { for (let i = 0; i < 180 && !predicate(); i++) await tick(); check(predicate(), "Stop race UI did not settle"); };
const deferred = () => { let resolve, reject; const promise = new Promise((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; };
const project = { id: "project-last-lamp", title: "停止竞态作品", logline: "", updated_at: "" };
const state = { project, canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1.0", scenes: [], anchors: [] }, novelEdition: { chapters: [] } };
const calls = { model: 0, interrupt: 0 }, snapshots = [];
let listener, prompt, stop, turnId, phase = "boot";
const event = (method, params) => listener({ method, params });
window.novelCodex = {
  app: { onCloseRequested: () => () => {} },
  runtime: { ...chapterRuntimeReads(() => project), listWorkTasks: async () => ({ tasks: [], nextCursor: null, activeCount: 0 }), status: async () => ({ state: "online", mode: "local", error: null }), listProjects: async () => ({ projects: [project] }), getState: async () => state },
  agent: { status: async () => ({ configured: true, state: "ready", mode: "novel-agent-engine", error: null }), listConnections: async () => ({ connections: [] }), recoveryReport: async () => [], listProposedActions: async () => ({ actions: [] }), setWorkspaceContext: async () => {}, listThreads: async () => ({ threads: [], nextCursor: null }),
    onEvent: fn => { listener = fn; return () => {}; }, onUserInputRequest: () => () => {},
    sendPrompt: async () => { calls.model++; turnId = `turn-${calls.model}`; prompt = deferred(); event("thread/started", { thread: { id: "thread" } }); await tick(); event("turn/started", { threadId: "thread", turn: { id: turnId } }); if (phase !== "reject-before-receipt") event("item/started", { threadId: "thread", turnId, item: { id: `read-${turnId}`, type: "mcpToolCall", server: "novel_workspace", tool: "read_context" } }); return prompt.promise; },
    interrupt: () => { calls.interrupt++; stop = deferred(); return stop.promise; },
  },
};
const root = createRoot(document.getElementById("root"));
const stopButton = () => document.querySelector('[aria-label="停止当前执行"]');
const wrapper = "Error invoking remote method 'agent:send-prompt': Error: 生成已停止";
const currentTimeline = () => document.querySelector(`[data-conversation-detail="turn:thread:${turnId}"]`);
const send = async () => { await until(() => document.querySelector('[aria-label="发送创作指令"]')); const input = document.querySelector('[aria-label="创作指令"]'); Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(input, `第${calls.model + 1}轮讨论`); input.dispatchEvent(new Event("input", { bubbles: true })); await tick(); document.querySelector('[aria-label="发送创作指令"]').click(); await until(() => stopButton() && prompt); await tick(); };
const stopped = async label => { await until(() => currentTimeline()?.classList.contains('is-cancelled')); await tick(); check(!document.querySelector('.transcript-error') && !document.querySelector('.agent-status-bar [role="alert"]'), `${label}: normal stop became failure`); check(!document.body.textContent.includes(wrapper), `${label}: wrapped cancellation leaked`); snapshots.push({ case: label, summary: currentTimeline().querySelector('summary').textContent.replace(/\d+秒/g, "<elapsed>") }); };
const finish = async () => { event("turn/completed", { threadId: "thread", turn: { id: turnId, status: "completed", items: [] } }); prompt.resolve({ message: "本轮完成", proposedActions: [] }); await until(() => !stopButton()); await tick(); };
(async () => { try {
  saveAuthor({ name: "停止作者", email: "fixture@example.invalid", passwordSalt: "fixture", passwordHash: "fixture" }); root.render(<App />);
  phase = "reject-before-receipt"; await send(); const button = stopButton(); button.click(); button.click(); await tick(); check(calls.interrupt === 1, "duplicate stop IPC"); prompt.reject(Error(wrapper)); await tick(); check(!document.querySelector('.transcript-error'), "rejection ended execution before stop receipt"); stop.resolve({ interrupted: true }); await stopped("reject-before-receipt");
  phase = "receipt-before-reject"; await send(); stopButton().click(); await tick(); stop.resolve({ interrupted: true }); await stopped("receipt-before-reject"); prompt.reject(Error(wrapper)); await tick(); await stopped("late-reject");
  phase = "native-before-receipt"; await send(); stopButton().click(); await tick(); const lateStop = stop; event("turn/completed", { threadId: "thread", turn: { id: turnId, status: "interrupted" } }); prompt.reject(Error(wrapper)); await stopped("native-before-receipt");
  phase = "late-new-scope"; await send(); const nextTurn = turnId; lateStop.resolve({ interrupted: true }); await tick(); check(stopButton() && turnId === nextTurn && !currentTimeline()?.classList.contains('is-error'), "old stop receipt ended new scope");
  const retainedStop = document.querySelector('.is-earlier [data-conversation-detail="turn:thread:turn-3"]');
  check(retainedStop?.classList.contains('is-cancelled') && !retainedStop.open && retainedStop.querySelector('.conversation-work-item.cancelled'), "Next author request discarded or falsified the stopped execution process"); await finish();
  phase = "provider-error"; await send(); stopButton().click(); await tick(); prompt.reject(Error("Error invoking remote method 'agent:send-prompt': Error: 真实供应商输出失败")); await until(() => document.querySelector('.transcript-error')?.textContent.includes("真实供应商输出失败")); stop.resolve({ interrupted: true }); await tick(); check(document.querySelector('.transcript-error')?.textContent.includes("真实供应商输出失败"), "stop receipt hid real provider failure");
  phase = "stop-error"; await send(); stopButton().click(); await tick(); stop.reject(Error("真实停止接口失败")); await until(() => document.querySelector('.transcript-error')?.textContent.includes("真实停止接口失败")); prompt.reject(Error(wrapper)); await tick(); check(document.querySelector('.transcript-error')?.textContent.includes("真实停止接口失败"), "cancellation rejection hid stop failure");
  phase = "false-receipt-retry"; await send(); stopButton().click(); await tick(); stop.resolve({ interrupted: false }); await tick(); stopButton().click(); await tick(); check(calls.interrupt === 7, "false receipt permanently blocked another stop"); stop.resolve({ interrupted: true }); prompt.reject(Error(wrapper)); await stopped("false-receipt-retry");
  phase = "unconfirmed-message"; await send(); prompt.reject(Error(wrapper)); await until(() => document.querySelector('.transcript-error')?.textContent.includes("生成已停止")); check(!currentTimeline()?.classList.contains('is-cancelled'), "unconfirmed cancellation text fabricated stop success");
  check(calls.model === 8 && calls.interrupt === 7, "stop race started an extra model or interrupt"); root.unmount(); window.testResult = { passed: true, calls, snapshots };
} catch (error) { window.testResult = { passed: false, error: `${phase}: ${error.stack}`, calls, dom: document.body.innerText.slice(0, 12000) }; } })();
