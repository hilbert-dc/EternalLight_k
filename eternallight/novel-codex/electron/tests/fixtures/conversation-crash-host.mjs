import {fileURLToPath} from 'node:url';
import {join} from 'node:path';
import {MultimediaHost} from '../../media-host.mjs';
import {mainHandlers,rendererEvent} from '../main-handler-harness.mjs';

const send=value=>new Promise((resolve,reject)=>process.send(value,error=>error?reject(error):resolve()));
process.once('message',async config=>{
  let main,armed=false,messageId,threadId=config.threadId;
  const writes=[];let resumeError=null,lastLease=null;
  const checkpoint=async(name,value)=>{
    await send({kind:'checkpoint',name,messageId,threadId,value,lease:lastLease});
    await new Promise(()=>{});
  };
  const request=async(path,body,options={},url)=>{
    const method=options.method??'GET';
    const response=await fetch(config.origin+path+(url?.search??''),{method,headers:{...Object.fromEntries(Object.entries(options.headers??{}).filter(([key])=>!["authorization","content-type"].includes(key.toLowerCase()))),authorization:`Bearer ${options.headers?.["x-novel-task-capability"]?config.candidateToken:config.authorToken}`,'content-type':'application/json'},...(body===null || body===undefined?{}:{body:JSON.stringify(body)})});
    const text=await response.text();
    if(!response.headers.get("content-type")?.includes("application/json"))throw Error(`${method} ${path}: ${response.status} ${text}`);
    const value=JSON.parse(text);
    if(!response.ok)throw Object.assign(Error(`${method} ${path}: ${JSON.stringify(value)}`),{code:value.error?.code});
    if(method!=='GET')writes.push({path,body});
    if(method==='POST' && /\/execution-owner(?:\/renew)?$/.test(path))lastLease=value;
    if(armed){
      if(path.endsWith('/messages') && method==='POST')messageId=value.id;
      const boundary=path.endsWith('/messages') && method==='POST'?'submitted'
        :body?.action==='claim'?'claimed':path.endsWith('/tasks') && method==='POST'?'taskBound'
        :body?.action==='sending'?'sending':path.endsWith('/turn') && method==='POST'?'turnBound'
        :body?.action==='delivered'?'delivered':null;
      if(boundary===config.window)await checkpoint(boundary,value);
    }
    return value;
  };
  try {
    const command=`${process.execPath} ${fileURLToPath(new URL('../fixture-app-server.mjs',import.meta.url))}`;
    const event=rendererEvent();
    const multimediaHost=config.media?new MultimediaHost({runtimeRequest:(path,options={})=>request(path,options.body?JSON.parse(options.body):null,options),directory:join(config.directory,'media'),connections:{list:async()=>({connections:[],capabilities:[]})}}):null;
    main=await mainHandlers(request,command,{...(multimediaHost?{multimediaHost}:{}),nativeRuntime:true,runtimeUrl:config.origin,runtimeProcess:{exitCode:null},rendererWebContents:event.sender,agentStorage:{budgetDb:config.budgetDb}});
    if(config.mode==='crash'){
      const created=await main.adapter.createThread();threadId=created.thread.id;
      await request(`/api/projects/${config.projectId}/conversations`,{threadId,title:'崩溃恢复验证',scopeId:config.media?.scope.scopeId??null,forkedFromId:null,lastTurnId:null},{method:'POST'});
      event.sender.send=(channel,payload)=>{
        if(config.window==='modelStarted' && channel==='agent:user-input-request')void checkpoint('modelStarted',payload);
      };
      armed=true;
      await main.handlers.get('agent:send-prompt')(event,config.projectId,config.window==='modelStarted'?'user-input-free-answer':'解释当前故事设定',null,null,threadId,config.media);
      throw Error('execution finished without reaching crash checkpoint');
    }
    const prior=await main.handlers.get('agent:read-message')(event,config.projectId,threadId,config.messageId);
    const listed=await main.handlers.get('agent:list-messages')(event,config.projectId,threadId);
    for(let i=0;i<3;i++)await main.conversationMessages.drain(config.projectId,event);
    await main.conversationMessages.settled();
    const automaticWrites=writes.length;
    const reconciled=await main.conversationMessages.change(prior,'reconcile');
    for(let i=0;i<3;i++)await main.conversationMessages.drain(config.projectId,event);
    await main.conversationMessages.settled();
    const reconciledTaskWrites=writes.filter(item=>item.path.endsWith('/tasks')).length;
    if(config.continueSafe){
      await main.conversationMessages.change(reconciled,'continue');
      await main.conversationMessages.drain(config.projectId,event);await main.conversationMessages.settled();
    }
    if(config.resumeBound){
      const work=await request(`/api/projects/${config.projectId}/tasks/${prior.taskId}/work-task`,null);
      await main.handlers.get('agent:control-work-task')(event,config.projectId,work.id,'pause',work.controlRevision,threadId);
      try {await main.handlers.get('agent:resume-work-task')(event,config.projectId,work.id);}
      catch(error){if(!config.rejectResume)throw error;resumeError=error.message;}
      const current=await main.handlers.get('agent:read-message')(event,config.projectId,threadId,config.messageId);
      await main.conversationMessages.change(current,'reconcile');
    }
    const final=await main.handlers.get('agent:read-message')(event,config.projectId,threadId,config.messageId);
    await send({kind:'result',resumeError,prior,listed,reconciled,final,automaticWrites,reconciledTaskWrites,
      taskWrites:writes.filter(item=>item.path.endsWith('/tasks')),active:main.executionDomains.active().length});
  }catch(error){await send({kind:'error',message:error.stack??String(error)});}
  finally {
    if(main){main.conversationMessages.pause();await main.workExecutionConsumer.stop();await main.executionDomains.closeAll();await main.conversationMessages.settled();await main.workExecutionOwners.stop();main.adapter.close();}
    process.disconnect();
  }
});
