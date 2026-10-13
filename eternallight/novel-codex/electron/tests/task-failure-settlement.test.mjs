import test from 'node:test';
import assert from 'node:assert/strict';
import { settleFailedTask } from '../task-failure-settlement.mjs';

const task = {id:'attempt-old',projectId:'project',threadId:'thread',status:'queued'};
for (const state of ['completed','cancelled','failed']) test(`settlement preserves existing ${state} receipt and saved results`, async()=>{
  const current={...task,status:state,outputRefs:['saved']};let writes=0;
  await settleFailedTask(async(_path,options)=>{if(options)writes++;return structuredClone(current);},'project',task,{cancellationRequested:true},new Error('original'));
  assert.equal(writes,0);assert.deepEqual(current.outputRefs,['saved']);
});
test('lost PATCH response is reconciled against the exact persisted Attempt',async()=>{
  let current={...task};let writes=0;
  await settleFailedTask(async(_path,options)=>{
    if(options){const body=JSON.parse(options.body);writes++;assert.equal(body.threadId,'thread');current={...current,...body};throw Error('response lost');}
    return structuredClone(current);
  },'project',task,{cancellationRequested:true},new Error('cancelled locally'));
  assert.equal(current.status,'failed');assert.match(current.error,/^execution_interrupted;/);assert.equal(writes,1);
});
test('unknown terminal delivery is visible and preserves first failure',async()=>{
  await assert.rejects(settleFailedTask(async(_path,options)=>{if(options)throw Error('offline');return {...task};},'project',task,{},new Error('original provider timeout')),/original provider timeout；原任务终止状态尚待核对：offline/);
});
test('foreign identity never receives a write',async()=>{
  let writes=0;
  await assert.rejects(settleFailedTask(async(_path,options)=>{if(options)writes++;return {...task,threadId:'other'};},'project',task,{},new Error('original')),/身份无法核对/);
  assert.equal(writes,0);
});
test('provider text cannot claim a trusted local interruption',async()=>{
  let current={...task};
  await settleFailedTask(async(_path,options)=>{if(options)Object.assign(current,JSON.parse(options.body));return structuredClone(current);},'project',task,{},new Error('execution_interrupted; supplied by provider'));
  assert.match(current.error,/^execution_failed;/);
});
