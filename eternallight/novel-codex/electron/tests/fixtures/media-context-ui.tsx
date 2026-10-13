import React from "react";
import { createRoot } from "react-dom/client";
import { MultimediaWorkspaceView } from "../../../src/multimedia/MultimediaWorkspaceView";
import "../../../src/styles.css";
const wait = () => new Promise(resolve => setTimeout(resolve, 15));
const check = (value, message) => { if (!value) throw Error(message); };
const until = async (predicate, message) => { for (let i=0;i<150&&!predicate();i++) await wait(); check(predicate(),message); };
const clone = value => structuredClone(value);
const source = { projectId:"project",scopeId:"scope",branchId:"branch",objectId:"edition",revisionId:"revision-1",digest:"a".repeat(64),selector:{kind:"whole"} };
const documentValue = { kind:"comic",title:"独立漫画",format:"pages",pages:[] };
let workspace = {project:{id:"project",title:"漫画",logline:"",updated_at:1},edition:{id:"edition",editionId:"edition",projectId:"project",scopeId:"scope",branchId:"branch",medium:"comic",title:"漫画",canonSnapshotId:"canon-old",sources:[]},revision:1,revisionId:"revision-1",documentSource:source,documentEncoding:JSON.stringify(documentValue),headSnapshotId:"head-1",sourceDigest:"b".repeat(64),document:documentValue,assets:[],sources:[]};
let canon = {projectId:"project",scopeId:"canon",branchId:"canon-branch",snapshotId:"canon-new",revisionId:"canon-v1",documentSource:null,document:{title:"共同设定",entries:[]}};
let adoptions=[], saves=[], failAdopt=true, failCanon=true;
const frozen = () => ({...clone(canon),snapshotId:workspace.edition.canonSnapshotId,document:workspace.edition.canonSnapshotId==="canon-old"?{title:"共同设定",entries:[]}:clone(canon.document)});
const api = {
  readWorkspace:async()=>clone(workspace), readCanon:async(_projectId,snapshot)=>snapshot?frozen():clone(canon),
  sourceImpacts:async()=>({projectId:"project",editionId:"edition",branchId:"branch",revisionId:workspace.revisionId,items:[{source:{...source,selector:{kind:"text",startByte:0,endByte:6,quote:"雨夜"}},latest:null,status:"changed",targets:[{objectId:"edition",selector:{kind:"whole"},purpose:"visual",decision:"沿用角色造型，调整叙事顺序"}]}],offset:0,total:1,nextOffset:null,canon:{pinnedSnapshotId:workspace.edition.canonSnapshotId,currentSnapshotId:canon.snapshotId,changed:workspace.edition.canonSnapshotId!==canon.snapshotId}}),
  patchDocument:async(_p,_e,request)=>{adoptions.push(clone(request));check(request.expectedRevisionId===workspace.revisionId,"Adopt CAS lost");if(failAdopt){failAdopt=false;throw Error("设定保存连接中断。");}workspace={...workspace,revisionId:"revision-2",edition:{...workspace.edition,canonSnapshotId:request.operations[0].canonSnapshotId},documentSource:{...source,revisionId:"revision-2"}};return clone(workspace);},
  saveCanon:async(_p,request)=>{saves.push(clone(request));if(failCanon){failCanon=false;throw Error("共同设定保存连接中断。");}check(request.expectedRevisionId===canon.revisionId,"Canon CAS lost");canon={...canon,revisionId:"canon-v2",snapshotId:"canon-newer",document:clone(request.document)};return clone(canon);},
};
const root=createRoot(document.getElementById("root"));
root.render(<MultimediaWorkspaceView api={api as any} authorKey="author" projectId="project" editionId="edition" />);
const button=label=>[...document.querySelectorAll<HTMLButtonElement>("button")].find(item=>item.textContent===label);
const click=async element=>{check(element&&!element.disabled,"Unavailable control");element.click();await wait();};
const change=async(element,value)=>{check(element,"Missing input");const proto=element instanceof HTMLTextAreaElement?HTMLTextAreaElement.prototype:HTMLInputElement.prototype;Object.getOwnPropertyDescriptor(proto,"value")!.set!.call(element,value);element.dispatchEvent(new Event("input",{bubbles:true}));await wait();};
window.addEventListener("error",event=>{(window as any).testResult={passed:false,error:event.error?.stack??event.message};});
(async()=>{try{
  await until(()=>button("设定与来源"),"Editor not loaded");check(!document.body.textContent!.includes("引用内容已更新"),"Source details forced into reading flow");
  await click(button("设定与来源"));await until(()=>button("使用最新设定"),"Canon change not shown");check(document.body.textContent!.includes("引用内容已更新"),"Meaningful source change missing");check(document.body.textContent!.includes("视觉参考：")&&document.body.textContent!.includes("沿用角色造型，调整叙事顺序")&&document.body.textContent!.includes("不代表作者已批准"),"Adaptation purpose and decision must remain distinct from author adoption");
  await click(button("使用最新设定"));await until(()=>document.body.textContent!.includes("设定保存连接中断"),"Failed adopt hidden");check(workspace.revisionId==="revision-1"&&workspace.edition.canonSnapshotId==="canon-old","Failed adopt changed fixed work");
  await click(button("使用最新设定"));await until(()=>workspace.revisionId==="revision-2"&&!button("使用最新设定"),"Adopt success not refreshed");check(adoptions[0].idempotencyKey===adoptions[1].idempotencyKey,"Uncertain adopt retried as new operation");
  check(workspace.document===documentValue,"Canon adoption rewrote manuscript");
  await click(button("编辑共同设定"));await until(()=>button("＋ 添加设定"),"Canon editor missing");await click(button("＋ 添加设定"));await change(document.querySelector<HTMLTextAreaElement>(".mm-canon-dialog textarea"),"灯不会自行熄灭。");await click(button("关联当前保存内容"));
  await click(button("保存共同设定"));await until(()=>document.querySelector(".mm-canon-dialog [role='alert']"),"Failed canonical save hidden");check(document.querySelector<HTMLTextAreaElement>(".mm-canon-dialog textarea")?.value==="灯不会自行熄灭。","Canonical failure lost author edit");
  await click([...document.querySelectorAll<HTMLButtonElement>(".mm-canon-dialog button")].find(item=>item.textContent==="关闭"));await click(button("编辑共同设定"));await until(()=>document.querySelector<HTMLTextAreaElement>(".mm-canon-dialog textarea")?.value==="灯不会自行熄灭。","Canonical draft not recovered");
  await click(button("保存共同设定"));await until(()=>canon.revisionId==="canon-v2","Recovered canonical save failed");check(saves[0].idempotencyKey===saves[1].idempotencyKey,"Canonical uncertain save changed operation key");check(canon.document.entries[0].sources[0].revisionId==="revision-2","Canonical citation used wrong revision");check(workspace.edition.canonSnapshotId==="canon-new","Canonical save silently rebound edition");
  await click([...document.querySelectorAll<HTMLButtonElement>(".mm-canon-dialog button")].find(item=>item.textContent==="关闭"));await until(()=>button("使用最新设定"),"Saved shared canon did not expose explicit choice");
  (window as any).testResult={passed:true,adoptions:adoptions.length,canonSaves:saves.length,modelCalls:0};
}catch(error){(window as any).testResult={passed:false,error:error.stack??String(error),dom:document.body.textContent?.slice(-6000)};}})();
