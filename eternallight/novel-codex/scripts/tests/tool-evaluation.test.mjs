import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { evaluateNovelTools } from "../evaluate-novel-tools.mjs";
import { checkToolEvaluation, safeFailure, comparisonModeHonored } from "../tool-evaluation-report.mjs";
import { evaluateLiveNovelTools, observeTool } from "../evaluate-live-novel-tools.mjs";

test("pure-mode comparisons reject mixed attempts without erasing delivery facts",()=>{
 const save=(mode,error=false)=>({name:"save_draft_checkpoint",status:"completed",error,requestedMode:mode});
 for(const mode of ["full","patch"]) {
  const other=mode==="full"?"patch":"full";
  assert.equal(comparisonModeHonored([save(mode)],mode),true);
  assert.equal(comparisonModeHonored([save(mode,true),save(mode)],mode),true);
  for(const tools of [[],[save(mode,true)],[save(mode),save(other)],[save(other,true),save(mode)],
   [{...save(mode),status:"running"}],[{...save(mode),requestedMode:null}]]) {
   assert.equal(comparisonModeHonored(tools,mode),false);
  }
 }
 assert.equal(comparisonModeHonored([save("other")],"other"),false);
});

test("independent case failures persist, stay in denominators and clean up",async()=>{
  const dir=await mkdtemp(join(tmpdir(),"tool-report-"));let calls=0,closed=0;
  try {
    const output=join(dir,"nested/report.json");
    const report=await evaluateNovelTools({repeats:1,outputPath:output,runtimeFactory:async()=>({
      binaries:{},request:async()=>{calls++;throw Object.assign(Error("secret-provider-key"),{code:"EVALUATION_TIMEOUT"});},close:async()=>{closed++;}})});
    assert.equal(calls,4);assert.equal(closed,1);assert.equal(report.runs.length,4);
    assert.ok(report.runs.every(r=>r.status==="failed"&&r.error.code==="timeout"&&r.error.phase==="fixture"));
    assert.equal(report.summaries.find(s=>s.mode==="patch").firstAttemptSuccessRate,0);
    assert.equal(report.gate.passed,false);
    const text=await readFile(output,"utf8");assert.doesNotMatch(text,/secret-provider-key/);assert.deepEqual(JSON.parse(text),report);
  }finally {await rm(dir,{recursive:true,force:true});}
});
test("startup failure leaves atomic diagnostic report",async()=>{
 const dir=await mkdtemp(join(tmpdir(),"tool-startup-"));try {
  const file=join(dir,"report.json");const result=await evaluateNovelTools({outputPath:file,runtimeFactory:async()=>{throw Object.assign(Error("private path"),{code:"ENOENT"});}});
  assert.equal(result.status,"failed");assert.equal(result.fatalError.code,"transport_unavailable");assert.equal(JSON.parse(await readFile(file,"utf8")).gate.passed,false);
 }finally {await rm(dir,{recursive:true,force:true});}
});
test("regression gate rejects missing cases, unknown preservation and token regression",()=>{
 const report={evidence:"deterministicToolReplay",status:"completed",repeats:1,runs:[]};
 for(const mode of ["full","patch","deliberateCorrection"])for(const size of ["short","long"])report.runs.push({repeat:0,mode,size,status:"passed",firstAttemptSucceeded:mode!=="deliberateCorrection",contentPreserved:true,corrections:mode==="deliberateCorrection"?1:0,saveInputTokens:{cl100k_base:mode==="full"?1000:100,o200k_base:mode==="full"?900:90}});
 assert.equal(checkToolEvaluation(report).passed,true);
 for(const mutate of [r=>r.runs.pop(),r=>r.runs[0].size="long",r=>r.runs[0].contentPreserved=null,r=>r.runs[3].saveInputTokens.cl100k_base=800]) {const altered=structuredClone(report);mutate(altered);assert.equal(checkToolEvaluation(altered).passed,false);}
 assert.deepEqual(safeFailure(Error("private prose"),"readback"),{phase:"readback",code:"execution_failed"});
});
test("live configuration failure is recorded before any credential or provider access",async()=>{
 const dir=await mkdtemp(join(tmpdir(),"live-tool-report-"));try {
  const output=join(dir,"report.json"),report=await evaluateLiveNovelTools({},output);
  assert.equal(report.status,"blocked");assert.equal(report.error.phase,"configuration");assert.equal(report.cases.length,0);assert.equal(report.providerUsage,null);
 }finally {await rm(dir,{recursive:true,force:true});}
});
test("live metrics consume actual MCP receipts without retaining arguments or prose",()=>{
 const row=observeTool({type:"mcpToolCall",tool:"save_draft_checkpoint",status:"completed",arguments:{patch:{private:"body"}},result:{isError:true,structuredContent:{error:{details:{reason:"draft_patch_invalid"}}}}});
 assert.equal(row.error,true);assert.equal(row.requestedMode,"patch");assert.equal(row.reason,"draft_patch_invalid");assert.doesNotMatch(JSON.stringify(row),/private|body/);
});

test("failure hints are bounded observations and never persist raw error text",()=>{
 for(const hint of ["task token budget exhausted","task request budget exhausted","task duration budget exhausted"]) {
  assert.deepEqual(safeFailure(Error(`secret-key ${hint} private prose`),"modelTurn"),{phase:"modelTurn",code:"execution_failed",reportedHint:hint});
 }
 assert.deepEqual(safeFailure(Error("secret-key novel budget exhausted private prose"),"modelTurn"),{phase:"modelTurn",code:"execution_failed"});
});

test("selected live retries keep explicit fixed-case identity and safe Engine error observations",async()=>{
 const {validateLiveOptions,observeResponse}=await import("../evaluate-live-novel-tools.mjs");
 const config={provider:"test",model:"test",evidencePath:"test",credentialSaltPath:"test",maxRequests:10,maxTokens:50000,timeoutSeconds:90,repeats:1};
 assert.equal(validateLiveOptions({...config,startupTimeoutMs:600000}).startupTimeoutMs,600000);
 for(const startupTimeoutMs of [0,-1,600001,1.5,null,"600000"])assert.throws(()=>validateLiveOptions({...config,startupTimeoutMs}));
 assert.equal(validateLiveOptions({...config,caseSelection:[{size:"long",mode:"patch"}]}).caseSelection.length,1);
 for(const caseSelection of [[],[{size:"other",mode:"patch"}],[{size:"long",mode:"patch"},{size:"long",mode:"patch"}]])assert.throws(()=>validateLiveOptions({...config,caseSelection}));
 assert.deepEqual(observeResponse({status:"failed",errorCode:"task_token_budget_exhausted",message:"secret",body:"private"}),{status:"failed",errorCode:"task_token_budget_exhausted"});
 assert.deepEqual(observeResponse({status:"secret",errorCode:"private"}),{status:"unknown",errorCode:null});
});

 test("Core terminal diagnostic retains only known categories and numeric HTTP status",async()=>{
  const {observeTurn}=await import("../evaluate-live-novel-tools.mjs");
  const observed=observeTurn({status:"failed",error:{codexErrorInfo:{httpConnectionFailed:{httpStatusCode:502,private:"credential"}},message:"credential private prose"}});
  assert.equal(observed.errorKind,"httpConnectionFailed");assert.equal(observed.httpStatus,502);assert.doesNotMatch(JSON.stringify(observed),/credential|private prose/);
  assert.equal(observeTurn({status:"secret",error:{codexErrorInfo:"secret"}}).errorKind,null);
  assert.equal(observeTurn({status:"failed",error:{codexErrorInfo:{httpConnectionFailed:{httpStatusCode:"secret"}}}}).httpStatus,null);
 });

test('final content is independently checked; unreadable and non-quiescent remain unknown',async()=>{
 const {observeFinalState}=await import('../tool-evaluation-observations.mjs');
 const original={chapterId:'chapter',chapterNumber:1,title:'月下',sceneGroups:[{id:'selected',body:'风𠮷é',locked:false},{id:'locked',body:'private-locked-prose',locked:true}]};
 const current=structuredClone(original);current.sceneGroups[0].body='雨𠮷é';
 const runtime={request:async(_method,path)=>path.endsWith('/creative-workspace')?{chapters:[{number:1,candidate:current}]}:{status:'running'}};
 const options={runtime,base:'/test',taskId:'task',original,quiescent:true};
 let result=await observeFinalState(options);
 assert.equal(result.contentPreserved,true);assert.equal(result.domainTaskStatus,'running');
 assert.equal(result.readback.bodyUnchanged,false);
 const queued=await observeFinalState({...options,runtime:{request:async(method,path)=>path.endsWith('/creative-workspace')?runtime.request(method,path):{status:'queued'}}});
 assert.equal(queued.domainTaskStatus,'queued');
 assert.doesNotMatch(JSON.stringify(result),/private-locked-prose|𠮷/);
 result=await observeFinalState({...options,quiescent:false});
 assert.equal(result.contentPreserved,null);assert.equal(result.readback.status,'matched');
 current.sceneGroups[1].body='changed';
 result=await observeFinalState(options);
 assert.equal(result.contentPreserved,false);assert.equal(result.readback.checks.protectedGroups,false);
 result=await observeFinalState({...options,runtime:{request:async()=>{throw Error('secret-key');}}});
 assert.equal(result.contentPreserved,null);assert.equal(result.readback.status,'unavailable');
 assert.doesNotMatch(JSON.stringify(result),/secret-key/);
});

test('read trajectory distinguishes versions, cursors and queries without storing prose',async()=>{
 const {summarizeReads}=await import('../tool-evaluation-observations.mjs');
 const item={type:'mcpToolCall',tool:'read_source_content',status:'completed',arguments:{sourceId:'private-source',cursor:'private-cursor',quote:'private prose'},result:{structuredContent:{sourceId:'private-source',contentHash:'version-1',entries:[{path:'/private-path',text:'private prose',characterRange:{start:0,end:12,total:12}}],coverage:{fromEntry:0,toEntry:1,totalEntries:1}}}};
 const a=observeTool(item),b=observeTool(structuredClone(item));
 item.result.structuredContent.contentHash='version-2';const c=observeTool(item);
 item.arguments.cursor='cursor-2';const d=observeTool(item);
 item.arguments.quote='another quote';const e=observeTool(item);
 assert.equal(summarizeReads([a,b,c,d,e]).repeatedExactResults,1);
 assert.deepEqual(a.read.entries[0].range,{start:0,end:12,total:12});
 assert.doesNotMatch(JSON.stringify([a,b,c,d,e]),/private|version-1|another quote/);
});


test("request cost observations hash identities and whitelist numeric fields",async()=>{
 const {observeRequest,observeResponse}=await import("../evaluate-live-novel-tools.mjs");
 const request=observeRequest({requestId:"private-id",requestMetrics:{maxTokens:8192,bodyBytes:12345,toolsBytes:-1,inputBytes:2.5,messagesBytes:"secret",instructionsBytes:0,extra:"private prose"}});
 assert.equal(request.maxTokens,8192);assert.equal(request.toolsBytes,null);assert.equal(request.inputBytes,null);assert.equal(request.messagesBytes,null);assert.equal(request.instructionsBytes,0);
 const response=observeResponse({requestId:"private-id",status:"failed",errorCode:"provider_output_limit",normalizedUsage:{inputTokens:900,outputTokens:8192,totalTokens:9092,reasoningTokens:null,cachedTokens:500,extra:"secret"}});
 assert.equal(response.request,request.request);assert.equal(response.normalizedUsage.reasoningTokens,null);assert.equal(response.normalizedUsage.outputTokens,8192);
 assert.doesNotMatch(JSON.stringify({request,response}),/private|secret/);
});
