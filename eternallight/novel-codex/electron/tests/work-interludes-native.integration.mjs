import assert from 'node:assert/strict';
import test from 'node:test';
import {spawn} from 'node:child_process';
import {createHash,randomUUID} from 'node:crypto';
import {mkdtemp,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {PersistentTaskBudget} from '@eternallight/agent-engine';
import {nativeTestBinary} from './helpers/native-binaries.mjs';
import {mainHandlers,rendererEvent} from './main-handler-harness.mjs';
import {createInitialTaskBudget} from '../task-budget-grant.mjs';
import {executeConversationCandidates} from '../conversation-analysis.mjs';
import {taskProviderBudgetOptions} from '../task-provider-budget.mjs';

for (const generation of [false,true]) test(`production Host answers a durable readonly interlude in its original ${generation?'generation':'ordinary'} Native Work`, {timeout:360_000}, async t=>{
 const directory=await mkdtemp(join(tmpdir(),'native-interlude-'));
 const budgetDb=join(directory,'engine.sqlite3'),database=join(directory,'story.sqlite3');
 const authorToken='interlude-author-token-123456789',candidateToken='interlude-candidate-token-123456789';
 const capability='interlude-task-capability-123456789';
 const command=`${process.execPath} ${fileURLToPath(new URL('./fixture-app-server.mjs',import.meta.url))}`;
 let runtime,origin,main;
 const children=[],hostCalls=[],attempts=[];
  const stop = async child => {
    if (!child || child.exitCode !== null || child.signalCode !== null) return;
    const stopped = new Promise(resolve => child.once("exit", resolve)); child.kill(); await stopped;
  };
  t.after(async () => {
    main?.conversationMessages.pause();
    await main?.workExecutionConsumer.stop(); await main?.executionDomains.closeAll();
    await main?.conversationMessages.settled();
    await main?.workExecutionOwners.stop(); main?.adapter.close();
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
  const { project } = await request("/api/projects", { title: "持久接续验证", logline: "隔离 Runtime 的真实持久结果与原任务消费", medium: "novel", idempotencyKey: `interlude-${randomUUID()}` });
  const base = `/api/projects/${project.id}`;
  const state = { nativeRuntime: true, runtimeUrl: origin, runtimeProcess: runtime,
    rendererWebContents: rendererEvent().sender, agentStorage: { budgetDb } };
  main = await mainHandlers(transport, command, state);
  const { thread } = await main.adapter.createThread();
  await request(`${base}/conversations`,{threadId:thread.id,title:"临时提问",scopeId:null,forkedFromId:null,lastTurnId:null});
  const workspace = await request(`${base}/creative-workspace`);

 let run=null,next=null;
 if(generation){
  run=await request(`${base}/generation-runs`,{expectedProjectRevision:workspace.revision,expectedBaseSnapshotId:workspace.headSnapshotId,
   expectedSourceDigest:workspace.sourceDigest,authorIntent:'写一个港城故事',targetChapters:1,maxTaskAttempts:10,maxProviderRequests:16,maxProviderTokens:200000,deadlineAt:null});
  next=await request(`${base}/generation-runs/${run.id}/next`,{});
 }
 const source=await request(`${base}/tasks`,{operation:'advisory',target:generation?next.action.target:{type:'project'},inputRevision:workspace.revision,
  threadId:thread.id,authorIntent:generation?'写一个港城故事':'解释世界设定',...(!generation?{initialBudget:createInitialTaskBudget({workspace})}:{}),
  capabilityHash:createHash('sha256').update(capability).digest('hex'),capabilityExpiresAt:Math.floor(Date.now()/1000)+600});
 const work=await request(`${base}/tasks/${source.id}/work-task`), workPath=`${base}/work-tasks/${work.id}`;
 const id=randomUUID();
 const held=await main.handlers.get('agent:begin-interlude')(rendererEvent(),project.id,work.id,{id,threadId:thread.id,
  expectedControlRevision:work.controlRevision,parentId:null,autoReturn:false,text:'仅解释当前世界设定，不修改作品。',focus:{view:'chapters'}});
 assert.equal(held.interlude.id,id);assert.equal(held.status,'running');
 let preservedAction=null;
 if(!generation) {
  preservedAction=await request(`${base}/tasks/${source.id}/actions`,{taskId:source.id,threadId:thread.id,
   expectedRevision:workspace.revision,expectedIntentRevision:0,idempotencyKey:randomUUID(),
   operation:'generateBrief',target:{type:'artifact'},instruction:'按原目标生成候选简报，不采用。'},'POST',capability);
  assert.equal(preservedAction.status,'proposed');
  const queued = await executeConversationCandidates(rendererEvent(),project.id,{workflow:{task:source}}, {
   runtimeRequest: async (path,options={})=>request(path,options.body?JSON.parse(options.body):undefined,options.method??'GET'),
   currentWorkFlowState:async ()=>'running',assertAvailable:()=>{},
   notifyCandidateStart:()=>assert.fail('interlude must hold dispatch'),
   runCreativeAgentTask:()=>assert.fail('interlude must not execute writer'),
  });
  assert.deepEqual(queued.candidateResults,[]);

 }

 const grant=await request(`${base}/tasks/${source.id}/provider-budget`);
 const budget=new PersistentTaskBudget({databasePath:budgetDb,runId:source.id,...taskProviderBudgetOptions(grant,{taskId:source.id,workTaskId:work.id,deadlineAt:grant.deadlineAt})});
 if(generation) await request(`${base}/tasks/${source.id}`,{threadId:thread.id,status:'failed',error:'invalid_tool_arguments'},'PATCH');
 else await request(`${base}/tasks/${source.id}/complete`,{threadId:thread.id,summary:'原步骤完成。'},'POST',capability);
 budget.complete(generation?'failed':'completed');await request(generation?`${base}/generation-runs/${run.id}/provider-usage`:`${workPath}/provider-usage`,{taskId:source.id});
 await main.handlers.get('agent:set-workspace-context')(rendererEvent(),{projectId:project.id,dirty:true,chapterScope:null,mediaScope:null});
 await main.workExecutionConsumer.tick();await main.workExecutionConsumer.settled();
 await main.conversationMessages.settled();
 const answered=await request(workPath);
 assert.equal(answered.interlude.phase,'answered',JSON.stringify({answered,calls:hostCalls.slice(-8)}));
 assert.equal(answered.goal,work.goal);assert.equal(answered.id,work.id);assert.equal(answered.controlRevision,work.controlRevision);
 const message=await request(`${base}/conversations/${thread.id}/messages/${id}`);
 assert.equal(message.status,'delivered');assert.notEqual(message.taskId,source.id);
 const question=await request(`${base}/tasks/${message.taskId}`);
 assert.equal(question.inputManifest.authorIntent,'仅解释当前世界设定，不修改作品。');
 assert.equal(question.inputManifest.interludeBinding.workId,work.id);
 assert.equal((await request(`${base}/tasks/${message.taskId}/work-task`)).id,work.id);
 const returned=await main.handlers.get('agent:return-interlude')(rendererEvent(),project.id,work.id,{commandId:randomUUID(),interludeId:id,
  threadId:thread.id,expectedControlRevision:answered.controlRevision,expectedRevision:answered.interlude.revision});
 assert.equal(returned.interlude,null);
 if(preservedAction) {
  assert.ok(returned.steps.some(step=>step.id===preservedAction.id&&step.status==='queued'));
  assert.notEqual(returned.deliveryStatus,'discussion_ended','return must retain the original candidate goal');
  assert.equal((await request(`${base}/tasks/${source.id}/actions`)).actions.some(a=>a.id===preservedAction.id&&a.status==='queued'),true);
 }

 if(!generation) {
  const latest=await request(`${base}/creative-workspace`);
  await request(`${base}/drafts/checkpoints`,{expectedRevision:latest.revision,expectedCandidateRevisionId:null,idempotencyKey:randomUUID(),payload:{chapterId:'chapter-edit',chapterNumber:1,title:'编辑中的章节',body:'Native已保存正文',sceneGroups:[{id:'saved-scene',sceneId:null,body:'Native已保存正文',locked:false}]}});
  const nextThread=(await main.adapter.createThread()).thread.id;
  await request(`${base}/conversations`,{threadId:nextThread,title:'章节普通消息',scopeId:null,forkedFromId:null,lastTurnId:null});
  const ordinary=await main.conversationMessages.submit(project.id,nextThread,{messageId:randomUUID(),input:{text:'解释已保存的章节',focus:{view:'chapters',chapterId:'chapter-edit',chapterNumber:1},media:null},trigger:{kind:'immediate'}});
  await main.conversationMessages.drain(project.id,rendererEvent());await main.conversationMessages.settled();
  assert.equal((await main.conversationMessages.read(project.id,nextThread,ordinary.id)).status,'queued','unknown draft scope must remain protected');
  await main.handlers.get('agent:set-workspace-context')(rendererEvent(),{projectId:project.id,dirty:true,chapterScope:{projectId:project.id,chapterId:'chapter-edit'},mediaScope:null});
  await main.conversationMessages.drain(project.id,rendererEvent());await main.conversationMessages.settled();
  const receipt=await main.conversationMessages.read(project.id,nextThread,ordinary.id);
  assert.equal(receipt.status,'delivered');
  assert.equal((await request(`${base}/creative-workspace`)).chapters[0].candidate.body,'Native已保存正文');
 }

});
