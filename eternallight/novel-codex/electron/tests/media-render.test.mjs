import test from "node:test";
import assert from "node:assert/strict";
import {mkdtemp,rm,writeFile,readFile} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join} from "node:path";
import {execFile} from "node:child_process";
import {promisify} from "node:util";
import {renderComicDocument,renderAnimationDocument} from "../media-render.mjs";
import {importMediaBytes,resolveMediaBlob,probeMediaFile} from "../media-files.mjs";

const run=promisify(execFile), time=value=>({value,rate:{numerator:1000,denominator:1}}), rect={x:0,y:0,width:100,height:100};
const page=id=>({id,width:200,height:300,panels:[{id:"panel",rect,assetRevisionId:"asset",balloons:[{kind:"speech",rect,text:'<script>alert("x")</script>'}]}]});
test("comic PDF is one multipage document, PNGs are separate, and untrusted captions cannot execute",async()=>{
  const calls=[];const input={document:{kind:"comic",pages:[page("one"),page("two")]},loadAsset:async()=>({bytes:Buffer.from("image"),mimeType:"image/png"}),renderHtml:async options=>{calls.push(options);return Buffer.from("rendered");}};
  const pdf=await renderComicDocument({...input,format:"pdf"});assert.equal(pdf.length,1);assert.equal(calls.length,1);assert.equal(calls[0].html.match(/<section/g).length,2);assert.ok(!calls[0].html.includes('<script>'));assert.ok(calls[0].html.includes("&lt;script&gt;"));
  calls.length=0;assert.equal((await renderComicDocument({...input,format:"png"})).length,2);assert.equal(calls.length,2);
  await assert.rejects(renderComicDocument({...input,format:"pdf",document:{kind:"comic",pages:[page("one"),{...page("two"),height:500}]}}),/统一页面尺寸/);
});
test("media blobs validate signatures, keep immutable content, and detect later tampering",async t=>{
  const directory=await mkdtemp(join(tmpdir(),"media-files-"));t.after(()=>rm(directory,{recursive:true,force:true}));
  const png=Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aL1sAAAAASUVORK5CYII=","base64");
  const metadata=await importMediaBytes({bytes:png,name:"test.png",rootDirectory:directory,decodeImage:()=>({width:1,height:1})});assert.equal(metadata.kind,"image");const path=await resolveMediaBlob({rootDirectory:directory,digest:metadata.digest});
  assert.deepEqual(await readFile(path),png);await assert.rejects(importMediaBytes({bytes:Buffer.from('<svg onload="bad"/>'),name:"test.svg",rootDirectory:directory}),/不支持/);
  await writeFile(path,"tampered");await assert.rejects(resolveMediaBlob({rootDirectory:directory,digest:metadata.digest}),/完整性/);
});
test("real ffmpeg renders still image and audio clips at their typed timeline offsets",{skip:!process.env.NOVEL_CODEX_FFMPEG||!process.env.NOVEL_CODEX_FFPROBE},async t=>{
  const directory=await mkdtemp(join(tmpdir(),"media-render-"));t.after(()=>rm(directory,{recursive:true,force:true}));const ffmpegPath=process.env.NOVEL_CODEX_FFMPEG,ffprobePath=process.env.NOVEL_CODEX_FFPROBE;
  const image=join(directory,"red.png"),audio=join(directory,"audio.wav");
  await run(ffmpegPath,["-v","error","-f","lavfi","-i","color=c=red:s=64x64","-frames:v","1","-y",image]);await run(ffmpegPath,["-v","error","-f","lavfi","-i","sine=frequency=500:duration=1","-y",audio]);
  const clip=(id,assetRevisionId,start,duration)=>({id,assetRevisionId,shotId:null,start:time(start),duration:time(duration),sourceStart:time(0),playbackRate:{numerator:1,denominator:1},caption:""});
  const document={kind:"animation",production:{form:"shortFilm",expression:"hybrid",deliverable:"finishedVideo",targetDuration:time(1000)},width:64,height:64,frameRate:{numerator:24,denominator:1},shots:[],timeline:{tracks:[{kind:"video",muted:false,clips:[clip("visual","image",0,1000)]},{kind:"audio",muted:false,clips:[clip("sound","audio",250,500)]}]}};
  const output=await renderAnimationDocument({document,ffmpegPath,ffprobePath,temporaryDirectory:directory,resolveAsset:async id=>id==="image"?{path:image,kind:"image",mimeType:"image/png"}:{path:audio,kind:"audio",mimeType:"audio/wav",duration:time(1000)}});
  assert.equal(output.metadata.width,64);assert.equal(output.metadata.channels,1);assert.equal(output.metadata.duration.value,1_000_000);const probed=await probeMediaFile(output.path,{ffprobePath});assert.equal(probed.kind,"video");await output.cleanup();
});

test("animation export rejects a relative subtitle font before any rendering", async () => {
  await assert.rejects(renderAnimationDocument({ document: { kind: "animation" }, ffmpegPath: "/explicit/ffmpeg", ffprobePath: "/explicit/ffprobe", fontPath: "relative-font.ttf" }), /绝对路径/);
});


test("a short timeline cannot satisfy a declared feature-film delivery duration", async () => {
  const document = { kind: "animation", width:64,height:64,frameRate:{numerator:24,denominator:1},shots:[],
    production:{form:"featureFilm",expression:"liveAction",deliverable:"finishedVideo",targetDuration:time(5400000)},
    timeline:{tracks:[{kind:"video",muted:false,clips:[{id:"clip",assetRevisionId:"asset",start:time(0),duration:time(1000),sourceStart:time(0),playbackRate:{numerator:1,denominator:1},caption:""}]}]} };
  let readAsset=false;
  await assert.rejects(renderAnimationDocument({document,ffmpegPath:"/explicit/ffmpeg",ffprobePath:"/explicit/ffprobe",resolveAsset:async()=>{readAsset=true;}}), /目标成片时长/);
  assert.equal(readAsset,false,"reject before any asset IO or renderer invocation");
});
