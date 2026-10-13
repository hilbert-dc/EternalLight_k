import { novelFixtureMedia } from "../helpers/novel-edition";
import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import { saveAuthor } from "../helpers/author-session";

window.addEventListener("error", event => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
const tick = () => new Promise(resolve => setTimeout(resolve, 25));
const check = (condition, message) => { if (!condition) throw new Error(message); };
const until = async predicate => { for (let i = 0; i < 150 && !predicate(); i++) await tick(); check(predicate(), "UI did not settle"); };
const project = { id: "project-last-lamp", title: "恢复作品 A", logline: "", updated_at: "" };
const other = { ...project, id: "project-b", title: "恢复作品 B" };
const runtimeState = project => ({ project, canon: { characters: [], relationships: [], events: [], locations: [], plotThreads: [], worldRules: [], notes: [] }, narrativeIr: { version: "1.0", scenes: [], anchors: [] }, novelEdition: { chapters: [{ id: "chapter-one", number: 1, title: "第一章", status: "draft", summary: "", updated_at: "", passages: [] }, { id: "chapter-two", number: 2, title: "第二章", status: "draft", summary: "", updated_at: "", passages: [] }] } });
const thread = id => ({ id, name: id, preview: null, modelProvider: "fixture", status: "idle", createdAt: 100, updatedAt: 100, isPinned: false, ephemeral: false });
saveAuthor({ name: "恢复作者", email: "fixture@example.invalid", passwordSalt: "fixture", passwordHash: "fixture" });
const calls = [], reads = [], resumes = [], heldReads = new Map();
let stateReads = 0, failState = false, listener, holdThread = false, failThread = false, holdResume, resumeRelease, holdState = false, stateRelease;
const finish = name => {
  const id = `turn-${calls.length}`;
  listener?.({ method: "turn/started", params: { threadId: "execution", turn: { id } } });
  listener?.({ method: "turn/completed", params: { threadId: "execution", turn: { id, status: "completed", items: [{ id: `${id}-answer`, type: "agentMessage", phase: "final_answer", text: `${name}结果已保存` }] } } });
};
const execute = async name => { calls.push(name); finish(name); return { message: `${name}已保存`, status: "completed", run: { status: "completed", roles: [], reports: [] } }; };
const api = { multimedia: novelFixtureMedia,
  app: { onCloseRequested: () => () => {} },
  runtime: { listChapterStructureChanges: async (projectId: string) => ({ projectId, items: [], nextCursor: null }),
  getWorkCatalog: async () => ({ revision: 0, volumes: [], placements: [] }), getIdentityCatalog: async () => ({ entries: [], ambiguousNames: [], identityRevisionId: null, identityDecisions: [], truncated: false, ambiguityScanComplete: true }), listWorkTasks: async () => ({ tasks: [], nextCursor: null, activeCount: 0 }),
    ...Object.fromEntries(["Search", "Entity", "Relation", "Knowledge", "SnapshotKnowledge", "Fact"].map(kind => [`get${kind}Audit`, async () => ({ job: null })])),
    status: async () => ({ state: "online", mode: "local", error: null }),
    listProjects: async () => ({ projects: [project, other] }),
    getState: async id => { stateReads++; if (holdState) await new Promise(resolve => { stateRelease = resolve; }); if (failState) throw Error("controlled read failure"); return runtimeState(id === other.id ? other : project); },
    cancelPlanningReview: async () => { calls.push("stopReview"); return { reports: [], roles: [] }; },
  },
  agent: {
    status: async () => ({ configured: true, state: "ready", mode: "novel-agent-engine", error: null, provider: "fixture", model: "fixture" }),
    listConnections: async () => ({ connections: [] }), recoveryReport: async () => [], listProposedActions: async () => ({ actions: [] }), setWorkspaceContext: async () => {},
    listExecutions: async () => [], readMessage: async () => { throw Error("message receipt not found"); }, beginInterlude: async () => { throw Error("unexpected interlude"); }, returnInterlude: async () => { throw Error("unexpected interlude return"); }, listMessages: async () => [], listThreads: async () => ({ threads: [thread("A"), thread("B"), thread("empty"), thread("error")], nextCursor: null }),
    onEvent: fn => { listener = fn; return () => {}; }, onUserInputRequest: () => () => {},
    generatePlanning: async (_id, operation) => execute(operation), generateWork: async () => execute("work"),
    continueWork: async () => execute("continue"), retryWork: async () => execute("retry"),
    resolveGenerationFeedback: async (_id, _run, _revision, choice) => execute(choice),
    generateChapter: async () => execute("chapter"), resumeKnowledge: async () => execute("knowledge"), reviewPlanning: async () => execute("review"),
    sendPrompt: async () => ({ ...await execute("discussion"), proposedActions: [] }),
    resumeThread: async (_project, id) => { resumes.push(id); if (holdResume === id) await new Promise(resolve => { resumeRelease = resolve; }); return { thread: thread(id) }; },
    readThread: async (projectId, id) => {
      reads.push({ projectId, id });
      if (holdThread) await new Promise(resolve => heldReads.set(id, resolve));
      if (failThread) throw Error("controlled history read failure");
      return { thread: thread(id), turns: id === "empty" ? [] : [{ id: `${id}-turn`, status: "completed", startedAt: null, completedAt: null, items: [
        { id: "user", type: "userMessage", phase: null, text: `${projectId}-${id}提问` },
        { id: "process", type: "agentMessage", phase: "commentary", text: `${id}读取过程` },
        { id: "answer", type: "agentMessage", phase: "final_answer", text: `${projectId}-${id}回答` },
        { id: "unphased", type: "agentMessage", phase: null, text: "NOT_THE_FINAL_ANSWER" },
      ] }, { id: `${id}-process-only`, status: "completed", startedAt: null, completedAt: null, items: [{ id: "only-process", type: "agentMessage", phase: "commentary", text: "只有公开过程记录" }] },
      ...["", null, "   "].map((text, index) => ({ id: `${id}-empty-final-${index}`, status: "completed", startedAt: null, completedAt: null, items: [
        { id: "earlier-final", type: "agentMessage", phase: "final_answer", text: "EARLIER_FINAL_MUST_NOT_SURVIVE" },
        { id: "empty-final", type: "agentMessage", phase: "final_answer", text },
        { id: "unphased-after-empty", type: "agentMessage", phase: null, text: "EMPTY_FINAL_MUST_NOT_FALL_BACK" },
      ] })),
      ] };
    },
  },
};
window.novelCodex = api;

let searchCalls = 0;
api.runtime.getCreativeWorkspace = async () => ({ project, revision: 1, headSnapshotId: "snapshot", sourceDigest: "a".repeat(64), index: { status: "ready" }, artifacts: Object.fromEntries(["brief", "outline", "plan", "identity"].map(kind => [kind, { candidate: null, accepted: null, working: null, locked: false }])), chapters: [], workingBaselines: [], continuationIntents: [], planningReviews: [], commits: [], tasks: [], modeling: [], factProposals: [], reviews: [], chapterPlanBindings: [] });
api.runtime.search = async (_project, query) => { searchCalls++; return { results: [{ id: query === '第二章' ? 'draft-chapter-two' : query, title: query, kind: query === '第二章' ? 'draft' : query, snippet: '匹配正文片段', source: 'story-revision', sourceRevisionId: 'current-revision', sourcePath: '/title', evidence: { quote: query, start: 0, end: query.length } }], nextCursor: null, coverage: { complete: true } }; };
const root=createRoot(document.getElementById('root')); root.render(<App />);
const findButton=text=>[...document.querySelectorAll('button')].find(node=>node.textContent===text);
const searchFor=async query=>{document.querySelector('button[title="搜索"]').click();await tick();const input=document.querySelector('.top-search');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(input,query);input.dispatchEvent(new Event('input',{bubbles:true}));await tick();input.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',isComposing:true,bubbles:true}));await tick();const before=searchCalls; input.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true}));input.dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true}));await until(()=>document.querySelector('.workspace-search-results button'));check(searchCalls===before+1,'App duplicated Enter search');return document.querySelector('.workspace-search-results button');};
(async()=>{try{
await until(()=>stateReads>=2&&document.querySelector('.project-switcher span[title]'));
const start=searchCalls;
(await searchFor('第二章')).click();await until(()=>document.querySelector('[data-search-view="chapters"]'));
check(document.querySelector('[data-search-chapter]').getAttribute('data-search-chapter')==='chapter-two','real App passed object ID instead of chapter ID');
const navigation=[{view:'chapters',chapter:document.querySelector('[data-search-chapter]').getAttribute('data-search-chapter')}];
const openReview = async () => {
window.dispatchEvent(new KeyboardEvent('keydown',{key:'p',metaKey:true,shiftKey:true,bubbles:true})); await tick();
[...document.querySelectorAll('.command-palette button')].find(node=>node.textContent.includes('查看作品目录')).click(); await tick();
};
[...document.querySelectorAll('button')].find(node=>node.textContent.includes('角色')).click(); await tick();
await openReview(); await until(()=>document.querySelector('[data-search-view="chapters"]'));
check(document.querySelector('[data-search-chapter]').getAttribute('data-search-chapter')==='chapter-two','chapter review navigation lost selected chapter');
document.querySelector('[aria-label="打开用户设置"]').click(); await until(()=>!document.querySelector('[data-search-view]'));
await openReview(); await until(()=>document.querySelector('[data-search-view="chapters"]'));
check(document.querySelector('[data-search-chapter]').getAttribute('data-search-chapter')==='chapter-two','review from author settings lost selected chapter');


for(const kind of ['brief','outline','plan']){(await searchFor(kind)).click();await until(()=>document.querySelector('[data-search-view="'+(kind==='plan'?'plans':kind)+'"]'));navigation.push({view:document.querySelector('[data-search-view]').getAttribute('data-search-view')});}
findButton('标记未保存').click();await until(()=>document.body.textContent.includes('创作修改未保存'));
(await searchFor('第二章')).click();await until(()=>document.querySelector('.unsaved-dialog'));
check(document.querySelector('[data-search-view]').getAttribute('data-search-view')==='plans','search bypassed unsaved navigation gate');
findButton('留在当前页面').click();await tick();check(!document.querySelector('.unsaved-dialog')&&document.querySelector('[data-search-view="plans"]'),'stay lost existing editor');
await openReview(); await until(()=>document.querySelector('.unsaved-dialog'));
check(document.querySelector('[data-search-view]').getAttribute('data-search-view')==='plans','chapter review bypassed dirty navigation gate');
findButton('留在当前页面').click(); await tick();

window.testResult={passed:true,appNavigation:navigation,dirtyGate:true,searchCalls:searchCalls-start,modelCalls:calls.length};root.unmount();
}catch(error){window.testResult={passed:false,error:error.stack};}})();
