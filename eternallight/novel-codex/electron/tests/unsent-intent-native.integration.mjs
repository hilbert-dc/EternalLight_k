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
import {taskProviderBudgetOptions} from '../task-provider-budget.mjs';

for (const outcome of ['notSent','coreRejected','unknown']) test(`production Host preserves directed input outcome ${outcome} in the original Native Work`, {timeout:360_000}, async t=>{
 const directory=await mkdtemp(join(tmpdir(),'native-interlude-'));
 const budgetDb=join(directory,'engine.sqlite3'),database=join(directory,'story.sqlite3');
 const authorToken='interlude-author-token-123456789',candidateToken='interlude-candidate-token-123456789';
 const capability='interlude-task-capability-123456789';
 const command=`${process.execPath} ${fileURLToPath(new URL('./fixture-app-server.mjs',import.meta.url))}`;
 let runtime,origin,main;
 const children=[],hostCalls=[],attempts=[],grants=[];let rejectBeforeSend=true;
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
    if (path.endsWith("/provider-budget")) grants.push(value);
    if (path.endsWith("/intents") && body && rejectBeforeSend) { rejectBeforeSend=false;
      if(outcome==='notSent')main.adapter.executionLease.revoked=true;
      else if(outcome==='unknown') {const send=main.adapter.request.bind(main.adapter);main.adapter.request=async(method,...args)=>{const response=await send(method,...args);if(method==='turn/steer')throw Error('steer receipt lost after send');return response;};} }
    return value;
  };
  await launch();
  const { project } = await request("/api/projects", { title: "持久接续验证", logline: "隔离 Runtime 的真实持久结果与原任务消费", medium: "novel", idempotencyKey: `interlude-${randomUUID()}` });
  const base = `/api/projects/${project.id}`;
  const state = { nativeRuntime: true, runtimeUrl: origin, runtimeProcess: runtime,
    rendererWebContents: rendererEvent().sender, agentStorage: { budgetDb },
    onPrepareDomain(domain) {
      if (outcome === 'coreRejected') domain.codexAdapter.env.FIXTURE_STEER_ERROR_CODE = '-32600';
    } };
  main = await mainHandlers(transport, command, state);
  const { thread } = await main.adapter.createThread();
  await request(`${base}/conversations`,{threadId:thread.id,title:"临时提问",scopeId:null,forkedFromId:null,lastTurnId:null});
  const workspace = await request(`${base}/creative-workspace`);


 const event=rendererEvent(),questions=[];
 event.sender.send=(channel,payload)=>{if(channel==='agent:user-input-request')questions.push(payload);};
 let originalFinished;
 const original=main.handlers.get('agent:send-prompt')(event,project.id,'user-input-free-answer',null,null,thread.id)
   .then(value=>(originalFinished={value}),error=>(originalFinished={error}));
 for(let i=0;i<1000 && !questions.length && !originalFinished;i++)await new Promise(r=>setTimeout(r,10));
 if(originalFinished?.error)throw originalFinished.error;
 assert.equal(questions.length,1);
 const before=await request(`${base}/tasks/${attempts[0].id}/work-task`);
 const budgetBefore=await request(`${base}/tasks/${attempts[0].id}/provider-budget`);
 const steer=main.handlers.get('agent:steer')(event,project.id,'保留结局，只解释人物动机',thread.id);
 if(outcome!=='unknown')assert.equal((await steer).retained,true);else await assert.rejects(steer,/receipt lost/);
 await original;
 const held=await request(`${base}/work-tasks/${before.id}`);
 if(outcome==='unknown'){
   assert.equal((await request(`${base}/tasks/${attempts[0].id}/intents`)).intents[0].status,'pending');
   assert.equal(hostCalls.filter(c=>c.path.endsWith('/not-sent')).length,0);
   const rejected=await main.handlers.get('agent:resume-work-task')(event,project.id,before.id);
   assert.notEqual((await request(`${base}/work-tasks/${before.id}`)).status,'running');
   assert.equal(attempts.length,1);return;
 }
 assert.equal(held.status,'paused');assert.equal(held.id,before.id);
 const {intents}=await request(`${base}/tasks/${attempts[0].id}/intents`);
 assert.equal(intents[0].status,'notSent');assert.equal(intents[0].deliveredAt,null);
 const forbidden=await fetch(`${origin}${base}/tasks/${attempts[0].id}/intents/1/not-sent`,{method:'POST',headers:{authorization:`Bearer ${candidateToken}`,'content-type':'application/json'},body:'{}'});
 assert.equal(forbidden.status,401);
 const message=await request(`${base}/conversations/${thread.id}/messages/${intents[0].authorMessageId}`);
 assert.equal(message.status,'failed');assert.match(message.error,/尚未发送/);
 await main.handlers.get('agent:resume-work-task')(event,project.id,before.id);
 assert.equal(attempts.length,2);
 assert.equal(attempts[1].threadId,thread.id);
 assert.match(attempts[1].inputManifest.authorIntent,/保留结局，只解释人物动机/);
 assert.equal(attempts[1].inputManifest.authorIntentBinding.sourceTaskId,attempts[0].id);
 assert.equal((await request(`${base}/tasks/${attempts[1].id}/work-task`)).id,before.id);
 const budgetAfter=grants.find(g=>g.taskId===attempts[1].id);
 assert.ok(budgetAfter);
 assert.equal(budgetAfter.workTaskId,budgetBefore.workTaskId);
 assert.equal(hostCalls.filter(c=>c.path.endsWith('/tasks') && c.body?.initialBudget).length,1);
 assert.equal((await request(`${base}/conversations/${thread.id}/messages/${intents[0].authorMessageId}`)).status,'delivered');
});
