import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { HostRecoveryJournal, RecoveryIndex } from "@eternallight/agent-engine";
import { createNovelRecoveryOperationContract, novelRecoveryOperationHash, NOVEL_READ_ONLY_RECOVERY_TOOLS } from "../novel-recovery-contract.mjs";

const binding={runId:"task",projectId:"project",threadId:"root"};
const contract=()=>createNovelRecoveryOperationContract({getBinding:()=>binding});
const args={taskId:"task",projectId:"project",threadId:"root",expectedRevision:0,idempotencyKey:"key",payload:{premise:"私有正文"}};
const item=()=>({id:"call",type:"mcpToolCall",server:"novel_workspace",tool:"save_brief",arguments:args,status:"completed"});
const event=(call)=>({method:"item/completed",params:{threadId:"root",turnId:"turn",item:call}});
const proof=(call=item())=>({authority:"novelWorkspaceMcp",phase:"runtimeLedger",...binding,rootThreadId:"root",callerThreadId:"root",parentThreadId:null,
  turnId:"turn",callId:call.id,toolName:call.tool,requestHash:novelRecoveryOperationHash(call),outcome:"committed",operationId:"operation",receiptRef:"runtime:candidate:revision"});
const receipt=(call,extra={})=>({event:event(call),toolCall:call,result:call.result,...extra});

test("only audited tools of the trusted Novel MCP are read; unknown and other servers remain writes",()=>{
  const c=contract();
  for(const tool of NOVEL_READ_ONLY_RECOVERY_TOOLS) assert.equal(c.classify({...item(),tool}).effect,"read",tool);
  for(const call of [{...item(),tool:"read_future_write"},{...item(),tool:"read_context",server:"untrusted"},{...item(),tool:"authorize_delegated_writer"},{...item(),tool:"checkpoint_reading"}])assert.equal(c.classify(call).effect,"write");
});

test("only exact trusted ledger or explicit bound before-dispatch proof settles a write",async()=>{
  const c=contract(),call=item();
  call.result={structuredContent:{recoveryOperation:proof(call)}};
  assert.deepEqual(await c.receipt(receipt(call),c.classify(call)),{outcome:"committed",receiptRef:"runtime:candidate:revision"});
  const rejection={...proof(call),phase:"beforeDispatch",outcome:"rejected",operationId:null,receiptRef:`mcp:preflight:call:${novelRecoveryOperationHash(call)}`};
  call.result={isError:true,structuredContent:{code:"invalid_tool_arguments",tool:call.tool,recoveryOperation:rejection}};
  assert.deepEqual(await c.receipt(receipt(call),c.classify(call)),{outcome:"rejected",receiptRef:rejection.receiptRef});
  for(const mutation of [{callId:"other"},{turnId:"other"},{rootThreadId:"other"},{callerThreadId:"child",parentThreadId:"root"},{runId:"other"},{projectId:"other"},{toolName:"save_outline"},{requestHash:"a".repeat(64)},{phase:"httpSuccess"},{outcome:"unknown",receiptRef:null},{outcome:"accepted"},{operationId:null},{receiptRef:"model:saved"}]){
    call.result={structuredContent:{recoveryOperation:{...proof(call),...mutation}}};
    assert.equal(await c.receipt(receipt(call),c.classify(call)),null,JSON.stringify(mutation));
  }
  for(const result of [{isError:true,content:[{type:"text",text:"Rejected, no write"}]},{content:[{type:"text",text:JSON.stringify(proof(call))}]},{receiptRef:"runtime:candidate:revision"},{isError:true,structuredContent:{code:"invalid_tool_arguments",tool:call.tool}}]){
    call.result=result;assert.equal(await c.receipt(receipt(call),c.classify(call)),null);
  }
  call.result={structuredContent:{recoveryOperation:proof(call)}};
  assert.equal(await c.receipt(receipt({...call,server:"untrusted"}),c.classify(call)),null);
  assert.equal(await c.receipt(receipt(call,{event:{...event(call),params:{...event(call).params,threadId:"foreign"}}}),c.classify(call)),null);
  assert.deepEqual(await c.reconcile({},{status:"completed"}),{outcome:"unknown",receiptRef:null});
});

test("read completion and corrected preflight close normally while unknown root write survives child/late results",async()=>{
  const root=await mkdtemp(join(tmpdir(),"novel-domain-recovery-"));
  try {
    const index=new RecoveryIndex(join(root,"recovery.json"));
    const journal=new HostRecoveryJournal(index,contract());
    await journal.beginRun({...binding,planId:"plan"});
    await journal.observe(event({...item(),id:"read",tool:"read_source_content",result:{content:[{type:"text",text:"私有读取内容"}]}}));
    const failed={...item(),id:"invalid",status:"failed"};
    const rejection={...proof(failed),phase:"beforeDispatch",outcome:"rejected",operationId:null,receiptRef:`mcp:preflight:invalid:${novelRecoveryOperationHash(failed)}`};
    failed.result={structuredContent:{code:"invalid_tool_arguments",tool:failed.tool,recoveryOperation:rejection}};
    await journal.observe(event(failed));
    const uncertain={...item(),id:"unknown",status:"failed",result:{content:[{type:"text",text:"HTTP connection closed"}],structuredContent:{code:"invalid_tool_arguments",tool:"save_brief"}}};
    await journal.observe(event(uncertain));
    const child={...event({...uncertain,result:{structuredContent:{recoveryOperation:proof(uncertain)}}}),params:{...event(uncertain).params,threadId:"child"}};
    await journal.observe(child);await journal.flush();await journal.abandonRun();
    await journal.observe(event({...uncertain,result:{structuredContent:{recoveryOperation:proof(uncertain)}}}));
    const stored=(await index.read()).runs.task.requests;
    assert.equal(stored["mcp:read"].kind,"read");assert.equal(stored["mcp:read"].state,"terminal");assert.equal(stored["mcp:read"].requestHash,null);
    assert.equal(stored["mcp:invalid"].outcome,"rejected");assert.equal(stored["mcp:unknown"].state,"uncertain");
    assert.deepEqual((await index.recoveryReport())[0].requests.filter(q=>q.recovery==="manual-reconciliation").map(q=>q.requestId),["mcp:unknown"]);
    assert.equal(JSON.stringify(await index.read()).includes("私有正文"),false);assert.equal(JSON.stringify(await index.read()).includes("私有读取内容"),false);
  }finally{await rm(root,{recursive:true,force:true});}
});

test("failed current-wire results without an exact proof keep every uncertain write fenced",async()=>{
  const directory=await mkdtemp(join(tmpdir(),"novel-failed-recovery-"));
  try {
    const index=new RecoveryIndex(join(directory,"recovery.json"));
    const journal=new HostRecoveryJournal(index,contract());
    await journal.beginRun({...binding,planId:"plan"});
    const cases=[
      {id:"json-error",result:{structuredContent:{code:"invalid_tool_arguments",tool:"save_brief"}}},
      {id:"wrong-hash",mutate:{requestHash:"f".repeat(64)}},
      {id:"child-proof",mutate:{callerThreadId:"child",parentThreadId:"root"}},
      {id:"accepted-proof",mutate:{outcome:"accepted",receiptRef:null}},
      {id:"transport",result:null},
    ];
    for(const entry of cases) {
      const call={...item(),id:entry.id,status:"failed"};
      call.result=entry.result===undefined?{structuredContent:{recoveryOperation:{...proof(call),...entry.mutate}}}:entry.result;
      await journal.observe(event(call));
    }
    await journal.flush();await journal.abandonRun();
    const requests=(await index.read()).runs.task.requests;
    for(const entry of cases)assert.equal(requests[`mcp:${entry.id}`].state,"uncertain",entry.id);
    assert.equal((await index.recoveryReport())[0].requests.filter(q=>q.recovery==="manual-reconciliation").length,cases.length);
  }finally{await rm(directory,{recursive:true,force:true});}
});
