import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { inflateSync } from "node:zlib";
import { test } from "node:test";
import { CAPABILITIES, CapabilityVerifier } from "@eternallight/agent-engine";
import { verifiedBaselineEvidence } from "@eternallight/agent-engine/test-support";
import { inspectCodexConnection, verifyCodexConnection, verifyCodexImageInput, loadCodexEvidence, listCodexConnections, resolveCodexProvider } from "../codex-runtime.mjs";

function pixels(item) {
  const png = Buffer.from(item.image_url.split(",")[1], "base64"), chunks=[];
  for (let i=8; i<png.length;) { const size=png.readUInt32BE(i); if (png.subarray(i+4,i+8).toString()==="IDAT") chunks.push(png.subarray(i+8,i+8+size)); i+=12+size; }
  const rows=inflateSync(Buffer.concat(chunks)), palette={"230,20,20":"red","20,190,20":"green","20,20,230":"blue","230,230,20":"yellow"};
  return [[32,32],[96,32],[32,96],[96,96]].map(([x,y])=>palette[[...rows.subarray(y*385+1+x*3,y*385+4+x*3)].join()]);
}
test("current DeepSeek route gains imageInput only from explicit pixel evidence and loses it after failed re-verification", async () => {
  const directory=await mkdtemp(join(tmpdir(),"media-image-evidence-"));
  try {
    const provider=inspectCodexConnection({provider:"deepseek",model:"deepseek-flash"});
    const options={baseEnvironment:{DEEPSEEK_API_KEY:"offline-fixture-secret"},credentialScopes:{[provider.id]:"offline-realm"},evidencePath:join(directory,"evidence.json"),provider:"deepseek",model:"deepseek-flash",qualificationContextDigest:"image-fixture-build"};
    const records=verifiedBaselineEvidence(provider.connection,{capabilities:[CAPABILITIES.TEXT,CAPABILITIES.STRUCTURED_TOOL_CALLS,CAPABILITIES.STREAMING,CAPABILITIES.USAGE,CAPABILITIES.TOOL_LOOP],credentialScope:"offline-realm",qualificationContextDigest:options.qualificationContextDigest});
    await verifyCodexConnection({...options,verifier:{verify:async()=>({selectedProtocol:"responses",attempts:[{protocol:"responses",status:"verified"}],records})}});
    const list=async()=>listCodexConnections({credentialScopes:options.credentialScopes,evidence:await loadCodexEvidence(options.evidencePath),qualificationContextDigest:options.qualificationContextDigest});
    assert.equal((await list())[0].status,"verified"); assert.equal((await list())[0].verifiedCapabilities.includes("imageInput"),false);
    let calls=0, correct=true;
    const verifier=new CapabilityVerifier({fetchImpl:async(_url,request)=> {
      calls++; const body=JSON.parse(request.body), image=body.input[0].content.find(item=>item.type==="input_image");
      const colours=correct?pixels(image):["red","red","red","red"];
      return new Response(JSON.stringify({id:"image-observation",status:"completed",output:[{type:"function_call",call_id:`vision-${calls}`,name:body.tools[0].name,arguments:JSON.stringify({colours})}],usage:{input_tokens:20,output_tokens:20,total_tokens:40}}),{headers:{"content-type":"application/json"}});
    }});
    const result=await verifyCodexImageInput({...options,verifier}); assert.equal(calls,2); assert.equal(result.records.length,1); assert.equal(result.records[0].capability,"imageInput");
    const evidence=await loadCodexEvidence(options.evidencePath); const configured=resolveCodexProvider({credentialScopes:options.credentialScopes,evidence,provider:options.provider,model:options.model,qualificationContextDigest:options.qualificationContextDigest});
    assert.ok(configured.sessionPlan.routes[0].authorizedExtensions.includes("imageInput"));
    assert.doesNotMatch(await readFile(options.evidencePath,"utf8"),/offline-fixture-secret|data:image/);
    await assert.rejects(verifyCodexImageInput({...options,qualificationContextDigest:"changed-build",verifier}),/先完成/); assert.equal(calls,2);
    await assert.rejects(verifyCodexImageInput({...options,credentialScopes:{[provider.id]:"rotated-realm"},verifier}),/先完成/); assert.equal(calls,2);
    correct=false; await assert.rejects(verifyCodexImageInput({...options,verifier}),/尚未通过/); assert.equal(calls,3);
    const failed=(await list())[0]; assert.equal(failed.status,"verified"); assert.equal(failed.verifiedCapabilities.includes("imageInput"),false); assert.ok(failed.failedCapabilities.includes("imageInput"));
  } finally { await rm(directory,{recursive:true,force:true}); }
});
