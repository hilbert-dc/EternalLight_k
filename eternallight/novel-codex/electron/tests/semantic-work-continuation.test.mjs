import test from 'node:test';
import assert from 'node:assert/strict';
import { rebindIdleContinuation, executeWorkContinuation } from '../semantic-work-continuation.mjs';
import { nativeRuntimeError } from '../runtime-request-error.mjs';
const id = `generation-${'a'.repeat(32)}`;
const task = { id:'task-source',projectId:'p',threadId:'t',operation:'advisory',target:{},status:'completed' };
const proposal = { proposalId:'generation-proposal-source',projectId:'p',sourceTaskId:task.id,threadId:'t',resumeRun:{id,expectedVersion:4} };
const discussion = { workflow:{task}, generationHandoff:proposal };
const originalWork={id:`work-${id}`,projectId:'p',threadId:'t',kind:'generation',status:'running'};
function runtime({handoff={task,proposal}, error=null, afterStart=null, shouldExecute=true}={}) {
 const calls=[];
 const request=async(path,options)=>{
  calls.push({path,options});
  if(path.endsWith('/generation-handoff/execute')) {
   assert.equal(options.method,'POST');assert.deepEqual(JSON.parse(options.body),{proposalId:handoff.proposal.proposalId});
   if(error)throw Error(error);
   afterStart?.();
   return {run:{id},receipt:{runId:id,startedVersion:5},shouldExecute};
  }
  if(path.endsWith('/generation-handoff'))return {handoff};
  if(path.endsWith(`/work-${id}`))return originalWork;
  assert.fail(`unexpected request ${path}`);
 };
 return {request,calls};
}
test('only a Native model proposal triggers continuation, independently of wording',async()=>{
 for(const message of ['继续','不要继续','先检查六章，再续写','continue explaining']) {
  assert.deepEqual(await executeWorkContinuation(()=>assert.fail('prose has no Work identity'),'p',{message},()=>assert.fail('no dispatch')),{message});
  const {request}=runtime({handoff:null});
  const result={...discussion,message,generationHandoff:null};
  assert.equal(await executeWorkContinuation(request,'p',result,()=>assert.fail('Native has no proposal')),result);
 }
 const {request}=runtime();let count=0;
 const result=await executeWorkContinuation(request,'p',discussion,async selection=>{
  count++;assert.deepEqual(selection,{workTaskId:`work-${id}`,expectedThreadId:'t',origin:'automatic',generationHandoff:{sourceTaskId:task.id,proposalId:proposal.proposalId}});
  return {...originalWork,status:'completed'};
 });
 assert.equal(count,1);assert.equal(result.workTask.status,'completed');
});
test('an already completed predecessor resumes from durable state without an in-memory decision',async()=>{
 const work={id:'work-task-source',projectId:'p',threadId:'t',status:'completed',steps:[]};
 const {request,calls}=runtime();let count=0;
 const result=await executeWorkContinuation(request,'p',{workTask:work},async()=>{count++;return originalWork;});
 assert.equal(count,1);assert.equal(result.workTask.id,originalWork.id);
 assert.ok(calls[0].path.includes('work-task-source/generation-handoff'));
});
test('a crash after Native start is recovered without repeating the version transition',async()=>{
 let persisted=false;
 const first=runtime({afterStart:()=>persisted=true});
 const failed=await executeWorkContinuation(first.request,'p',discussion,async()=>{throw Error('Host exited');});
 assert.equal(persisted,true);assert.equal(failed.executionFailure.code,'WORK_CONTINUATION_FAILED');
 const recovered=runtime();let dispatched=0;
 const result=await executeWorkContinuation(recovered.request,'p',{workTask:{id:'work-task-source',projectId:'p',threadId:'t'}},async()=>{dispatched++;return {...originalWork,status:'completed'};});
 assert.equal(dispatched,1);assert.equal(result.workTask.status,'completed');
 assert.equal(recovered.calls.filter(c=>c.path.endsWith('/rebind')||c.path.endsWith('/control')).length,0);
});
test('fulfilled gate, stale CAS and unknown writes prevent any Host dispatch',async()=>{
 for(const error of ['prerequisite partial','needsAuthorDecision','original run version changed','unknown write']) {
  const {request}=runtime({error});
  const result=await executeWorkContinuation(request,'p',discussion,()=>assert.fail('Native rejection prevents dispatch'));
  assert.equal(result.executionFailure.code,'WORK_CONTINUATION_FAILED');assert.match(result.executionFailure.message,/核对当前任务状态与已保存结果/);
  assert.equal(result.executionFailure.details.diagnostic,error);
 }
 const {request}=runtime({handoff:{task:{...task,threadId:'foreign'},proposal}});
 const result=await executeWorkContinuation(request,'p',discussion,()=>assert.fail('foreign decision prevents dispatch'));
 assert.match(result.executionFailure.message,/来源/);
});

test('untyped network failures remain diagnostic details while trusted local explanations stay clear',async()=>{
 const {request}=runtime();
 const result=await executeWorkContinuation(async(path,options)=>{
  if(path.endsWith('/execute'))throw Error('fetch failed; Bearer transient-token12345');
  return request(path,options);
 },'p',discussion,()=>assert.fail('network failure must not dispatch'));
 assert.match(result.executionFailure.message,/接续未成功/);
 assert.ok(!result.executionFailure.message.includes('fetch failed'));
 assert.ok(result.executionFailure.details.diagnostic.includes('fetch failed'));
 assert.ok(!result.executionFailure.details.diagnostic.includes('transient-token'));
 const mismatch=runtime({handoff:{task:{...task,threadId:'foreign'},proposal}});
 const local=await executeWorkContinuation(mismatch.request,'p',discussion,()=>assert.fail('foreign task cannot dispatch'));
 assert.equal(local.executionFailure.message,'原任务恢复决定的来源不匹配');
});
test('an already consumed proposal cannot override a later author pause or duplicate completion',async()=>{
 const {request,calls}=runtime({shouldExecute:false});
 const result=await executeWorkContinuation(request,'p',discussion,()=>assert.fail('Native receipt forbids another resume'));
 assert.equal(result.workTask.id,originalWork.id);
 assert.equal(calls.filter(c=>c.path.endsWith('/generation-handoff/execute')).length,1);
});
test('production send and resumed Work share the same durable handoff and retain saved results',async()=>{
 const {registerAdvisoryPromptHandler}=await import('../revision-repair-handlers.mjs');
 for(const prompt of ['照原来的安排往下写','先不要续写，只说明进度']) {
  const chosen=prompt.startsWith('照')?proposal:null; // Scripted model fixture, never production routing.
  const calls=[];const handlers=new Map();
  const saved={...discussion,candidateResults:[{taskId:'saved-edit'}],generationHandoff:chosen};
  const {request}=runtime({handoff:chosen?{task,proposal}:null});
  registerAdvisoryPromptHandler({handle:(n,f)=>handlers.set(n,f)},{routingState:()=>({}),withWorkFlow:async(_p,fn)=>{calls.push('flow');return fn();},
   runAdvisoryAgentTask:async(_e,_p,text)=>{assert.equal(text,prompt);calls.push('model');return {...discussion,generationHandoff:null};},
   analysisDependencies:{runtimeRequest:request,runConversationReviews:async()=>({}),currentWorkFlowState:async()=> 'completed',continueCreativeWork:async()=>saved},
   resumeConversationWork:async()=>{calls.push('resume');return {...originalWork,status:'paused'};}});
  const result=await handlers.get('agent:send-prompt')({},'p',prompt,null,null,'t');
  assert.deepEqual(calls,chosen?['flow','model','resume']:['flow','model']);
  assert.deepEqual(result.candidateResults,saved.candidateResults);
  if(chosen)assert.equal(result.executionFailure.code,'WORK_CONTINUATION_INCOMPLETE');
 }
});
test('direct author checkpoint rebind preserves original scope, allowance and intent fields',async()=>{
 const run={id,projectId:'p',version:86,status:'paused',activeTaskId:'cancelled-writer',currentChapterId:'chapter-8',sourceDigest:'old'};
 const calls=[];
 const request=async(path,options)=>{calls.push({path,options});return {run,availableFrom:['currentChapter'],activeAttemptStatus:'cancelled',projectRevision:19,sourceDigest:'revised-7'};};
 assert.equal(await rebindIdleContinuation(request,'p',id,{resumeBlockReason:null},run),true);
 const body=JSON.parse(calls[1].options.body);
 assert.deepEqual(body,{expectedVersion:86,expectedProjectRevision:19,expectedSourceDigest:'revised-7',restartFrom:'currentChapter'});
 await assert.rejects(rebindIdleContinuation(request,'p',id,{resumeBlockReason:'pendingRecovery'},run),/保存结果尚未核对/);
 await assert.rejects(rebindIdleContinuation(async()=>({run:{...run,version:87},availableFrom:['currentChapter']}),'p',id,{},run),/变化/);
 await assert.rejects(rebindIdleContinuation(async()=>({run,availableFrom:['currentChapter'],activeAttemptStatus:'running'}),'p',id,{},run),/执行中/);
 assert.equal(await rebindIdleContinuation(request,'p',id,{}, {...run,status:'running'}),false);
});

test('Native codes drive clear Chinese failures while internal diagnostics are bounded and redacted',async()=>{
 const error=nativeRuntimeError({error:{code:'generation_handoff_unsettled_results',message:'A tool write has no receipt',details:{diagnostic:'pending writer; sk-testcredential12345 Bearer test-access-token12345'},recovery:null}});
 assert.equal(error.code,'generation_handoff_unsettled_results');
 assert.ok(!error.details.diagnostic.includes('testcredential'));assert.ok(!error.details.diagnostic.includes('test-access'));
 const {request}=runtime();
 const result=await executeWorkContinuation(async(path,options)=>{
  if(path.endsWith('/execute'))throw error;
  return request(path,options);
 },'p',discussion,()=>assert.fail('unknown writes cannot dispatch'));
 assert.match(result.executionFailure.message,/待核对的保存或结算/);
 assert.equal(result.executionFailure.details.nativeCode,error.code);
 assert.ok(!result.executionFailure.message.includes('tool write'));
 assert.ok(!result.executionFailure.details.diagnostic.includes('testcredential'));
 const bounded=nativeRuntimeError({error:{code:'conflict',message:'x'.repeat(3000),details:{}}});
 assert.equal(bounded.details.diagnostic.length,2000);
 const unknown=nativeRuntimeError({error:{code:'internal',message:'An internal Native diagnostic',details:{}}});
 const unknownResult=await executeWorkContinuation(async(path,options)=>{
  if(path.endsWith('/execute'))throw unknown;
  return request(path,options);
 },'p',discussion,()=>assert.fail('unrecognised Native failure cannot dispatch'));
 assert.match(unknownResult.executionFailure.message,/核对当前任务状态与已保存结果/);
 assert.equal(unknownResult.executionFailure.details.diagnostic,'An internal Native diagnostic');
});
