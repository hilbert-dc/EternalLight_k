import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { CAPABILITIES, DELEGATION_EXECUTION_CAPABILITIES, ModelBridge, TaskBudget,
  compileCodexConfig, compileCodexModelCatalog, createExecutionLease, defineConnection, resolveSessionPlan } from "@eternallight/agent-engine";
import { verifiedBaselineEvidence } from "@eternallight/agent-engine/test-support";
import { CodexAppServerAdapter } from "../codex-adapter.mjs";
import { installManagedAgentRoles, NOVEL_SUBAGENT_DEVELOPER_INSTRUCTIONS } from "../codex-runtime.mjs";
import { NOVEL_COLLABORATION_INSTRUCTIONS, NOVEL_COLLABORATION_MODE } from "../novel-collaboration.mjs";
import { createMcpStartupWaiter } from "./helpers/mcp-startup.mjs";

// Real Codex, bridge and installed role files; scripted local model/MCP responses
// test role loading, bounded source pages and artifact references. Native tests
// separately prove scope enforcement; trusted spawn identity is not a prose role claim.
test("managed research and candidate roles read pages and return references in the active Engine turn", { timeout: 45000 }, async t => {
  const directory = mkdtempSync(join(tmpdir(), "novel-managed-collaboration-"));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const auditPath = join(directory, "calls.jsonl"), mcpPath = join(directory, "mcp.mjs");
  const names = ["read_context", "read_source_content", "read_tool_guide", "search_story", "search_relations", "query_story", "read_creative_strategy", "read_delegated_draft", "save_delegated_draft", "save_draft_checkpoint", "complete_task"];
  writeFileSync(mcpPath, `import {createInterface} from "node:readline"; import {appendFileSync} from "node:fs";
const names=${JSON.stringify(names)}; const reply=(id,result)=>process.stdout.write(JSON.stringify({jsonrpc:"2.0",id,result})+"\\n");
createInterface({input:process.stdin}).on("line",line=>{const m=JSON.parse(line); if(m.id===undefined)return;
if(m.method==="initialize")return reply(m.id,{protocolVersion:"2025-06-18",capabilities:{tools:{}},serverInfo:{name:"bounded-fixture",version:"1"}});
if(m.method==="tools/list")return reply(m.id,{tools:names.map(name=>({name,description:"Current scripted workspace contract",inputSchema:{type:"object",properties:{sourceId:{type:"string"},cursor:{type:"string"},delegatedCandidateId:{type:"string"},candidateId:{type:"string"}},additionalProperties:false}}))});
if(m.method!=="tools/call")return reply(m.id,{}); const p=m.params; appendFileSync(process.env.FIXTURE_AUDIT_PATH,JSON.stringify(p)+"\\n");
let value=p.name==="read_context"?{executionIdentity:{callerThreadId:p._meta.threadId,rootThreadId:p._meta.parentThreadId??p._meta.threadId},taskSources:{entries:[{sourceId:"source-frozen",revisionId:"source-revision-1",contentHash:"source-hash"}]}}:
p.name==="read_source_content"?{text:p.arguments.cursor?"SOURCE_PAGE_2":"SOURCE_PAGE_1",nextCursor:p.arguments.cursor?null:"next-page"}:
p.name==="save_delegated_draft"?{candidateId:"isolated-v1",sourceRevisionId:"source-revision-1",formalSaved:false}:
p.name==="read_delegated_draft"?{candidateId:"isolated-v1",sourceRevisionId:"source-revision-1",body:"BOUND_DRAFT"}:
p.name==="save_draft_checkpoint"?{revisionId:"formal-v1",delegatedCandidateId:p.arguments.delegatedCandidateId}:{};
reply(m.id,{content:[{type:"text",text:JSON.stringify(value)}]});});`);
  const requests = [], failures = [];
  t.after(() => {
    if (!requests.some(body => body.model === "managed-root-wire" && JSON.stringify(body.messages).includes("formal-v1"))) {
      t.diagnostic(JSON.stringify({failures:failures.map(error=>error.message),requests:requests.slice(-5).map(body=>({model:body.model,tail:body.messages.slice(-2)}))}));
    }
  });
  const read = async request => { const chunks=[]; for await (const chunk of request) chunks.push(chunk); return JSON.parse(Buffer.concat(chunks)); };
  const stream = (response, content) => {
    response.writeHead(200,{"content-type":"text/event-stream"});
    response.write(`data: ${JSON.stringify({id:"fixture",choices:[{delta:content,finish_reason:content.tool_calls?"tool_calls":"stop"}],usage:{prompt_tokens:10,completion_tokens:10,total_tokens:20}})}\n\n`);
    response.end("data: [DONE]\n\n");
  };
  const server = createServer(async (request,response) => {
    try {
      const body=await read(request); requests.push(body);
      const text=JSON.stringify(body.messages), tool=name=>body.tools?.find(item=>item.function?.name===name||item.function?.name?.endsWith(`__${name}`))?.function?.name;
      const call=(name,id,args={})=>{const actual=tool(name); assert.ok(actual,`Missing actual tool ${name} for ${body.model}`); stream(response,{tool_calls:[{index:0,id,function:{name:actual,arguments:JSON.stringify(args)}}]});};
      if(body.model==="managed-root-wire") {
        if(!text.includes("spawn_research"))return call("spawn_agent","spawn_research",{task_name:"research",agent_type:"explorer",model:"managed-child",message:"Inspect the frozen source pages and return evidence with exact source revision.",fork_turns:"none"});
        if(!text.includes("spawn_writer"))return call("spawn_agent","spawn_writer",{task_name:"candidate",agent_type:"candidate_writer",model:"managed-child",message:"Read frozen source pages, save only an isolated draft and return its immutable candidate reference.",fork_turns:"none"});
        if(!text.includes("EVIDENCE_RESEARCH_READY")||!text.includes("EVIDENCE_CANDIDATE_READY")) {
          if(!text.includes("wait_children"))return call("wait_agent","wait_children",{timeout_ms:10000});
          return call("list_agents",`list_children_${requests.length}`);
        }
        if(!text.includes("read_isolated"))return call("read_delegated_draft","read_isolated",{candidateId:"isolated-v1"});
        if(!text.includes("integrate_reference"))return call("save_draft_checkpoint","integrate_reference",{delegatedCandidateId:"isolated-v1"});
        assert.ok(text.includes("formal-v1")); return stream(response,{content:"已读取冻结资料，核对并整合隔离候选引用。"});
      }
      assert.equal(body.model,"managed-child-wire");
      const candidate=text.includes("isolated candidate-writing 小说工坊 subagent");
      if(!text.includes("read_bound"))return call("read_context","read_bound");
      if(!text.includes("page_first"))return call("read_source_content","page_first",{sourceId:"source-frozen"});
      if(!text.includes("page_second"))return call("read_source_content","page_second",{sourceId:"source-frozen",cursor:"next-page"});
      assert.ok(text.includes("SOURCE_PAGE_1")&&text.includes("SOURCE_PAGE_2"));
      if(candidate&&!text.includes("save_isolated"))return call("save_delegated_draft","save_isolated");
      return stream(response,{content:candidate?"EVIDENCE_CANDIDATE_READY isolated-v1 source-revision-1":"EVIDENCE_RESEARCH_READY source-revision-1"});
    } catch(error) { failures.push(error); response.writeHead(500,{"content-type":"application/json"}); response.end(JSON.stringify({error:{message:error.message}})); }
  });
  await new Promise(resolve=>server.listen(0,"127.0.0.1",resolve));
  t.after(()=>new Promise(resolve=>{server.closeAllConnections();server.close(resolve);}));
  const endpoint=`http://127.0.0.1:${server.address().port}`;
  const connections=["root","child"].map(kind=>defineConnection({id:`managed-${kind}`,provider:"fixture",endpoint,model:`managed-${kind}-wire`,credentialRef:"FIXTURE",protocol:"chat-completions",declaredLimits:{contextTokens:64000,outputTokens:4000}}));
  const capabilities=[CAPABILITIES.TEXT,CAPABILITIES.STRUCTURED_TOOL_CALLS,CAPABILITIES.TOOL_LOOP,CAPABILITIES.STREAMING,...DELEGATION_EXECUTION_CAPABILITIES];
  const edges=connections.flatMap(parent=>connections.map(child=>({parentConnectionId:parent.id,childConnectionId:child.id})));
  const plan=resolveSessionPlan({connections,evidence:connections.flatMap(connection=>verifiedBaselineEvidence(connection,{capabilities,routeGrants:edges})),policy:{defaultConnectionId:"managed-root",allowedChildConnectionIds:connections.map(connection=>connection.id),requiredCapabilities:DELEGATION_EXECUTION_CAPABILITIES,productExtensions:DELEGATION_EXECUTION_CAPABILITIES,allowedMcpServerIds:["novel_workspace"]}});
  const bridge=new ModelBridge({sessionPlan:plan,connections,credentialResolver:async()=>"fixture"});
  t.after(()=>bridge.close()); const address=await bridge.start(),budget=new TaskBudget({maxRequests:48,maxTokens:1000000});
  bridge.beginTaskBudget(budget); bridge.beginExecutionLease(createExecutionLease({sessionPlan:plan,runId:"managed-collaboration",budgetId:budget.id}));
  const catalog=join(directory,"models.json"); writeFileSync(catalog,JSON.stringify(compileCodexModelCatalog(plan,{baseInstructions:"Scripted integration only."})));
  installManagedAgentRoles(directory,fileURLToPath(new URL("../../codex-runtime/agents",import.meta.url)));
  writeFileSync(join(directory,"config.toml"),compileCodexConfig({sessionPlan:plan,modelCatalogPath:catalog,bridge:{baseUrl:address.baseUrl,credentialRef:"AGENT_ENGINE_BRIDGE_TOKEN"},subagentDeveloperInstructions:NOVEL_SUBAGENT_DEVELOPER_INSTRUCTIONS,usageHintText:NOVEL_COLLABORATION_INSTRUCTIONS,multiAgentModeHintText:NOVEL_COLLABORATION_MODE,mcpServers:[{id:"novel_workspace",command:process.execPath,args:[mcpPath],envVars:["FIXTURE_AUDIT_PATH"],required:true,approvalMode:"approve"}]}));
  mkdirSync(join(directory,"sqlite"));
  const codex=fileURLToPath(new URL("../../../../codex-rs/target/debug/codex",import.meta.url));
  const adapter=new CodexAppServerAdapter({command:[codex,"app-server"],cwd:directory,model:"managed-root",env:{PATH:process.env.PATH,CODEX_HOME:directory,CODEX_SQLITE_HOME:join(directory,"sqlite"),AGENT_ENGINE_BRIDGE_TOKEN:address.token,FIXTURE_AUDIT_PATH:auditPath}});
  t.after(()=>adapter.close()); adapter.startProcess();
  const startup=createMcpStartupWaiter({serverName:"novel_workspace",process:adapter.process,timeoutMs:adapter.timeouts.startupMs}); t.after(()=>startup.dispose());
  const events=[]; adapter.setCallbacks({onEvent:event=>{events.push(event);startup.onEvent(event);}});
  await adapter.ensureSession(); await startup.ready;
  let result;
  try { result=await adapter.sendTurn([{type:"text",text:"检查资料并按需使用隔离候选写作。"}],{disableEnvironment:true}); }
  catch(error) {
    const spawn=requests[0]?.tools?.find(item=>item.function?.name?.endsWith("__spawn_agent"));
    throw new Error(`${error.message}; spawn properties=${JSON.stringify(Object.keys(spawn?.function?.parameters?.properties??{}))}; invalid role warnings=${events.filter(event=>JSON.stringify(event).includes("invalid transport")).length}`,{cause:error});
  }
  assert.equal(failures.length,0,failures[0]?.message); assert.equal(result.message,"已读取冻结资料，核对并整合隔离候选引用。");
  const calls=readFileSync(auditPath,"utf8").trim().split("\n").map(JSON.parse);
  const children=calls.filter(call=>call._meta.parentThreadId);
  assert.equal(new Set(children.map(call=>call._meta.threadId)).size,2);
  assert.equal(children.filter(call=>call.name==="read_source_content").length,4);
  assert.equal(children.filter(call=>call.name==="save_delegated_draft").length,1);
  assert.equal(children.filter(call=>["save_draft_checkpoint","complete_task","authorize_delegated_writer"].includes(call.name)).length,0);
  assert.ok(!calls.some(call => call.name === "authorize_delegated_writer"));
  const writerCall = children.find(call => call.name === "save_delegated_draft");
  assert.equal(writerCall._meta.agentRole, "candidate_writer", "Real Core must attest the selected execution role");
  assert.equal(writerCall._meta.parentThreadId, adapter.threadId);
  assert.ok(children.filter(call => call._meta.agentRole === "explorer").every(call => call.name !== "save_delegated_draft"));
  assert.equal(events.filter(event=>JSON.stringify(event).includes("invalid transport")).length,0);
  const integrated=calls.find(call=>call.name==="save_draft_checkpoint");
  assert.deepEqual(integrated.arguments,{delegatedCandidateId:"isolated-v1"});
  for(const request of requests.filter(request=>request.model==="managed-child-wire")) {
    const developer=request.messages.filter(message=>message.role==="developer").map(message=>message.content).join("\n");
    assert.match(developer,/Public language contract:/);
  }
  assert.equal(events.filter(event=>event.method==="turn/started"&&event.params?.threadId===adapter.threadId).length,1,"Child results must return inside the existing root turn");
});
