import React from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import { saveAuthor } from "../helpers/author-session";
import "../../../src/styles.css";

const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (condition, message) => { if (!condition) throw Error(message); };
const until = async (predicate, message) => { for (let i = 0; i < 150 && !predicate(); i++) await tick(); check(predicate(), message); };
window.addEventListener("error", event => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
const calls = { taskReads: 0, models: 0, writes: 0 }, cases = [];
const api = {
  app: { onCloseRequested: () => () => {} },
  runtime: {
    status: async () => ({ state: "online", mode: "local", error: null }),
    listProjects: async () => ({ projects: [] }),
    listWorkTasks: async () => { calls.taskReads++; throw Error("An empty workspace must not read tasks"); },
  },
  agent: {
    status: async () => ({ configured: true, state: "ready", mode: "novel-agent-engine", provider: "fixture", model: "fixture", error: null }),
    listConnections: async () => ({ connections: [] }), recoveryReport: async () => [],
    setWorkspaceContext: async () => {}, onEvent: () => () => {}, onUserInputRequest: () => () => {},
    sendPrompt: async () => { calls.models++; throw Error("An empty workspace must not start a model"); },
    controlWorkTask: async () => { calls.writes++; throw Error("An empty workspace must not control tasks"); },
  },
};
let root;
async function mount(withTaskApi) {
  if (root) { root.unmount(); await tick(); }
  localStorage.clear();
  saveAuthor({ name: "空作品验收作者", email: "empty-fixture@example.invalid", passwordSalt: "fixture", passwordHash: "fixture" });
  window.novelCodex = { ...api, runtime: withTaskApi ? api.runtime : { status: api.runtime.status, listProjects: api.runtime.listProjects } };
  root = createRoot(document.getElementById("root")); root.render(<App />);
  await until(() => document.querySelector('.project-empty-state'), "Actual empty App did not reach its create-project state");
  const rail = document.querySelector<HTMLElement>('.conversation-rail');
  check(rail && !rail.innerText.includes("当前环境未提供任务读取接口") && !rail.querySelector('[role="alert"]'), "Actual empty App invented a task interface failure");
  check(!rail.innerText.includes("正在读取对话与执行记录") && !rail.querySelector('.work-task-panel'), "Actual empty App remained in a task loading state");
  check(![...rail.querySelectorAll('button')].some(button => /重.*读取任务/.test(button.textContent)), "Actual empty App exposed a meaningless task retry action");
  const composer = rail.querySelector<HTMLTextAreaElement>('textarea');
  check(composer?.placeholder === "请先创建或打开作品……", "Actual empty App did not explain the next useful action");
  check(document.querySelector<HTMLButtonElement>('.project-empty-state .primary-button')?.textContent === "新建作品", "Actual empty App lost the create-project action");
  await new Promise(resolve => setTimeout(resolve, 1000));
  check(calls.taskReads === 0 && calls.models === 0 && calls.writes === 0, "Empty App lifecycle triggered a task read or write");
}
(async () => {
  await mount(true); cases.push("fresh empty App");
  window.testScreenshot = "empty workspace quiet conversation";
  await until(() => window.testScreenshot === null, "Empty App screenshot was not captured by the Electron harness");
  await mount(false); cases.push("empty App without task API");
  await mount(true); cases.push("empty App after restart");
  root.unmount(); window.testResult = { passed: true, calls, cases };
})().catch(error => { window.testResult = { passed: false, error: error.stack, dom: document.body.innerText, calls }; });
