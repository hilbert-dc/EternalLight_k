import test from "node:test";
import assert from "node:assert/strict";
import {mkdtemp,readFile,writeFile,rm} from "node:fs/promises";
import {randomBytes,createCipheriv,createDecipheriv} from "node:crypto";
import {tmpdir} from "node:os";
import {join} from "node:path";
import {MediaConnectionStore,resolveMediaExecutables} from "../media-host.mjs";

async function fixture(t){const directory=await mkdtemp(join(tmpdir(),"media-connections-"));t.after(()=>rm(directory,{recursive:true,force:true}));const salt=join(directory,"salt");await writeFile(salt,randomBytes(32).toString("base64url"));const key=randomBytes(32);
  const encryption={isEncryptionAvailable:()=>true,encryptString:text=>{const iv=randomBytes(12),cipher=createCipheriv("aes-256-gcm",key,iv),body=Buffer.concat([cipher.update(text,"utf8"),cipher.final()]);return Buffer.concat([iv,cipher.getAuthTag(),body]);},decryptString:bytes=>{const decipher=createDecipheriv("aes-256-gcm",key,bytes.subarray(0,12));decipher.setAuthTag(bytes.subarray(12,28));return Buffer.concat([decipher.update(bytes.subarray(28)),decipher.final()]).toString("utf8");}};
  return {directory,salt,encryption,store:new MediaConnectionStore({directory,encryption,credentialSaltPath:salt})};}
const connection={id:"images",title:"Images",protocol:"openai-images",baseUrl:"https://api.openai.com/v1",model:"gpt-image-1.5"};
test("connection storage keeps credentials encrypted and stable realms survive reopening",async t=>{
  const {store,directory,encryption,salt}=await fixture(t);const first=await store.save({...connection,apiKey:"fixture-sensitive-key-one"});assert.equal(first.credentialConfigured,true);assert.ok(!JSON.stringify(first).includes("sensitive"));
  const publicList=await store.list();assert.ok(!JSON.stringify(publicList).includes("encryptedCredential"));assert.ok(!JSON.stringify(publicList).includes("apiKey"));
  const bytes=await readFile(join(directory,"media-services.json"),"utf8");assert.ok(!bytes.includes("fixture-sensitive-key-one"));
  const resolved=await store.resolve("images","image.generate");assert.match(resolved.credentialRealmId,/^[a-f0-9]{64}$/);
  const reopened=new MediaConnectionStore({directory,encryption,credentialSaltPath:salt});assert.equal((await reopened.resolve("images","image.generate")).credentialRealmId,resolved.credentialRealmId);
  await reopened.save({...connection,title:"Renamed"});assert.equal((await reopened.resolve("images","image.edit")).credentialRealmId,resolved.credentialRealmId);
  await reopened.save({...connection,id:"second-images",apiKey:"fixture-sensitive-key-one"});assert.equal((await reopened.resolve("second-images","image.generate")).credentialRealmId,resolved.credentialRealmId);
  await reopened.save({...connection,id:"same-account-audio",protocol:"openai-audio",model:"gpt-4o-mini-tts",apiKey:"fixture-sensitive-key-one"});assert.equal((await reopened.resolve("same-account-audio","audio.speech")).credentialRealmId,resolved.credentialRealmId);
  await reopened.save({...connection,apiKey:"fixture-sensitive-key-two"});assert.notEqual((await reopened.resolve("images","image.generate")).credentialRealmId,resolved.credentialRealmId);
});
test("endpoint changes require an explicit credential and unavailable capabilities stay unavailable",async t=>{
  const {store,encryption}=await fixture(t);await store.save({...connection,apiKey:"fixture-key"});
  const changed=await store.save({...connection,baseUrl:"https://another.example/v1"});assert.equal(changed.credentialConfigured,false);await assert.rejects(store.resolve("images","image.generate"),/不支持|密钥/);
  const speech=await store.save({id:"audio",title:"Speech",protocol:"openai-audio",baseUrl:"https://api.openai.com/v1",model:"gpt-4o-mini-tts",apiKey:"fixture-key"});assert.equal(speech.capabilities.find(item=>item.kind==="audio.speech").available,true);assert.equal(speech.capabilities.find(item=>item.kind==="audio.transcribe").available,false);
  encryption.isEncryptionAvailable=()=>false;await assert.rejects(store.save({...connection,apiKey:"new-key"}),/系统密钥存储不可用/);assert.ok((await store.list()).capabilities.every(item=>!item.available));
});
test("packaged rendering uses its fixed runtime and development never searches PATH",()=>{
  assert.deepEqual(resolveMediaExecutables({packaged:false,resourcesPath:"/ignored",environment:{PATH:"/untrusted"}}),{ffmpegPath:null,ffprobePath:null,fontPath:null});
  assert.throws(()=>resolveMediaExecutables({packaged:false,resourcesPath:"/ignored",environment:{NOVEL_CODEX_FFMPEG:"ffmpeg"}}),/绝对路径/);
  const resolved=resolveMediaExecutables({packaged:true,resourcesPath:"/resources",environment:{NOVEL_CODEX_FFMPEG:"/untrusted/ffmpeg"}});assert.equal(resolved.ffmpegPath,join("/resources","media-runtime",process.platform==="win32"?"ffmpeg.exe":"ffmpeg"));
});
