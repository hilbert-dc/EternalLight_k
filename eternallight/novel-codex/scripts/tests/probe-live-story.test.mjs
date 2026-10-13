import test from 'node:test';
import assert from 'node:assert/strict';
import {storyProbeOptions,probeLiveStory,storyProbePrompt,storyProbeAccounting} from '../probe-live-story.mjs';
import {mkdtemp,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {PersistentTaskBudget} from '@eternallight/agent-engine';

test('story probe reports durable terminal usage after completion closes the active budget',async t=>{
 const directory=await mkdtemp(join(tmpdir(),'story-probe-accounting-'));
 t.after(()=>rm(directory,{recursive:true,force:true}));
 for(const known of [true,false]){
  const path=join(directory,`${known}.sqlite`);
  const budget=new PersistentTaskBudget({databasePath:path,runId:`test-${known}`,maxRequests:6,maxTokens:80000});
  t.after(()=>budget.close());
  const id=budget.reserveRequest({estimatedInputTokens:30,maxOutputTokens:20});
  if(known) budget.settleReservation(id,{total_tokens:17});
  assert.equal(storyProbeAccounting(budget,path,false).requests,1);
  budget.complete('completed');
  assert.throws(()=>budget.snapshot(),/closed/);
  const report=storyProbeAccounting(budget,path,true);
  assert.equal(report.status,'completed');
  assert.equal(report.requests,1);
  assert.equal(report.tokens,known?17:0);
  assert.equal(report.unknownTokens,known?0:50);
  assert.equal(report.maxTokens,80000);
  assert.equal(report.maxRequests,6);
  assert.equal(report.overrun,false);
 }
});
test('story probe does not ask the model to supply trusted transport identity or derived purpose',()=>{
 const prompt=storyProbePrompt(7);
 const args=JSON.parse(prompt.match(/参数为 (\{.*?\})。/)[1]);
 assert.deepEqual(args,{expectedRevision:7,budget:12,byteBudget:16000});
 for(const field of ['projectId','taskId','threadId','purpose']) assert.ok(!prompt.includes(field));
 assert.match(prompt,/按需读取作品资料/);
 assert.match(prompt,/不要调用写入工具/);
});
test('story probe keeps explicit cost approval separate from the exact requested token ceiling',()=>{
 assert.deepEqual(storyProbeOptions([]),{confirmCost:false,maxTokens:40000});
 assert.deepEqual(storyProbeOptions(['--max-tokens=80000']),{confirmCost:false,maxTokens:80000});
 assert.deepEqual(storyProbeOptions(['--confirm-cost','--max-tokens=80000']),{confirmCost:true,maxTokens:80000});
 for(const args of [['--max-tokens=0'],['--max-tokens=-1'],['--max-tokens=1.5'],['--max-tokens=9007199254740992'],['--max-tokens=1','--max-tokens=2'],['--confirm-cost','--confirm-cost'],['--unknown']])assert.throws(()=>storyProbeOptions(args));
});
test('invalid programmatic budgets fail before reading credentials or starting a provider',async()=>{
 for(const maxTokens of [0,-1,1.5,NaN,Infinity])await assert.rejects(probeLiveStory({maxTokens}),/maxTokens/);
});
