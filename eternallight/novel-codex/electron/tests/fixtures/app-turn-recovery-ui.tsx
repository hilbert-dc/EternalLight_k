import { chapterRuntimeReads } from "../helpers/chapter-runtime";
import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import { saveAuthor } from "../helpers/author-session";
const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (value, message) => { if (!value) throw Error(message); };
const until = async predicate => { for (let i = 0; i < 180 && !predicate(); i++) await tick(); check(predicate(), "Recovery UI did not settle"); };
const project = { id: "project-last-lamp", title: "连接恢复作品", logline: "", updated_at: "" };
const state = { project, canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1.0", scenes: [], anchors: [] }, novelEdition: { chapters: [] } };
const calls = { model: 0, reconnect: 0 }, snapshots = [];
let listener, settleTurn, settleRead, turnId;
const event = (method, params) => listener({ method, params });
window.novelCodex = {
  app: { onCloseRequested: () => () => {} },
  runtime: { ...chapterRuntimeReads(() => project), listWorkTasks: async () => ({ tasks: [], nextCursor: null, activeCount: 0 }), status: async () => ({ state: "online", mode: "local", error: null }), listProjects: async () => ({ projects: [project] }), getState: async () => state },
  agent: { status: async () => ({ configured: true, state: "ready", mode: "novel-agent-engine", error: null }), listConnections: async () => ({ connections: [] }),
    recoveryReport: async () => [], listProposedActions: async () => ({ actions: [] }), setWorkspaceContext: async () => {}, listThreads: async () => ({ threads: [], nextCursor: null }),
    onEvent: fn => { listener = fn; return () => {}; }, onUserInputRequest: () => () => {},
    sendPrompt: async () => { calls.model++; turnId = `turn-${calls.model}`; event("thread/started", { thread: { id: "thread" } }); await tick(); event("turn/started", { threadId: "thread", turn: { id: turnId } }); await new Promise(resolve => { settleTurn = resolve; }); return { message: "原请求已保存", proposedActions: [] }; },
    reconnectThread: async (_project, thread, expectedTurn) => { calls.reconnect++; check(thread === "thread" && expectedTurn === turnId, "recovery lost its source turn"); return new Promise((resolve, reject) => { settleRead = { resolve, reject, turnId: expectedTurn }; }); },
  },
};
const root = createRoot(document.getElementById("root"));
const send = async () => { const input = document.querySelector('[aria-label="创作指令"]'); Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(input, "有限范围修改"); input.dispatchEvent(new Event("input", { bubbles: true })); await tick(); document.querySelector('[aria-label="发送创作指令"]').click(); await until(() => document.querySelector('[aria-label="停止当前执行"]')); };
const retry = async () => { event("agent/connection", { state: "reconnecting" }); event("error", { threadId: "thread", turnId, willRetry: true, message: "服务正在重试" }); await until(() => document.querySelector('.status-reconnect')); const button = document.querySelector('.status-reconnect'); button.click(); button.click(); await tick(); };
const finish = async () => { event("turn/completed", { threadId: "thread", turn: { id: turnId, status: "completed", items: [] } }); settleTurn(); await until(() => !document.querySelector('[aria-label="停止当前执行"]')); await tick(); };
(async () => { try {
  saveAuthor({ name: "恢复作者", email: "fixture@example.invalid", passwordSalt: "fixture", passwordHash: "fixture" }); root.render(<App />);
  await until(() => document.querySelector('[aria-label="发送创作指令"]')); await send(); await retry(); check(calls.reconnect === 1, "duplicate reconnect dispatched");
  settleRead.reject(Error("受控状态读取失败")); await until(() => document.querySelector('[aria-label="原任务连接核对"]')?.textContent.includes("受控状态读取失败"));
  check(document.querySelector('[aria-label="停止当前执行"]') && !document.body.textContent.includes("执行失败"), "read failure ended the original turn"); snapshots.push(document.querySelector('[aria-label="原任务连接核对"]').textContent);
  await finish(); check(!document.querySelector('.status-reconnect') && !document.body.textContent.includes("执行失败"), "successful delivery left a false execution failure");
  await send(); await retry(); const late = settleRead; await finish(); await send();
  late.resolve({ thread: { id: "thread" }, recovery: { turnId: late.turnId, state: "completed" } }); await tick();
  check(document.querySelector('[aria-label="停止当前执行"]') && calls.model === 3 && calls.reconnect === 2, "late recovery ended the next turn or started a model retry");
  await retry(); settleRead.resolve({ thread: { id: "thread" }, recovery: { turnId, state: "running" } }); await until(() => !document.querySelector('.status-reconnect'));
  check(document.querySelector('[aria-label="停止当前执行"]'), "active recovery lost original execution"); await finish();
  await send(); await retry(); const racing = settleRead; await finish();
  const completedDuration = document.querySelector('.conversation-timeline.is-completed summary').textContent;
  const realNow = Date.now; Date.now = () => realNow() + 60000;
  racing.resolve({ thread: { id: "thread" }, recovery: { turnId: racing.turnId, state: "running" } }); await tick();
  check(document.querySelector('.conversation-timeline.is-completed summary')?.textContent === completedDuration
    && !document.querySelector('[aria-label="停止当前执行"]'), "late running receipt erased the completed turn duration");
  Date.now = realNow;
  await send();
  event("turn/completed", { threadId: "thread", turn: { id: turnId, status: "failed", error: { message: "受控模型输出过限" } } });
  settleTurn(); await until(() => !document.querySelector('[aria-label="停止当前执行"]'));
  const failedDuration = document.querySelector('.conversation-timeline.is-error summary').textContent;
  check(!document.querySelector('.status-reconnect')
    && document.querySelector('.agent-status-bar').textContent.includes('本轮任务未完成；创作引擎已连接')
    && !document.querySelector('.agent-status-bar').textContent.includes('未连接'), 'task failure falsely disconnected a ready transport');
  event('agent/connection', { state: 'error' }); await until(() => document.querySelector('.status-reconnect'));
  document.querySelector('.status-reconnect').click(); await tick();
  settleRead.reject(Error("终态任务不能恢复为普通对话"));
  await until(() => document.querySelector('[aria-label="原任务连接核对"]')?.textContent.includes("此次核对没有重新生成"));
  check(!document.querySelector('[aria-label="停止当前执行"]')
    && !document.querySelector('.sr-only[role="status"]').textContent.includes("恢复")
    && !document.querySelector('.agent-status-bar').textContent.includes("等待恢复")
    && document.querySelector('.conversation-timeline.is-error summary').textContent === failedDuration
    && document.body.textContent.includes("受控模型输出过限"), "terminal read failure lost the original result or left a false reconnecting state");
  snapshots.push(document.querySelector('[aria-label="原任务连接核对"]').textContent);
  await send(); check(calls.model === 6 && calls.reconnect === 5, "terminal recovery blocked a new instruction or duplicated generation"); await finish();
  root.unmount(); window.testResult = { passed: true, calls, snapshots };
} catch (error) { window.testResult = { passed: false, error: error.stack }; } })();
