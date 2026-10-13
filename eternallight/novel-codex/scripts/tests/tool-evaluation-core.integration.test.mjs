import test from "node:test";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { defineConnection, resolveSessionPlan } from "@eternallight/agent-engine";
import { verifiedBaselineEvidence } from "@eternallight/agent-engine/test-support";
import { resolveCodexCli, NOVEL_TOOL_DISCOVERY_POLICY } from "../../electron/codex-runtime.mjs";
import { evaluateLiveNovelTools } from "../evaluate-live-novel-tools.mjs";

// Only the provider is scripted. Core, budget, MCP, Runtime and saved content are real.
function resultFrom(output) {
  const queue=[output];
  for(let n=0;queue.length&&n<10000;n++) {
    const value=queue.shift();
    if(typeof value==="string"){const marker="\nOutput:\n";const payload=value.includes(marker)?value.slice(value.indexOf(marker)+marker.length):value;try{queue.push(JSON.parse(payload));}catch{}}
    else if(value&&typeof value==="object") {
      if(!Array.isArray(value)&&(value.taskSources||value.toolDiscovery||Array.isArray(value.entries)||value.saved||value.recoveryOperation||value.revisionId||value.status==="completed"||value.error))return value;
      queue.push(...Object.values(value));
    }
  }
  throw Error("Missing fixture tool result: "+JSON.stringify(output).slice(0,800));
}
test("evaluation driver uses real Core and durable budgets for both save modes",{timeout:180000},async()=>{
 const projectRoot=resolve(import.meta.dirname,"../..");
 const baseInstructions=(await readFile(join(projectRoot,"codex-runtime/engine-prompt.md"),"utf8")).trim();
 const cli=resolveCodexCli({projectRoot,resourcesPath:projectRoot,isPackaged:false,explicitPath:process.env.NOVEL_CODEX_CLI});
 assert.ok(cli.path,cli.reason);
 const dir=await mkdtemp(join(tmpdir(),"live-eval-core-"));let failure,skipFinalReading=false;
 const server=createServer(async(req,res)=>{
  try {
   const chunks=[];for await(const chunk of req)chunks.push(chunk);const input=JSON.parse(Buffer.concat(chunks).toString("utf8"));
   const outputs=input.input.filter(i=>i.type==="function_call_output").map(i=>resultFrom(i.output));
   const toolCalls=input.input.filter(i=>i.type==="function_call");
   const tools=input.tools.flatMap(tool=>tool.tools?.map(t=>({...t,namespace:tool.name}))??[tool]);
   const call=(name,args)=>{const tool=tools.find(t=>t.name===name||t.name.endsWith(`__${name}`));assert.ok(tool,`missing ${name}`);return {type:"function_call",id:`id-${outputs.length}`,call_id:`call-${outputs.length}`,name:tool.name,...(tool.namespace?{namespace:tool.namespace}:{}),arguments:JSON.stringify(args),status:"completed"};};
   const context=outputs.find(v=>v.taskSources),last=outputs.at(-1);
   assert.ok(!last?.error,JSON.stringify(last?.error));
   if(!outputs.length) {
    const text=JSON.stringify(input);
    assert.ok(JSON.stringify(input).includes(JSON.stringify(baseInstructions).slice(1,-1)),"product base instructions reach the real Core provider request intact");
    assert.ok(text.includes("mandatory execution contract"),"frozen method reaches first provider request");
    assert.ok(text.includes("requiredOutputKind"),"task binding reaches first provider request");
    assert.ok(!/tokens truncated/.test(text),"no application instructions may be silently truncated");
   }
   let item;
   if(!context)item=call("read_context",{expectedRevision:0});
   else if(skipFinalReading&&!toolCalls.some(t=>t.name.endsWith("discover_creative_capabilities")))item=call("discover_creative_capabilities",{query:"query_story"});
   else if(!outputs.some(o=>o.entries))item=call("read_source_content",{expectedRevision:0,sourceId:context.taskSources.entries.find(e=>e.sectionPath==="/relevantText").sourceId});
   else if(last?.nextRead)item=call("read_source_content",last.nextRead.arguments);
   else if(last?.saved===true) {
    assert.equal(last.savedSource.status,"available");
    assert.equal(last.savedSource.revisionId,last.revisionId);
    assert.equal(last.savedSource.contentRead,false);
    assert.equal(last.savedSource.read.tool,"read_source_content");
    item=skipFinalReading?call("complete_task",{summary:"已保存精确改字候选。"}):call("read_source_content",last.savedSource.read.arguments);
   }
   else if(!toolCalls.some(t=>t.name.endsWith("save_draft_checkpoint"))) {
    const entries=outputs.flatMap(v=>v.entries??[]),anchor=entries.find(e=>e.editAnchor?.groupId==="selected").editAnchor;
    const parts=entries.filter(e=>e.editAnchor?.groupId==="selected");
    let offset=0;
    for(const part of parts) {
      assert.equal(part.characterRange.start,offset,"body pages must be contiguous");
      assert.equal(Array.from(part.text??part.value).length,part.characterRange.end-offset,"body page must not be truncated");
      offset=part.characterRange.end;
    }
    assert.equal(offset,parts[0].characterRange.total,"full original body must be returned");
    const body=parts.map(e=>e.text??e.value).join("");
    const rules=input.input.flatMap(i=>i.content??[]).map(c=>c.text?.match(/<novelRules(?:_part_\d+)?>([\s\S]*)<\/novelRules(?:_part_\d+)?>/)?.[1]??"").join("");
    const patch=rules.includes("save_draft_checkpoint.patch 局部修改");
    assert.ok(JSON.stringify(input).includes("mandatory execution contract"),"frozen method must reach the actual provider");
    assert.ok(JSON.stringify(input).includes("requiredOutputKind"),"task binding must reach the actual provider");
    const args={expectedRevision:0,expectedCandidateRevisionId:anchor.sourceRevisionId,idempotencyKey:"evaluation-save"};
    if(patch)args.patch={chapterId:anchor.chapterId,sourceRevisionId:anchor.sourceRevisionId,edits:[{groupId:"selected",start:0,end:1,oldText:"风",newText:"雨"}]};
    else args.payload={chapterId:anchor.chapterId,chapterNumber:1,title:"月下",completeness:"chapterComplete",sceneGroups:[{id:"selected",sceneId:null,body:"雨"+body.slice(1),locked:false},{id:"locked",sceneId:null,body:"锁定结尾。",locked:true}]};
    item=call("save_draft_checkpoint",args);
   }else if(!toolCalls.some(t=>t.name.endsWith("complete_task")))item=call("complete_task",{expectedRevision:0,summary:"已保存精确改字候选。"});
   else item={type:"message",id:"final",role:"assistant",status:"completed",content:[{type:"output_text",text:"候选已保存。",annotations:[]}]};
   const response={id:`response-${outputs.length}`,object:"response",status:"completed",model:input.model,output:[item],usage:{input_tokens:100,output_tokens:30,total_tokens:130}};
   const events=[{type:"response.created",response:{...response,status:"in_progress",output:[]}},{type:"response.output_item.added",output_index:0,item},{type:"response.output_item.done",output_index:0,item},{type:"response.completed",response}];
   res.writeHead(200,{"content-type":"text/event-stream"});res.end(events.map(e=>`data: ${JSON.stringify(e)}\n\n`).join(""));
  }catch(error){failure=error;res.writeHead(400);res.end("fixture failed");}
 });
 await new Promise(r=>server.listen(0,"127.0.0.1",r));
 try {
  const connection=defineConnection({id:"fixture",provider:"fixture",model:"fixture",protocol:"responses",endpoint:`http://127.0.0.1:${server.address().port}`,credentialRef:"FIXTURE",declaredLimits:{contextTokens:131072,outputTokens:8192}});
  const sessionPlan=resolveSessionPlan({connections:[connection],evidence:verifiedBaselineEvidence(connection),policy:{allowedMcpServerIds:["novel_workspace"],toolDiscovery:NOVEL_TOOL_DISCOVERY_POLICY}});
  const output=join(dir,"report.json");
  const report=await evaluateLiveNovelTools({provider:"fixture",model:"fixture",evidencePath:"fixture",credentialSaltPath:"fixture",readingPolicy:"completeOriginalAndFinal",maxRequests:20,maxTokens:200000,timeoutSeconds:30,repeats:1},output,{prepareConnection:async()=>({environment:{PATH:process.env.PATH,FIXTURE:"fixture"},cli,baseInstructions,qualificationContextDigest:"controlled-fixture",provider:{...connection,sessionPlan,connections:[connection]}})});
  assert.ifError(failure);assert.equal(report.evidence,"controlledProviderToolComparison");
  assert.equal(report.status,"completed",JSON.stringify(report));assert.equal(report.cases.length,4);
  assert.ok(report.cases.every(c=>c.contentPreserved&&c.usage.tokens>0&&c.usage.unknownTokens===0));
  assert.ok(report.cases.every(c=>c.reading.honored));
  assert.equal(report.buildStable,true);
  assert.equal(report.comparison.eligiblePairs,2);
  assert.equal(report.providerUsage.requests,report.cases.reduce((n,c)=>n+c.usage.requests,0));
  assert.equal(report.providerUsage.tokens,report.cases.reduce((n,c)=>n+c.usage.tokens,0));
  assert.deepEqual(JSON.parse(await readFile(output,"utf8")),report);
  console.log(JSON.stringify({evidence:report.evidence,cases:report.cases.map(c=>({size:c.size,mode:c.mode,status:c.status,contentPreserved:c.contentPreserved,reads:c.readSummary?.successfulReads,requests:c.usage.requests}))}));
  const failedOutput=join(dir,"after-save-failure.json");
  const failed=await evaluateLiveNovelTools({provider:"fixture",model:"fixture",evidencePath:"fixture",credentialSaltPath:"fixture",maxRequests:3,maxTokens:200000,timeoutSeconds:30,repeats:1,caseSelection:[{size:"short",mode:"full"}]},failedOutput,{prepareConnection:async()=>({environment:{PATH:process.env.PATH,FIXTURE:"fixture"},cli,baseInstructions,qualificationContextDigest:"controlled-fixture",provider:{...connection,sessionPlan,connections:[connection]}})});
  assert.ifError(failure);
  assert.equal(failed.status,"failed",JSON.stringify(failed));
  const row=failed.cases[0];
  assert.equal(row.executionStatus,"failed");
  assert.equal(row.firstSaveAccepted,true);
  assert.equal(row.firstAttemptSucceeded,false);
  assert.equal(row.contentPreserved,true,JSON.stringify(row));
  assert.equal(row.quiescent,true);
  assert.equal(row.readback.status,"matched");
  assert.notEqual(row.domainTaskStatus,"completed");
  assert.equal(row.usage.requests,3);assert.ok(row.usage.tokens>0);
  assert.ok(row.tools.some(t=>t.save?.saved));
  assert.equal(failed.summaries[0].passed,0);
  assert.equal(failed.summaries[0].contentPreservationRate,1);
  assert.deepEqual(JSON.parse(await readFile(failedOutput,"utf8")),failed);

  skipFinalReading=true;
  const unread=await evaluateLiveNovelTools({provider:"fixture",model:"fixture",evidencePath:"fixture",credentialSaltPath:"fixture",readingPolicy:"completeOriginalAndFinal",maxRequests:10,maxTokens:200000,timeoutSeconds:30,repeats:1,caseSelection:[{size:"short",mode:"full"}]},join(dir,"unread-final.json"),{prepareConnection:async()=>({environment:{PATH:process.env.PATH,FIXTURE:"fixture"},cli,baseInstructions,qualificationContextDigest:"controlled-fixture",provider:{...connection,sessionPlan,connections:[connection]}})});
  assert.ifError(failure);
  assert.equal(unread.status,"failed");
  assert.equal(unread.cases[0].contentPreserved,true);
  assert.equal(unread.cases[0].executionStatus,"completed");
  assert.equal(unread.cases[0].domainTaskStatus,"completed");
  assert.equal(unread.cases[0].reading.originalCompleteBeforeSave,true);
  assert.equal(unread.cases[0].reading.finalCompleteBeforeCompletion,false);
  assert.equal(unread.cases[0].firstSaveAccepted,true);
  assert.equal(unread.cases[0].firstAttemptSucceeded,false);
  assert.equal(unread.comparison.eligiblePairs,0);
  assert.ok(unread.cases[0].admittedTools.includes("query_story"));
  assert.ok(unread.cases[0].tools.some(t=>t.name==="discover_creative_capabilities"&&!t.error));

 }finally {await new Promise(r=>server.close(r));await rm(dir,{recursive:true,force:true});}
});
