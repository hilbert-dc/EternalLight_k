import assert from 'node:assert/strict';
import test from 'node:test';
import {mkdtemp,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {TaskBudget,PersistentTaskBudget} from '../src/index.mjs';

for (const persistent of [false,true]) for (const finite of [false,true]) {
 test(`${persistent?'persistent':'memory'} unknown usage preserves ${finite?'finite ceiling':'metering-only continuation'}`,async t=>{
  const directory=await mkdtemp(join(tmpdir(),'budget-continuation-'));
  t.after(()=>rm(directory,{recursive:true,force:true}));
  const limits={maxRequests:2,maxTokens:finite?1000:null,maxDurationMs:null};
  const databasePath=join(directory,'engine.sqlite3');
  const budget=persistent?new PersistentTaskBudget({...limits,databasePath,runId:'continuation'}):new TaskBudget(limits);
  t.after(()=>budget.close?.());
  const original={reservationId:'original',estimatedInputTokens:30,maxOutputTokens:70};
  const first=budget.reserveRequest(original);
  budget.settleReservation(first,null,{outcome:'unknown'});
  assert.throws(()=>budget.reserveRequest(original),/usage is unknown/);
  const next={reservationId:'new-request',estimatedInputTokens:10,maxOutputTokens:20};
  if(finite) assert.throws(()=>budget.reserveRequest(next),/usage is unknown/);
  else {
   const second=budget.reserveRequest(next);
   assert.notEqual(second,first);
   budget.settleReservation(second,{total_tokens:15});
   assert.throws(()=>budget.reserveRequest({...next,reservationId:'third-request'}),/request budget exhausted/);
  }
  assert.equal(budget.snapshot().requests,finite?1:2);
  assert.equal(budget.snapshot().unknownTokens,100);
  assert.equal(budget.snapshot().tokens,finite?0:15);
  if(persistent){
   budget.complete('completed');
   const receipt=PersistentTaskBudget.receipt(databasePath,'continuation');
   assert.equal(receipt.usage.unknownTokens,100);
   assert.equal(receipt.usage.requests,finite?1:2);
  }
 });
}
