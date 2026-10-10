import assert from 'node:assert/strict';
import test from 'node:test';
import { mainHandlers, rendererEvent } from './main-handler-harness.mjs';

test('a Native-claimed completed Work reaches decision admission instead of silently returning', async () => {
  const work = {id:'work-original',projectId:'project',threadId:'original-thread',kind:'conversation',status:'completed',version:1,
    steps:[{id:'source-step',operation:'advisory',status:'completed',target:{},currentAttemptId:'source-attempt',attempts:[{id:'source-attempt',status:'completed'}]}]};
  const calls=[]; let lease;
  const main=await mainHandlers(async (path,body) => {
    calls.push(path);
    if(path.endsWith('/execution-condition'))return {recorded:true};
    if(path.endsWith('/execution-owner/renew'))return lease;
    if(path.endsWith('/provider-usage'))return {pending:null};
    if(path.endsWith('/creative-workspace'))return {revision:0,sourceDigest:'a'.repeat(64)};
    if(path.endsWith('/decision'))return {workTaskId:work.id,threadId:work.threadId,step:null,stopReason:'completed'};
    if(path.endsWith('/generation-handoff'))return {handoff:null};
    if(path.endsWith('/work-tasks/'+work.id))return work;
    throw Error('Unexpected request '+path);
  },`${process.execPath} ${new URL('./fixture-app-server.mjs',import.meta.url).pathname}`,{nativeRuntime:true});
  const hostSessionId=main.workExecutionOwners.hostSessionId;
  lease={leaseId:'lease-original',hostSessionId,workTaskId:work.id,rootThreadId:work.threadId,expiresAt:Math.floor(Date.now()/1000)+60};
  const event={eventId:'event-original',workTaskId:work.id,rootThreadId:work.threadId,claimId:'claim-original',hostSessionId,sourceStepId:'source-step',sourceAttemptId:'source-attempt'};
  main.workExecutionOwners.adoptClaim('project',work,lease,event);
  try {
    await main.resumeWorkTask(rendererEvent(),'project',work.id,{origin:'automatic',expectedThreadId:work.threadId,
      durableWake:{eventId:event.eventId,sourceStepId:event.sourceStepId,sourceAttemptId:event.sourceAttemptId,claimId:event.claimId,leaseId:lease.leaseId,hostSessionId}});
    assert.equal(calls.filter(path=>path.endsWith('/decision')).length,1);
    assert.ok(!calls.some(path=>/\/control$|\/tasks$|budget-authorization/.test(path)));
  } finally {await main.workExecutionConsumer.stop(); await main.workExecutionOwners.stop(); main.adapter.close();}
});
