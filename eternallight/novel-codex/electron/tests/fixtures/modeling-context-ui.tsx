import React from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import { clearAuthor, markSignedOut, saveAuthor } from "../../../src/author-session";

window.addEventListener("error", (event) => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
const contexts = [];
const tick = () => new Promise((resolve) => setTimeout(resolve, 40));
const check = (condition, message) => { if (!condition) throw new Error(message); };
const profile = { name: "测试作者", email: "author@example.test", passwordSalt: "test-salt", passwordHash: "test-hash" };
const project = { id: "modeling-ui", title: "测试作品", logline: "", updated_at: "" };
const state = { project, canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1.0", scenes: [], anchors: [] }, novelEdition: { chapters: [] } };
let releaseState;
let delayedState = false;
window.novelCodex = {
  app: { onCloseRequested: () => () => {} },
  runtime: {
    status: async () => ({ state: "online", mode: "local", error: null }),
    listProjects: async () => ({ projects: [project] }),
    getState: async () => { if (delayedState) await new Promise((resolve) => { releaseState = resolve; }); return state; },
  },
  agent: {
    onEvent: () => () => {}, onUserInputRequest: () => () => {},
    status: async () => ({ configured: false, state: "idle", mode: "novel-agent-engine", error: null }),
    listConnections: async () => ({ connections: [] }), recoveryReport: async () => [],
    listProposedActions: async () => ({ actions: [] }),
    setModelingContext: async (context) => { contexts.push(context); return { selected: context.projectId !== null }; },
  },
};

async function run() {
  localStorage.setItem("novel-codex-active-project", project.id);
  saveAuthor(profile); markSignedOut();
  let root = createRoot(document.getElementById("root"));
  root.render(<App />); await tick(); await tick();
  check(contexts.length > 0 && contexts.every((context) => context.projectId === null), "Signed-out author selected a modeling project");
  root.unmount(); await tick();
  contexts.length = 0; saveAuthor(profile); delayedState = true;
  root = createRoot(document.getElementById("root")); root.render(<App />);
  await tick(); await tick();
  check(typeof releaseState === "function", "Project loading did not start");
  check(contexts.every((context) => context.projectId === null), "Placeholder project was scheduled while loading");
  delayedState = false; releaseState(); await tick(); await tick();
  check(contexts.at(-1)?.projectId === project.id, "Loaded signed-in project was not selected");
  document.querySelector('[aria-label="打开用户设置"]').click(); await tick();
  const signOut = [...document.querySelectorAll("button")].find((button) => button.textContent.includes("退出登录"));
  check(signOut, "Sign-out action missing"); signOut.click(); await tick();
  check(contexts.at(-1)?.projectId === null, "Signing out left automatic modeling enabled");
  contexts.length = 0; root.unmount(); await tick();
  check(contexts.length > 0 && contexts.every((context) => context.projectId === null), "Unmount did not release the modeling context");
  clearAuthor(); window.testResult = { passed: true };
}
void run().catch((error) => { window.testResult = { passed: false, error: error.stack ?? String(error) }; });
