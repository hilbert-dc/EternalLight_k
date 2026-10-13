import test from 'node:test';
import assert from 'node:assert/strict';
import {build} from 'esbuild';
import {ExecutionEventJournal} from '../../shared/execution-event-journal.mjs';
const bundled=await build({entryPoints:[new URL('../../src/execution-snapshot.ts',import.meta.url).pathname],bundle:true,platform:'node',format:'esm',write:false});
const {ExecutionSnapshotBuffer}=await import(`data:text/javascript;base64,${Buffer.from(bundled.outputFiles[0].text).toString('base64')}`);
const event=(sequence,method='item/agentMessage/delta',params={threadId:'thread',turnId:'turn',itemId:'answer',delta:'字'})=>({method,params,hostExecution:{id:'execution',projectId:'project',requestId:'request',threadId:'thread',sequence}});
const snapshot=(cursor,events=[])=>({executionId:'execution',cursor,events,questions:[]});
test('snapshot replay retains only later events and merges question delivery/resolution',()=>{
 const buffer=new ExecutionSnapshotBuffer();
 buffer.capture(event(1),'project');buffer.capture(event(2),'project');assert.equal(buffer.capture(event(2),'project'),false);
 buffer.capture(event(3,'agent/user-input-requested',{request:{executionId:'execution',requestId:7}}),'project');
 let restored=buffer.merge(snapshot(1,[event(1)]));
 assert.deepEqual(restored.events.map(item=>item.hostExecution.sequence),[1,2,3]);assert.equal(restored.questions[0].requestId,7);
 buffer.capture(event(4,'serverRequest/resolved',{requestId:7}),'project');
 assert.equal(buffer.merge(snapshot(1)).questions.length,0);
 buffer.capture({...event(5),hostExecution:{...event(5).hostExecution,projectId:'foreign'}},'project');
 assert.equal(buffer.merge(snapshot(4)).events.length,0);
});
test('a missing or evicted suffix refuses incomplete restoration',()=>{
 const buffer=new ExecutionSnapshotBuffer();buffer.capture(event(3),'project');
 assert.throws(()=>buffer.merge(snapshot(1)),/不连续/);
 const journal=new ExecutionEventJournal();for(let i=1;i<=600;i++)journal.append(event(i));
 assert.equal(journal.snapshot().events.length,512);assert.equal(journal.snapshot().truncated,true);
 assert.throws(()=>journal.after(0),/不连续/);assert.equal(journal.after(599).length,1);
});
test('Host compacts contiguous deltas and returns isolated snapshots under memory bounds',()=>{
 const journal=new ExecutionEventJournal({compact:true});for(let i=1;i<=1000;i++)journal.append(event(i));
 const result=journal.snapshot();assert.equal(result.cursor,1000);assert.equal(result.events.length,1);assert.equal(result.events[0].params.delta.length,1000);
 result.events[0].params.delta='changed';assert.equal(journal.snapshot().events[0].params.delta.length,1000);
 journal.append(event(1001,'agent/error',{message:'x'.repeat(2*1024*1024)}));assert.equal(journal.snapshot().truncated,true);
 journal.startTurn();journal.append(event(1002,'turn/started',{turn:{id:'next'}}));assert.equal(journal.snapshot().truncated,false);assert.equal(journal.snapshot().cursor,1002);
});

test('a restored cursor rejects duplicate live delivery even with no events buffered',()=>{
 const buffer=new ExecutionSnapshotBuffer();buffer.merge(snapshot(20));
 assert.equal(buffer.capture(event(20),'project'),false);
 assert.equal(buffer.capture(event(21),'project'),true);
 assert.equal(buffer.merge(snapshot(20)).events.length,1);
});
