import assert from "node:assert/strict";
import { inflateSync } from "node:zlib";
import { test } from "node:test";
import { CAPABILITIES, CapabilityVerifier, defineConnection } from "../src/index.mjs";

const connection = protocol => defineConnection({ id: "image-route", provider: "Fixture", endpoint: "https://models.example.test", model: "fixture", credentialRef: "KEY", protocol,
  declaredCapabilities: ["text"], declaredLimits: { contextTokens: 131072, outputTokens: 2000 } });
const jsonResponse = (status, payload) => new Response(JSON.stringify(payload), { status, headers: { "content-type": "application/json" } });
function readPixels(image) {
  const png = Buffer.from(image.image_url.split(",")[1], "base64");
  assert.equal(png.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
  const compressed = []; for (let offset = 8; offset < png.length;) { const size = png.readUInt32BE(offset); if (png.subarray(offset + 4, offset + 8).toString() === "IDAT") compressed.push(png.subarray(offset + 8, offset + 8 + size)); offset += size + 12; }
  const rows = inflateSync(Buffer.concat(compressed));
  const colour = { "230,20,20": "red", "20,190,20": "green", "20,20,230": "blue", "230,230,20": "yellow" };
  return [[32,32],[96,32],[32,96],[96,96]].map(([x,y]) => { assert.equal(rows[y * 385], 0); const offset = y * 385 + 1 + x * 3; return colour[[...rows.subarray(offset, offset + 3)].join()]; });
}
const toolOutput = (body, colours) => ({ id: "vision-observation", status: "completed", output: [{ type: "function_call", call_id: "image-probe-call", name: body.tools[0].name, arguments: JSON.stringify({ colours }) }], usage: { input_tokens: 20, output_tokens: 20, total_tokens: 40 } });

test("explicit image qualification observes randomized real pixels and binds only imageInput evidence", async () => {
  const requests = [];
  const verifier = new CapabilityVerifier({ fetchImpl: async (_url, options) => {
    const body = JSON.parse(options.body); requests.push(body);
    const images = body.input[0].content.filter(item => item.type === "input_image"); assert.equal(images.length, 1);
    const observed = readPixels(images[0]);
    assert.equal(body.stream, false); assert.equal(body.max_output_tokens, 384);
    assert.deepEqual(Object.keys(body.tools[0].parameters.properties), ["colours"]);
    assert.doesNotMatch(body.instructions + body.input[0].content[0].text, /230|190|"colours"/);
    return jsonResponse(200, toolOutput(body, observed));
  } });
  const result = await verifier.verifyImageInput(connection("responses"), { apiKey: "fixture-secret", credentialScope: "realm-one", qualificationContextDigest: "context-one" });
  assert.equal(requests.length, 2); assert.equal(result.selectedProtocol, "responses");
  assert.deepEqual(result.records.map(record => [record.capability, record.status]), [[CAPABILITIES.IMAGE_INPUT, "verified"]]);
  const record = result.records[0]; assert.equal(record.scope.suite, "synthetic-image-probe-v1"); assert.equal(record.scope.mode, "buffered");
  assert.equal(record.sample.imageCount, 2); assert.equal(record.sample.basis, "randomized-pixel-recognition");
  assert.doesNotMatch(JSON.stringify(record), /fixture-secret|data:image|base64/);
  const other = await verifier.verifyImageInput(connection("responses"), { apiKey: "fixture-secret", credentialScope: "realm-two", qualificationContextDigest: "context-one" });
  assert.notEqual(record.environmentDigest, other.records[0].environmentDigest);
});

test("HTTP success without correct visual content cannot qualify images", async () => {
  const verifier = new CapabilityVerifier({ fetchImpl: async (_url, options) => jsonResponse(200, toolOutput(JSON.parse(options.body), ["red","red","red","red"])) });
  await assert.rejects(verifier.verifyImageInput(connection("responses"), { apiKey: "fixture-secret" }), error => {
    assert.deepEqual(error.records.map(record => [record.capability, record.status]), [[CAPABILITIES.IMAGE_INPUT, "failed"]]);
    assert.match(error.records[0].sample.message, /image pixels/); return true;
  });
});

test("image qualification refuses an unexecutable protocol before transport and redacts access failures", async () => {
  let calls = 0;
  const verifier = new CapabilityVerifier({ fetchImpl: async () => { calls++; return jsonResponse(401, { error: { message: "rejected fixture-secret" } }); } });
  await assert.rejects(verifier.verifyImageInput(connection("chat-completions"), { apiKey: "fixture-secret" }), /executable Responses/); assert.equal(calls, 0);
  await assert.rejects(verifier.verifyImageInput(connection("responses"), { apiKey: "fixture-secret" }), error => { assert.equal(error.attempts[0].category, "authentication"); assert.doesNotMatch(JSON.stringify(error.records), /fixture-secret/); return true; });
  assert.equal(calls, 1);
});

test("image qualification keeps the same image when a route rejects forced tool choice", async () => {
  const requests=[];
  const verifier=new CapabilityVerifier({fetchImpl:async(_url,options)=>{
    const body=JSON.parse(options.body); requests.push(body);
    if (body.tool_choice) return jsonResponse(400,{error:{message:"unsupported tool_choice"}});
    return jsonResponse(200,toolOutput(body,readPixels(body.input[0].content.find(item=>item.type==="input_image"))));
  }});
  const result=await verifier.verifyImageInput(connection("responses"),{apiKey:"fixture-secret"});
  assert.equal(result.records[0].status,"verified"); assert.equal(requests.length,3);
  assert.equal(requests[0].input[0].content[1].image_url,requests[1].input[0].content[1].image_url);
  assert.equal(requests[1].tool_choice,undefined); assert.equal(requests[2].tool_choice,undefined);
});
