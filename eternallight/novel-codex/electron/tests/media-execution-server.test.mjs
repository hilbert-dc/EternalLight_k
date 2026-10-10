import test from "node:test";
import assert from "node:assert/strict";
import {mkdtemp,rm} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import {digest,TaskBudget,PersistentMediaLedger} from "@eternallight/agent-engine";
import {createMediaExecutionServer} from "../media-execution-server.mjs";

const png=Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aL1sAAAAASUVORK5CYII=","base64");
async function setup(t,{protocol="openai-images",model="gpt-image-1.5",kind="image.generate",parameters={prompt:"test"},units={images:1},providerFetch,beforeResolve=async()=>{},inputAssetIds=[],loadAsset}={}){
  const directory=await mkdtemp(join(tmpdir(),"media-server-"));t.after(()=>rm(directory,{recursive:true,force:true}));const databasePath=join(directory,"budget.sqlite"),seals=[],order=[];
  const operation={id:"operation",projectId:"p",editionId:"edition",scopeId:"s",branchId:"b",workTaskId:"work",attemptId:"task",providerId:"service",kind,idempotencyKey:"intent",request:parameters,requestDigest:digest(parameters),reservation:{units,costMinor:null,currency:null},source:{assetRevisionIds:inputAssetIds},target:null};
  const task={id:"task",projectId:"p",workTaskId:"work"};let savedSeal=null;
  const intent={operation,task,grant:{id:"grant",projectId:"p",scopeId:"s",branchId:"b",workTaskId:"work",maxRequests:2,unitLimits:units,maxCostMinor:null,currency:null,expiresAt:Date.now()+100000},authorization:{canDispatch:true,canReconcile:true}};
  const runtimeRequest=async(path,options)=>{
    if(path==="/api/projects/p/media-operations/operation")return {...intent,state:savedSeal?.state??"prepared",seal:savedSeal};
    if(path.endsWith("/outputs")){order.push("outputs");return {};}
    if(path.endsWith("/imports"))throw Error("generation outputs must use their existing domain pins");
    if(path.endsWith("/assets")){order.push("asset");return {assetRevisionId:"asset-rev"};}
    if(path.endsWith("/transcript")){order.push("transcript");return {artifactRevisionId:"transcript-rev"};}
    if(path.endsWith("/seal")){const body=JSON.parse(options.body);if(body.state!=="ready")assert.equal(body.outputs,null,"terminal seals cannot contain outputs");seals.push(body);order.push("seal");savedSeal={operationId:operation.id,state:body.state,outputs:body.outputs,providerReceipt:body.providerReceipt??null,error:body.error??null};return savedSeal;}
    throw Error(`unexpected mock domain route ${path}`);
  };
  const taskBudget=new TaskBudget({maxRequests:2,maxTokens:10});
  const originalFetch=globalThis.fetch;let providerCalls=0;
  globalThis.fetch=async(url,options)=>{
    const address=String(url);
    if(address.startsWith("https://provider.invalid")){providerCalls++;order.push("provider");return providerFetch?providerFetch(url,options):Response.json({data:[{b64_json:png.toString("base64")}]});}
    if(new URL(address).hostname==="127.0.0.1")return originalFetch(url,options);
    throw Error("test forbids external network");
  };
  t.after(()=>{globalThis.fetch=originalFetch;});
  const multimediaHost={directory,connections:{resolve:async()=>{await beforeResolve();return {id:"service",protocol,baseUrl:"https://provider.invalid/v1",model,apiKey:"fixture-credential",credentialRealmId:"realm"};}},
    decodeImage:()=>({width:1,height:1}),executables:{ffprobePath:null},route:()=>"/api/projects/p/editions/edition",rememberAsset:asset=>asset,
    workspace:async()=>({}),assetBytes:loadAsset??(async()=>({name:"source.wav",mimeType:"audio/wav",bytes:Buffer.from("audio")}))};
  const server=await createMediaExecutionServer({runtimeRequest,multimediaHost,databasePath,getExecution:()=>({task,taskBudget})});let closed=false;
  const close=async()=>{if(!closed){closed=true;await server.close();}};t.after(close);
  const call=async(action,token=server.token)=>{const response=await originalFetch(server.url,{method:"POST",headers:{Authorization:`Bearer ${token}`,"Content-Type":"application/json"},body:JSON.stringify({action,projectId:"p",operationId:"operation"})});return {status:response.status,body:await response.json()};};
  return {server,call,close,seals,order,taskBudget,databasePath,providerCalls:()=>providerCalls};
}
test("execution server authenticates, submits once, and seals only durable generated assets",async t=>{
  const f=await setup(t);assert.equal((await f.call("prepare","wrong-token")).status,403);assert.equal(f.providerCalls(),0);
  const generated=await f.call("prepare");assert.equal(generated.status,200);assert.equal(generated.body.state,"ready");assert.deepEqual(generated.body.outputAssetRevisionIds,["asset-rev"]);assert.deepEqual(f.order,["provider","outputs","asset","seal"]);
  assert.equal((await f.call("prepare")).body.state,"ready");assert.equal(f.providerCalls(),1);assert.equal(f.taskBudget.snapshot().requests,1);assert.ok(!JSON.stringify(generated.body).includes("fixture-credential"));
});
test("an uncertain provider response keeps its reservation and cannot seal a fabricated result",async t=>{
  const f=await setup(t,{providerFetch:async()=>{throw Error("private provider failure");}});const result=await f.call("prepare");assert.equal(result.status,200);assert.equal(result.body.state,"outcome_unknown");assert.equal(f.seals.length,0);assert.equal(f.taskBudget.snapshot().requests,1);
  await f.call("read");assert.equal(f.providerCalls(),1);assert.equal(f.seals.length,0);
});
test("closing the control server preserves submitted remote jobs without claiming cancellation",async t=>{
  const f=await setup(t,{protocol:"runway",model:"gen4.5",kind:"video.generate",parameters:{prompt:"scene",duration:2,ratio:"1280:720"},units:{videoMilliseconds:2000},providerFetch:async()=>Response.json({id:"provider-job"})});
  assert.equal((await f.call("prepare")).body.state,"submitted");await f.close();const ledger=new PersistentMediaLedger(f.databasePath);const record=ledger.find("operation");assert.equal(record.state,"submitted");assert.equal(record.cancelRequested,false);assert.equal(record.providerJobId,"provider-job");assert.equal(f.providerCalls(),1);assert.equal(f.seals.length,0);
});
test("transcription uses the formal nested artifact seal and does not invent a media asset",async t=>{
  const f=await setup(t,{protocol:"openai-audio",model:"gpt-4o-transcribe",kind:"audio.transcribe",parameters:{audioAssetRevisionId:"audio"},inputAssetIds:["audio"],units:{audioMilliseconds:1000},providerFetch:async()=>Response.json({text:"spoken words"})});
  const result=await f.call("prepare");assert.equal(result.status,200);assert.equal(result.body.state,"ready");assert.deepEqual(result.body.outputAssetRevisionIds,[]);assert.equal(result.body.receipt.outputs.artifactRevisionId,"transcript-rev");assert.ok(!f.order.includes("asset"));
});
test("a fixed Domain input authorizes an exact borrowed Edition asset, never an unrelated project asset",async t=>{
  let reads=0;
  const f=await setup(t,{kind:"image.edit",parameters:{prompt:"edit borrowed image",imageAssetRevisionIds:["borrowed-image"]},inputAssetIds:["borrowed-image"],
    loadAsset:async(...args)=>{reads++;assert.deepEqual(args,["borrowed-image","p"]);return {name:"borrowed.png",mimeType:"image/png",bytes:png};}});
  assert.equal((await f.call("prepare")).body.state,"ready");assert.equal(reads,1);assert.equal(f.providerCalls(),1);
});
test("a requested asset absent from the persisted operation pin is refused before file or provider access",async t=>{
  let reads=0;
  const f=await setup(t,{kind:"image.edit",parameters:{prompt:"invalid borrowed image",imageAssetRevisionIds:["unrelated-image"]},inputAssetIds:["authorized-image"],
    loadAsset:async()=>{reads++;throw Error("must not read");}});
  await f.call("prepare");assert.equal(reads,0);assert.equal(f.providerCalls(),0);assert.equal(f.seals.some(seal=>seal.state==="ready"),false);
});
test("shared native capacity blocks dispatch before overall request accounting and remains cancellable",async t=>{
  const f=await setup(t);const ledger=new PersistentMediaLedger(f.databasePath),request={prompt:"another work"};
  const grant={id:"other-grant",projectId:"other-project",scopeId:"other-scope",branchId:"b",workTaskId:"other-work",maxRequests:1,unitLimits:{images:1},maxCostMinor:null,currency:null,expiresAt:Date.now()+100000};
  ledger.prepare(grant,{id:"occupying",projectId:grant.projectId,scopeId:grant.scopeId,branchId:"b",workTaskId:grant.workTaskId,attemptId:"other-task",providerId:"other-connection",kind:"image.generate",idempotencyKey:"occupied",request,requestDigest:digest(request),reservation:{units:{images:1},costMinor:null,currency:null},connection:{baseUrl:"https://provider.invalid/v1",credentialRealmId:"realm"}});
  let owner=ledger.claim("occupying","other-host");ledger.admit(owner);owner=ledger.transition(owner,"submitting");ledger.transition(owner,"outcome_unknown");ledger.release("occupying");
  const result=await f.call("prepare");assert.equal(result.status,200);assert.equal(result.body.state,"prepared");assert.ok(result.body.nextPollAt>Date.now());assert.equal(f.providerCalls(),0);assert.equal(f.taskBudget.snapshot().requests,0);assert.equal(f.seals.length,0);
  const cancelled=await f.call("cancel");assert.equal(cancelled.body.state,"cancelled");assert.equal(f.providerCalls(),0);assert.equal(f.taskBudget.snapshot().requests,0);assert.equal(ledger.get("occupying").state,"outcome_unknown");
});
test("server shutdown interrupts live provider IO but preserves unknown outcome and request accounting",async t=>{
  let started;const submitting=new Promise(resolve=>started=resolve);
  const f=await setup(t,{providerFetch:async(_url,{signal})=>{started();return new Promise((resolve,reject)=>signal.addEventListener("abort",()=>reject(signal.reason),{once:true}));}});
  const pending=f.call("prepare");await submitting;const closed=f.close();const result=await pending;await closed;
  assert.equal(result.body.state,"outcome_unknown");const record=new PersistentMediaLedger(f.databasePath).get("operation");assert.equal(record.cancelRequested,false);assert.equal(f.taskBudget.snapshot().requests,1);assert.equal(f.providerCalls(),1);assert.equal(f.seals.length,0);
});
test("cancel received during credential resolution persists before any submission or budget request",{timeout:5000},async t=>{
  let entered,release,seen;const resolving=new Promise(resolve=>entered=resolve),gate=new Promise(resolve=>release=resolve),cancelSeen=new Promise(resolve=>seen=resolve);
  const f=await setup(t,{beforeResolve:async()=>{entered();await gate;}});t.after(()=>release());
  const ledger=f.server.service.ledger,find=ledger.find.bind(ledger);let misses=0;ledger.find=id=>{const record=find(id);if(!record&&++misses===2)seen();return record;};
  const prepare=f.call("prepare");await resolving;const cancel=f.call("cancel");await cancelSeen;release();const [created,cancelled]=await Promise.all([prepare,cancel]);
  assert.equal(created.body.state,"cancelled");assert.equal(cancelled.body.state,"cancelled");assert.equal(f.providerCalls(),0);assert.equal(f.taskBudget.snapshot().requests,0);assert.equal(ledger.get("operation").cancelRequested,true);assert.equal(f.seals[0].state,"cancelled");
});
