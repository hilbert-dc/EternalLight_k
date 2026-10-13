import {test} from 'node:test';import assert from 'node:assert/strict';
import {verifyHttpContracts} from '../verify-runtime-http-contracts.mjs';
function fixture(){return{source:'interface Api {runtime: {read: () => Promise<import("../shared/generated/responses/Read").Read>}}',inventory:{methods:[{method:'read',kind:'http',response:'Read',routes:[{verb:'GET',path:'/api/projects/{id}'}]}]},validator:'responses!(Read);',generatedNames:new Set(['Read'])};}
test('every API method has a generated, production-checked wire response',()=>{assert.equal(verifyHttpContracts(fixture()),1);});
test('new API methods cannot evade inventory',()=>{const f=fixture();f.source=f.source.replace('read:','added: () => Promise<unknown>; read:');assert.throws(()=>verifyHttpContracts(f),/no wire ownership/);});
test('handwritten or unknown returns cannot masquerade as coverage',()=>{const f=fixture();f.source=f.source.replace('import("../shared/generated/responses/Read").Read','unknown');assert.throws(()=>verifyHttpContracts(f),/does not consume/);});
test('missing generation and missing production validation fail closed',()=>{const f=fixture();f.generatedNames.clear();assert.throws(()=>verifyHttpContracts(f),/Unexported/);f.generatedNames.add('Read');f.validator='';assert.throws(()=>verifyHttpContracts(f),/No production validator/);});
test('a manifest cannot invent an unregistered Runtime route',()=>{const f=fixture();f.routeSources='.route("/api/other", get(read))';assert.throws(()=>verifyHttpContracts(f),/No Runtime route/);f.routeSources='.route("/api/projects/{project_id}", get(read))';assert.equal(verifyHttpContracts(f),1);});
test('media method signatures also require their generated response',()=>{const f=fixture();f.api='MultimediaApi';f.source='interface MultimediaApi {read(): Promise<import("./generated/responses/Read").Read>}';assert.equal(verifyHttpContracts(f),1);});
test('new untyped content leaves cannot silently widen a stable response',async()=>{
 const {verifyContentFields}=await import('../verify-runtime-http-contracts.mjs');
 const source='pub struct Receipt {\n pub identity: serde_json::Value,\n}';
 assert.throws(()=>verifyContentFields([source],{fields:[]}),/Unreviewed dynamic/);
 assert.equal(verifyContentFields([source],{fields:[{type:'Receipt',field:'identity',reason:'test-only reviewed fixture'}]}),1);
});
