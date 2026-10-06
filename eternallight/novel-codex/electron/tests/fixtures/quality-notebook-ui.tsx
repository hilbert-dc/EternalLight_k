import React from "react";
import { createRoot } from "react-dom/client";
import { QualityNotebook } from "../../../src/QualityNotebook";
import type { NovelCodexApi } from "../../../src/types";
import "../../../src/styles.css";
import "../../../src/project-information.css";
const host=window as typeof window & {testResult?:unknown};
const notes:any[]=[{id:"trial-1",ordinal:2,note:{kind:"trial",scope:"work",text:"克制的关心",evidence:[{quote:"伞留在门边",revisionId:"draft",role:"current"}],body:"她关上门。伞留在门边。",question:"如何表达关心"},status:"proposed",sourceCurrent:true,authorKind:"model",decisions:[]},{id:"trial-2",ordinal:1,note:{kind:"trial",scope:"work",text:"换个观察位置",evidence:[{quote:"伞留在门边",revisionId:"draft",role:"current"}],body:"他回头，看见门边的伞。",question:"如何表达关心"},status:"confirmed",sourceCurrent:true,authorKind:"author",decisions:[]}];
const packets:any[]=[];let version=0,feedback:any[]=[],ran=0;const changes:any[]=[];
const api={runtime:{
 getCreativeWorkspace:async()=>({chapters:[{number:1,title:"雨",candidate:{id:"draft"}}]}),
 getQualityNotebook:async()=>({version,notes:structuredClone(notes),packets:structuredClone(packets),nextCursor:null}),
 changeQualityNotebook:async(_p:string,request:any)=>{changes.push(request);if(request.expectedVersion!==version)throw Error("版本已变化");version++;const c=request.change;
  if(c.action==="decide")notes.find(n=>n.id===c.id).status=c.status;
  if(c.action==="note")notes.push({id:"author-note",ordinal:version,note:c.note,status:"confirmed",sourceCurrent:true,authorKind:"author",decisions:[]});
  if(c.action==="packet")packets.push({id:"packet",kind:c.kind,labels:["A","B"],status:"prepared",sourceCurrent:true,hasReport:false,feedbackCount:0});
  if(c.action==="feedback"){feedback.push({...c,version});packets[0].feedbackCount++;}
  return {version,result:{id:"packet"}};},
 getQualityPacket:async()=>({kind:"comparison",items:[{label:"A",text:"她关上门。伞留在门边。"},{label:"B",text:"他回头，看见门边的伞。"}]}),
 getQualityReview:async()=>({sourceCurrent:true,calibration:{preferenceDisagreement:ran?true:null},report:ran?{report:{summary:"两个视角各有余味",preference:"A",limitations:"模型假设",findings:[{label:"A",quote:"伞留在门边",effect:"关心藏在动作里"}]}}:null,latestAuthorFeedback:feedback,feedbackCount:feedback.length}),
 reviewQualityPacket:async()=>{ran++;packets[0].hasReport=true;packets[0].status="completed";}
}} as unknown as NovelCodexApi;
createRoot(document.getElementById("root")!).render(<div className="project-information" style={{maxWidth:560,margin:"20px auto",padding:24}}><h2>创作回看</h2><QualityNotebook api={api} projectId="book"/></div>);
let stage="load";
const tick=()=>new Promise(r=>setTimeout(r,20));const until=async(fn:()=>unknown)=>{for(let i=0;i<100;i++){if(fn())return;await tick();}throw Error(`UI did not settle at ${stage}: ${document.body.textContent}`);};
const button=(text:string)=>Array.from(document.querySelectorAll("button")).find(b=>b.textContent===text)!;
function check(value:unknown,message:string){if(!value)throw Error(message);}
async function run(){
 await until(()=>document.querySelectorAll('input[type="checkbox"]').length===2);
 check(!document.body.textContent?.includes("文学评分"),"Unexpected literary scoring");
 stage="confirm";button("确认这项记录").click();await tick();await until(()=>changes.length===1&&!document.querySelector('[role="status"]'));
 stage="packet";document.querySelectorAll<HTMLInputElement>('input[type="checkbox"]').forEach(c=>c.click());await tick();button("匿名比较所选试写").click();await until(()=>document.querySelector(".quality-reader"));
 await tick();check(changes[1].change.sources.length===2,"Comparison missed a candidate");check(ran===0,"Preparing a packet consumed a model call");
 const reader=document.querySelector(".quality-reader")!;const area=reader.querySelector("textarea")!;
 stage="feedback";Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,"value")!.set!.call(area,"各有适用场景，保留分歧");area.dispatchEvent(new Event("input",{bubbles:true}));await tick();button("保存作者反馈").click();await until(()=>feedback.length===1);await tick();
 button("启动独立读者任务").click();await until(()=>ran===1&&packets[0].hasReport);await tick();
 check(document.body.textContent?.includes("模型阅读假设"),"Reader report not visible");
 check(document.body.textContent?.includes("作者与模型的选择不同"),"Disagreement was hidden");
 check(document.body.textContent?.includes("各有适用场景，保留分歧"),"Author feedback disappeared");
 button("撤回").click();await until(()=>notes[0].status==="withdrawn");await tick();
 check(document.body.textContent?.includes("已撤回"),"Withdrawn preference was hidden");
 stage="author preference";const form=document.querySelector(".quality-notebook form")!;
 (form.parentElement as HTMLDetailsElement).open=true;
 const preferenceText=form.querySelector("textarea")!;
 Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,"value")!.set!.call(preferenceText,"允许留白，不把每个动作都解释清楚。");preferenceText.dispatchEvent(new Event("input",{bubbles:true}));await tick();button("保存记录").click();await until(()=>notes.some(n=>n.id==="author-note"));await tick();
 check(changes.at(-1).change.note.kind==="preference","Author preference was not saved");
 (form.parentElement as HTMLDetailsElement).open=false;
 check(document.documentElement.scrollWidth<=innerWidth+1,"UI overflows viewport");
 host.testResult={passed:true,mutations:changes.map(c=>c.change.action),readerRuns:ran,feedbackKept:feedback.length};
}
void run().catch(e=>{host.testResult={passed:false,error:String(e.stack??e)};});
