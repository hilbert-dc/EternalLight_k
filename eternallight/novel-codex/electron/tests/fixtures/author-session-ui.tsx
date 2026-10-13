import { novelFixtureMedia } from "../helpers/novel-edition";
import { chapterRuntimeReads } from "../helpers/chapter-runtime";
import React, { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import { LoginScreen } from "../../../src/LoginScreen";
import { authorSurfaceMatrix } from "../helpers/page-geometry";
import "../../../src/styles.css";

const tick = () => new Promise(resolve => setTimeout(resolve, 30));
const check = (value, message) => { if (!value) throw Error(message); };
const until = async predicate => { for (let i = 0; i < 100 && !predicate(); i++) await tick(); check(predicate(), "session UI did not settle"); };
const profile = { name: "持久作者", email: "author@example.invalid", passwordSalt: "fixture", passwordHash: "c".repeat(64) };
const project = { id: "session-ui", title: "持久作品", logline: "", updated_at: "" };
const state = { project, canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1.0", scenes: [], anchors: [] }, novelEdition: { chapters: [] } };
const reads = [];
let snapshot = { profile, signedIn: true }, failRead = false, holdRead = true, failWrite = false;
window.novelAuthorSession = {
  read: async () => { if (failRead) throw Error("会话读取失败"); return holdRead ? new Promise((resolve, reject) => reads.push({ resolve, reject })) : structuredClone(snapshot); },
  save: async next => { if (failWrite) throw Error("会话保存失败"); snapshot = { profile: next, signedIn: true }; return structuredClone(snapshot); },
  signOut: async () => { if (failWrite) throw Error("会话保存失败"); snapshot = { ...snapshot, signedIn: false }; return structuredClone(snapshot); },
  clear: async () => { if (failWrite) throw Error("会话保存失败"); snapshot = { profile: null, signedIn: false }; return structuredClone(snapshot); },
};
window.novelCodex = { multimedia: novelFixtureMedia,
  app: { onCloseRequested: () => () => {} },
  runtime: { ...chapterRuntimeReads(() => project), listWorkTasks: async () => ({ tasks: [], nextCursor: null, activeCount: 0 }),
    ...Object.fromEntries(["Search", "Entity", "Relation", "Knowledge", "SnapshotKnowledge", "Fact"].map(kind => [`get${kind}Audit`, async () => ({ job: null })])), status: async () => ({ state: "online", mode: "local", error: null }), listProjects: async () => ({ projects: [project] }), getState: async () => state },
  agent: { onEvent: () => () => {}, onUserInputRequest: () => () => {}, status: async () => ({ configured: false, state: "idle", mode: "novel-agent-engine", error: null }),
    listConnections: async () => ({ connections: [] }), listExecutions: async () => [], readMessage: async () => { throw Error("message receipt not found"); }, beginInterlude: async () => { throw Error("unexpected interlude"); }, returnInterlude: async () => { throw Error("unexpected interlude return"); }, listMessages: async () => [], listThreads: async () => ({ threads: [], nextCursor: null }), recoveryReport: async () => [], listProposedActions: async () => ({ actions: [] }), setWorkspaceContext: async () => ({ selected: false }) },
};
window.addEventListener("error", event => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
const button = text => [...document.querySelectorAll("button")].find(node => node.textContent === text);
const loginShown = () => Boolean(document.querySelector(".login-card form, form.login-card"));
const geometry = [], brands = [];
const appTheme = async mode => { if (document.documentElement.dataset.theme !== mode) { const control = document.querySelector<HTMLElement>(`.theme-choice input[value="${mode}"]`) ?? document.querySelector<HTMLElement>(".theme-toggle, .login-theme"); check(control, "Theme control missing"); control.click(); } await until(() => document.documentElement.dataset.theme === mode); };
async function run() {
  localStorage.setItem("novel-codex-active-project", project.id);
  let root = createRoot(document.getElementById("root"));
  root.render(<StrictMode><App /></StrictMode>);
  await until(() => reads.length >= 2);
  const loadingSnapshot = { text: document.querySelector('[role="status"]').textContent.replace(/\s+/g, " ").trim(), loginShown: loginShown() };
  check(!loginShown(), "login flashed before restore finished");
  check(loadingSnapshot.text.startsWith("小说工坊"), "Restore displayed a different application brand"); brands.push({ surface: "restore", name: document.querySelector("h1").textContent });
  reads[0].reject(Error("stale restore must not replace the latest read"));
  reads.at(-1).resolve(snapshot); holdRead = false;
  await until(() => document.querySelector('[aria-label="打开用户设置"]'));
  check(document.querySelector(".brand-lockup").textContent.includes("小说工坊") && document.querySelector(".project-switcher span[title]").textContent === project.title, "Brand rename lost the restored project"); brands.push({ surface: "workspace", name: document.querySelector(".brand-lockup > span").textContent });
  check(!document.body.textContent.includes("stale restore"), "StrictMode stale restore leaked");
  check(!document.querySelector('.workspace-diagnostics'), "Writing showed technical diagnostics");
  document.querySelector('[aria-label="打开用户设置"]').click(); await tick();
  await until(() => document.querySelector('[aria-label="当前作品诊断"] .workspace-diagnostics'));
  check(document.querySelector('[aria-label="当前作品诊断"]').textContent.includes(project.title), "Diagnostics lost their owning project");
  check(document.querySelectorAll('.workspace-diagnostic-panels > details').length === 0 && !document.querySelector('.workspace-diagnostics').open, "Settings lost collapsed diagnostic controls");
  geometry.push(...await authorSurfaceMatrix(".user-settings", "settings", appTheme));
  check(document.querySelectorAll('input[name="appearance"]:checked').length === 1, "Native appearance radios did not retain a unique choice");
  failWrite = true; button("退出登录").click();
  await until(() => document.querySelector('[role="alert"]'));
  check(!loginShown() && snapshot.signedIn, "failed logout changed active session");
  const logoutFailure = document.querySelector('[role="alert"]').textContent;
  failWrite = false; button("退出登录").click();
  await until(loginShown);
  root.unmount(); await tick();
  root = createRoot(document.getElementById("root")); root.render(<App />); await until(loginShown);
  check(document.querySelector("h1").textContent === "登录", "logout was lost on remount");
  check(document.querySelector(".login-brand strong").textContent === "小说工坊", "Login brand did not match workspace"); brands.push({ surface: "login", name: document.querySelector(".login-brand strong").textContent });
  geometry.push(...await authorSurfaceMatrix(".login-card", "login", appTheme));
  root.unmount(); await tick();
  failRead = true; root = createRoot(document.getElementById("root")); root.render(<App />);
  await until(() => button("重试"));
  const restoreFailure = { text: document.querySelector('[role="status"] p').textContent, loginShown: loginShown() };
  check(!loginShown(), "read failure was disguised as first-run registration");
  failRead = false; snapshot = { profile, signedIn: true }; button("重试").click();
  await until(() => document.querySelector('[aria-label="打开用户设置"]'));
  root.unmount(); await tick();
  root = createRoot(document.getElementById("root"));
  root.render(<LoginScreen existing={null} theme="light" onToggleTheme={() => {}} onSignedIn={async () => { throw Error("会话保存失败"); }} />);
  await tick();
  geometry.push(...await authorSurfaceMatrix(".login-card", "registration", async theme => { document.documentElement.dataset.theme = theme; root.render(<LoginScreen existing={null} theme={theme as "light" | "dark"} onToggleTheme={() => {}} onSignedIn={async () => { throw Error("会话保存失败"); }} />); await tick(); }));
  const values = ["新作者", "new@example.invalid", "synthetic-password", "synthetic-password"];
  [...document.querySelectorAll("input")].forEach((input, index) => { Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(input, values[index]); input.dispatchEvent(new Event("input", { bubbles: true })); });
  await tick(); document.querySelector("form").dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  await until(() => document.querySelector('[role="alert"]'));
  check(document.querySelector('[role="alert"]').textContent === "会话保存失败", "failed sign-in was not reported");
  check(!document.querySelector('button[type="submit"]').disabled, "failed sign-in cannot be retried");
  root.unmount();
  window.testResult = { passed: true, loadingSnapshot, logoutFailure, restoreFailure, geometry, brands };
}
void run().catch(error => { window.testResult = { passed: false, error: error.stack ?? String(error) }; });
