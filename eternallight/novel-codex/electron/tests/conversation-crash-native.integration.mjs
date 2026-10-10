import assert from 'node:assert/strict';
import test from 'node:test';
import {fork,execFile} from 'node:child_process';
import {randomUUID} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {promisify} from 'node:util';
import {rm} from 'node:fs/promises';
import {PersistentTaskBudget} from '@eternallight/agent-engine';
import {nativeRuntimeFixture} from './helpers/native-runtime-fixture.mjs';
import {WorkExecutionOwners} from '../work-execution-events.mjs';

async function killTree(child) {
  if(child.exitCode!==null || child.signalCode!==null)return;
  const exited=new Promise(resolve=>child.once('exit',resolve));
  if(process.platform==='win32')await promisify(execFile)('taskkill',['/pid',String(child.pid),'/t','/f']);
  else process.kill(-child.pid,'SIGKILL');
  await exited;
}
function host(t,config) {
  const child=fork(fileURLToPath(new URL('./fixtures/conversation-crash-host.mjs',import.meta.url)),[],{detached:process.platform!=='win32',stdio:['ignore','pipe','pipe','ipc']});
  const closed=new Promise(resolve=>child.once('exit',resolve));
  let diagnostic='';
  for(const stream of [child.stdout,child.stderr])stream.on('data',data=>{diagnostic=(diagnostic+data).slice(-5000);});
  t.after(()=>killTree(child));
  const result=new Promise((resolve,reject)=>{
    const timer=setTimeout(()=>reject(Error(`Host checkpoint timeout: ${diagnostic}`)),30_000);
    child.once('error',error=>{clearTimeout(timer);reject(error);});
    child.once('exit',(code,signal)=>{clearTimeout(timer);reject(Error(`Host exited ${code}/${signal}: ${diagnostic}`));});
    child.once('message',message=>{clearTimeout(timer);message.kind==='error'?reject(Error(message.message)):resolve(message);});
  });
  child.send(config);
  return {child,result,closed};
}
for(const scenario of ['submitted','claimed','taskBound','sending','modelStarted','turnBound','delivered','taskBoundResume','taskBoundFirstResume','sendingLostLedger','mediaTaskBoundResume','mediaTurnBound','ownerExpired']) {
  const mediaScenario=scenario.startsWith('media');
  const rejectResume=scenario==='sendingLostLedger',resumeBound=['taskBoundResume','taskBoundFirstResume','sendingLostLedger','mediaTaskBoundResume'].includes(scenario),window=rejectResume?'sending':resumeBound?'taskBound':scenario==='mediaTurnBound'?'turnBound':scenario==='ownerExpired'?'modelStarted':scenario;
  test(`hard Host crash after ${scenario} preserves Native delivery without implicit replay`,{timeout:360_000},async t=>{
    const native=await nativeRuntimeFixture(t);
    if(scenario==='taskBoundResume')new PersistentTaskBudget({databasePath:native.budgetDb,runId:'prior-completed-fixture',id:'budget-prior-completed-fixture'}).complete('completed');
    const {project,activeEditionId}=await native.request('/api/projects',{title:'隔离崩溃窗口',logline:'',medium:mediaScenario?'comic':'novel',idempotencyKey:randomUUID()});
    const workspace=mediaScenario?await native.request(`/api/projects/${project.id}/editions/${activeEditionId}/workspace`):null;
    const media=workspace?{scope:Object.fromEntries(['projectId','editionId','scopeId','branchId'].map(key=>[key,workspace.edition[key]])),expectedRevisionId:workspace.revisionId,selection:null,imageAssetRevisionIds:[]}:null;
    const config=()=>({projectId:project.id,origin:native.origin,authorToken:native.authorToken,candidateToken:native.candidateToken,budgetDb:native.budgetDb,directory:native.directory,media});
    const first=host(t,{...config(),mode:'crash',window});
    const point=await first.result;
    assert.equal(point.name,window);assert.ok(point.messageId);assert.ok(point.threadId);
    const path=`/api/projects/${project.id}/conversations/${point.threadId}/messages/${point.messageId}`;
    const before=await native.request(path);
    await killTree(first.child);
    if(rejectResume)for(const suffix of ['', '-wal', '-shm'])await rm(native.budgetDb+suffix,{force:true});
    await native.restart();
    assert.deepEqual(await native.request(path),before,'hard termination must not run graceful failure cleanup');
    if(scenario==='ownerExpired'){
      const workPath=`/api/projects/${project.id}/work-tasks/${point.lease.workTaskId}`;
      const work=await native.request(workPath);
      const owners=new WorkExecutionOwners({hostSessionId:randomUUID(),request:(path,options={})=>native.request(path,options.body?JSON.parse(options.body):undefined,options.method??'GET')});
      t.after(()=>owners.stop());
      assert.ok(point.lease.expiresAt*1000>Date.now(),'test must observe the unexpired original lease');
      await assert.rejects(owners.acquire(project.id,work),'a second Host cannot steal a live lease');
      await new Promise(resolve=>setTimeout(resolve,Math.max(0,point.lease.expiresAt*1000-Date.now()+100)));
      const {record}=await owners.acquire(project.id,await native.request(workPath));
      assert.notEqual(record.lease.leaseId,point.lease.leaseId);
      const stale={hostSessionId:point.lease.hostSessionId,leaseId:point.lease.leaseId};
      await assert.rejects(native.request(`${workPath}/execution-owner/renew`,stale));
      assert.equal((await native.request(`${workPath}/execution-owner/release`,stale)).released,false);
      await owners.verify(record);
      assert.equal(record.lease.hostSessionId,owners.hostSessionId);
      await owners.stop();
      assert.deepEqual(await native.request(path),before,'lease takeover is not message delivery');
    }
    const safe=['submitted','claimed'].includes(window);
    const second=host(t,{...config(),mode:'recover',threadId:point.threadId,messageId:point.messageId,continueSafe:safe,resumeBound,rejectResume});
    const recovered=await second.result;
    await second.closed;
    assert.equal(recovered.kind,'result');assert.equal(recovered.prior.id,before.id);
    assert.equal(recovered.prior.input.text,before.input.text);
    if(mediaScenario)assert.deepEqual(recovered.final.input.media,media);
    assert.equal(recovered.automaticWrites,0,'new Host must only read before author reconciliation');
    assert.equal(recovered.reconciledTaskWrites,0,'reconciliation must not dispatch a Task');
    if(safe){
      assert.equal(recovered.reconciled.status,'queued');
      assert.equal(recovered.listed[0].requiresContinue,true);
      assert.equal(recovered.final.status,'delivered');assert.equal(recovered.taskWrites.length,1);
      assert.equal(recovered.taskWrites[0].body.target.messageDispatch.messageId,before.id);
      if(window==='claimed'){
        const response=await fetch(native.origin+path,{method:'POST',headers:{authorization:`Bearer ${native.authorToken}`,'content-type':'application/json'},body:JSON.stringify({action:'sending',expectedRevision:before.revision,sessionId:before.sessionId})});
        assert.equal(response.ok,false,'old Host claim cannot send after reconciliation and continuation');
      }
    }else if(rejectResume){
      assert.match(recovered.resumeError,/TaskBindingInvalid|费用|receipt/);
      assert.equal(recovered.taskWrites.length,0);assert.equal(recovered.final.status,'sending');
      const work=await native.request(`/api/projects/${project.id}/tasks/${before.taskId}/work-task`);
      assert.ok((await native.request(`/api/projects/${project.id}/work-tasks/${work.id}/provider-usage`)).pending);
    }else if(resumeBound){
      assert.equal(recovered.taskWrites.length,1,'explicit Work resume must reuse the admitted step');
      assert.ok(recovered.taskWrites[0].body.target.resumeStepId);assert.equal(recovered.taskWrites[0].body.initialBudget,undefined);
      assert.equal(recovered.final.status,'delivered','continued original message must not remain claimed forever');
      const work=await native.request(`/api/projects/${project.id}/tasks/${before.taskId}/work-task`);
      assert.equal(work.steps.flatMap(step=>step.attempts).length,2);
      if(mediaScenario){
        const resumed=await native.request(`/api/projects/${project.id}/tasks/${recovered.final.taskId}`);
        assert.equal(resumed.target.type,'edition');assert.equal(resumed.target.scopeId,media.scope.scopeId);
        assert.equal(resumed.target.revisionId,media.expectedRevisionId);
        assert.equal(resumed.threadId,point.threadId);
      }
    }else{
      assert.equal(recovered.taskWrites.length,0);
      assert.equal(recovered.final.taskId,before.taskId);
      assert.equal(recovered.final.status,['turnBound','delivered'].includes(window)?'delivered':before.status);
      const task=await native.request(`/api/projects/${project.id}/tasks/${before.taskId}`);
      assert.equal(task.target.messageDispatch.messageId,before.id);
      const work=await native.request(`/api/projects/${project.id}/tasks/${before.taskId}/work-task`);
      assert.equal(work.steps.flatMap(step=>step.attempts).length,1);
    }
  });
}
