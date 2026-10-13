import { novelFixtureMedia } from "../helpers/novel-edition";
import React from "react";
import { createRoot } from "react-dom/client";
import App from "../../../src/App";
import "../../../src/styles.css";
import { saveAuthor } from "../helpers/author-session";
import { chapterRuntimeReads } from "../helpers/chapter-runtime";

const tick = () => new Promise(resolve => setTimeout(resolve,20));
const check = (value,message) => { if (!value) throw Error(message); };
let renderError, phase = "initialization";
window.addEventListener("error",event => { renderError = event.error ?? event.message; });
const until = async (predicate,message) => { for (let index=0;index<220 && !predicate();index+=1) { if (renderError) throw renderError; await tick(); } check(predicate(),message); };
const project = {id:"project-last-lamp",title:"本地服务恢复",logline:"",updated_at:""};
const chapter = {id:"chapter-one",number:1,title:"保留编辑",status:"draft",summary:"",updated_at:"",passages:[]};
const candidate = {id:"source-one",title:chapter.title,body:"已保存的原始正文。",version:1,sourcePlanRefs:{lineageStatus:"current",localOnly:true},sceneGroups:[{id:"scene-one",sceneId:null,body:"已保存的原始正文。",locked:false}]};
const connection = {id:"fixture",provider:"fixture",model:"fixture",protocol:"responses",selectedProtocol:"responses",status:"verified",declaredCapabilities:[],verifiedCapabilities:[],failedCapabilities:[],collaborationStatus:"unverified",operations:[],certificationIds:[],accountStatus:"configured",isDefault:true};
let current = {state:"online",mode:"local",error:null}, retries = 0, stateReads = 0, emptyProjectQueries = 0, sends = 0, writes = 0, pendingRetry;
const base = chapterRuntimeReads(() => project);
window.novelCodex = { multimedia: novelFixtureMedia,
  app:{onCloseRequested:() => () => {}},
  runtime:{...base,
    status:async () => structuredClone(current),
    retryStartup:async () => { retries += 1; return new Promise((resolve,reject) => { pendingRetry = {resolve,reject}; }); },
    listProjects:async () => ({projects:[project]}),
    getState:async () => { stateReads += 1; await new Promise(resolve => setTimeout(resolve,180)); return {project,canon:{characters:[],relationships:[],events:[],locations:[],plotThreads:[],worldRules:[],notes:[]},narrativeIr:{version:"1",scenes:[],anchors:[]},novelEdition:{chapters:[chapter]}}; },
    getCreativeWorkspace:async () => ({...await base.getCreativeWorkspace(project.id),chapters:[{...chapter,candidate,accepted:null,working:null,workingSelection:null}]}),
    getWriterPreview:async (projectId,projectRevision,readingChapter,sceneOrder,povCharacterId,storyTime) => ({purpose:"writer",taskId:null,target:{projectId,projectRevision,position:{readingChapter,sceneOrder,povCharacterId,storyTime}},plannedEffects:[],relevantText:[],coverage:{complete:true,missingScope:[],truncationReasons:[],scannedScope:{}},budgetsUsed:{byteSize:0,byteLimit:60000}}),
    saveDraftCheckpoint:async () => { writes += 1; throw Error("Unexpected draft save during service recovery"); },
    saveArtifact:async () => { writes += 1; throw Error("Unexpected planning save during service recovery"); },
    setWorkingBaseline:async () => { writes += 1; throw Error("Unexpected source change during service recovery"); },
  },
  agent:{status:async () => ({configured:true,state:"ready",mode:"novel-agent-engine",provider:"fixture",model:"fixture",error:null}),
    listConnections:async () => ({connections:[connection]}),recoveryReport:async () => [],listExecutions: async () => [], readMessage: async () => { throw Error("message receipt not found"); }, beginInterlude: async () => { throw Error("unexpected interlude"); }, returnInterlude: async () => { throw Error("unexpected interlude return"); }, listMessages: async () => [], listThreads:async () => ({threads:[],nextCursor:null}),
    listProposedActions:async id => { if (!id) emptyProjectQueries += 1; return {actions:[]}; },
    setWorkspaceContext:async () => {},onEvent:() => () => {},onUserInputRequest:() => () => {},
    sendPrompt:async () => { sends += 1; throw Error("Unexpected creative request during service recovery"); },
    generateChapter:async () => { sends += 1; throw Error("Unexpected chapter generation during service recovery"); },
  },
} as any;
const composer = () => document.querySelector<HTMLTextAreaElement>('[aria-label="创作指令"]');
const send = () => document.querySelector<HTMLButtonElement>('[aria-label="发送创作指令"]');
const retry = () => document.querySelector<HTMLButtonElement>(".runtime-connection-status button");
const input = async (node,value) => { Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,"value").set.call(node,value); node.dispatchEvent(new Event("input",{bubbles:true})); await tick(); };

(async () => {
  let root;
  try {
    saveAuthor({id:"service-author",name:"作者",email:"service@example.invalid",passwordSalt:"fixture",passwordHash:"fixture"});
    root = createRoot(document.getElementById("root")); root.render(<App />);
    await until(() => stateReads >= 1 && composer() && document.querySelector('textarea[aria-label="场景 1 正文"]'),"Actual App editor did not load");
    await new Promise(resolve => setTimeout(resolve,250));
    check(emptyProjectQueries === 0,"Project actions were queried before a project was loaded");
    await input(composer(),"恢复后再处理创作要求");
    check(!send().disabled,"Online model was not initially usable");
    const editor = document.querySelector<HTMLTextAreaElement>('textarea[aria-label="场景 1 正文"]');
    const atLoaded = stateReads;

    phase = "offline creation guard";
    current = {state:"offline",mode:"local",error:"fixture service exited"};
    await until(() => retry(),"Online editor has no service recovery action");
    check(send().disabled,"Disconnected local service still allowed new creative requests");
    composer().dispatchEvent(new KeyboardEvent("keydown",{key:"Enter",bubbles:true}));
    await tick(); check(sends === 0 && writes === 0,"Offline Enter sent a model or wrote a draft");
    const unsaved = "尚未保存的小说正文，保留我的修改和选区。";
    await input(editor,unsaved); editor.focus(); editor.setSelectionRange(3,8);
    const preserved = () => document.querySelector('textarea[aria-label="场景 1 正文"]') === editor
      && editor.value === unsaved && editor.selectionStart === 3 && editor.selectionEnd === 8;

    phase = "single-flight with late offline poll";
    const first = retry(); first.click(); first.click();
    await tick(); check(retries === 1 && !retry(),"Synchronous repeated clicks made duplicate reconnect requests");
    await new Promise(resolve => setTimeout(resolve,2150));
    check(!retry() && document.querySelector(".runtime-connection-status").textContent.includes("连接中"),"An old offline poll reopened reconnect while its request was pending");
    check(preserved(),"Pending reconnect changed the unsaved editor or selection");
    pendingRetry.reject(Error("fixture startup rejected"));
    await until(() => retry(),"A real reconnect failure did not allow another attempt");
    check(preserved(),"Failed reconnect lost the chapter draft or selection");

    phase = "successful service-only recovery";
    const second = retry(); second.click(); second.click(); await tick();
    check(retries === 2 && !retry(),"Second reconnect was not single-flight");
    current = {state:"online",mode:"local",error:null};
    pendingRetry.resolve(structuredClone(current));
    await until(() => document.querySelector(".runtime-connection-status").textContent.includes("本地在线"),"Successful recovery did not restore online status");
    check(!retry() && preserved(),"Recovery kept a normal-state button or replaced the editor");
    check(stateReads === atLoaded && sends === 0 && writes === 0,"Service reconnect reloaded the project or triggered creation");
    window.testResult = {passed:true,retries,emptyProjectQueries,sends,writes,preservedEditor:true,preservedSelection:true,noReload:true};
  } catch (error) { window.testResult = {passed:false,error:`${phase}: ${error.stack ?? error}`,dom:document.body.innerHTML}; }
  finally { root?.unmount(); }
})();
