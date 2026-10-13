import React from "react";
import { createRoot } from "react-dom/client";
import { MultimediaWorkspaceView } from "../../../src/multimedia/MultimediaWorkspaceView";
import "../../../src/styles.css";
const tick=()=>new Promise(r=>setTimeout(r,20));
const check=(v,m)=>{if(!v)throw Error(m)};
const until=async(f,m)=>{for(let i=0;i<120&&!f();i++)await tick();check(f(),m)};
const previous={projectId:"project",scopeId:"original-scope",branchId:"original-branch",objectId:"original",revisionId:"original-old",digest:"a".repeat(64),selector:{kind:"whole"}};
const next={...previous,revisionId:"original-new",digest:"b".repeat(64)};
const documentValue={kind:"comic",title:"保留的改编",format:"pages",pages:[]};
const own={projectId:"project",scopeId:"adaptation-scope",branchId:"adaptation-branch",objectId:"adaptation",revisionId:"adaptation-old",digest:"c".repeat(64),selector:{kind:"whole"}};
const link={source:previous,targetObjectId:"adaptation",targetSelector:{kind:"whole"},relation:"adapts",purpose:"plot",decision:"原版剧情对应关系"};
let workspace={project:{id:"project",title:"项目",logline:"",updated_at:1},edition:{id:"adaptation",editionId:"adaptation",projectId:"project",scopeId:own.scopeId,branchId:own.branchId,medium:"comic",title:"改编",canonSnapshotId:"canon-fixed",sources:[previous]},revision:1,revisionId:own.revisionId,documentSource:own,headSnapshotId:"head",sourceDigest:"d".repeat(64),document:documentValue,documentEncoding:JSON.stringify(documentValue),assets:[],sources:[link]};
const prompts=[];const requests=[];let receipt=null;const canonRequests=[];let canonReceipt=null;let corrupt=value=>value;
const api={
 readWorkspace:async()=>structuredClone(workspace),
 readCanon:async(_p,snapshotId)=>({projectId:"project",snapshotId,document:{entries:[]}}),
 sourceImpacts:async(_p,_e,offset=0)=>({projectId:"project",editionId:"adaptation",branchId:own.branchId,revisionId:workspace.revisionId,items:offset ? [{source:{...previous,objectId:"another-source"},status:"removed",targets:[]}] : [{source:previous,latest:next,status:"changed",targets:[{objectId:"adaptation",selector:{kind:"whole"},purpose:"plot",decision:"原版剧情对应关系"}]}],offset,total:2,nextOffset:offset ? null : 1,canon:{pinnedSnapshotId:workspace.edition.canonSnapshotId,currentSnapshotId:"canon-new",changed:workspace.edition.canonSnapshotId!=="canon-new"}}),
 patchDocument:async(p,e,request)=>{
   if(request.operations[0].kind==="adoptCanonSnapshot"){
     canonRequests.push(structuredClone(request));
     if(canonReceipt){check(JSON.stringify(canonRequests[0])===JSON.stringify(request),"Canon retry changed original request");return corrupt(structuredClone(canonReceipt));}
     workspace={...workspace,revisionId:"canon-adopted",edition:{...workspace.edition,canonSnapshotId:"canon-new"}};canonReceipt=structuredClone(workspace);return corrupt(structuredClone(canonReceipt));
   }
   requests.push(structuredClone(request));check(p==="project"&&e==="adaptation"&&request.operations.length===1&&request.operations[0].kind==="replaceEditionSource","Wrong domain operation");
   if(receipt){check(JSON.stringify(requests[0])===JSON.stringify(request),"Unknown save retry changed identity");return corrupt(structuredClone(receipt))}
   check(request.expectedRevisionId===workspace.revisionId&&JSON.stringify(request.operations[0].previous)===JSON.stringify(previous)&&JSON.stringify(request.operations[0].next)===JSON.stringify(next),"Lost exact source baseline or CAS");
   workspace={...workspace,revisionId:"adaptation-new",headSnapshotId:"head-new",documentSource:{...own,revisionId:"adaptation-new"},edition:{...workspace.edition,sources:[next]}};receipt=structuredClone(workspace);throw Error("来源切换回执暂不可用");
 }
};
const root=createRoot(document.getElementById("root"));root.render(<MultimediaWorkspaceView api={api as never} onDiscuss={prompt=>prompts.push(prompt)} authorKey="source-author" projectId="project" editionId="adaptation" />);
const button=t=>[...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent===t);
(async()=>{try{
 await until(()=>button("设定与来源 · 1 处改编来源"),"Source panel missing");button("设定与来源 · 1 处改编来源").click();
 await until(()=>button("后续改编使用这个新版"),"Source change has no update action");button("后续改编使用这个新版").click();
 await until(()=>document.body.textContent.includes("来源切换回执暂不可用"),"Unknown update was hidden");
 const badReceipts=[value=>({...value,edition:{...value.edition,scopeId:"foreign"}}),value=>({...value,edition:{...value.edition,branchId:"foreign"}}),value=>({...value,document:{...value.document,creativeDesign:"不应进入的设计"}}),value=>({...value,sources:[]}),value=>({...value,edition:{...value.edition,canonSnapshotId:"unasked-canon"}})];
 for(const mutation of badReceipts){corrupt=mutation;const before=requests.length;button("后续改编使用这个新版").click();await tick();await until(()=>requests.length===before+1&&!button("后续改编使用这个新版").disabled,"Bad source receipt did not return");check(!document.body.textContent.includes("尚未重新生成")&&!document.body.textContent.includes("不应进入的设计"),"Foreign receipt replaced current projection");}
 corrupt=value=>({...value,document:Object.fromEntries(Object.entries(value.document).reverse()),sources:value.sources.map(link=>Object.fromEntries(Object.entries(link).reverse()))});
 check(button("后续改编使用这个新版"),"Retry action lost");button("后续改编使用这个新版").click();
 await until(()=>document.body.textContent.includes("尚未重新生成")&&!button("后续改编使用这个新版"),"Updated baseline not projected");
 check(document.body.textContent.includes("原版剧情对应关系")&&document.body.textContent.includes("引用内容已更新"),"Source adoption pretended to update old mappings");
 check(requests.length===7&&JSON.stringify(workspace.document)===JSON.stringify(documentValue)&&workspace.sources[0]===link,"Source update rewrote adaptation");
 button("查看更多来源").click();await until(()=>document.body.textContent.includes("原内容已移除"),"Remaining source page unavailable");
 check([...document.querySelectorAll("button")].filter(b=>b.textContent==="在对话中分析变化").length===1,"Repeated source discussion actions");button("在对话中分析变化").click();check(prompts.length===1&&prompts[0].includes("adaptation-new")&&prompts[0].includes("完整来源变化报告")&&prompts[0].includes("先讨论，不修改作品"),"Discussion lost source identity or expanded authority");
 const badCanon=[value=>({...value,edition:{...value.edition,scopeId:"foreign"}}),value=>({...value,edition:{...value.edition,branchId:"foreign"}}),value=>({...value,document:{...value.document,creativeDesign:"不应进入的设计"}}),value=>({...value,sources:[]}),value=>({...value,edition:{...value.edition,sources:[previous]}})];
 for(const mutation of badCanon){corrupt=mutation;const before=canonRequests.length;button("使用最新设定").click();await tick();await until(()=>canonRequests.length===before+1&&!button("使用最新设定").disabled,"Bad canon receipt did not return");check(!document.body.textContent.includes("已保存使用新设定")&&!document.body.textContent.includes("不应进入的设计"),"Bad canon receipt changed projection");}
 corrupt=value=>value;button("使用最新设定").click();await until(()=>document.body.textContent.includes("已保存使用新设定")&&!button("使用最新设定"),"Original canon receipt did not recover");check(canonRequests.length===6,"Canon retry count mismatch");
 window.testScreenshot="source-replacement";for(let i=0;i<100&&window.testScreenshot;i++)await tick();
 root.unmount();window.testResult={passed:true};
}catch(e){window.testResult={passed:false,error:e.stack,dom:document.body.textContent};}})();
