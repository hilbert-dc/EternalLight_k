import assert from "node:assert/strict";
import { createHash, randomUUID } from "node:crypto";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { performance } from "node:perf_hooks";
import { getEncoding } from "js-tiktoken";
import { saveReport, safeFailure, summarizeRuns, checkToolEvaluation } from "./tool-evaluation-report.mjs";
import { evaluationRuntime } from "./tool-evaluation-runtime.mjs";

const encodings = { cl100k_base:getEncoding("cl100k_base"), o200k_base:getEncoding("o200k_base") };
const tokens = value => Object.fromEntries(Object.entries(encodings).map(([name,encoding])=>[name,encoding.encode(JSON.stringify(value)).length]));
const add = (a,b) => Object.fromEntries(Object.keys(encodings).map(name=>[name,(a[name]??0)+(b[name]??0)]));
const decode = result => result.structuredContent ?? JSON.parse(result.content.find(part=>part.type==="text").text);


export async function evaluateNovelTools({ repeats=3, outputPath, runtimeFactory=evaluationRuntime }={}) {
  const runs=[];
  const report={version:2,generatedAt:new Date().toISOString(),evidence:"deterministicToolReplay",status:"running",providerUsage:null,repeats,runs,
    tokenMeasurement:"Named local encodings over name/arguments and normalized structured results; excludes prompt history, duplicate MCP wrappers and provider billing.",
    latencyMeasurement:"Local tool round trips; process startup, provider and fixture creation excluded. No latency gate.",
    limitations:["Scripted arguments do not measure model selection accuracy or literary quality.","Failures stay in denominators; deliberate corrections are separate."]};
  const persist=async()=>{report.summaries=summarizeRuns(runs);if(outputPath)await saveReport(outputPath,report);};
  await persist();
  let runtime, outerPhase="configuration";

  try {
    assert.ok(Number.isSafeInteger(repeats)&&repeats>=1&&repeats<=20,"Evaluation repeats must be 1..20");
    outerPhase="runtime";
    runtime=await runtimeFactory();
    for (let repeat=0;repeat<repeats;repeat++) for (const size of ["short","long"]) for (const mode of ["full","patch"]) {
      const measurements=[]; let mcp, phase="fixture";
      const record={repeat,size,mode,status:"running",firstAttemptSucceeded:false,corrections:0,contentPreserved:false,providerUsage:null};
      runs.push(record); await persist();
      try {
      const {project}=await runtime.request("POST","/api/projects",{title:"隔离工具效率样本",logline:"",medium:"novel",idempotencyKey:randomUUID()});
      const base=`/api/projects/${project.id}`, chapterId=randomUUID();
      const editable="风🌙风月。".repeat(size==="short"?1:720);
      const groups=[{id:"selected",sceneId:null,body:editable,locked:false},{id:"locked",sceneId:null,body:"锁定结尾保持原样。",locked:true}];
      const original={chapterId,chapterNumber:1,title:"月下",completeness:"chapterComplete",sceneGroups:groups,body:groups.map(g=>g.body).join("\n\n---\n\n")};
      const saved=await runtime.request("POST",`${base}/drafts/checkpoints`,{expectedRevision:0,expectedCandidateRevisionId:null,idempotencyKey:"baseline",payload:original});
      const thread=randomUUID(),capability=randomUUID();
      const workspace=await runtime.request("GET",`${base}/creative-workspace`);
      await runtime.request("POST",`${base}/tasks`,{operation:"generateChapter",threadId:thread,inputRevision:0,authorIntent:"只把开头的风改成雨，保留其他正文和锁定结尾。",
        target:{type:"chapter",chapterId,chapterNumber:1,readingChapter:1,sceneOrder:1,scope:{type:"selectedScenes",sourceRevisionId:saved.revisionId,groupIds:["selected"]}},
        capabilityHash:createHash("sha256").update(capability).digest("hex"),capabilityExpiresAt:Math.floor(Date.now()/1000)+600,
        initialBudget:{authorizationId:randomUUID(),authorMessageId:randomUUID(),expectedBaseSnapshotId:workspace.headSnapshotId,expectedSourceDigest:workspace.sourceDigest,maxRequests:null,maxTokens:null,deadlineAt:null}});
      phase="startup"; mcp=runtime.mcp(capability,thread); await mcp.rpc("initialize",{protocolVersion:"2025-06-18",capabilities:{},clientInfo:{name:"tool-evaluation",version:"1"}});
      const catalog=(await mcp.rpc("tools/list")).tools;
      const call=async(name,args)=>{
        phase=name;
        const argumentsValue={expectedRevision:0,...args},start=performance.now();
        let result;
        try {result=await mcp.call(name,argumentsValue);}
        catch(error) {
          measurements.push({tool:name,success:false,elapsedMs:performance.now()-start,inputTokens:tokens({name,arguments:argumentsValue}),outputTokens:null,error:safeFailure(error,name)});
          throw error;
        }
        const elapsedMs=performance.now()-start,value=decode(result);
        measurements.push({tool:name,success:result.isError!==true,elapsedMs,inputTokens:tokens({name,arguments:argumentsValue}),outputTokens:tokens(value)});
        return {value,success:result.isError!==true};
      };
      const context=(await call("read_context",{})).value;
      const source=context.taskSources.entries.find(e=>e.sectionPath==="/relevantText"); assert.ok(source);
      let cursor,chosen;
      do {
        const page=(await call("read_source_content",{sourceId:source.sourceId,...(cursor?{cursor}:{})})).value;
        for(const entry of page.entries??[]) if(entry.editAnchor?.groupId==="selected" && entry.editAnchor.start===0)chosen=entry;
        cursor=page.nextCursor;
      }while(cursor);
      assert.ok(chosen?.editAnchor,"Native must return an edit anchor from actual source pages");
      if(mode==="patch") {
        const located=(await call("read_source_content",{sourceId:source.sourceId,view:"locate",quote:"风🌙风月。",groupId:"selected"})).value;
        chosen=located.entries.find(entry=>entry.editAnchor.start===0);
        assert.ok(chosen); assert.equal(located.coverage.completeSource,false);
      }
      const anchor=chosen.editAnchor,oldText=chosen.text??chosen.value;
      assert.equal(anchor.sourceRevisionId,saved.revisionId);assert.equal(anchor.offsetUnit,"unicodeScalar");
      assert.equal([...oldText].length,anchor.end-anchor.start);
      const desired="雨"+editable.slice(1),fragment="雨"+oldText.slice(1);
      const payload={...original,sceneGroups:[{...groups[0],body:desired},groups[1]]};delete payload.body;
      const patch={chapterId:anchor.chapterId,sourceRevisionId:anchor.sourceRevisionId,edits:[{groupId:anchor.groupId,start:anchor.start,end:anchor.end,oldText,newText:fragment}]};
      const args={expectedCandidateRevisionId:saved.revisionId,idempotencyKey:"save",...(mode==="full"?{payload}:{patch})};
      const result=await call("save_draft_checkpoint",args);assert.ok(result.success,JSON.stringify(result.value));
      assert.equal(result.value.recoveryOperation.outcome,"committed");
      phase="readback";
      const current=(await runtime.request("GET",`${base}/creative-workspace`)).chapters[0].candidate;
      assert.equal(current.sceneGroups[0].body,desired);assert.deepEqual(current.sceneGroups[1],groups[1]);
      const save=measurements.at(-1);
      Object.assign(record,{status:"passed",contentPreserved:true,firstAttemptSucceeded:result.success,catalogTokens:tokens(catalog),saveInputTokens:save.inputTokens,
        toolCalls:measurements.length,inputTokens:measurements.reduce((a,m)=>add(a,m.inputTokens),{}),outputTokens:measurements.reduce((a,m)=>add(a,m.outputTokens),{}),
        elapsedMs:measurements.reduce((sum,m)=>sum+m.elapsedMs,0),saveLatencyMs:save.elapsedMs,providerUsage:null});
      // A separate deliberately bad-input case validates correction telemetry;
      // it never contaminates the valid-case first-attempt success denominator.
      if(mode==="patch") {
        const correction={repeat,size,mode:"deliberateCorrection",status:"running",firstAttemptSucceeded:false,corrections:0,providerUsage:null};
        runs.push(correction);
        try {
        const corrected={expectedCandidateRevisionId:result.value.revisionId,idempotencyKey:"corrected",patch:{...patch,sourceRevisionId:result.value.revisionId,edits:[{...patch.edits[0],oldText:fragment,newText:oldText}]}};
        const invalid=structuredClone(corrected);invalid.patch.edits[0].oldText="错误原文";
        const bad=await call("save_draft_checkpoint",invalid);assert.equal(bad.success,false);
        assert.equal(bad.value.error.details.constraint,"old_text_mismatch");assert.equal(bad.value.recoveryOperation.outcome,"rejected");
        const repair=await call("save_draft_checkpoint",corrected);assert.ok(repair.success,JSON.stringify(repair.value));
        Object.assign(correction,{status:"passed",corrections:1,toolCalls:2,
          inputTokens:measurements.slice(-2).reduce((a,m)=>add(a,m.inputTokens),{}),outputTokens:measurements.slice(-2).reduce((a,m)=>add(a,m.outputTokens),{}),
          elapsedMs:measurements.slice(-2).reduce((sum,m)=>sum+m.elapsedMs,0),providerUsage:null});
        } catch(error) {correction.status="failed";correction.error=safeFailure(error,phase);}
      }
      } catch(error) {record.status="failed";record.error=safeFailure(error,phase);}
      finally {
        record.measurements=measurements;
        if(record.status==="failed") {
          record.inputTokens=measurements.reduce((a,m)=>add(a,m.inputTokens),{});
          record.outputTokens=measurements.filter(m=>m.outputTokens).reduce((a,m)=>add(a,m.outputTokens),{});
          record.missingOutputs=measurements.filter(m=>!m.outputTokens).length;
          record.elapsedMs=measurements.reduce((sum,m)=>sum+m.elapsedMs,0);
        }
        try {await mcp?.close();} catch(error) {record.status="failed";record.error=safeFailure(error,"cleanup");}
        await persist();
      }
    }
    const binaries=Object.fromEntries(await Promise.all(Object.entries(runtime.binaries).map(async([name,path])=>[name,{path,sha256:createHash("sha256").update(await readFile(path)).digest("hex")}])));
    report.binaries=binaries;
    report.status="completed";
  } catch(error) {report.status="failed";report.fatalError=safeFailure(error,outerPhase);}
  finally {
    try {await runtime?.close();} catch(error) {report.status="failed";report.fatalError=safeFailure(error,"cleanup");}
    report.completedAt=new Date().toISOString();report.gate=checkToolEvaluation(report);await persist();
  }
  return report;
}

if (process.argv[1] && resolve(process.argv[1])===fileURLToPath(import.meta.url)) {
  const output=process.argv[2];if(!output)throw Error("Usage: node scripts/evaluate-novel-tools.mjs <report.json> [repeats]");
  const repeatArg=process.argv.slice(3).find(value=>value!=="--check");
  const report=await evaluateNovelTools({outputPath:output,repeats:repeatArg===undefined?3:Number(repeatArg)});
  if(!report.gate.passed)process.exitCode=1;
  console.log(JSON.stringify({report:resolve(output),evidence:report.evidence,summaries:report.summaries},null,2));
}
