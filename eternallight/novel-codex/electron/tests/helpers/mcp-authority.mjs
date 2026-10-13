import { createHash } from "node:crypto";
import { ModelBridge, TaskBudget, createExecutionLease, defineConnection, resolveSessionPlan } from "@eternallight/agent-engine";
import { verifiedBaselineEvidence } from "@eternallight/agent-engine/test-support";
import { executorValidatedTools } from "../../executor-validation-policy.mjs";

// Current protocol fixture: real Engine admission/correlation, scripted local
// provider, no external model. Production never accepts fixture MCP metadata.
export async function createMcpAuthorityFixture(t, capability) {
  const connection=defineConnection({id:"fixture",provider:"fixture",model:"fixture",endpoint:"https://fixture.invalid",credentialRef:"FIXTURE",protocol:"responses",declaredLimits:{contextTokens:256000,outputTokens:4000}});
  const plan=resolveSessionPlan({connections:[connection],evidence:verifiedBaselineEvidence(connection)});
  let output=[], identity={};
  const bridge=new ModelBridge({sessionPlan:plan,connections:[connection],executorValidatedTools,mcpAuthority:true,credentialResolver:async()=>"fixture",
    requestConfigurationProvider:async({threadId})=>({mcpScope:createHash("sha256").update(capability).digest("hex"),mcpIdentity:{threadId,agentRole:identity.agentRole??null}}),
    fetchImpl:async()=>new Response(JSON.stringify({id:"fixture",status:"completed",output,usage:{input_tokens:1,output_tokens:1,total_tokens:2}}),{headers:{"content-type":"application/json"}})});
  const address=await bridge.start();t.after(()=>bridge.close());
  const budget=new TaskBudget();bridge.beginTaskBudget(budget);bridge.beginExecutionLease(createExecutionLease({sessionPlan:plan,runId:"mcp-fixture",budgetId:budget.id}));
  const roots=new Set();
  const request=async(meta,tools)=>{
    const response=await fetch(`${address.baseUrl}/responses`,{method:"POST",headers:{authorization:`Bearer ${address.token}`,"content-type":"application/json","x-codex-turn-metadata":JSON.stringify(meta)},body:JSON.stringify({model:"fixture",input:[],stream:false,tools})});
    const text=await response.text();return {status:response.status,text};
  };
  return {
    env:{NOVEL_CODEX_ENGINE_AUTHORITY_URL:address.baseUrl,NOVEL_CODEX_ENGINE_AUTHORITY_TOKEN:address.token},
    async admit(name,args,meta,tools){
      const root=meta.sessionId, caller=meta.threadId??meta.callerThreadId;
      if(!roots.has(root)){output=[];identity={};const admitted=await request({thread_id:root,session_id:root,turn_id:"fixture-root"},[]);if(admitted.status===200)roots.add(root);}
      identity=meta;output=[{type:"function_call",namespace:"mcp__novel_workspace",name,call_id:meta.callId,arguments:JSON.stringify(args)}];
      return request({thread_id:caller,session_id:root,parent_thread_id:meta.parentThreadId,turn_id:meta.turnId},[{type:"namespace",name:"mcp__novel_workspace",tools:tools.map(t=>({type:"function",name:t.name,parameters:t.inputSchema}))}]);
    },
  };
}
