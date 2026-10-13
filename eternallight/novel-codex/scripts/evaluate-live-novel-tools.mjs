// Developer evaluation only: production Core owns the single model/tool loop.
import assert from "node:assert/strict";
import { createHash, randomUUID } from "node:crypto";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { performance } from "node:perf_hooks";
import { createTaskBoundHandoff, taskMethodText, taskBoundInstructions, taskExecutionPrompt } from "../electron/workflow-handoff.mjs";
import { executorValidatedTools } from "../electron/executor-validation-policy.mjs";
import { evaluationRuntime } from "./tool-evaluation-runtime.mjs";
import { saveReport, safeFailure, comparisonModeHonored } from "./tool-evaluation-report.mjs";
import { observeSourceRead, observeSavedRevision, observeContext, observeFinalState, summarizeReads } from "./tool-evaluation-observations.mjs";
import { readingPolicies, readingInstructions, assessReading, summarizeComparablePairs, summarizePayloadCost, sumLedgerUsage } from "./tool-evaluation-comparison.mjs";
const root=fileURLToPath(new URL("..",import.meta.url));
const hash=value=>createHash("sha256").update(value).digest("hex");
export function validateLiveOptions(options) {
  for(const key of ["provider","model","evidencePath","credentialSaltPath"]) assert.ok(typeof options[key]==="string"&&options[key].trim(),`${key} required`);
  for(const key of ["maxRequests","maxTokens","timeoutSeconds","repeats"]) assert.ok(Number.isSafeInteger(options[key])&&options[key]>0,`${key} must be positive`);
  assert.ok(options.timeoutSeconds<=2147483,"timeout exceeds timer range");
  if(options.readingPolicy!==undefined)assert.ok(readingPolicies.includes(options.readingPolicy),"unknown reading policy");
  if(options.startupTimeoutMs!==undefined)assert.ok(Number.isSafeInteger(options.startupTimeoutMs)&&options.startupTimeoutMs>0&&options.startupTimeoutMs<=600000,"startupTimeoutMs must be within 1..600000");
  if(options.caseSelection!==undefined) {
    assert.ok(Array.isArray(options.caseSelection)&&options.caseSelection.length>0&&options.caseSelection.length<=4,"invalid case selection");
    assert.equal(new Set(options.caseSelection.map(c=>`${c.size}:${c.mode}`)).size,options.caseSelection.length,"duplicate cases");
    assert.ok(options.caseSelection.every(c=>["short","long"].includes(c.size)&&["full","patch"].includes(c.mode)),"unknown case");
  }
  return options;
}
const numericFields=(value,keys)=>Object.fromEntries(keys.map(key=>[key,Number.isSafeInteger(value?.[key])&&value[key]>=0?value[key]:null]));
const requestDigest=value=>typeof value?.requestId==="string"?hash(value.requestId):null;
export function observeRequest(value) {
  return {request:requestDigest(value),...numericFields(value?.requestMetrics,["maxOutputTokens","maxTokens","maxCompletionTokens","generationMaxOutputTokens","bodyBytes","toolsBytes","inputBytes","messagesBytes","instructionsBytes"])};
}
export function observeResponse(value) {
  const codes=["task_token_budget_exhausted","task_request_budget_exhausted","task_duration_budget_exhausted","provider_output_item_limit","provider_output_limit","provider_context_limit","invalid_tool_arguments","response_incomplete","provider_stream_failed","provider_rate_limited","provider_server_error","bridge_error"];
  return {status:["completed","failed","incomplete"].includes(value?.status)?value.status:"unknown",errorCode:codes.includes(value?.errorCode)?value.errorCode:null,...(typeof value?.requestId==="string"?{request:requestDigest(value)}:{}),...(value?.normalizedUsage?{normalizedUsage:numericFields(value.normalizedUsage,["inputTokens","outputTokens","totalTokens","cachedTokens","reasoningTokens"])}:{})};
}
export function observeTurn(turn) {
  const info=turn?.error?.codexErrorInfo;
  const simple=["contextWindowExceeded","sessionBudgetExceeded","usageLimitExceeded","rateLimitExceeded","flexUnavailable","serverOverloaded","cyberPolicy","misalignmentPolicyViolation","tooManyDenials","internalServerError","unauthorized","badRequest","threadRollbackFailed","sandboxError","other"];
  const transport=["httpConnectionFailed","responseStreamConnectionFailed","responseStreamDisconnected","responseTooManyFailedAttempts"];
  const kind=typeof info==="string"&&simple.includes(info)?info:transport.find(key=>info&&typeof info==="object"&&Object.hasOwn(info,key))??null;
  const statusCode=kind&&typeof info==="object"?info[kind]?.httpStatusCode:null;
  return {status:["completed","failed","interrupted"].includes(turn?.status)?turn.status:"unknown",errorKind:kind,
    httpStatus:Number.isInteger(statusCode)&&statusCode>=100&&statusCode<=599?statusCode:null,
    ...(turn?.error?{failure:safeFailure(turn.error,"coreTurn")}: {})};
}
export function observeTool(item) {
  if(item?.type!=="mcpToolCall")return null;
  let result=item.result?.structuredContent;
  if(!result) {try {result=JSON.parse(item.result?.content?.find(x=>x.type==="text")?.text);}catch { /* no parsed result */ }}
  return {name:item.tool,status:item.status,error:!!(item.error||item.status==="failed"||item.result?.isError||result?.error),
    resultBytes:result===undefined?null:Buffer.byteLength(JSON.stringify(result)),
    reason:result?.error?.details?.reason??result?.code??null,
    ...(item.tool==="read_context"?{context:observeContext(result)}:{}),
    ...(item.tool==="read_source_content"?{read:observeSourceRead(item.arguments,result)}:{}),
    ...(item.tool==="save_draft_checkpoint"?{save:observeSavedRevision(result,item.arguments)}:{}),
    requestedMode:item.tool==="save_draft_checkpoint"?(item.arguments?.patch?"patch":item.arguments?.payload?"full":"other"):null};
}
async function qualifiedConnection(options) {
  const api=await import("../electron/codex-runtime.mjs");
  const credentials=await import("../electron/development-credentials.mjs");
  const environment=credentials.resolveDevelopmentEnvironment(credentials.loadProjectDevelopmentEnvironment(root,process.env));
  const cli=api.resolveCodexCli({explicitPath:environment.NOVEL_CODEX_CLI,isPackaged:false,projectRoot:root,resourcesPath:root});
  assert.ok(cli.path,"qualified CLI required");
  const baseInstructions=(await readFile(join(root,"codex-runtime/engine-prompt.md"),"utf8")).trim();
  const qualificationContextDigest=api.createQualificationContextDigest({baseInstructions,cliPath:cli.path});
  let provider;
  try {
    provider=api.resolveCodexProvider({provider:options.provider,model:options.model,operation:"generateChapter",
      credentialScopes:api.credentialScopesFromEnvironment(environment,resolve(options.credentialSaltPath)),
      evidence:await api.loadCodexEvidence(resolve(options.evidencePath)),qualificationContextDigest});
  } catch {
    throw Object.assign(Error("Current connection verification is required"),{code:"task_qualification_unavailable"});
  }
  return {environment,cli,baseInstructions,qualificationContextDigest,provider};
}

export async function evaluateLiveNovelTools(options,outputPath,{prepareConnection=qualifiedConnection}={}) {
  const report={version:2,evidence:prepareConnection===qualifiedConnection?"liveModelToolComparison":"controlledProviderToolComparison",status:"running",cases:[],providerUsage:null,
    limitations:["Exact-edit synthetic cases measure tool use and content preservation, not literary quality.","Provider usage comes from Engine receipts; unknown usage stays unknown. No qualification is fabricated."]};
  const persist=()=>saveReport(outputPath,report);await persist();
  let runtime,phase="configuration",identityFiles;
  const identity=async files=>Object.fromEntries(await Promise.all(Object.entries(files).map(async([name,path])=>[name,hash(await readFile(path))])));
  try {
    validateLiveOptions(options);
    report.configuration={provider:options.provider,model:options.model,repeats:options.repeats,
      readingPolicy:options.readingPolicy??"task",toolScope:"nativeOperationCatalog",limitsPerCase:{requests:options.maxRequests,tokens:options.maxTokens,seconds:options.timeoutSeconds},startupTimeoutMs:options.startupTimeoutMs??30000,plannedCases:(options.caseSelection?.length??4)*options.repeats,caseSelection:options.caseSelection??null};
    const engine=await import("@eternallight/agent-engine");
    const api=await import("../electron/codex-runtime.mjs");
    const {CodexAppServerAdapter}=await import("../electron/codex-adapter.mjs");
    const {taskProviderBudgetOptions}=await import("../electron/task-provider-budget.mjs");
    const {createMcpStartupWaiter}=await import("../electron/tests/helpers/mcp-startup.mjs");
    phase="qualification";
    const {environment,cli,baseInstructions,qualificationContextDigest,provider}=await prepareConnection(options);
    report.build={codex:hash(await readFile(cli.path)),engine:hash(await readFile(engine.nativeEnginePath)),engineContractHash:engine.contractHash(),qualificationContextDigest};
    phase="runtime";await persist();const runtimeStarted=performance.now();runtime=await evaluationRuntime({startupTimeoutMs:options.startupTimeoutMs??30000});report.runtimeStartupMs=performance.now()-runtimeStarted;
    report.build.native=Object.fromEntries(await Promise.all(Object.entries(runtime.binaries).map(async([name,path])=>[name,hash(await readFile(path))])));
    identityFiles={...runtime.binaries,codex:cli.path,engine:engine.nativeEnginePath,
      ...Object.fromEntries(["codex-runtime/engine-prompt.md","scripts/evaluate-live-novel-tools.mjs","scripts/tool-evaluation-comparison.mjs","scripts/tool-evaluation-observations.mjs","scripts/tool-evaluation-report.mjs","scripts/tool-evaluation-runtime.mjs","electron/workflow-handoff.mjs","electron/task-provider-budget.mjs","electron/codex-adapter.mjs","electron/codex-runtime.mjs"].map(file=>[file,join(root,file)]))};
    report.comparisonIdentity={files:await identity(identityFiles),baseInstructions:hash(baseInstructions),sessionPlan:hash(JSON.stringify(provider.sessionPlan)),configuration:hash(JSON.stringify(report.configuration))};
    for(let repeat=0;repeat<options.repeats;repeat++)for(const size of ["short","long"])for(const mode of repeat%2?["patch","full"]:["full","patch"]) {
      if(options.caseSelection&&!options.caseSelection.some(c=>c.size===size&&c.mode===mode))continue;
      const record={repeat,size,mode,requests:[],responses:[],turns:[],status:"running",tools:[],usage:null,contentPreserved:null,firstAttemptSucceeded:false,corrections:0};
      report.cases.push(record);await persist();
      let bridge,adapter,budget,startup,task,grant,base,lease,original,startupStarted=null,terminal=false;
      const started=performance.now();
      try {
        phase="fixture";
        const {project}=await runtime.request("POST","/api/projects",{title:"隔离模型工具对照",logline:"",medium:"novel",idempotencyKey:randomUUID()});
        base=`/api/projects/${project.id}`;
        const chapterId=randomUUID(),body="风吹过窗棂。"+("月光照着安静的书房。".repeat(size==="long"?240:2));
        original={chapterId,chapterNumber:1,title:"月下",sceneGroups:[{id:"selected",sceneId:null,body,locked:false},{id:"locked",sceneId:null,body:"锁定结尾。",locked:true}]};
        original.body=original.sceneGroups.map(g=>g.body).join("\n\n---\n\n");
        record.fixtureDigest=hash(JSON.stringify({chapterNumber:original.chapterNumber,title:original.title,sceneGroups:original.sceneGroups}));
        const saved=await runtime.request("POST",`${base}/drafts/checkpoints`,{expectedRevision:0,expectedCandidateRevisionId:null,idempotencyKey:"seed",payload:original});
        const workspace=await runtime.request("GET",`${base}/creative-workspace`);
        phase="startup";startupStarted=performance.now();
        const capability=randomUUID();
        const catalogClient=runtime.mcp(capability,"evaluation-catalog");
        let allowed;
        try {
          const catalog=await catalogClient.rpc("tools/list");
          assert.ok(Array.isArray(catalog.tools)&&catalog.tools.length>0,"actual operation catalog required");
          allowed=catalog.tools.map(tool=>tool.name);
          assert.ok(allowed.every(name=>typeof name==="string"&&name.length>0));
          record.admittedCatalogDigest=hash(JSON.stringify(catalog.tools));
          record.admittedTools=allowed;
        } finally { await catalogClient.close(); }
        bridge=new engine.ModelBridge({sessionPlan:provider.sessionPlan,connections:provider.connections,credentialResolver:async ref=>environment[ref],executorValidatedTools,requestObserver:value=>{if(record.requests.length<options.maxRequests+10)record.requests.push(observeRequest(value));},responseObserver:value=>{if(record.responses.length<options.maxRequests+10)record.responses.push(observeResponse(value));},
          toolPolicy:{rootAllowed:allowed.flatMap(n=>[n,`mcp__novel_workspace.${n}`,`mcp__novel_workspace__${n}`]),childAllowed:[]}});
        const address=await bridge.start(),home=join(runtime.directory,randomUUID());await mkdir(join(home,"sqlite"),{recursive:true});
        const catalog=join(home,"catalog.json");await writeFile(catalog,JSON.stringify(engine.compileCodexModelCatalog(provider.sessionPlan,{baseInstructions})),{mode:0o600});
        await writeFile(join(home,"config.toml"),api.buildCodexConfig({sessionPlan:provider.sessionPlan,modelCatalogPath:catalog,
          bridge:{baseUrl:address.baseUrl,credentialRef:"AGENT_ENGINE_BRIDGE_TOKEN"},
          mcpServerCommand:runtime.binaries.mcp}),{mode:0o600});
        const safeEnv=Object.fromEntries(["PATH","PATHEXT","SystemRoot","WINDIR","TEMP","TMP","TMPDIR"].filter(k=>environment[k]).map(k=>[k,environment[k]]));
        adapter=new CodexAppServerAdapter({command:[cli.path,"app-server"],cwd:home,env:{...safeEnv,CODEX_HOME:home,CODEX_SQLITE_HOME:join(home,"sqlite"),AGENT_ENGINE_BRIDGE_TOKEN:address.token,...runtime.mcpEnvironment(capability)},model:provider.id,provider:provider.provider});
        adapter.setCallbacks({onEvent:event=>{startup?.onEvent(event);if(event.method==="turn/completed")record.turns.push(observeTurn(event.params?.turn));if(event.method==="item/completed"){const observation=observeTool(event.params?.item);if(observation)record.tools.push(observation);}}});
        adapter.startProcess();startup=createMcpStartupWaiter({serverName:"novel_workspace",process:adapter.process,timeoutMs:options.startupTimeoutMs??30000});
        await adapter.ensureSession();await startup.ready;record.mcpSessionStartupMs=performance.now()-startupStarted;
        const deadlineAt=Math.floor(Date.now()/1000)+options.timeoutSeconds;
        const intent="只把正文开头的‘风’改为‘雨’，其余文字、标题、组顺序和锁定内容逐字保留，保存候选。";
        task=await runtime.request("POST",`${base}/tasks`,{operation:"generateChapter",threadId:adapter.threadId,inputRevision:workspace.revision,authorIntent:intent,
          target:{type:"chapter",chapterId,chapterNumber:1,readingChapter:1,sceneOrder:1,scope:{type:"selectedScenes",sourceRevisionId:saved.revisionId,groupIds:["selected"]}},
          capabilityHash:hash(capability),capabilityExpiresAt:deadlineAt,
          initialBudget:{authorizationId:randomUUID(),authorMessageId:randomUUID(),expectedBaseSnapshotId:workspace.headSnapshotId,expectedSourceDigest:workspace.sourceDigest,maxRequests:options.maxRequests,maxTokens:options.maxTokens,deadlineAt}});
        grant=await runtime.request("GET",`${base}/tasks/${task.id}/provider-budget`);
        budget=new engine.PersistentTaskBudget({databasePath:runtime.budgetPath,runId:task.id,...taskProviderBudgetOptions(grant,{taskId:task.id,workTaskId:grant.workTaskId,deadlineAt})});
        await bridge.beginTaskBudget(budget);lease=engine.createExecutionLease({sessionPlan:provider.sessionPlan,runId:task.id,budgetId:budget.id});await bridge.beginExecutionLease(lease);
        phase="modelTurn";
        let timer;const modelStarted=performance.now();
        const handoff=createTaskBoundHandoff(project.id,workspace,task);
        const prompt=api.composeNovelCodexPrompt(taskMethodText(task),taskExecutionPrompt(task),`${taskBoundInstructions(handoff)}\n\n工具对照实验：本样本用 ${mode==="full"?"save_draft_checkpoint.payload 完整载荷":"save_draft_checkpoint.patch 局部修改，可用 read_source_content locate 精确定位"} 保存。${readingInstructions(report.configuration.readingPolicy)}expectedRevision=${workspace.revision}。`,{internal:true,binding:handoff});
        try {await Promise.race([adapter.sendTurn(prompt.input,{disableEnvironment:true,additionalContext:prompt.additionalContext}),new Promise((_,reject)=>{timer=setTimeout(()=>reject(Object.assign(Error("timeout"),{code:"EVALUATION_TIMEOUT"})),options.timeoutSeconds*1000);})]);}
        finally {clearTimeout(timer);record.modelElapsedMs=performance.now()-modelStarted;}
        record.executionStatus="completed";
      } catch(error) {record.status="failed";record.executionStatus=phase==="modelTurn"?"failed":"notRun";record.error=safeFailure(error,phase);}
      finally {
        if(startupStarted!==null && record.mcpSessionStartupMs===undefined)record.failedStartupElapsedMs=performance.now()-startupStarted;
        startup?.dispose();
        let quiescent=true;
        for(const close of [()=>lease&&bridge?.endExecutionLease(lease),()=>bridge?.close(),()=>adapter?.finishTaskOperation(),()=>adapter?.close()]) {
          try {await close();}catch(error){quiescent=false;record.status="failed";record.cleanupError=safeFailure(error,"cleanup");}
        }
        // Freeze observations after shutdown, including events delivered during cleanup.
        const saves=record.tools.filter(t=>t.name==="save_draft_checkpoint");
        record.firstSaveAccepted=saves[0]?.status==="completed"&&!saves[0].error&&saves[0].save?.saved===true;
        record.corrections=saves.filter(t=>t.error).length;
        record.comparisonModeHonored=comparisonModeHonored(record.tools,mode);
        record.toolErrorCount=record.tools.filter(t=>t.error).length;
        record.readSummary=summarizeReads(record.tools);
        record.reading=assessReading(record.tools,report.configuration.readingPolicy);
        record.payloadCost=summarizePayloadCost(record);
        Object.assign(record,await observeFinalState({runtime,base,taskId:task?.id,original,quiescent}));
        if(record.executionStatus==="completed"&&quiescent) {
          try {
            assert.equal(record.contentPreserved,true,"persisted exact edit and protected content must match");
            assert.equal(record.comparisonModeHonored,true,"all save attempts must honor comparison mode");
            assert.equal(record.domainTaskStatus,"completed","Native task must complete");
            if(report.configuration.readingPolicy==="completeOriginalAndFinal")assert.equal(record.reading.honored,true,"experiment reading obligations must be independently verified");
            budget.complete("completed");terminal=true;record.status="passed";
          }catch(error){record.status="failed";record.error=safeFailure(error,"validation");}
        }
        if(budget) {
          try {
            if(!terminal) {
              try {
                const currentTask=await runtime.request("GET",`${base}/tasks/${task.id}`);
                if(!["completed","failed","cancelled"].includes(currentTask.status)) await runtime.request("POST",`${base}/tasks/${task.id}/cancel`);
        } catch(error) {record.status="failed";record.cleanupError=safeFailure(error,"taskTermination");}
              budget.complete("failed");
            }
            await runtime.request("POST",`${base}/work-tasks/${grant.workTaskId}/provider-usage`,{taskId:task.id});
            record.usage=engine.PersistentTaskBudget.receipt(runtime.budgetPath,task.id).usage;
          }catch(error){record.status="failed";record.usageError=safeFailure(error,"usageSettlement");}
          finally {budget.close();}
        }
        record.firstAttemptSucceeded=record.status==="passed"&&saves.length===1&&record.firstSaveAccepted&&record.toolErrorCount===0;
        record.elapsedMs=performance.now()-started;await persist();
      }
    }
    report.summaries=["full","patch"].map(mode=>{
      const cases=report.cases.filter(c=>c.mode===mode), known=cases.every(c=>c.usage&&c.usage.unknownTokens===0);
      return {mode,cases:cases.length,passed:cases.filter(c=>c.status==="passed").length,
        firstAttemptSuccessRate:cases.length?cases.filter(c=>c.firstAttemptSucceeded).length/cases.length:null,
        contentCheckedCases:cases.filter(c=>c.contentPreserved!==null).length,
        contentUnknownCases:cases.filter(c=>c.contentPreserved===null).length,
        contentPreservationRate:cases.some(c=>c.contentPreserved!==null)?cases.filter(c=>c.contentPreserved===true).length/cases.filter(c=>c.contentPreserved!==null).length:null,
        correctionCount:cases.reduce((sum,c)=>sum+c.corrections,0),
        providerUsage:known?{requests:cases.reduce((n,c)=>n+c.usage.requests,0),tokens:cases.reduce((n,c)=>n+c.usage.tokens,0)}:null};
    });
    report.providerUsage=sumLedgerUsage(report.cases);
    report.buildStable=JSON.stringify(await identity(identityFiles))===JSON.stringify(report.comparisonIdentity.files);
    report.comparison=summarizeComparablePairs(report.cases,{...report.configuration,repeats:options.repeats},report.buildStable);
    report.status=report.cases.every(c=>c.status==="passed")&&report.buildStable?"completed":"failed";
  } catch(error) {report.status="blocked";report.error=safeFailure(error,phase);}
  finally {try{await runtime?.close();}catch(error){report.status="failed";report.cleanupError=safeFailure(error,"cleanup");}await persist();}
  return report;
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url)) {
  const [config,output]=process.argv.slice(2);if(!config||!output)throw Error("Usage: node scripts/evaluate-live-novel-tools.mjs <config.json> <report.json>");
  const report=await evaluateLiveNovelTools(JSON.parse(await readFile(config,"utf8")),output);
  console.log(JSON.stringify({status:report.status,cases:report.cases.length,report:resolve(output)}));if(report.status!=="completed")process.exitCode=1;
}
