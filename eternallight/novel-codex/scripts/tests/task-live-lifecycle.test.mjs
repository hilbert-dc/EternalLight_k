import test from 'node:test';
import assert from 'node:assert/strict';
import {settleLiveProject,stopLiveProject,executeLiveHandoff,liveRemainingRequests,approveLiveCandidate} from '../task-live-lifecycle.mjs';
const clock=()=>{let time=0;return {now:()=>time,sleep:async ms=>{time+=ms;},timeoutMs:1000,quietMs:200};};
test('foreground rejection does not end observation while background recovery or settlement remains',async()=>{
 let reads=0;
 const api={agent:{status:async()=>({executionHealth:[]}),listExecutions:async()=>[]},runtime:{listWorkTasks:async()=>({tasks:[{id:'w',status:++reads<4?'running':'completed',deliveryReport:{unsettledAttemptIds:reads<6?['attempt']:[]}}]})}};
 const result=await settleLiveProject(api,'p',clock());assert.equal(result.settled,true);assert.ok(reads>=8);
});
test('cleanup pauses durable between-turn work before interrupting its exact local request',async()=>{
 const calls=[];let paused=false,active=true;
 const api={runtime:{listWorkTasks:async()=>({tasks:[{id:'w',status:paused?'paused':'running',threadId:'t',controlRevision:3}]})},agent:{status:async()=>({executionHealth:[]}),
  controlWorkTask:async(...args)=>{calls.push(args);paused=true;},listExecutions:async()=>active?[{requestId:'r'}]:[],interrupt:async target=>{calls.push(target);active=false;}}};
 const result=await stopLiveProject(api,'p',clock());assert.equal(result.settled,true);assert.deepEqual(calls,[['p','w','pause',3,'t'],{projectId:'p',requestId:'r'}]);
});
test('open interlude and unresolved usage produce a bounded non-success result',async()=>{
 const api={agent:{status:async()=>({executionHealth:[]}),listExecutions:async()=>[]},runtime:{listWorkTasks:async()=>({tasks:[{status:'running',interlude:{phase:'answered'}}]})}};
 assert.equal((await settleLiveProject(api,'p',clock())).settled,false);
 api.runtime.listWorkTasks=async()=>({tasks:[],nextCursor:'more'});
 await assert.rejects(settleLiveProject(api,'p',clock()),/truncated/);
});
test('one-chapter proposal follows real approval contract; changed sources and expanded scope do not execute',async()=>{
 const calls=[],proposal={projectId:'p',state:'proposed',targetChapters:1,proposalId:'proposal',authorIntent:'写开场',expectedRevision:3,baseSnapshotId:'s',sourceDigest:'d'};
 const api={runtime:{getCreativeWorkspace:async()=>({revision:3,headSnapshotId:'s',sourceDigest:'d'})},agent:{generateWork:async(...args)=>{calls.push(args);return {status:'completed'};}}};
 await executeLiveHandoff(api,'p',proposal);assert.deepEqual(calls,[['p','写开场',1,3,'s','d',null,'proposal']]);
 await assert.rejects(executeLiveHandoff(api,'p',{...proposal,targetChapters:2}),/scope/);
 await assert.rejects(executeLiveHandoff(api,'p',{...proposal,sourceDigest:'old'}),/changed/);assert.equal(calls.length,1);
});
test('handoff keeps total scenario requests across attempts and retains unknown cost without inventing tokens',()=>{
 const receipts=[{runId:'first',status:'failed',usage:{requests:2,tokens:123,unknownTokens:700}},{runId:'second',status:'completed',usage:{requests:1}}];
 assert.equal(liveRemainingRequests(6,receipts),3);
 assert.throws(()=>liveRemainingRequests(3,receipts),/exhausted/);
 assert.throws(()=>liveRemainingRequests(6,[...receipts,receipts[0]]),/duplicate/);
 assert.throws(()=>liveRemainingRequests(6,[{runId:'active',status:'active',usage:{requests:1}}]),/Incomplete/);
});

test('a scheduled background retry is not mistaken for settled work between requests',async()=>{
 const api={agent:{listExecutions:async()=>[],status:async()=>({executionHealth:[{projectId:'p',workTaskId:'w',state:'waiting',nextCheckAt:60000}]})},runtime:{listWorkTasks:async()=>({tasks:[{id:'w',status:'blocked'}]})}};
 const result=await settleLiveProject(api,'p',clock());assert.equal(result.settled,false);assert.equal(result.scheduled.length,1);
 api.runtime.listWorkTasks=async()=>({tasks:[{id:'w',status:'paused'}]});
 assert.equal((await settleLiveProject(api,'p',clock())).settled,true);
});

test('candidate approval uses only the current synthetic chapter and never repeats an action',async()=>{
 const calls=[],attempted=new Set();let action={id:'a',sourceTaskId:'t',status:'proposed',operation:'generateChapter',target:{chapterId:'chapter'}};
 const api={agent:{listProposedActions:async()=>({actions:[action]}),approveAction:async(...args)=>{calls.push(args);return {saved:true};}}};
 assert.deepEqual(await approveLiveCandidate(api,'p','chapter',attempted),{actionId:'a',result:{saved:true}});
 assert.equal(await approveLiveCandidate(api,'p','chapter',attempted),null);
 assert.deepEqual(calls,[['p','t','a']]);
 action={...action,id:'b',target:{chapterId:'other'}};
 await assert.rejects(approveLiveCandidate(api,'p','chapter',attempted),/scope/);
 assert.equal(calls.length,1);
});
test('idle approval is held while the parent is executing or an interlude remains open',async()=>{
 let tick=0,approvals=0;
 const api={agent:{listExecutions:async()=>++tick===1?[{requestId:'r'}]:[],status:async()=>({executionHealth:[]})},runtime:{listWorkTasks:async()=>({tasks:[{id:'w',status:tick<4?'running':'completed',interlude:tick===2?{phase:'answered'}:null}]})}};
 await settleLiveProject(api,'p',{...clock(),onIdle:async()=>{assert.ok(tick>=3);approvals++;}});
 assert.ok(approvals>0);
});

test('explicit two-chapter acceptance checks exact scope and does not allow arbitrary expansion',async()=>{
 const calls=[]; const proposal={projectId:'p',state:'proposed',targetChapters:2,resumeRun:null,proposalId:'proposal',expectedRevision:3,baseSnapshotId:'s',sourceDigest:'d',authorIntent:'写两章'};
 const api={runtime:{getCreativeWorkspace:async()=>({revision:3,headSnapshotId:'s',sourceDigest:'d'})},agent:{generateWork:async(...args)=>calls.push(args)}};
 await executeLiveHandoff(api,'p',proposal,2);assert.equal(calls[0][2],2);
 await assert.rejects(executeLiveHandoff(api,'p',{...proposal,targetChapters:3},3),/scope/);assert.equal(calls.length,1);
});
