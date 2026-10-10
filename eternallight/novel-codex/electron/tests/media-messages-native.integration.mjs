import {MultimediaHost} from '../media-host.mjs';
import assert from 'node:assert/strict';
import test from 'node:test';
import {spawn} from 'node:child_process';
import {randomUUID} from 'node:crypto';
import {mkdtemp,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {nativeTestBinary} from './helpers/native-binaries.mjs';
import {mainHandlers,rendererEvent} from './main-handler-harness.mjs';

test('production Host binds media messages and readonly interludes to their original Native Work', {timeout:360_000}, async t=>{
 const directory=await mkdtemp(join(tmpdir(),'native-media-message-'));
 const budgetDb=join(directory,'engine.sqlite3'),database=join(directory,'story.sqlite3');
 const authorToken='interlude-author-token-123456789',candidateToken='interlude-candidate-token-123456789';
 const command=`${process.execPath} ${fileURLToPath(new URL('./fixture-app-server.mjs',import.meta.url))}`;
 let runtime,origin,main;
 const children=[],hostCalls=[],attempts=[];
  const stop = async child => {
    if (!child || child.exitCode !== null || child.signalCode !== null) return;
    const stopped = new Promise(resolve => child.once("exit", resolve)); child.kill(); await stopped;
  };
  t.after(async () => {
    main?.conversationMessages.pause();await main?.workExecutionConsumer.stop();await main?.executionDomains.closeAll();
    await main?.conversationMessages.settled();await main?.workExecutionOwners.stop();main?.adapter.close();
    for (const child of children) await stop(child);
    await rm(directory, { recursive: true, force: true });
  });
  const launch = async () => {
    const child = spawn(nativeTestBinary("novel-codex-story-runtime"), ["--port", "0", "--db", database,
      "--engine-budget-db", budgetDb, "--token-stdin"], { stdio: ["pipe", "pipe", "pipe"] });
    children.push(child);
    let output = "", diagnostic = "";
    child.stderr.on("data", data => { diagnostic += data; });
    const address = await new Promise((resolve, reject) => {
      const timeout = setTimeout(() => reject(new Error(`Native not ready: ${diagnostic.slice(-1000)}`)), 300_000);
      child.stdout.on("data", data => {
        output += data;
        const value = /Story Runtime listening on (http:\/\/127\.0\.0\.1:\d+)/.exec(output)?.[1];
        if (value) { clearTimeout(timeout); resolve(value); }
      });
      child.once("error", error => { clearTimeout(timeout); reject(error); });
      child.once("exit", code => { clearTimeout(timeout); reject(new Error(`Native exit ${code}: ${diagnostic.slice(-1000)}`)); });
      child.stdin.end(`${JSON.stringify({ authorToken, candidateToken })}\n`);
    });
    runtime = child; origin = address;
  };
  const request = async (path, body = null, method = body === null ? "GET" : "POST", secret = null) => {
    const response = await fetch(`${origin}${path}`, { method, headers: { "content-type": "application/json",
      authorization: `Bearer ${secret ? candidateToken : authorToken}`,
      ...(secret ? { "x-novel-task-capability": secret } : {}) }, ...(body === null ? {} : { body: JSON.stringify(body) }) });
    const text = await response.text();
    assert.match(response.headers.get("content-type") ?? "", /^application\/json/);
    const value = JSON.parse(text); assert.ok(response.ok, `${method} ${path}: HTTP ${response.status} ${text}`); return value;
  };
  const transport = async (path, body, options, url) => {
    hostCalls.push({ path, method: options.method ?? "GET", body });
    const value = await request(path + url.search, body, options.method ?? "GET", options.headers?.["x-novel-task-capability"] ?? null);
    if (path.endsWith("/tasks") && body) attempts.push(value);
    return value;
  };
  await launch();
  const created=await request("/api/projects",{title:"媒体持久提交",logline:"",medium:"comic",idempotencyKey:randomUUID()});
  const project=created.project,base=`/api/projects/${project.id}`;
  const workspace=await request(`${base}/editions/${created.activeEditionId}/workspace`);
  const runtimeRequest=(path,options={})=>request(path,options.body?JSON.parse(options.body):null,options.method??"GET");
  const multimediaHost=new MultimediaHost({runtimeRequest,directory:join(directory,'media'),connections:{list:async()=>({connections:[],capabilities:[]})}});
  main=await mainHandlers(transport,command,{nativeRuntime:true,runtimeUrl:origin,runtimeProcess:runtime,
    rendererWebContents:rendererEvent().sender,agentStorage:{budgetDb},multimediaHost});
  const {thread}=await main.adapter.createThread();
  await request(`${base}/conversations`,{threadId:thread.id,title:"媒体问题",scopeId:workspace.edition.scopeId,forkedFromId:null,lastTurnId:null});
  const media={scope:workspace.edition,expectedRevisionId:workspace.revisionId,selection:null,imageAssetRevisionIds:[]};
  const event=rendererEvent(),questions=[];
  event.sender.send=(channel,payload)=>{if(channel==='agent:user-input-request')questions.push(payload);};
  const running=main.handlers.get('agent:send-prompt')(event,project.id,'user-input-free-answer',null,null,thread.id,media);
  // Retain the completion even if an earlier assertion fails and cleanup cancels it.
  let finished;
  const completion=running.then(value=>(finished={value}),error=>(finished={error}));
  for(let i=0;i<1000 && !questions.length && !finished;i++)await new Promise(resolve=>setTimeout(resolve,10));
  if(finished?.error)throw finished.error;
  assert.equal(questions.length,1);
  const work=await request(`${base}/tasks/${attempts[0].id}/work-task`),interludeId=randomUUID();
  await main.handlers.get('agent:begin-interlude')(event,project.id,work.id,{id:interludeId,threadId:thread.id,expectedControlRevision:work.controlRevision,parentId:null,autoReturn:false,text:'只解释当前画格，不修改作品。',focus:null,media});
  await main.handlers.get('agent:set-workspace-context')(event,{projectId:project.id,dirty:true,mediaScope:null});
  main.handlers.get('agent:resolve-user-input')(event,questions[0],{answers:{direction:'自由回答'}});
  const original=await completion;if(original.error)throw original.error;
  const submissions=hostCalls.filter(call=>call.path.endsWith(`/conversations/${thread.id}/messages`) && call.method==='POST');
  assert.equal(submissions.length,1);
  const message=await request(`${base}/conversations/${thread.id}/messages/${submissions[0].body.messageId}`);
  assert.equal(message.status,'delivered',JSON.stringify(message));
  assert.equal(message.input.media.scope.scopeId,workspace.edition.scopeId);
  assert.equal(message.input.media.expectedRevisionId,workspace.revisionId);
  const task=await request(`${base}/tasks/${message.taskId}`);
  assert.equal(task.target.type,'edition');assert.equal(task.target.messageDispatch.messageId,message.id);
  assert.equal(task.target.scopeId,message.input.media.scope.scopeId);
  assert.equal(task.inputManifest.authorIntent,message.input.text);
  assert.equal(attempts.length,1);
  await main.workExecutionConsumer.tick();await main.workExecutionConsumer.settled();await main.conversationMessages.settled();
  const answered=await request(`${base}/work-tasks/${work.id}`);
  assert.equal(answered.interlude?.phase,'answered',JSON.stringify(answered));
  assert.equal(answered.goal,work.goal);assert.equal(attempts.length,2);
  const question=attempts[1];assert.equal(question.target.readOnly,true);assert.equal(question.target.scopeId,workspace.edition.scopeId);
  assert.equal((await request(`${base}/tasks/${question.id}/work-task`)).id,work.id);
  const receipt=await request(`${base}/conversations/${thread.id}/messages/${interludeId}`);
  assert.equal(receipt.status,'delivered');assert.equal(receipt.taskId,question.id);
  const returned=await main.handlers.get('agent:return-interlude')(event,project.id,work.id,{commandId:randomUUID(),interludeId,threadId:thread.id,expectedControlRevision:answered.controlRevision,expectedRevision:answered.interlude.revision});
  assert.equal(returned.interlude,null);assert.equal(returned.goal,work.goal);
  const queued=await main.conversationMessages.submit(project.id,thread.id,{messageId:randomUUID(),input:{text:'修改当前画格',focus:null,media},trigger:{kind:'immediate'}});
  await main.conversationMessages.drain(project.id,event);await main.conversationMessages.settled();
  assert.equal((await main.conversationMessages.read(project.id,thread.id,queued.id)).status,'queued');
  assert.equal(attempts.length,2,'unsaved editing still blocks ordinary media dispatch');
  const editingScope=Object.fromEntries(['projectId','editionId','scopeId','branchId'].map(key=>[key,workspace.edition[key]]));
  await main.handlers.get('agent:set-workspace-context')(event,{projectId:project.id,dirty:true,mediaScope:editingScope});
  const other=await request(`${base}/editions`,{title:'独立漫画版本',medium:'comic',sources:[],idempotencyKey:randomUUID()});
  const otherWorkspace=await request(`${base}/editions/${other.id}/workspace`);
  const otherThread=(await main.adapter.createThread()).thread.id;
  await request(`${base}/conversations`,{threadId:otherThread,title:'另一版本',scopeId:otherWorkspace.edition.scopeId,forkedFromId:null,lastTurnId:null});
  const otherMessage=await main.conversationMessages.submit(project.id,otherThread,{messageId:randomUUID(),input:{text:'解释这个版本',focus:null,media:{scope:otherWorkspace.edition,expectedRevisionId:otherWorkspace.revisionId,selection:null,imageAssetRevisionIds:[]}},trigger:{kind:'immediate'}});
  await main.conversationMessages.drain(project.id,event);await main.conversationMessages.settled();
  assert.equal((await main.conversationMessages.read(project.id,otherThread,otherMessage.id)).status,'delivered');
  assert.equal((await main.conversationMessages.read(project.id,thread.id,queued.id)).status,'queued','another Edition does not release the edited Edition');
  assert.equal(attempts.length,3);assert.equal(attempts[2].target.editionId,other.id);
});
