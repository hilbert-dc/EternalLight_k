import test from "node:test";
import assert from "node:assert/strict";
import {createOpenAIImagesAdapter,createOpenAIAudioAdapter} from "../src/media/openai.mjs";
import {createRunwayAdapter} from "../src/media/runway.mjs";
import {downloadMedia} from "../src/media/http.mjs";

const options={id:"service",model:"gpt-image-1.5",baseUrl:"https://api.openai.com/v1",apiKey:"mock-credential"};
test("OpenAI edit resolves only managed assets and maps current image edit contract",async()=>{
  const calls=[];const adapter=createOpenAIImagesAdapter({...options,loadAsset:async id=>{assert.equal(id,"asset");return {mimeType:"image/png",bytes:Buffer.from("managed-image")};},fetchImpl:async(url,request)=>{calls.push({url,body:JSON.parse(request.body)});assert.equal(request.redirect,"error");return Response.json({data:[{b64_json:Buffer.from("output").toString("base64")}]},{headers:{"x-request-id":"receipt"}});}});
  const result=await adapter.submit({prompt:"edit",imageAssetRevisionIds:["asset"]},{kind:"image.edit",operationId:"op"});
  assert.equal(calls[0].url,"https://api.openai.com/v1/images/edits");assert.equal(calls[0].body.images[0].image_url,"data:image/png;base64,bWFuYWdlZC1pbWFnZQ==");assert.equal(result.receipt.id,"receipt");assert.equal((await adapter.fetchOutputs(result))[0].bytes.toString(),"output");
  assert.deepEqual(await adapter.lookup({idempotencyKey:"op"}),{state:"outcome_unknown"});
  await assert.rejects(adapter.validate({prompt:"x",apiKey:"injected"},"image.generate"),/未支持/);
});
test("speech and transcription retain source identity and produce distinct output types",async()=>{
  const speech=createOpenAIAudioAdapter({...options,model:"gpt-4o-mini-tts",fetchImpl:async(url,request)=>{assert.equal(url,"https://api.openai.com/v1/audio/speech");assert.equal(JSON.parse(request.body).input,"hello");return new Response(Buffer.from("RIFF-audio"),{headers:{"content-type":"audio/wav"}});}});
  const output=await speech.submit({text:"hello",voice:"alloy"},{kind:"audio.speech",operationId:"op"});assert.equal(output.outputs[0].name,"speech.wav");
  const transcribe=createOpenAIAudioAdapter({...options,model:"gpt-4o-transcribe",loadAsset:async()=>({name:"input.wav",mimeType:"audio/wav",bytes:Buffer.from("sound")}),fetchImpl:async(url,request)=>{assert.equal(url,"https://api.openai.com/v1/audio/transcriptions");assert.ok(request.body instanceof FormData);assert.equal(request.body.get("model"),"gpt-4o-transcribe");return Response.json({text:"transcript"});}});
  const result=await transcribe.submit({audioAssetRevisionId:"audio-original"},{kind:"audio.transcribe",operationId:"op"});assert.deepEqual(result.outputs,[{kind:"transcript",text:"transcript",sourceAssetRevisionId:"audio-original"}]);
});
test("Runway maps async submit/poll/output and only cancels an active task",async()=>{
  const calls=[];let complete=false;
  const adapter=createRunwayAdapter({...options,model:"gen4.5",baseUrl:"https://api.dev.runwayml.com",fetchImpl:async(url,request)=>{
    calls.push({url:String(url),method:request.method??"GET"});
    if(String(url).includes("cloudfront.net"))return new Response(Buffer.from("video"),{headers:{"content-type":"video/mp4"}});
    assert.equal(request.headers["X-Runway-Version"],"2024-11-06");
    if(request.method==="POST")return Response.json({id:"job-1"});
    if(request.method==="DELETE")return new Response(null,{status:204});
    return Response.json({id:"job-1",status:complete?"SUCCEEDED":"RUNNING",output:complete?["https://example.cloudfront.net/video.mp4"]:undefined,cost:{credits:12}});
  }});
  const submitted=await adapter.submit({prompt:"a landscape",duration:3,ratio:"1280:720"},{kind:"video.generate"});assert.equal(submitted.jobId,"job-1");assert.equal(calls[0].url,"https://api.dev.runwayml.com/v1/text_to_video");
  assert.equal((await adapter.poll("job-1")).state,"running");assert.equal((await adapter.cancel("job-1")).state,"cancelled");assert.equal(calls.filter(call=>call.method==="DELETE").length,1);
  complete=true;const finished=await adapter.cancel("job-1");assert.equal(finished.state,"collecting");assert.equal(calls.filter(call=>call.method==="DELETE").length,1,"never delete a completed provider asset");assert.equal((await adapter.fetchOutputs(finished))[0].bytes.toString(),"video");
  assert.deepEqual(await adapter.lookup({jobId:null}),{state:"outcome_unknown"});
  const before=calls.length;await assert.rejects(adapter.fetchOutputs({...finished,outputUrls:["https://example.cloudfront.net/a.mp4","https://example.cloudfront.net/b.mp4"]}),/单个/);assert.equal(calls.length,before,"unexpected batches cannot expand the one-video grant");
});
test("unavailable models and unsafe output URLs fail honestly without fetching",async()=>{
  assert.equal(createRunwayAdapter({...options,model:"retired-model"}).inspect().capabilities[0].available,false);
  let fetched=false;await assert.rejects(downloadMedia("http://127.0.0.1/private",{allowedHosts:["cloudfront.net"],fetchImpl:async()=>{fetched=true;}}),/地址/);assert.equal(fetched,false);
  const adapter=createOpenAIImagesAdapter({...options,fetchImpl:async()=>new Response("secret provider body",{status:401})});
  await assert.rejects(adapter.submit({prompt:"private"},{kind:"image.generate",operationId:"op"}),error=>error.code==="MEDIA_HTTP_401"&&!error.message.includes("secret"));
});
test("transport timeout is uncertain and never exposes provider details",async()=>{
  const adapter=createOpenAIImagesAdapter({...options,timeoutMs:5,fetchImpl:async(_url,{signal})=>new Promise((resolve,reject)=>{signal.addEventListener("abort",()=>reject(Error("sensitive transport detail")),{once:true});})});
  // Keep the test event loop alive; production Host already owns its lifecycle.
  const hold=setTimeout(()=>{},1000);
  try{await assert.rejects(adapter.submit({prompt:"private"},{kind:"image.generate",operationId:"op"}),error=>error.code==="MEDIA_TRANSPORT_UNCERTAIN"&&!error.message.includes("sensitive"));}finally{clearTimeout(hold);}
});
