import { userInputQuestions } from "../../shared/author-questions.mjs";
import { ExecutionSnapshotBuffer, truncatedExecutionNotice } from "../execution-snapshot";
import { useEffect, useRef, useState } from "react";
import { createAgentRun, reduceAgentEvent, type AgentRun } from "../agent-events";
import type { NovelCodexApi, ConversationMessage } from "../types";
import type { UserInputQuestion } from "../AuthorQuestionCard";

export type MediaSubmission = {key:string;text:string;images:string[];threadId:string|null;messageId:string|null};
type Image = {id:string;label:string;url:string};
type Question = {executionId:string;requestId:string|number;questions:UserInputQuestion[]};
type Owner = {projectId:string;scopeId:string;taskId:string|null;threadId:string|null;requestedThreadId:string|null};
const ref = <T,>(current:T) => ({current});
function empty(threadId:string|null) {
  return {requestedThreadId:threadId,runRef:ref(createAgentRun()),pending:ref(false),preparingCancelled:ref(false),
    executionRequest:ref<{projectId:string;requestId:string}|null>(null),eventOwner:ref<Owner|null>(null),
    submittedDraft:ref<MediaSubmission|null>(null),images:[] as Image[],questions:[] as Question[],error:null as string|null};
}
type Execution = ReturnType<typeof empty>;

// Disposable Renderer projections. Native and Host remain the execution authorities.
export function useMediaExecutions({api,projectId,scopeId,editionId,branchId,selectedId,onReceipt,onSettled}:{
  api:NovelCodexApi;projectId:string;scopeId:string;editionId:string;branchId:string;selectedId:string|null;
  onReceipt:(submission:MediaSubmission)=>void;onSettled:()=>void;
}) {
  const records=useRef<Execution[]>([]), requesting=useRef<Execution|null>(null), completed=useRef(new Set<string>());
  const snapshotsBuffer=useRef(new ExecutionSnapshotBuffer());
  const mounted=useRef(true), callbacks=useRef({onReceipt,onSettled});callbacks.current={onReceipt,onSettled};
  const [,render]=useState(0);
  const changed=()=>{if(mounted.current)render(value=>value+1);};
  useEffect(()=>{mounted.current=true;return()=>{mounted.current=false;};},[]);
  const matches=(record:Execution,id:string|null)=>id===null ? record.requestedThreadId===null
    : record.requestedThreadId===id || record.runRef.current.threadId===id;
  const find=(id:string|null)=>[...records.current].reverse().find(record=>matches(record,id));
  const get=(id:string|null)=>{let record=find(id);if(!record){record=empty(id);records.current.push(record);}return record;};
  const begin=(id:string|null)=>{const record=empty(id);record.pending.current=true;
    records.current=records.current.filter(item=>item.pending.current || !matches(item,id));records.current.push(record);changed();return record;};
  const bind=(record:Execution)=>({
    ...record,
    commitRun:(next:AgentRun)=>{record.runRef.current=next;changed();},
    setSentImages:(images:Image[])=>{record.images=images;changed();},
    setQuestions:(value:Question[]|((prior:Question[])=>Question[]))=>{record.questions=typeof value==='function'?value(record.questions):value;changed();},
    setBusy:(busy:boolean)=>{record.pending.current=busy;changed();},
    setExecutionError:(error:string|null)=>{record.error=error;changed();},
  });
  const restoreImages=(record:Execution,ids:string[])=>{
    if(!ids.length){record.images=[];changed();return;}
    const request=record.executionRequest.current;
    void api.multimedia.readWorkspace(projectId,editionId,branchId).then(workspace=>{
      if(!mounted.current || record.executionRequest.current!==request || workspace.project.id!==projectId || workspace.edition.scopeId!==scopeId)return;
      record.images=workspace.assets.filter(asset=>ids.includes(asset.assetRevisionId) && asset.kind==='image' && asset.scopeId===scopeId)
        .map(asset=>({id:asset.assetRevisionId,label:asset.name,url:asset.previewUrl}));changed();
    }).catch(()=>{if(mounted.current && record.executionRequest.current===request){record.error='原消息图片预览尚未读取；执行和素材引用仍保留。';changed();}});
  };
  useEffect(()=>api.agent.onEvent(event=>{
    if(!snapshotsBuffer.current.capture(event,projectId))return;
    const value=event as {method?:string;hostExecution?:{projectId:string;requestId:string};params?:{
      projectId?:string;threadId?:string;turnId?:string;taskId?:string;requestId?:string|number;messageId?:string;
      error?:string;failure?:{message?:string};message?:ConversationMessage;thread?:{id:string};
      workflow?:{task?:{id:string;projectId:string;threadId:string;target?:{scopeId?:string}}};}};
    const p=value.params,method=value.method;
    if(method==='agent/execution-request'){
      const record=requesting.current;
      if(!record || p?.projectId!==projectId || typeof p.requestId!=='string' || record.executionRequest.current)return;
      record.executionRequest.current={projectId,requestId:p.requestId};
      if(record.submittedDraft.current){record.submittedDraft.current.messageId=p.requestId;
        try{localStorage.setItem(`${record.submittedDraft.current.key}:submission`,JSON.stringify(record.submittedDraft.current));}
        catch{record.error='消息核对编号尚未保存，请勿关闭应用。';}}
      changed();return;
    }
    if(method==='agent/message-started'){
      const message=p?.message;
      if(message?.projectId!==projectId || message.input.media?.scope.scopeId!==scopeId || find(message.threadId)?.pending.current)return;
      const record=begin(message.threadId);record.executionRequest.current={projectId,requestId:message.id};
      record.eventOwner.current={projectId,scopeId,threadId:message.threadId,requestedThreadId:message.threadId,taskId:message.taskId??null};
      record.runRef.current={...createAgentRun(message.input.text),threadId:message.threadId};
      restoreImages(record,message.input.media.imageAssetRevisionIds);changed();return;
    }
    if(method==='agent/message-updated' && p?.projectId===projectId){
      const record=records.current.find(item=>item.submittedDraft.current?.messageId===p.messageId);
      const submission=record?.submittedDraft.current;
      if(record && submission){submission.threadId=p.threadId??submission.threadId;
        try{localStorage.setItem(`${submission.key}:submission`,JSON.stringify(submission));}catch{}
        callbacks.current.onReceipt(submission);record.submittedDraft.current=null;}
      return;
    }
    if(method==='agent/message-ended' || method==='agent/execution-ended'){
      const id=p?.messageId??p?.requestId;
      if(p?.projectId!==projectId || typeof id!=='string')return;
      completed.current.add(id);
      const record=records.current.find(item=>item.executionRequest.current?.requestId===id && (!p.threadId || item.runRef.current.threadId===p.threadId));if(!record)return;
      const error=p.error??p.failure?.message;
      if(error){record.error=error;record.runRef.current={...record.runRef.current,state:'error',error,completedAt:Date.now()};}
      record.pending.current=false;record.eventOwner.current=null;record.executionRequest.current=null;record.questions=[];
      changed();callbacks.current.onSettled();return;
    }
    for(const record of records.current){
      const owner=record.eventOwner.current;if(!owner)continue;
      const domain=value.hostExecution;
      if(domain && (domain.projectId!==projectId || domain.requestId!==record.executionRequest.current?.requestId))continue;
      const task=p?.workflow?.task;
      if(method==='agent/workflow'){
        if(!task || task.projectId!==projectId || task.target?.scopeId!==scopeId || !task.threadId
          || owner.taskId && owner.taskId!==task.id || owner.threadId && owner.threadId!==task.threadId)continue;
        owner.taskId=task.id;owner.threadId=task.threadId;
      }
      if(!owner.taskId)continue;
      const thread=p?.threadId??p?.thread?.id;
      if(thread && thread!==owner.threadId || p?.taskId && p.taskId!==owner.taskId)continue;
      if(method==='agent/workflow' || thread || p?.taskId===owner.taskId)record.runRef.current=reduceAgentEvent(record.runRef.current,event);
      if(method==='serverRequest/resolved' && p?.requestId!==undefined)record.questions=record.questions.filter(item=>String(item.requestId)!==String(p.requestId));
    }
    changed();
  }),[api,projectId,scopeId,editionId,branchId]);
  useEffect(()=>api.agent.onUserInputRequest(request=>{
    if(request.projectId!==projectId)return;
    for(const record of records.current){
      const run=record.runRef.current;
      if(!record.eventOwner.current?.taskId || !request.threadId || !request.turnId || request.threadId!==run.threadId
        || request.turnId!==run.turnId || !['running','waiting'].includes(run.state))continue;
      const value=request as {executionId:string;requestId:string|number;params?:{questions?:UserInputQuestion[]}};
      if(Array.isArray(value.params?.questions) && !record.questions.some(item=>String(item.requestId)===String(value.requestId))) {
        record.questions.push({executionId:value.executionId,requestId:value.requestId,questions:userInputQuestions(value.params)});changed();
      }
    }
  }),[api,projectId]);
  useEffect(()=>{
    let disposed=false;if(!selectedId || get(selectedId).pending.current)return;
    void api.agent.listExecutions(projectId).then(snapshots=>{
      if(disposed || get(selectedId).pending.current)return;
      const snapshot=snapshots.find(item=>item.projectId===projectId && item.threadId===selectedId);
      const workflow=snapshot?.workflow as {task?:{id:string;projectId:string;threadId:string;target?:{scopeId?:string;imageAssetRevisionIds?:string[]}}}|null;
      const task=workflow?.task;
      if(!snapshot || completed.current.has(snapshot.requestId) || task?.projectId!==projectId || task.threadId!==selectedId || task.target?.scopeId!==scopeId)return;
      const restored=snapshotsBuffer.current.merge(snapshot);
      const record=begin(selectedId);record.executionRequest.current={projectId,requestId:snapshot.requestId};
      record.eventOwner.current={projectId,scopeId,taskId:task.id,threadId:selectedId,requestedThreadId:selectedId};
      let run:AgentRun={...createAgentRun(snapshot.prompt),threadId:selectedId};
      run=reduceAgentEvent(run,{method:'agent/workflow',params:{workflow}});
      if(snapshot.turnId){run=reduceAgentEvent(run,{method:'turn/started',params:{threadId:selectedId,turn:{id:snapshot.turnId}}});
      }
      for(const event of restored.events)if(event.method!=='agent/workflow')run=reduceAgentEvent(run,event);
      if(snapshot.truncated)record.error=truncatedExecutionNotice;
      record.questions=restored.questions.flatMap(question=>Array.isArray(question.params.questions)?[{executionId:question.executionId,requestId:question.requestId,questions:userInputQuestions(question.params)}]:[]);
      record.runRef.current=record.questions.length?{...run,state:'waiting'}:run;
      restoreImages(record,task.target?.imageAssetRevisionIds??[]);changed();
    }).catch(reason=>{if(!disposed){get(selectedId).error=reason instanceof Error?reason.message:'运行状态尚未读取';changed();}});
    return()=>{disposed=true;};
  },[api,projectId,scopeId,selectedId]);
  const current=get(selectedId);
  return {current,bind,begin,invoke:<T,>(record:Execution,callback:()=>T):T=>{requesting.current=record;try{return callback();}finally{requesting.current=null;}},
    runningIds:records.current.filter(item=>item.pending.current).flatMap(item=>item.runRef.current.threadId?[item.runRef.current.threadId]:[])};
}
