import { novelFixtureMedia } from "../helpers/novel-edition";
import { chapterRuntimeReads } from "../helpers/chapter-runtime";
import React from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import { clearAuthor, markSignedOut, saveAuthor } from "../helpers/author-session";

window.addEventListener("error", (event) => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
const contexts = [];
const tick = () => new Promise((resolve) => setTimeout(resolve, 40));
const check = (condition, message) => { if (!condition) throw new Error(message); };
// Settings is a lazy region. Wait for the actual action to mount, not a
// guessed import duration; use the fixture's existing 4s readiness budget.
async function waitForElement(find, message) {
  const existing = find();
  if (existing) return existing;
  return new Promise((resolve, reject) => {
    const observer = new MutationObserver(() => {
      const element = find();
      if (element) { observer.disconnect(); clearTimeout(deadline); resolve(element); }
    });
    const deadline = setTimeout(() => { observer.disconnect(); reject(new Error(message)); }, 4000);
    observer.observe(document.body, { childList: true, subtree: true });
  });
}
const profile = { name: "测试作者", email: "author@example.test", passwordSalt: "test-salt", passwordHash: "test-hash" };
const project = { id: "workspace-context-ui", title: "测试作品", logline: "", updated_at: "" };
const state = { project, canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1.0", scenes: [], anchors: [] }, novelEdition: { chapters: [] } };
let releaseState;
let delayedState = false;
window.novelCodex = { multimedia: novelFixtureMedia,
  app: { onCloseRequested: () => () => {} },
  runtime: { ...chapterRuntimeReads(() => project), listWorkTasks: async () => ({ tasks: [], nextCursor: null, activeCount: 0 }),
    ...Object.fromEntries(["Search", "Entity", "Relation", "Knowledge", "SnapshotKnowledge", "Fact"].map(kind => [`get${kind}Audit`, async () => ({ job: null })])),
    status: async () => ({ state: "online", mode: "local", error: null }),
    listProjects: async () => ({ projects: [project] }),
    getState: async () => { if (delayedState) await new Promise((resolve) => { releaseState = resolve; }); return state; },
  },
  agent: {
    onEvent: () => () => {}, onUserInputRequest: () => () => {},
    status: async () => ({ configured: false, state: "idle", mode: "novel-agent-engine", error: null }),
    listConnections: async () => ({ connections: [] }), recoveryReport: async () => [],
    listExecutions: async () => [], readMessage: async () => { throw Error("message receipt not found"); }, beginInterlude: async () => { throw Error("unexpected interlude"); }, returnInterlude: async () => { throw Error("unexpected interlude return"); }, listMessages: async () => [], listThreads: async () => ({ threads: [], currentThreadId: null, nextCursor: null }),
    listProposedActions: async () => ({ actions: [] }),
    setWorkspaceContext: async (context) => { contexts.push(context); return { selected: context.projectId !== null }; },
  },
};

async function run() {
  localStorage.setItem("novel-codex-active-project", project.id);
  saveAuthor(profile); markSignedOut();
  let root = createRoot(document.getElementById("root"));
  root.render(<App />); await tick(); await tick();
  check(contexts.length > 0 && contexts.every((context) => context.projectId === null), "Signed-out author selected a workspace");
  root.unmount(); await tick();
  contexts.length = 0; saveAuthor(profile); delayedState = true;
  root = createRoot(document.getElementById("root")); root.render(<App />);
  await tick(); await tick();
  check(typeof releaseState === "function", "Project loading did not start");
  check(contexts.every((context) => context.projectId === null), "Placeholder project was scheduled while loading");
  delayedState = false; releaseState(); await tick(); await tick();
  check(contexts.at(-1)?.projectId === project.id, "Loaded signed-in project was not selected");
  check(contexts.at(-1)?.mediaScope === null, "Novel editing must retain project-wide protection");
  document.querySelector('[aria-label="打开用户设置"]').click();
  const signOut = await waitForElement(() => [...document.querySelectorAll("button")].find(button => button.textContent.includes("退出登录")), "Settings never exposed the sign-out action");
  signOut.click(); await tick();
  check(contexts.at(-1)?.projectId === null, "Signing out left the workspace context selected");
  contexts.length = 0; root.unmount(); await tick();
  check(contexts.length > 0 && contexts.every((context) => context.projectId === null), "Unmount did not release the workspace context");
  const edition={id:'comic-edition',editionId:'comic-edition',projectId:project.id,scopeId:'comic-scope',branchId:'main',medium:'comic',title:'漫画版本',canonSnapshotId:null,sources:[]};
  window.novelCodex.multimedia={listEditions:async()=>({editions:[edition]}),listConnections:async()=>({connections:[],capabilities:[]}),readWorkspace:async()=>({project,edition,revision:0,revisionId:null,headSnapshotId:'head',sourceDigest:'a'.repeat(64),document:{kind:'comic',title:'漫画',format:'pages',pages:[]},assets:[],sources:[]})};
  saveAuthor(profile);contexts.length=0;
  root=createRoot(document.getElementById('root'));root.render(<App/>);
  const findAddPage=()=>[...document.querySelectorAll('button')].find(button=>button.textContent==='＋ 添加页面');
  for(let i=0;i<100 && !findAddPage();i++)await tick();
  check(findAddPage(),'Media workspace did not open');findAddPage().click();
  for(let i=0;i<100 && !contexts.at(-1)?.dirty;i++)await tick();
  const current=contexts.at(-1);
  check(current?.dirty && JSON.stringify(current.mediaScope)===JSON.stringify({projectId:project.id,editionId:edition.id,scopeId:edition.scopeId,branchId:edition.branchId}),'Unsaved media draft did not report its exact scope');
  check(document.querySelector('.mm-comic-page'),'Reporting edit context lost the local page');
  root.unmount();await tick();
  check(contexts.at(-1)?.projectId===null && contexts.at(-1)?.mediaScope===null,'Media unmount retained an editing scope');
  clearAuthor(); window.testResult = { passed: true };
}
void run().catch((error) => { window.testResult = { passed: false, error: error.stack ?? String(error) }; });
