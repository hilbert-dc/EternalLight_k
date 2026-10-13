import { mcpRequestIdentity } from "../mcp-authority.mjs";
import { novelRecoveryOperationHash } from "../novel-recovery-contract.mjs";
import { nativeTestBinary } from "./helpers/native-binaries.mjs";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

import { CAPABILITIES,ModelBridge,PersistentTaskBudget,compileCodexConfig,compileCodexModelCatalog,
  createExecutionLease,defineConnection,resolveSessionPlan } from "@eternallight/agent-engine";
import { verifiedBaselineEvidence } from "@eternallight/agent-engine/test-support";
import { CodexAppServerAdapter } from "../codex-adapter.mjs";
import { STORY_QUERY_INSTRUCTIONS } from "../workflow-handoff.mjs";
import { taskProviderBudgetOptions } from "../task-provider-budget.mjs";
import { waitForRuntimeReady } from "../runtime-readiness.mjs";
import { createMcpStartupWaiter } from "./helpers/mcp-startup.mjs";

function unwrapTool(message) {
  assert.equal(message?.role,"tool");
  assert.match(message.content,/^Wall time: [0-9.]+ seconds\nOutput:/);
  const marker="\nOutput:";
  const offset=message.content.indexOf(marker);
  assert.ok(offset>=0);
  return JSON.parse(message.content.slice(offset+marker.length).trim());
}

// Runtime and required MCP each have the production 90s startup window;
// the outer harness must not cancel setup before those windows elapse.
test("real Codex and native Runtime read unmatched passages, continue and rerank through the existing engine",{timeout:240_000},async(t)=>{
  const stages = [];
  const began = performance.now();
  const mark = phase => stages.push({ phase, elapsedMs: Math.round(performance.now() - began) });
  const root = await mkdtemp(join(tmpdir(),"novel-passage-engine-"));
  const suffix = process.platform === "win32" ? ".exe" : "";
  const binary = (name)=>nativeTestBinary(name);
  const author = "passage-engine-author-token-12345678";
  const candidate = "passage-engine-candidate-token-12345";
  const capability = "passage-engine-capability-1234567890";
  let runtime,downstream,bridge,adapter,budget;
  t.after(async()=>{
    t.diagnostic(JSON.stringify({ stages }));
    await adapter?.close();
    await bridge?.close();
    budget?.close();
    if (runtime && runtime.exitCode === null && runtime.signalCode === null) {
      const stopped = new Promise((resolve)=>runtime.once("exit",resolve));
      runtime.kill();
      await stopped;
    }
    if (downstream?.listening) await new Promise((resolve)=>{downstream.closeAllConnections();downstream.close(resolve);});
    await rm(root,{recursive:true,force:true});
  });
  const probe = createServer();
  await new Promise((resolve,reject)=>{probe.once("error",reject);probe.listen(0,"127.0.0.1",resolve);});
  const port = probe.address().port;
  await new Promise((resolve,reject)=>probe.close(error=>error?reject(error):resolve()));
  const origin = `http://127.0.0.1:${port}`;
  runtime = spawn(binary("novel-codex-story-runtime"),["--port",String(port),"--db",join(root,"story.sqlite3"),"--engine-budget-db",join(root,"engine-budget.sqlite3"),"--token-stdin"],{stdio:["pipe","pipe","pipe"]});
  runtime.stdin.end(`${JSON.stringify({authorToken:author,candidateToken:candidate})}\n`);
  runtime.stdout.resume();
  runtime.stderr.resume();
  await Promise.race([waitForRuntimeReady(runtime,origin),new Promise((_resolve,reject)=>runtime.once("error",reject))]);
  mark("runtimeReady");
  const authorRequest = async(path,payload)=>{
    const response = await fetch(`${origin}${path}`,{method:"POST",headers:{authorization:`Bearer ${author}`,"content-type":"application/json"},body:JSON.stringify(payload)});
    const value = await response.json();
    assert.ok(response.ok,JSON.stringify(value));
    return value;
  };
  const {project} = await authorRequest("/api/projects",{title:"改述证据回环",logline:"",medium:"novel",idempotencyKey:randomUUID()});
  const path = `/api/projects/${project.id}`;
  const draft = await authorRequest(`${path}/drafts/checkpoints`,{expectedRevision:0,expectedCandidateRevisionId:null,idempotencyKey:"source-one",
    payload:{chapterId:"chapter-1",chapterNumber:1,title:"折返",body:`${"晨光".repeat(90)}暴雨淹没了渡口，他只好折返。${"潮声".repeat(200)}`}});
  mark("storySeeded");
  const plan = {question:"为什么行程被延误？",byteBudget:9000,ranking:{type:"literal"},
    steps:[{id:"read",operation:{type:"passages",cursor:null,maxChars:80,limit:2}}]};
  let binding,first,second,ranked,semanticPlan,reading,failure,checkpointArguments;
  let stage = 0;
  const delivered = [];
  downstream = createServer(async(request,response)=>{
    try {
      const chunks=[];
      for await(const chunk of request) chunks.push(chunk);
      const body = JSON.parse(Buffer.concat(chunks).toString("utf8"));
      const tool = (name)=>body.tools.find((entry)=>entry.function?.name?.endsWith(`__${name}`) || entry.function?.name === name)?.function.name;
      assert.equal(tool("save_draft_checkpoint"),undefined);
      let args,name;
      if(stage === 0){name=tool("query_story");args={...binding,plan};}
      else if(stage === 1){
        first=unwrapTool(body.messages.findLast((message)=>message.role === "tool"));
        assert.ok(Array.isArray(first.steps),JSON.stringify(first));
        assert.ok(first.steps[0].nextCursor);
        name=tool("checkpoint_reading");args={...binding,plan,reportHash:first.contentHash,expectedVersion:0,summary:"已读首段，解释未认证"};
      } else if(stage === 2){
        reading=unwrapTool(body.messages.findLast((message)=>message.role === "tool"));
        assert.equal(reading.checkpoint.version,1);
        assert.equal(reading.checkpoint.acknowledgedTraversalComplete,false);
        assert.equal(reading.semanticCompleteness,"unknown");
        name=tool("read_reading_progress");args={...binding};
      } else if(stage === 3){
        const restored=unwrapTool(body.messages.findLast((message)=>message.role === "tool"));
        assert.deepEqual(restored,reading);
        assert.equal(restored.checkpoint.resumePlan.steps[0].operation.cursor,first.steps[0].nextCursor);
        name=tool("query_story");args={...binding,plan:restored.checkpoint.resumePlan};
      } else if(stage === 4){
        second=unwrapTool(body.messages.findLast((message)=>message.role === "tool"));
        assert.ok(Object.values(second.evidence).some((item)=>item.quote.includes("暴雨淹没了渡口")));
        semanticPlan=structuredClone(second.plan);
        semanticPlan.ranking={type:"modelSemantic",baseReportHash:second.contentHash,
          judgments:second.steps[0].results.map((row)=>({resultId:row.resultId,score:second.evidence[row.evidenceRef].quote.includes("暴雨")?1000:0,
            rationale:"受控模型测试指定的相关性候选，不是文学质量认证"}))};
        name=tool("query_story");args={...binding,plan:semanticPlan};
      } else if(stage === 5){
        ranked=unwrapTool(body.messages.findLast((message)=>message.role === "tool"));
        assert.ok(ranked.evidence[ranked.steps[0].results[0].evidenceRef].quote.includes("暴雨淹没了渡口"));
        assert.equal(ranked.ranking.authority,"modelCandidate");
        assert.equal(ranked.coverage.complete,false);
        name=tool("read_context");args={...binding,queryPlan:semanticPlan};
      } else {
        const context=unwrapTool(body.messages.findLast((message)=>message.role === "tool"));
        assert.deepEqual(context.queryReport,ranked);
        const quote=ranked.evidence[ranked.steps[0].results[0].evidenceRef].quote;
        response.writeHead(200,{"content-type":"text/event-stream"});
        response.end(`data: ${JSON.stringify({id:"passage-answer",usage:{prompt_tokens:20,completion_tokens:3,total_tokens:23},choices:[{delta:{content:`根据版本绑定原文：${quote}。未完成全书语义扫描。`},finish_reason:"stop"}]})}\n\ndata: [DONE]\n\n`);
        return;
      }
      assert.ok(name);
      if (stage === 1) checkpointArguments = structuredClone(args);
      delivered.push(name);
      response.writeHead(200,{"content-type":"text/event-stream"});
      response.write(`data: ${JSON.stringify({id:`passage-${stage}`,choices:[{delta:{tool_calls:[{index:0,id:`passage-call-${stage}`,function:{name,arguments:JSON.stringify(args)}}]}}]})}\n\n`);
      response.end(`data: ${JSON.stringify({id:`passage-${stage++}`,usage:{prompt_tokens:20,completion_tokens:3,total_tokens:23},choices:[{delta:{},finish_reason:"tool_calls"}]})}\n\ndata: [DONE]\n\n`);
    }catch(error){failure=error;response.writeHead(500,{"content-type":"application/json"});response.end(JSON.stringify({error:{message:error.message}}));}
  });
  await new Promise((resolve,reject)=>{downstream.once("error",reject);downstream.listen(0,"127.0.0.1",resolve);});
  const connection=defineConnection({id:"passage-model",provider:"fixture",endpoint:`http://127.0.0.1:${downstream.address().port}`,model:"fixture-passage",
    credentialRef:"FIXTURE_KEY",protocol:"chat-completions",declaredLimits:{contextTokens:64000,outputTokens:4000}});
  const evidence=verifiedBaselineEvidence(connection,{protocol:connection.protocol,
    capabilities:[CAPABILITIES.TEXT,CAPABILITIES.STRUCTURED_TOOL_CALLS,CAPABILITIES.TOOL_LOOP,CAPABILITIES.STREAMING]});
  const sessionPlan=resolveSessionPlan({connections:[connection],evidence,policy:{defaultConnectionId:connection.id,allowedMcpServerIds:["novel_workspace"]}});
  bridge=new ModelBridge({mcpAuthority:true,requestConfigurationProvider:({threadId})=>mcpRequestIdentity(adapter,threadId),sessionPlan,connections:[connection],credentialResolver:async()=>"fixture-key",token:"passage-bridge-key"});
  const address=await bridge.start();
  mark("bridgeReady");
  const home=join(root,"codex-home");
  await mkdir(join(home,"sqlite"),{recursive:true,mode:0o700});
  const catalog=join(home,"catalog.json");
  await writeFile(catalog,JSON.stringify(compileCodexModelCatalog(sessionPlan,{baseInstructions:STORY_QUERY_INSTRUCTIONS})),{mode:0o600});
  await writeFile(join(home,"config.toml"),compileCodexConfig({sessionPlan,modelCatalogPath:catalog,
    bridge:{baseUrl:address.baseUrl,credentialRef:"AGENT_ENGINE_BRIDGE_TOKEN"},mcpServers:[{id:"novel_workspace",command:binary("novel-workspace-mcp"),args:[],
      envVars:["NOVEL_CODEX_RUNTIME_URL","NOVEL_CODEX_CANDIDATE_TOKEN","NOVEL_CODEX_TASK_CAPABILITY","NOVEL_CODEX_OPERATION","NOVEL_CODEX_ENGINE_AUTHORITY_URL","NOVEL_CODEX_ENGINE_AUTHORITY_TOKEN"],approvalMode:"approve",required:true,startupTimeoutMs:90_000}]}),{mode:0o600});
  const codex=fileURLToPath(new URL(`../../../../codex-rs/target/debug/codex${suffix}`,import.meta.url));
  adapter=new CodexAppServerAdapter({command:[codex,"app-server"],cwd:root,env:{PATH:process.env.PATH,CODEX_HOME:home,CODEX_SQLITE_HOME:join(home,"sqlite"),
    AGENT_ENGINE_BRIDGE_TOKEN:address.token,NOVEL_CODEX_ENGINE_AUTHORITY_URL:address.baseUrl,NOVEL_CODEX_ENGINE_AUTHORITY_TOKEN:address.token,NOVEL_CODEX_RUNTIME_URL:origin,NOVEL_CODEX_CANDIDATE_TOKEN:candidate,
    NOVEL_CODEX_TASK_CAPABILITY:capability,NOVEL_CODEX_OPERATION:"advisory"},model:connection.id,provider:connection.provider});
  adapter.startProcess();
  const startup=createMcpStartupWaiter({serverName:"novel_workspace",process:adapter.process,timeoutMs:adapter.timeouts.startupMs});
  t.after(()=>startup.dispose());
  const started=startup.ready;
  adapter.setCallbacks({onEvent:startup.onEvent});
  await adapter.ensureSession();
  mark("codexSessionReady");
  const workspaceResponse = await fetch(`${origin}${path}/creative-workspace`, { headers: { authorization: `Bearer ${author}` } });
  assert.equal(workspaceResponse.ok, true);
  const workspace = await workspaceResponse.json();
  const task=await authorRequest(`${path}/tasks`,{operation:"advisory",inputRevision:0,threadId:adapter.threadId,target:{type:"project"},authorIntent:plan.question,
    capabilityHash:createHash("sha256").update(capability).digest("hex"),capabilityExpiresAt:Math.floor(Date.now()/1000)+600,
    initialBudget: { authorizationId: randomUUID(), authorMessageId: randomUUID(), expectedBaseSnapshotId: workspace.headSnapshotId,
      expectedSourceDigest: workspace.sourceDigest, maxRequests: 16, maxTokens: 1_000_000, deadlineAt: Math.floor(Date.now()/1000)+600 } });
  const grantResponse = await fetch(`${origin}${path}/tasks/${task.id}/provider-budget`, { headers: { authorization: `Bearer ${author}` } });
  assert.equal(grantResponse.ok, true);
  const grant = await grantResponse.json();
  budget = new PersistentTaskBudget({ databasePath: join(root,"engine-budget.sqlite3"), runId: task.id,
    ...taskProviderBudgetOptions(grant, { taskId: task.id, workTaskId: `work-${task.id}`, deadlineAt: grant.deadlineAt }) });
  bridge.beginTaskBudget(budget);
  bridge.beginExecutionLease(createExecutionLease({sessionPlan,runId:task.id,budgetId:budget.id}));
  mark("taskBudgetReady");
  binding={};
  await started;
  mark("mcpReady");
  const result=await adapter.sendTurn([{type:"text",text:plan.question}],{disableEnvironment:true}).catch((error)=>{assert.ifError(failure);throw error;});
  mark("turnCompleted");
  assert.ifError(failure);
  assert.equal(delivered.length,6);
  assert.match(result.message,/暴雨淹没了渡口/);
  assert.match(result.message,/未完成全书语义扫描/);
  assert.equal(Object.values(ranked.evidence).every((item)=>item.sourceRevisionId === draft.revisionId),true);
  assert.deepEqual(ranked.evidence,second.evidence);
  const recovery=await authorRequest(`${path}/tasks/recovery-snapshot`,{taskIds:[task.id],fence:null,cursor:0,limit:500});
  const operation=recovery.operations.find((entry)=>entry.tool_name === "checkpoint_reading");
  assert.equal(operation.state,"committed");
  assert.equal(operation.receipt_ref,reading.checkpoint.receiptRef);
  assert.equal(operation.thread_id,adapter.threadId);
  assert.equal(operation.request_hash, novelRecoveryOperationHash({tool:"checkpoint_reading",arguments:checkpointArguments}),
    "Host raw model request and Native expanded request must retain one recovery identity");
  assert.equal(recovery.operations.some((entry)=>entry.tool_name === "read_reading_progress"),false);
  await authorRequest(`${path}/tasks/${task.id}/complete`, { threadId: adapter.threadId, summary: "有界阅读完成" });
  budget.complete();
  const usage = await authorRequest(`${path}/work-tasks/work-${task.id}/provider-usage`, { taskId: task.id });
  assert.equal(usage.usedRequests, 7);
});
