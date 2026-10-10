import test from "node:test";
import assert from "node:assert/strict";
import {mkdtemp,rm} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import {PersistentMediaLedger,MediaOperationExecutor,digest} from "../src/index.mjs";

const grant=()=>({id:"grant",projectId:"p",scopeId:"s",branchId:"b",workTaskId:"work",maxRequests:2,unitLimits:{images:2},maxCostMinor:100,currency:"USD",expiresAt:100000});
const operation=(id="op")=>({id,projectId:"p",scopeId:"s",branchId:"b",workTaskId:"work",attemptId:"a",providerId:"test",kind:"image.generate",idempotencyKey:id,requestDigest:digest({prompt:"test"}),request:{prompt:"test"},reservation:{units:{images:1},costMinor:50,currency:"USD"},connection:{id:"test",protocol:"openai-images",baseUrl:"https://api.example/v1",model:"gpt-image-1.5",credentialRealmId:"test-realm"}});
async function fixture(t){const dir=await mkdtemp(join(tmpdir(),"media-ledger-"));t.after(()=>rm(dir,{recursive:true,force:true}));let now=100;const ledger=new PersistentMediaLedger(join(dir,"budget.sqlite"),{clock:()=>now});return {dir,ledger,clock:()=>now,setNow:value=>{now=value;}};}
test("native ledger persists exact operation and prevents replenishment, double ownership and stale fences",async t=>{
  const {dir,ledger,clock,setNow}=await fixture(t);const initial=ledger.prepare(grant(),operation());assert.equal(initial.state,"prepared");
  assert.equal(ledger.find("missing"),null);assert.equal(ledger.find("op").id,"op");
  assert.deepEqual(ledger.prepare(grant(),operation()),initial);assert.throws(()=>ledger.prepare({...grant(),maxRequests:3},operation()),/replenished/);
  assert.throws(()=>ledger.prepare(grant(),{...operation("different-id"),idempotencyKey:"op"}),/UNIQUE/);
  assert.throws(()=>ledger.prepare(grant(),{...operation("bad"),request:{prompt:"changed"}}),/digest mismatch/);
  const owned=ledger.claim("op","owner",100);const second=new PersistentMediaLedger(join(dir,"budget.sqlite"),{clock});assert.throws(()=>second.claim("op","other",100),/already has an owner/);
  ledger.admit(owned);const submitting=ledger.transition(owned,"submitting");ledger.release("op");setNow(101);const recovered=second.claim("op","other",100);
  assert.equal(recovered.state,"submitting");assert.ok(recovered.fence>submitting.fence);assert.throws(()=>ledger.transition(submitting,"failed"),/ownership is stale/);
  assert.throws(()=>second.transition(recovered,"submitting"),/cannot be repeated/);
  assert.throws(()=>second.settle(recovered,{receiptId:"fake-refund",outcome:"not_sent",usage:{units:{},costMinor:0}}),/cannot be refunded/);
  const unknown=second.transition(recovered,"outcome_unknown");second.settle(unknown,{receiptId:"unknown",outcome:"unknown",usage:{units:{images:0},costMinor:0}});second.release("op");
  ledger.prepare(grant(),operation("op2"));assert.throws(()=>ledger.prepare(grant(),operation("op3")),/budget exhausted/);
});
test("cancelled-before-submit releases budget but a terminal result needs durable receipt",async t=>{
  const {ledger}=await fixture(t);ledger.prepare(grant(),operation());let owned=ledger.claim("op","owner");ledger.requestCancel("op");owned=ledger.get("op");assert.throws(()=>ledger.transition(owned,"submitting"),/cancelled/);
  const cancelled=ledger.transition(owned,"cancelled");ledger.settle(cancelled,{receiptId:"not-sent",outcome:"not_sent",usage:{units:{},costMinor:0}});ledger.release("op");
  ledger.prepare(grant(),operation("op2"));ledger.prepare(grant(),operation("op3"));
  let active=ledger.claim("op2","owner");ledger.admit(active);active=ledger.transition(active,"submitting");active=ledger.transition(active,"collecting");assert.throws(()=>ledger.transition(active,"ready"),/durable asset receipt/);
  active=ledger.transition(active,"ready",{outputAssetRevisionIds:["asset-rev"],resultReceipt:{operationId:"op2"}});ledger.release("op2");active=ledger.claim("op2","reconciler");assert.throws(()=>ledger.transition(active,"ready",{outputAssetRevisionIds:["changed"]}),/terminal/);
  assert.equal(ledger.settle(active,{receiptId:"bill",outcome:"known",usage:{units:{images:1},costMinor:20}}).settlement.usage.costMinor,20);ledger.release("op2");
});
test("expired execution leases cannot mutate or renew and require a fresh fence",async t=>{
  const {ledger,setNow}=await fixture(t);ledger.prepare(grant(),operation());const owned=ledger.claim("op","owner",10);setNow(111);
  assert.throws(()=>ledger.transition(owned,"submitting"),/ownership is stale/);assert.throws(()=>ledger.renew(owned),/ownership is stale/);
  ledger.release("op");const recovered=ledger.claim("op","new-owner");assert.ok(recovered.fence>owned.fence);assert.equal(recovered.state,"prepared");ledger.release("op");
});
function executor(ledger,adapter,options={}) {return new MediaOperationExecutor({ledger,adapters:[adapter],clock:ledger.clock,authorize:async()=>{},admitResource:async record=>{const admission=ledger.admit(record);return admission.admitted?()=>ledger.releaseAdmission(record):admission;},collectOutputs:async()=>({assetRevisionIds:["asset"],receipt:{operationId:"op"}}),lookupOutputs:async()=>null,...options});}
const capabilities={capabilities:[{kind:"image.generate",available:true}]};
test("uncertain synchronous submit is never automatically repeated across executor recreation",async t=>{
  const {ledger,setNow}=await fixture(t);ledger.prepare(grant(),operation());let submits=0,lookups=0;
  const adapter={id:"test",inspect:()=>capabilities,validate:async()=>{},submit:async()=>{submits++;throw Error("transport dropped");},lookup:async()=>{lookups++;return {state:"outcome_unknown"};}};
  assert.equal((await executor(ledger,adapter).step("op")).state,"outcome_unknown");setNow(6000);assert.equal((await executor(ledger,adapter).step("op")).state,"outcome_unknown");assert.equal(submits,1);assert.equal(lookups,1);
});
test("cancel during submit keeps late job identity and cancels remote job without refund claim",async t=>{
  const {ledger}=await fixture(t);ledger.prepare(grant(),operation());let started,finish,cancels=0;const startedPromise=new Promise(resolve=>started=resolve);const submitted=new Promise(resolve=>finish=resolve);
  const adapter={id:"test",inspect:()=>capabilities,validate:async()=>{},submit:async()=>{started();return submitted;},cancel:async id=>{assert.equal(id,"late-job");cancels++;return {state:"cancelled",receipt:{id:"cancel"}};}};
  const engine=executor(ledger,adapter);const running=engine.step("op");await startedPromise;await engine.cancel("op");finish({state:"submitted",jobId:"late-job"});const final=await running;
  assert.equal(final.state,"cancelled");assert.equal(final.providerJobId,"late-job");assert.equal(cancels,1);assert.equal(final.settlement.outcome,"unknown");
});
test("saved output receipt completes crash recovery without provider resubmission or duplicate collection",async t=>{
  const {ledger}=await fixture(t);ledger.prepare(grant(),operation());let owned=ledger.claim("op","dead-process");ledger.admit(owned);owned=ledger.transition(owned,"submitting");ledger.transition(owned,"collecting");ledger.release("op");
  const adapter={id:"test",inspect:()=>capabilities};const engine=executor(ledger,adapter,{lookupOutputs:async()=>({assetRevisionIds:["original"],receipt:{operationId:"op"}}),collectOutputs:async()=>assert.fail("must not collect twice")});
  assert.deepEqual((await engine.step("op")).outputAssetRevisionIds,["original"]);
});
test("busy resources defer a prepared operation without request accounting, paid submission or failure",async t=>{
  const {dir,ledger,clock,setNow}=await fixture(t);ledger.prepare(grant(),operation());ledger.prepare(grant(),operation("waiting"));
  const other=new PersistentMediaLedger(join(dir,"budget.sqlite"),{clock});let occupying=other.claim("op","other-host");other.admit(occupying);occupying=other.transition(occupying,"submitting");other.transition(occupying,"outcome_unknown");other.release("op");
  let requests=0,submits=0,admissions=0;
  const adapter={id:"test",inspect:()=>capabilities,validate:async()=>{},submit:async()=>{submits++;return {state:"collecting",outputs:[]};},fetchOutputs:async result=>result.outputs};
  const engine=executor(ledger,adapter,{admitResource:async record=>{admissions++;const admitted=ledger.admit(record);if(!admitted.admitted)return admitted;requests++;return ()=>ledger.releaseAdmission(record);}});
  const waiting=await engine.step("waiting");assert.equal(waiting.state,"prepared");assert.equal(waiting.dispatchAttempted,false);assert.equal(waiting.nextPollAt,5100);assert.equal(requests,0);assert.equal(submits,0);
  await engine.step("waiting");assert.equal(admissions,1);
  setNow(6000);await engine.step("waiting");assert.equal(admissions,2);assert.equal(requests,0);
  await engine.cancel("waiting");assert.equal((await engine.step("waiting")).state,"cancelled");assert.equal(submits,0);assert.equal(requests,0);
});
test("remote admission survives owner release, unknown TTL and different work scopes",async t=>{
  const {dir,ledger,clock,setNow}=await fixture(t);const other=new PersistentMediaLedger(join(dir,"budget.sqlite"),{clock});
  for(const [id,realm] of [["a","realm-a"],["b","realm-b"],["c","realm-c"]]){const op=operation(id);op.scopeId=id;op.connection.credentialRealmId=realm;ledger.prepare({...grant(),id:`grant-${id}`,scopeId:id},op);}
  for(const [handle,id] of [[ledger,"a"],[other,"b"]]){let record=handle.claim(id,`host-${id}`,10);assert.equal(handle.admit(record).admitted,true);record=handle.transition(record,"submitting");record=handle.transition(record,"outcome_unknown");assert.deepEqual(handle.releaseAdmission(record),{released:false,heldForRemote:true});handle.release(id);}
  setNow(99999);const third=other.claim("c","host-c");assert.equal(other.admit(third).admitted,false);
  const recovered=ledger.claim("a","reconciler");assert.equal(ledger.admit(recovered).admitted,true);assert.equal(other.admit(third).admitted,false);
  ledger.transition(recovered,"cancelled");assert.equal(other.admit(third).admitted,true);other.release("c");ledger.release("a");
});
test("admission is held while downloading and collecting immutable outputs",async t=>{
  const {dir,ledger,clock}=await fixture(t);ledger.prepare(grant(),operation());ledger.prepare(grant(),operation("other"));
  const second=new PersistentMediaLedger(join(dir,"budget.sqlite"),{clock});const waiting=second.claim("other","second-host");const checkpoints=[];
  const assertHeld=phase=>{checkpoints.push(phase);assert.equal(second.admit(waiting).admitted,false);};
  const adapter={id:"test",inspect:()=>capabilities,validate:async()=>{},submit:async()=>({state:"collecting",outputs:[]}),fetchOutputs:async result=>{assertHeld("download");return result.outputs;}};
  const final=await executor(ledger,adapter,{collectOutputs:async()=>{assertHeld("collect");return {assetRevisionIds:["asset"],receipt:{operationId:"op"}};}}).step("op");
  assert.equal(final.state,"ready");assert.deepEqual(checkpoints,["download","collect"]);assert.equal(second.admit(waiting).admitted,true);second.release("other");
});
test("shutdown aborts download IO without cancelling the operation or resubmitting on recovery",async t=>{
  const {ledger,setNow}=await fixture(t);ledger.prepare(grant(),operation());const shutdown=new AbortController();let started,submits=0,collections=0;const downloading=new Promise(resolve=>started=resolve);
  const adapter={id:"test",inspect:()=>capabilities,validate:async()=>{},submit:async()=>{submits++;return {state:"collecting",jobId:"original-job"};},fetchOutputs:async(result,{signal})=>{assert.ok(signal);started();await new Promise((resolve,reject)=>signal.addEventListener("abort",()=>reject(signal.reason),{once:true}));},lookup:async()=>({state:"outcome_unknown"})};
  const engine=executor(ledger,adapter,{collectOutputs:async()=>{collections++;throw Error("must not collect aborted download");}});const running=engine.step("op",{signal:shutdown.signal});await downloading;shutdown.abort();
  const result=await running;assert.equal(result.state,"outcome_unknown");assert.equal(result.cancelRequested,false);assert.equal(collections,0);assert.equal(result.providerJobId,"original-job");setNow(6000);assert.equal((await engine.step("op")).state,"outcome_unknown");assert.equal(submits,1);
});
test("defer does not repeat a submission transition when shutdown interrupts recovery",async t=>{
  const {ledger}=await fixture(t);ledger.prepare(grant(),operation());let active=ledger.claim("op","previous-host");ledger.admit(active);ledger.transition(active,"submitting");ledger.release("op");
  const signal=AbortSignal.abort();const result=await executor(ledger,{id:"test",inspect:()=>capabilities}).step("op",{signal});assert.equal(result.state,"submitting");assert.equal(result.nextPollAt,5100);assert.equal(result.cancelRequested,false);
});
