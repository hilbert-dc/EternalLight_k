import assert from "node:assert/strict";
import test from "node:test";
import { ModelBridge, TaskBudget, createExecutionLease, defineConnection, resolveSessionPlan } from "../src/index.mjs";
import { verifiedBaselineEvidence } from "../src/testing/index.mjs";

test("Engine authenticates MCP catalogs and binds calls before Core dispatch",async t=>{
  const connection=defineConnection({id:"fixture",provider:"fixture",model:"fixture",endpoint:"https://fixture.invalid",protocol:"responses",credentialRef:"FIXTURE",declaredLimits:{contextTokens:64000,outputTokens:4000}});
  const plan=resolveSessionPlan({connections:[connection],evidence:verifiedBaselineEvidence(connection)});
  const schema={type:"object",properties:{text:{type:"string",minLength:1}},required:["text"],additionalProperties:false};
  let sent;
  const bridge=new ModelBridge({sessionPlan:plan,connections:[connection],mcpAuthority:true,credentialResolver:async()=>"fixture",
    requestConfigurationProvider:async({threadId})=>({mcpScope:"scope",mcpIdentity:{threadId,agentRole:"candidate_writer"}}),
    fetchImpl:async(_url,options)=>{sent=JSON.parse(options.body);return new Response(JSON.stringify({id:"response",status:"completed",output:[1,2].map(i=>({type:"function_call",call_id:`call${i}`,name:"save",namespace:"mcp__workspace",arguments:JSON.stringify({text:"正文"})})),usage:{input_tokens:10,output_tokens:10,total_tokens:20}}),{headers:{"content-type":"application/json"}});}});
  t.after(()=>bridge.close());const address=await bridge.start();
  const post=async(path,body,headers={})=>fetch(`${address.baseUrl}${path}`,{method:"POST",headers:{"content-type":"application/json",authorization:`Bearer ${address.token}`,...headers},body:JSON.stringify(body)});
  const catalog={scope:"scope",namespace:"mcp__workspace",tools:[{name:"save",inputSchema:schema}]};
  assert.equal((await post("/_engine/mcp/catalog",catalog,{authorization:"Bearer wrong"})).status,401);
  const receipt=await (await post("/_engine/mcp/catalog",catalog)).json();assert.ok(receipt.catalogDigest);
  const budget=new TaskBudget();bridge.beginTaskBudget(budget);const lease=createExecutionLease({sessionPlan:plan,runId:"run",budgetId:budget.id});bridge.beginExecutionLease(lease);
  const rootResponse=await post("/responses",{model:"fixture",input:[],stream:false,tools:[{type:"namespace",name:"mcp__workspace",tools:[{type:"function",name:"save",parameters:{}}]}]},{"x-codex-turn-metadata":JSON.stringify({thread_id:"root",session_id:"root",turn_id:"root-turn"})});
  assert.equal(rootResponse.status,200,await rootResponse.text());
  const response=await post("/responses",{model:"fixture",input:[],stream:false,tools:[{type:"namespace",name:"mcp__workspace",tools:[{type:"function",name:"save",parameters:{}}]}]},{"x-codex-turn-metadata":JSON.stringify({thread_id:"child",parent_thread_id:"root",session_id:"root",turn_id:"turn"})});
  assert.equal(response.status,200,await response.text());assert.deepEqual(sent.tools[0].tools[0].parameters,schema);
  const invocation={scope:"scope",namespace:"mcp__workspace",name:"save",catalogDigest:receipt.catalogDigest,arguments:{text:"正文"},meta:{threadId:"child",sessionId:"root",callId:"call1",agentRole:"forged"}};
  const first=await (await post("/_engine/mcp/call",invocation)).json();assert.equal(first.agentRole,"candidate_writer");assert.equal(first.rootThreadId,"root");assert.equal(first.parentThreadId,"root");assert.ok(first.samplingRequestId);
  invocation.meta.callId="call2";const second=await (await post("/_engine/mcp/call",invocation)).json();assert.equal(first.samplingRequestId,second.samplingRequestId);
  assert.equal((await post("/_engine/mcp/call",{...invocation,arguments:{text:"篡改"}})).status,403);
  await bridge.endExecutionLease(lease);
  assert.equal((await post("/_engine/mcp/call",invocation)).status,403);
});
