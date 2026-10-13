import test from 'node:test';
import assert from 'node:assert/strict';
import { mainHandlers, runtimeFixture, rendererEvent } from './main-handler-harness.mjs';
const command = `${process.execPath} ${new URL('./fixture-app-server.mjs', import.meta.url).pathname}`;
const ready = async condition => { for (let i = 0; i < 500; i++) { if (condition()) return; await new Promise(r => setTimeout(r, 10)); } throw Error('execution did not become ready'); };
test('real Host handlers isolate A and B processes, events, questions and stop targets', async () => {
  const runtime = runtimeFixture(), host = await mainHandlers(runtime.request.bind(runtime), command);
  for (const id of ['thr-history', 'thr-history-2']) await runtime.request('/api/projects/project/conversations', {threadId:id,title:id});
  const eventA = rendererEvent(), eventB = rendererEvent(), eventsA = [], eventsB = [];
  eventA.sender.send = (channel, payload) => eventsA.push({channel,payload});
  eventB.sender.send = (channel, payload) => eventsB.push({channel,payload});
  const start = (event, thread) => host.handlers.get('agent:send-prompt')(event,'project','fixture-wait-for-steer',null,null,thread).catch(error => error);
  let a, b;
  try {
    a = start(eventA,'thr-history');
    await ready(() => host.executionDomains.find({projectId:'project',threadId:'thr-history'})?.codexAdapter.turnId);
    b = start(eventB,'thr-history-2');
    await ready(() => host.executionDomains.find({projectId:'project',threadId:'thr-history-2'})?.codexAdapter.turnId);
    const domainA = host.executionDomains.find({projectId:'project',threadId:'thr-history'});
    const domainB = host.executionDomains.find({projectId:'project',threadId:'thr-history-2'});
    assert.notEqual(domainA.codexAdapter.process, domainB.codexAdapter.process);
    assert.notEqual(domainA.modelBridge, domainB.modelBridge);
    await assert.rejects(host.handlers.get('agent:send-prompt')(eventB,'project','duplicate',null,null,'thr-history'), /已有任务/);
    assert.ok(eventsA.some(event => event.payload.hostExecution?.threadId === 'thr-history'));
    assert.ok(eventsB.some(event => event.payload.hostExecution?.threadId === 'thr-history-2'));
    await host.handlers.get('agent:interrupt')(eventA,host.executionTargets.get(eventA));
    assert.equal((await a).code,'TASK_CANCELLED');
    assert.equal(host.executionDomains.find({projectId:'project',threadId:'thr-history'}),null);
    assert.ok(domainB.codexAdapter.turnWaiter);
    await host.handlers.get('agent:steer')(eventB,'project','只回答 B','thr-history-2');
    const completed = await b;
    assert.equal(completed.workflow.task.threadId,'thr-history-2');
    assert.equal(runtime.workTasks.get(domainA.activeWorkFlow?.id)?.status ?? [...runtime.workTasks.values()][0].status,'paused');
    assert.equal(host.executionDomains.active().length,0);
  } finally {
    await host.executionDomains.closeAll();
    await Promise.allSettled([a,b]);
    host.adapter.close();
  }
});

test('same request numbers in A and B questions remain bound across a read-only snapshot', async () => {
  const runtime = runtimeFixture(), host = await mainHandlers(runtime.request.bind(runtime), command);
  for (const id of ['thr-history','thr-history-2']) await runtime.request('/api/projects/project/conversations', {threadId:id,title:id});
  const questions = [], event = rendererEvent();
  event.sender.send = (channel,payload) => { if(channel === 'agent:user-input-request') questions.push(payload); };
  const start = thread => host.handlers.get('agent:send-prompt')(event,'project','user-input-free-answer',null,null,thread).catch(error => error);
  const a = start('thr-history'), b = start('thr-history-2');
  try {
    await ready(() => questions.length === 2);
    assert.equal(questions[0].requestId,questions[1].requestId);
    assert.notEqual(questions[0].executionId,questions[1].executionId);
    for(const domain of host.executionDomains.active()) domain.codexAdapter.readThread=()=>{throw Error('Active snapshot must not await Core history');};
    const snapshots = await host.handlers.get('agent:list-executions')(event,'project');
    assert.ok(snapshots.every(item=>item.cursor>0 && item.events.length>0 && item.events.every(event=>event.hostExecution.sequence<=item.cursor)));
    const domain=host.executionDomains.active()[0];domain.settled=true;
    assert.equal((await host.handlers.get('agent:list-executions')(event,'project')).length,1);domain.settled=false;
    assert.equal(snapshots.length,2);
    assert.ok(snapshots.every(item => item.questions.length === 1 && item.questions[0].executionId === item.executionId));
    const first = questions.find(item => item.threadId === 'thr-history');
    await assert.rejects(async () => host.handlers.get('agent:resolve-user-input')(event,{...first,projectId:'other'},{answers:{direction:'自由回答'}}),/原执行/);
    await host.handlers.get('agent:resolve-user-input')(event,first,{answers:{direction:'自由回答'}});
    await a;
    assert.equal(host.executionDomains.active().length,1);
    const second = questions.find(item => item.threadId === 'thr-history-2');
    await host.handlers.get('agent:resolve-user-input')(event,second,{answers:{direction:'自由回答'}});
    await b;
  } finally { await host.executionDomains.closeAll(); await Promise.allSettled([a,b]); host.adapter.close(); }
});

test('queued A and B start independent real Host domains before either model turn completes',async()=>{
  const runtime=runtimeFixture(),host=await mainHandlers(runtime.request.bind(runtime),command),event=rendererEvent();
  const messages=host.conversationMessages;
  for(const threadId of ['thr-history','thr-history-2']) {
    await runtime.request('/api/projects/project/conversations',{threadId,title:threadId});
    await messages.submit('project',threadId,{messageId:`queued-${threadId}`,input:{text:'fixture-wait-for-steer'},trigger:{kind:'immediate'}});
  }
  try {
    assert.equal(await messages.drain('project',event),true);
    await ready(()=>host.executionDomains.active().length===1 && host.executionDomains.active()[0].codexAdapter.turnId);
    const origin=host.executionDomains.active()[0];
    assert.equal(await host.executionDomains.enter(origin,()=>host.workExecutionConsumer.runInContext(()=>messages.drain('project',event))),true);
    await ready(()=>host.executionDomains.active().length===2 && host.executionDomains.active().every(domain=>domain.codexAdapter.turnId));
    assert.equal(messages.activeCount,2);
    const snapshots=await host.handlers.get('agent:list-executions')(event,'project');
    assert.equal(snapshots.length,2);
    assert.deepEqual(new Set(snapshots.map(item=>item.requestId)),new Set(['queued-thr-history','queued-thr-history-2']));
    for(const threadId of ['thr-history','thr-history-2'])await host.handlers.get('agent:steer')(event,'project',`结束 ${threadId}`,threadId);
    await messages.settled();
    for(const threadId of ['thr-history','thr-history-2']) {
      const receipt=await messages.read('project',threadId,`queued-${threadId}`);
      assert.equal(receipt.status,'delivered');assert.ok(receipt.taskId);
    }
    assert.equal(host.executionDomains.active().length,0);
  } finally {messages.pause();await host.executionDomains.closeAll();await messages.settled();host.adapter.close();}
});

test('repeated concurrent Host runs release their processes, waiters and renderer listeners', {timeout:90_000}, async t => {
  const runtime=runtimeFixture(),state={},host=await mainHandlers(runtime.request.bind(runtime),command,state);
  const threads=['thr-history','thr-history-2'],events=threads.map(()=>rendererEvent());
  for(const id of threads)await runtime.request('/api/projects/project/conversations',{threadId:id,title:id});
  const before=process.memoryUsage();let peakRss=before.rss,peakProcesses=0,exited=0,warmed=null;
  const listeners=events.map(event=>event.sender.listenerCount('destroyed'));
  let pending=[];
  try {
    for(let round=0;round<24;round++){
      pending=threads.map((thread,index)=>host.handlers.get('agent:send-prompt')(events[index],'project','fixture-wait-for-steer',null,null,thread).catch(error=>error));
      await ready(()=>host.executionDomains.active().length===2 && host.executionDomains.active().every(domain=>domain.codexAdapter.turnId));
      const domains=host.executionDomains.active(),children=domains.map(domain=>domain.codexAdapter.process);
      peakProcesses=Math.max(peakProcesses,children.filter(child=>child.exitCode===null && child.signalCode===null).length);
      const stopped=round%2,completed=1-stopped;
      await host.handlers.get('agent:interrupt')(events[stopped],host.executionTargets.get(events[stopped]));
      assert.equal((await pending[stopped]).code,'TASK_CANCELLED');
      assert.ok(host.executionDomains.find({projectId:'project',threadId:threads[completed]}));
      await host.handlers.get('agent:steer')(events[completed],'project','只回答当前问题',threads[completed]);
      assert.equal((await pending[completed]).workflow.task.threadId,threads[completed]);
      await ready(()=>children.every(child=>child.exitCode!==null || child.signalCode!==null));
      exited+=children.length;
      assert.equal(host.executionDomains.active().length,0);
      assert.equal(host.executionDomains.hasCapacity(),true);
      for(const domain of domains){assert.equal(domain.closed,true);assert.equal(domain.codexAdapter.process,null);assert.equal(domain.codexAdapter.turnWaiter,null);}
      events.forEach((event,index)=>assert.equal(event.sender.listenerCount('destroyed'),listeners[index]));
      peakRss=Math.max(peakRss,process.memoryUsage().rss);
      // The harness records domains for inspection; discard only these test
      // observations after verifying release, not product history or messages.
      state.executionDomains.length=0;
      if(round===0){if(global.gc)global.gc();warmed=process.memoryUsage();}
    }
    assert.equal(exited,48);assert.equal(peakProcesses,2);
    const after=process.memoryUsage();
    if(global.gc){await new Promise(resolve=>setImmediate(resolve));global.gc();}
    t.diagnostic(JSON.stringify({rounds:24,turns:48,peakProcesses,exited,before,warmed,after,afterGc:global.gc?process.memoryUsage():null,peakRss}));
  } finally {
    await host.executionDomains.closeAll();await Promise.allSettled(pending);
    await host.workExecutionConsumer.stop();await host.workExecutionOwners.stop();host.adapter.close();
  }
});

for (const foreign of [false, true]) test(`queued Host completion preserves only its bound Native handoff: foreign=${foreign}`, async () => {
  const runtime = runtimeFixture(), event = rendererEvent(), events = [];
  event.sender.send = (channel, payload) => { if (channel === 'agent:event') events.push(payload); };
  const host = await mainHandlers(async (path, ...args) => {
    if (path.endsWith('/generation-handoff')) {
      const taskId = path.split('/').at(-2), task = [...runtime.tasks.values()].find(task => task.id === taskId);
      return { proposal: task ? { proposalId: 'queued-proposal', projectId: task.projectId,
        sourceTaskId: foreign ? 'foreign-task' : task.id, threadId: task.threadId,
        authorIntent: task.inputManifest.authorIntent, targetChapters: 2, expectedRevision: 0,
        sourceDigest: 'source-fixture', baseSnapshotId: 'snapshot-fixture', state: 'proposed' } : null };
    }
    return runtime.request(path, ...args);
  }, command, { rendererWebContents: event.sender });
  try {
    await runtime.request('/api/projects/project/conversations', { threadId: 'thr-history', title: 'queued' });
    await host.conversationMessages.submit('project', 'thr-history', { messageId: 'queued-proposal-message', input: { text: 'completed-message' }, trigger: { kind: 'immediate' } });
    await host.conversationMessages.drain('project', event);
    await host.conversationMessages.settled();
    const ended = events.find(event => event.method === 'agent/message-ended');
    assert.ok(ended);
    assert.equal(ended.params.error, null);
    assert.equal(ended.params.generationHandoff?.proposalId ?? null, foreign ? null : 'queued-proposal');
    assert.equal(events.some(event => event.method === 'agent/error'), false, 'normal domain cleanup must not invalidate the handoff');
    assert.equal((await host.conversationMessages.read('project', 'thr-history', 'queued-proposal-message')).status, 'delivered');
  } finally { host.conversationMessages.pause(); await host.executionDomains.closeAll(); await host.conversationMessages.settled(); host.adapter.close(); }
});
