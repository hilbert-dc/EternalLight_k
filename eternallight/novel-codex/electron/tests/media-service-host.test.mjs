import test from "node:test";
import assert from "node:assert/strict";
import {mkdtemp,rm} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import {MediaServiceHost} from "../media-service-host.mjs";

async function setup(t,overrides={}){
  const directory=await mkdtemp(join(tmpdir(),"media-host-"));t.after(()=>rm(directory,{recursive:true,force:true}));let dispatches=0;const order=[];const saved=new Map();
  const connection={id:"images",protocol:"openai-images",baseUrl:"https://api.openai.com/v1",model:"gpt-image-1.5",apiKey:"mock",credentialRealmId:"fixture-realm"};
  const host=new MediaServiceHost({databasePath:join(directory,"engine.sqlite"),clock:()=>100,connections:{resolve:async()=>connection},
    registerIntent:async()=>{order.push("pin");},authorizeOperation:async()=>{},loadAsset:async()=>{},importOutput:async(op,file)=>{order.push(`asset-${file.outputIndex}`);return {assetRevisionId:`asset-${file.outputIndex}`};},sealOutputs:async(op,collection)=>{const receipt={operationId:op.id,...collection};saved.set(op.id,{assetRevisionIds:collection.assetRevisionIds,receipt});return receipt;},lookupOutputs:async op=>saved.get(op.id)??null,admitResource:async record=>{const admission=host.ledger.admit(record);return admission.admitted?()=>host.ledger.releaseAdmission(record):admission;},
    fetchImpl:async()=>{dispatches++;order.push("dispatch");return Response.json({data:[{b64_json:Buffer.from("generated").toString("base64")}]});},...overrides});
  const input={task:{id:"task",projectId:"p",workTaskId:"work"},editionId:"edition",grant:{id:"grant",projectId:"p",scopeId:"s",branchId:"b",workTaskId:"work",maxRequests:1,unitLimits:{images:1},maxCostMinor:null,currency:null,expiresAt:100000},request:{providerId:"images",kind:"image.generate",parameters:{prompt:"create a panel"},idempotencyKey:"intent-1"},reservation:{units:{images:1},costMinor:null,currency:null},source:{revisionId:"source"},target:{panelIds:["panel"]}};
  return {host,input,order,dispatches:()=>dispatches,connection};
}
test("host pins original domain intent before persistent reservation and service dispatch",async t=>{
  const f=await setup(t);const prepared=await f.host.prepare(f.input);assert.equal(prepared.state,"prepared");assert.deepEqual(f.order,["pin"]);
  const result=await f.host.step(prepared.id);assert.equal(result.state,"ready");assert.deepEqual(f.order,["pin","dispatch","asset-0"]);assert.deepEqual(result.outputAssetRevisionIds,["asset-0"]);
  assert.equal((await f.host.prepare(f.input)).id,result.id);assert.equal((await f.host.step(result.id)).state,"ready");assert.equal(f.dispatches(),1);
});
test("failed domain pin prevents dispatch, changed provider configuration blocks original job reconciliation",async t=>{
  let pinFails=true;const f=await setup(t,{registerIntent:async()=>{if(pinFails)throw Error("domain unavailable");}});
  await assert.rejects(f.host.prepare(f.input),/domain unavailable/);assert.equal(f.host.ledger.list().length,0);assert.equal(f.dispatches(),0);
  pinFails=false;const prepared=await f.host.prepare(f.input);f.connection.baseUrl="https://another.example/v1";await assert.rejects(f.host.step(prepared.id),/原媒体服务配置已变化/);assert.equal(f.dispatches(),0);
});
test("prepared cancellation needs no configured provider and never applies to a document",async t=>{
  const f=await setup(t);const prepared=await f.host.prepare(f.input);await f.host.cancel(prepared.id);f.host.connections.resolve=async()=>{throw Error("connection removed");};
  const result=await f.host.step(prepared.id);assert.equal(result.state,"cancelled");assert.equal(result.settlement.outcome,"not_sent");assert.equal(f.dispatches(),0);assert.deepEqual(f.order,["pin"]);
});
test("changing credential account cannot redirect a previously pinned operation",async t=>{
  const f=await setup(t);const prepared=await f.host.prepare(f.input);f.connection.credentialRealmId="another-account";
  await assert.rejects(f.host.step(prepared.id),/原媒体服务配置已变化/);assert.equal(f.dispatches(),0);assert.equal(prepared.connection.credentialRealmId,"fixture-realm");assert.ok(!JSON.stringify(prepared).includes('"apiKey"'));
});
