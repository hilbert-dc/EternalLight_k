import React, { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { MediaApplication } from "../../../src/multimedia/MediaApplication";
import "../../../src/styles.css";

const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (value, message) => { if (!value) throw Error(message); };
const until = async (predicate, message) => { for (let i = 0; i < 130 && !predicate(); i++) await tick(); check(predicate(), message); };
const edition = { id: "comic", editionId: "comic", projectId: "book", scopeId: "comic-scope", branchId: "comic-branch", medium: "comic", title: "独立漫画", canonSnapshotId: "canon-fixed", sources: [] };
const image = { id: "image", assetRevisionId: "image-r1", projectId: "book", scopeId: "comic-scope", name: "参考图.png", kind: "image", mimeType: "image/png", digest: "a".repeat(64), bytes: 10, width: 1, height: 1, duration: null, sampleRate: null, channels: null, previewUrl: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jK5sAAAAASUVORK5CYII=", createdAt: 1 };
const workspace = { project: { id: "book", title: "独立漫画", logline: "", updated_at: 1 }, edition, revision: 1, sourceDigest: "a".repeat(64), revisionId: "comic-r1", documentSource: { projectId: "book", scopeId: "comic-scope", branchId: "comic-branch", objectId: "comic", revisionId: "comic-r1", digest: "a".repeat(64), selector: { kind: "whole" } }, headSnapshotId: "snapshot", document: { kind: "comic", title: "独立漫画", creativeDesign: "序🌙章：保留全局故事线", format: "pages", pages: [] }, assets: [image], sources: [] };
const thread = id => ({ id, projectId: "book", name: id === "a" ? "对话 A" : id === "b" ? "对话 B" : id === "d" ? "图片对话" : "新对话", preview: "", status: "idle", modelProvider: "fixture", createdAt: 1, updatedAt: 1, isPinned: false, ephemeral: false });
let threads = [thread("a"), thread("b")], details = { a: { thread: thread("a"), turns: [] }, b: { thread: thread("b"), turns: [] } };
let fakeProviderCalls = 0, imageVerificationCalls = 0, createdThreads = 0, releaseSend, rejectSend, releaseImport, releaseCreate, holdImport = false, holdHistory = false, releaseHistory, busy = false;
const listeners = new Set(), questionListeners = new Set();
const emit = event => listeners.forEach(listener => listener(event));
const emitQuestion = event => questionListeners.forEach(listener => listener(event));
const sends = [];
const stops=[];
let executionSnapshots = [];
let snapshotRace=false;
let mediaWork=null;
const interludeRequests=[];
let currentModel = "Fixture", holdWorkspace = false, releaseWorkspace;
const fixtureConnection = model => ({ id: model, provider: "fixture", model, protocol: "responses", selectedProtocol: "responses", declaredCapabilities: model === "Vision Fixture" ? ["imageInput"] : [], verifiedCapabilities: model === "Vision Fixture" ? ["imageInput"] : [], failedCapabilities: [], collaborationStatus: "unverified", operations: [], status: "verified", certificationIds: [], accountStatus: "configured", isDefault: model === "Fixture" });
const api = { runtime:{listWorkTasks:async()=>({tasks:mediaWork?[structuredClone(mediaWork)]:[],nextCursor:null,activeCount:mediaWork?1:0})}, multimedia: {
    readCanon: async () => ({projectId:"book",snapshotId:"canon-fixed",document:{entries:[]}}),
    sourceImpacts: async () => ({projectId:"book",editionId:"comic",branchId:"comic-branch",revisionId:workspace.revisionId,items:[{source:{projectId:"book",scopeId:"original-scope",branchId:"original-branch",objectId:"original",revisionId:"original-old",digest:"b".repeat(64),selector:{kind:"whole"}},latest:null,status:"changed",targets:[]}],offset:0,total:1,nextOffset:null,canon:{pinnedSnapshotId:"canon-fixed",currentSnapshotId:"canon-fixed",changed:false}}),
    readWorkspace: async () => { if (holdWorkspace) { holdWorkspace = false; await new Promise(resolve => { releaseWorkspace = resolve; }); } return structuredClone(workspace); }, listConnections: async () => ({ connections: [], capabilities: [] }), importFiles: async () => { if (holdImport) { holdImport = false; await new Promise(resolve => { releaseImport = resolve; }); } return { assets: [image] }; } }, agent: {
  status: async () => ({ configured: true, state: "ready", model: currentModel, mode: "novel-agent-engine", provider: "fixture", error: null }),
  verifyImageInput: async (provider, model) => { imageVerificationCalls++; check(provider === "fixture" && model === "Fixture", "Image validation lost the selected connection"); throw Error("受控图片验证失败"); },
  listConnections: async () => ({ connections: [fixtureConnection("Fixture"), fixtureConnection("Vision Fixture")] }), configure: async (provider, model) => { check(provider === "fixture" && model === "Vision Fixture", "Incorrect model applied"); currentModel = model; return { configured: true, state: "ready", model, provider, mode: "novel-agent-engine", error: null }; },
  listExecutions: async () => {
    const snapshots=structuredClone(executionSnapshots);
    if(snapshotRace && snapshots.length){snapshotRace=false;await new Promise(resolve=>setTimeout(resolve,25));
      emit({method:"item/agentMessage/delta",params:{threadId:"d",turnId:"restored-turn",itemId:"race",delta:"媒体读取期间的新进度"},
        hostExecution:{id:"restored-domain",requestId:"restored-message",projectId:"book",threadId:"d",sequence:1}});
    }
    return snapshots;
  }, readMessage: async () => { throw Error("message receipt not found"); }, beginInterlude: async(projectId,workId,input)=>{check(mediaWork && workId===mediaWork.id && projectId==='book' && input.focus===null && input.media.scope.scopeId==='comic-scope','Media interlude lost original Work or scope');interludeRequests.push(input);mediaWork.interlude={id:input.id,messageId:input.id,stepId:'question-step',controlRevision:0,revision:0,parentId:null,autoReturn:false,phase:'answered'};return structuredClone(mediaWork);}, returnInterlude: async(projectId,workId,input)=>{check(workId===mediaWork.id && input.interludeId===mediaWork.interlude.id,'Returned to another media Work');mediaWork.interlude=null;return structuredClone(mediaWork);}, listMessages: async () => [], listThreads: async (projectId, archived, cursor, scopeId) => { check(projectId === "book" && scopeId === "comic-scope", "Thread query lost Edition scope"); return { threads: [...threads], nextCursor: null }; },
  readConversation: async (projectId, id) => { check(projectId === "book", "History project lost"); const result = structuredClone(details[id]); if (holdHistory) { holdHistory = false; await new Promise(resolve => { releaseHistory = resolve; }); } return result; },
  createThread: async (projectId, request) => { createdThreads++; check(projectId === "book" && request.scopeId === "comic-scope", "New thread missing scope"); await new Promise(resolve => { releaseCreate = resolve; }); const created = thread("c"); threads.push(created); details.c = { thread: created, turns: [] }; return { thread: created }; },
  onEvent: listener => { listeners.add(listener); return () => listeners.delete(listener); },
  onUserInputRequest: listener => { questionListeners.add(listener); return () => questionListeners.delete(listener); },
  resolveUserInput: async () => ({}),
  sendPrompt: async (projectId, prompt, repair, focus, threadId, media) => {
    fakeProviderCalls++; const n = fakeProviderCalls; emit({ method: "agent/execution-request", params: { projectId, requestId: `media-${n}` } }); sends.push({ projectId, prompt, threadId, media }); if (!threadId) { threadId = "d"; const created = thread("d"); threads.push(created); details.d = { thread: created, turns: [] }; }
    if(n===1) emit({method:"agent/message-updated",params:{projectId,threadId,messageId:`media-${n}`,status:"queued"}});
    emit({ method: "agent/workflow", params: { workflow: { task: { id: `task-${n}`, threadId, projectId, operation: "advisory", target: { scopeId: "comic-scope" } } } } });
    emit({ method: "turn/started", params: { threadId, turn: { id: `turn-${n}` } } });
    await new Promise((resolve, reject) => { releaseSend = resolve; rejectSend = reject; });
    details[threadId].turns.push({ id: `turn-${n}`, status: "completed", startedAt: 1, completedAt: 2, items: [{ id: `user-${n}`, type: "userMessage", status: "completed", phase: null, text: prompt, mediaAttachments: media.imageAssetRevisionIds.map(id => ({ assetRevisionId: id, name: image.name, previewUrl: image.previewUrl })) }, { id: `answer-${n}`, type: "agentMessage", status: "completed", phase: "final_answer", text: `已完成第 ${n} 轮` }] });
    emit({ method: "turn/completed", params: { threadId, turn: { id: `turn-${n}`, status: "completed", items: [{ id: `answer-${n}`, type: "agentMessage", phase: "final_answer", text: `已完成第 ${n} 轮` }] } } });
    return { message: `已完成第 ${n} 轮`, proposedActions: [] };
  }, interrupt: async target => {stops.push(target);return { interrupted: true };},
} };
localStorage.clear();
localStorage.setItem(`novel-codex:selected-conversation:${JSON.stringify([JSON.stringify(["author", "comic-scope"]), "book"])}`, "a");
let root = createRoot(document.getElementById("root"));
let editorDirty=false,discardEditor;
const render = () => root.render(<StrictMode><MediaApplication api={api} edition={edition} authorKey="author" topbar={<header>多媒介工作区</header>} onBusyChange={value => { busy = value; }} onDirtyChange={value=>{editorDirty=value;}} onSaveHandlerChange={() => {}} onDiscardHandlerChange={handler=>{discardEditor=handler;}} /></StrictMode>);
const button = label => [...document.querySelectorAll<HTMLButtonElement>("button")].find(item => item.getAttribute("aria-label") === label || item.textContent === label);
const composer = () => document.querySelector<HTMLTextAreaElement>('[aria-label="创作要求"]');
const fill = async text => { const input = composer(); Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(input, text); input.dispatchEvent(new Event("input", { bubbles: true })); await tick(); };
const click = async element => { check(element && !element.disabled, `Unavailable action: ${element?.getAttribute("aria-label")}`); element.click(); await tick(); };
const choose = id => click(button(`切换对话：对话 ${id.toUpperCase()}`));
window.addEventListener("error", event => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
(async () => { try {
  render(); await until(() => composer() && button("切换对话：对话 A") && document.querySelector(".mm-status-row")?.textContent === "已保存", "StrictMode shell/workspace did not load");
  await fill("保留我的草稿");await click([...document.querySelectorAll<HTMLButtonElement>("button")].find(b=>b.textContent==="设定与来源"));
  await until(()=>[...document.querySelectorAll<HTMLButtonElement>("button")].some(b=>b.textContent==="在对话中分析变化"),"Source discussion missing");
  await click([...document.querySelectorAll<HTMLButtonElement>("button")].find(b=>b.textContent==="在对话中分析变化"));
  check(composer().value.startsWith("保留我的草稿\n\n")&&composer().value.includes("comic-r1")&&composer().value.includes("完整来源变化报告")&&document.activeElement===composer()&&fakeProviderCalls===0,"Source discussion erased draft, lost exact identity or sent without author action");
  await click([...document.querySelectorAll<HTMLButtonElement>("button")].find(b=>b.textContent==="设定与来源"));
  const settingsButton = button("媒体服务设置"); settingsButton.focus(); await click(settingsButton); await until(() => document.querySelector(".media-settings-dialog"), "Media settings unavailable"); const modal = document.querySelector(".media-settings-dialog"); const closeSettings = [...modal.querySelectorAll<HTMLButtonElement>("button")].find(item => item.textContent === "关闭"); closeSettings.focus(); closeSettings.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", shiftKey: true, bubbles: true, cancelable: true })); check(modal.contains(document.activeElement), "Settings focus escaped dialog"); modal.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })); await until(() => !document.querySelector(".media-settings-dialog"), "Settings Escape did not close"); check(document.activeElement === settingsButton, "Settings dismissal lost focus");
  await fill("第一轮创作"); await click(button("添加图片")); await until(() => document.querySelectorAll(".media-composer .media-attachments img").length === 1, "Image not attached");
  check(button("发送创作要求").disabled && document.body.textContent.includes("当前模型不能读取图片") && fakeProviderCalls === 0, "Unverified image route accepted submission"); await click(button("验证当前模型的图片输入")); await until(() => document.body.textContent.includes("受控图片验证失败") && imageVerificationCalls === 1, "Explicit image qualification failure was not shown"); check(fakeProviderCalls === 0 && composer().value === "第一轮创作" && document.querySelectorAll(".media-composer .media-attachments img").length === 1, "Image qualification failed to retain the original message"); await click(button("选择模型：Fixture")); await click([...document.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]')].find(item => item.textContent.includes("Vision Fixture"))); await until(() => button("选择模型：Vision Fixture") && !button("发送创作要求").disabled, "Verified image route did not become usable"); check(composer().value === "第一轮创作" && document.querySelectorAll(".media-composer .media-attachments img").length === 1, "Model switch discarded input or image");
  document.querySelector<HTMLDetailsElement>(".mm-creative-design").open=true;
  const designInput=document.querySelector<HTMLTextAreaElement>(".mm-creative-design textarea");designInput.focus();designInput.setSelectionRange(1,4);designInput.dispatchEvent(new KeyboardEvent("keyup",{key:"ArrowRight",shiftKey:true,bubbles:true}));await tick();
  check(document.querySelector(".media-selection-context")?.textContent.includes("创作设计选段"),"Design selection did not enter the real conversation composer");
  for(const extra of [{isComposing:true},{keyCode:229},{repeat:true},{shiftKey:true},{altKey:true}]) {
    composer().dispatchEvent(new KeyboardEvent("keydown",{key:"Enter",ctrlKey:true,bubbles:true,cancelable:true,...extra}));
    await tick();check(fakeProviderCalls===0 && composer().value==="第一轮创作" && document.querySelectorAll(".media-composer .media-attachments img").length===1,"Media composition or repeated shortcut consumed input");
  }
  composer().dispatchEvent(new KeyboardEvent("keydown",{key:"Enter",ctrlKey:true,bubbles:true,cancelable:true}));
  await until(() => fakeProviderCalls === 1 && busy, "First turn not submitted");
  check(composer().value==="" && !document.querySelector(".media-composer .media-attachments"),"Durable receipt did not accept original text and images before model completion");
  check(sends[0].media.selection?.selector.quote === "🌙章" && sends[0].media.selection.selector.startChar===1 && sends[0].media.selection.selector.endChar===3 && sends[0].media.selection.revisionId==="comic-r1", "Conversation did not send exact design selection");
  if(button("取消选区引用"))await click(button("取消选区引用"));
  await fill("运行中写下的下一条");
  emit({ method: "agent/workflow", params: { workflow: { task: { id: "foreign-task", threadId: "b", projectId: "book", target: { scopeId: "comic-scope" } } } } });
  emit({ method: "item/agentMessage/delta", params: { threadId: "b", turnId: "foreign-turn", itemId: "foreign", delta: "不应出现的外来回复" } });
  emitQuestion({ requestId: "foreign-question", threadId: "b", turnId: "foreign-turn", params: { questions: [{ id: "q", header: "外来", question: "不应出现的外来问题", options: [] }] } });
  await tick(); check(!document.body.textContent.includes("不应出现"), "Foreign event or question leaked into current turn");
  releaseSend(); await until(() => !busy && document.body.textContent.includes("已完成第 1 轮"), "First completion missing"); check(composer().value === "运行中写下的下一条", "Success cleared newer draft"); check(sends[0].media.imageAssetRevisionIds.join() === "image-r1" && !document.querySelector(".media-composer .media-attachments"), "Image contract or sent attachment clear failed");
  check(document.querySelectorAll('[aria-label="本轮图片"] img, .conversation-reader-turn .conversation-media-attachments img').length === 1, "Sent image disappeared or duplicated while handing off to durable history");
  await click(button("发送创作要求")); await until(() => fakeProviderCalls === 2, "Second turn not submitted"); check([...document.querySelectorAll(".conversation-reader-turn")].filter(item => item.textContent.includes("第一轮创作")).length === 1, "Completed conversation disappeared or duplicated on next send");
  rejectSend(Error("受控模型失败")); await until(() => !busy && document.body.textContent.includes("受控模型失败"), "Failed turn missing"); check(composer().value === "运行中写下的下一条", "Failed turn consumed draft");
  await fill("第三轮创作"); await click(button("发送创作要求")); await until(() => fakeProviderCalls === 3, "Third turn not submitted"); await choose("b"); await until(() => composer().value === "", "B draft not isolated"); await fill("B 的独立构想"); releaseSend(); await until(() => !busy, "Background completion did not settle"); check(composer().value === "B 的独立构想" && !document.querySelector(".media-live-turn"), "A completion changed B input or reply");
  await choose("a"); await until(() => document.querySelectorAll(".conversation-reader-turn").length === 2, "Completed A history not restored"); check(composer().value === "", "Sent A draft revived"); check(document.querySelectorAll(".conversation-reader-turn .conversation-media-attachments img").length === 1, "Durable image attachment missing after history restoration");
  holdImport = true; await click(button("添加图片")); await until(() => releaseImport, "Import not held"); await choose("b"); await until(() => composer().value === "B 的独立构想", "B draft missing after import switch"); releaseImport(); await until(() => !button("添加图片").disabled, "Import did not settle"); check(!document.querySelector(".media-composer .media-attachments"), "Late attachment attached to other conversation");
  await choose("a"); holdHistory = true; await choose("b"); await until(() => releaseHistory, "History not held"); await choose("a"); await until(() => document.querySelectorAll(".conversation-reader-turn").length === 2, "A history missing"); releaseHistory(); await tick(); check(document.querySelectorAll(".conversation-reader-turn").length === 2, "Late B history replaced A");
  button("新建对话").click(); button("新建对话").click(); await until(() => createdThreads === 1, "Thread creation duplicated"); releaseCreate(); await until(() => button("切换对话：新对话"), "Created thread unavailable");
  root.unmount(); localStorage.removeItem(`novel-codex:selected-conversation:${JSON.stringify([JSON.stringify(["author", "comic-scope"]), "book"])}`); root = createRoot(document.getElementById("root")); render(); await until(() => composer() && button("发送创作要求"), "New root did not restore"); await fill("自动建立对话的第一条"); await click(button("发送创作要求")); await until(() => fakeProviderCalls === 4 && document.querySelector(".media-live-turn")?.textContent.includes("自动建立对话的第一条"), "First unbound conversation hid its active progress"); await fill("自动绑定后保留的下一条"); releaseSend(); await until(() => !busy && composer().value === "自动绑定后保留的下一条", "New thread binding lost a newer draft");
  await fill(""); check(button("发送创作要求").disabled, "Empty message without images was accepted"); await click(button("添加图片")); await until(() => !button("发送创作要求").disabled, "Image-only message stayed blocked");
  await click(button("发送创作要求")); await until(() => fakeProviderCalls === 5 && document.querySelector('[aria-label="本轮图片"] img'), "Image-only turn hid its live progress"); check(sends[4].prompt === "" && sends[4].media.imageAssetRevisionIds.join() === "image-r1", "Image-only turn fabricated author text or lost its asset binding");
  rejectSend(Error("图片轮次受控失败")); await until(() => !busy && document.body.textContent.includes("图片轮次受控失败"), "Image-only failure not visible"); check(composer().value === "" && document.querySelectorAll(".media-composer .media-attachments img").length === 1, "Image-only failure consumed the attachment");
  await click(button("发送创作要求")); await until(() => fakeProviderCalls === 6, "Image-only retry not sent"); releaseSend(); await until(() => !busy && document.body.textContent.includes("已完成第 6 轮"), "Image-only completion missing");
  await choose("a"); await click(button("切换对话：图片对话")); await until(() => document.querySelectorAll(".conversation-reader-turn .conversation-media-attachments img").length === 1, "Image-only original history failed to restore"); check(!document.body.textContent.includes("请描述图片"), "Image-only message was replaced with synthetic intent");
  await fill("准备期间停止，保留这条输入"); holdWorkspace = true;
  await click(button("发送创作要求")); await until(() => releaseWorkspace, "Preparation was not held");
  await click(button("停止本轮")); releaseWorkspace(); await until(() => !busy, "Stopped preparation did not settle");
  check(fakeProviderCalls === 6 && composer().value === "准备期间停止，保留这条输入", "Preparation stop sent a model request or lost the draft");
  await choose("a");await fill("A 的并行请求");await click(button("发送创作要求"));
  await until(()=>fakeProviderCalls===7 && busy,"Parallel A did not start");const finishA=releaseSend;
  check(!button("新建对话").disabled,"Running media conversation blocked independent thread creation");
  await choose("b");await fill("B 的并行请求");await click(button("发送创作要求"));
  await until(()=>fakeProviderCalls===8 && busy,"A blocked B media submission");const failB=rejectSend;
  check(document.querySelectorAll('.conversation-navigator-row.is-running').length===2,"Both media executions were not projected independently");
  emitQuestion({projectId:"book",executionId:"domain-a",requestId:"same-question",threadId:"a",turnId:"turn-7",params:{questions:[{id:"q",header:"A",question:"只属于 A 的问题",options:[]}]}});
  emitQuestion({projectId:"book",executionId:"domain-b",requestId:"same-question",threadId:"b",turnId:"turn-8",params:{questions:[{id:"q",header:"B",question:"只属于 B 的问题" + "保留故事线与画面关系。".repeat(30) + "末尾限定：先讨论"}]}});
  await until(()=>document.body.textContent.includes("只属于 B 的问题"),"B question was lost");
  check(document.querySelector(".approval-question p").textContent === "只属于 B 的问题" + "保留故事线与画面关系。".repeat(30) + "末尾限定：先讨论", "Media question was truncated or absent options broke rendering");check(!document.body.textContent.includes("只属于 A 的问题"),"A question leaked into B");
  await choose("a");await until(()=>document.body.textContent.includes("只属于 A 的问题"),"A question did not survive navigation");check(!document.body.textContent.includes("只属于 B 的问题"),"B question leaked into A");await choose("b");

  finishA();await until(()=>details.a.turns.some(turn=>turn.id==="turn-7"),"Background A did not complete");await tick();await tick();
  check(busy && document.querySelector('.media-live-turn')?.textContent.includes("B 的并行请求"),"A completion replaced or released B");
  await choose("a");await until(()=>!busy && document.body.textContent.includes("已完成第 7 轮"),"Completed A was not independently readable");
  await choose("b");await until(()=>busy,"Switching back did not restore active B");
  await click(button("停止本轮"));check(stops.at(-1)?.requestId==="media-8","Stopping B targeted A");failB(Error("B 的原请求已停止"));
  await until(()=>!busy && document.body.textContent.includes("B 的原请求已停止"),"B stop outcome missing");
  await click(button("切换对话：图片对话"));await until(()=>composer().value==="准备期间停止，保留这条输入","Parallel work changed D draft");
  const queued={id:"queued-media",projectId:"book",threadId:"d",taskId:null,input:{text:"队列中的原消息",media:{scope:edition,imageAssetRevisionIds:[]}},status:"claimed"};
  emit({method:"agent/message-started",params:{message:{...queued,id:"foreign-queued-media",threadId:"a"}}});await tick();check(!busy,"Foreign queued message took current UI ownership");
  emit({method:"agent/message-started",params:{message:queued}});
  await until(()=>busy,"Queued media did not bind execution ownership");
  emit({method:"agent/workflow",params:{workflow:{task:{id:"queued-task",threadId:"d",projectId:"book",target:{scopeId:"comic-scope"}}}}});
  emit({method:"turn/started",params:{threadId:"d",turn:{id:"queued-turn"}}});
  emit({method:"item/agentMessage/delta",params:{threadId:"d",turnId:"queued-turn",itemId:"queued-answer",delta:"队列恢复进度"}});
  await until(()=>document.body.textContent.includes("队列恢复进度"),"Queued media progress was dropped");
  emit({method:"agent/execution-request",params:{projectId:"book",requestId:"unrelated-request"}});
  emit({method:"item/agentMessage/delta",hostExecution:{projectId:"book",requestId:"unrelated-request"},params:{threadId:"d",turnId:"queued-turn",itemId:"foreign-delta",delta:"不应出现的错误执行域"}});
  await tick();check(!document.body.textContent.includes("不应出现的错误执行域"),"Foreign execution domain leaked into queued turn");
  await click(button("停止本轮"));check(stops.at(-1)?.requestId==="queued-media","Queue stop targeted another request");
  emit({method:"agent/message-ended",params:{projectId:"book",messageId:"foreign"}});await tick();check(busy,"Foreign end released queued execution");
  emit({method:"agent/message-ended",params:{projectId:"book",messageId:"queued-media",error:"受控队列失败"}});
  await until(()=>!busy && document.body.textContent.includes("受控队列失败"),"Queued media failure did not release UI");
  check(composer().value==="准备期间停止，保留这条输入","Queued execution consumed unrelated draft");
  mediaWork={id:'media-work',projectId:'book',threadId:'d',goal:'原漫画任务',status:'running',controlRevision:0,steps:[{id:'original-step',target:{scopeId:'comic-scope',editionId:'comic'}}]};
  await click(button('＋ 添加页面'));await until(()=>editorDirty,'Media editor did not retain unsaved changes');
  await click(button('＋ 添加画格'));await until(()=>button('取消选区引用'),'New unsaved panel was not selected');
  check(button('发送创作要求').disabled && document.body.textContent.includes('当前选区暂不能引用'), 'Unsaved selection was treated as a saved reference');
  if(button('取消选区引用'))await click(button('取消选区引用'));
  emit({method:'agent/message-ended',params:{projectId:'book',threadId:'d',messageId:'refresh-work'}});
  await fill('解释这个画格，不改作品');
  await until(()=>button('临时提问'),'Media interlude entry missing');await click(button('临时提问'));
  await until(()=>button('返回原任务') && composer().value==='','Media interlude failed to accept original draft');
  check(interludeRequests.length===1 && interludeRequests[0].media.expectedRevisionId==='comic-r1' && fakeProviderCalls===8,'Media interlude changed version or sent a new root');
  await fill('保留另一条新输入');await click(button('返回原任务'));
  await until(()=>!button('返回原任务'),'Media interlude return did not refresh');
  check(composer().value==='保留另一条新输入','Returning consumed unrelated draft');
  check(editorDirty && document.querySelector('.mm-comic-page') && !button('发送创作要求')?.disabled,'Readonly interlude discarded editing or blocked saved-version discussion');
  mediaWork=null;
  await click(button('发送创作要求'));await until(()=>fakeProviderCalls===9,'Dirty media draft blocked ordinary saved-version discussion');
  check(sends.at(-1).media.expectedRevisionId==='comic-r1' && sends.at(-1).media.selection===null && editorDirty,'Dirty draft leaked into saved-version input');
  releaseSend();await until(()=>!busy,'Dirty draft discussion did not finish');check(editorDirty,'Discussion completion discarded local edits');
  await discardEditor();await until(()=>!editorDirty,'Explicit discard did not reset the isolated editor draft');
  mediaWork=null;
  executionSnapshots=[{executionId:"restored-domain",requestId:"restored-message",projectId:"book",threadId:"d",prompt:"",taskId:"restored-task",executionThreadId:"d",turnId:"restored-turn",workflow:{task:{id:"restored-task",projectId:"book",threadId:"d",target:{scopeId:"comic-scope",imageAssetRevisionIds:["image-r1"]}}},events:[],cursor:0,truncated:false,questions:[{executionId:"restored-domain",requestId:"restored-question",projectId:"book",threadId:"d",turnId:"restored-turn",params:{questions:[{id:"restored-q",question:"恢复后完整问题".repeat(50)}]}}]}];
  snapshotRace=true;
  root.unmount();root=createRoot(document.getElementById("root"));render();
  await until(()=>busy && document.querySelector('[aria-label="本轮图片"] img'),"Active media snapshot did not restore on reload");
  await until(()=>document.body.textContent.includes("媒体读取期间的新进度"),"Media reload lost the live event after snapshot capture");
  check(fakeProviderCalls===9,"Snapshot recovery resent author input");
  check(document.querySelector(".approval-question p")?.textContent === "恢复后完整问题".repeat(50), "Restored media question lost full text or optional-field defaults");
  emit({method:"agent/execution-ended",params:{projectId:"book",requestId:"restored-message",failure:{message:"原执行已结束"}}});
  await until(()=>!busy && document.body.textContent.includes("原执行已结束"),"Restored media execution never released ownership");
  root.unmount(); check(listeners.size === 0 && questionListeners.size === 0, "Event listeners survived unmount");
  window.testResult = { passed: true, fakeProviderCalls, imageVerificationCalls, createdThreads };
} catch (error) { window.testResult = { passed: false, error: error.stack ?? String(error), dom: document.body.textContent.slice(-6000) }; } })();
