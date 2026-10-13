import "../../../src/styles.css";
import React, { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { CreateMediaEditionDialog } from "../../../src/multimedia/CreateMediaEditionDialog";
const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (v, message) => { if (!v) throw Error(message); };
const until = async (predicate, message) => { for (let n=0;n<100 && !predicate();n++) await tick(); check(predicate(),message); };
const button = label => [...document.querySelectorAll<HTMLButtonElement>("button")].find(b => b.textContent === label);
const select = async (node,value) => { Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,"value").set.call(node,value);node.dispatchEvent(new Event("change",{bubbles:true}));await tick(); };
const ref = (edition, revision) => ({projectId:"project",scopeId:`scope-${edition}`,branchId:`branch-${edition}`,objectId:edition === "novel" ? `chapter-${revision}` : edition,revisionId:revision,digest:"a".repeat(64),selector:{kind:"whole"}});
const editions = ["novel","comic"].map(medium => ({id:medium,editionId:medium,projectId:"project",scopeId:`scope-${medium}`,branchId:`branch-${medium}`,medium,title:medium === "novel" ? "原作" : "漫画",sources:[],canonSnapshotId:"canon"}));
const reads=[], creates=[];let opens=0,closed=false, newer=false;
const api = {
 listEditions:async()=>({editions}),
 sourceCatalog:async(projectId,editionId,snapshotId,offset=0)=>{
   reads.push({editionId,snapshotId,offset});
   if(offset) check(snapshotId === "novel-snapshot","Continuation changed fixed source snapshot");
   const revision=editionId === "comic" ? (newer ? "comic-new" : "comic-old") : offset ? "chapter-two" : "chapter-one";
   return {projectId,editionId,branchId:`branch-${editionId}`,snapshotId:snapshotId ?? `${editionId}-snapshot`,offset,total:editionId === "novel" ? 2 : 1,nextOffset:editionId === "novel" && !offset ? 1 : null,items:[{source:ref(editionId,revision),title:revision,kind:editionId === "novel" ? "draft" : "comic"}]};
 },
 createEdition:async(projectId,request)=>{
   creates.push(structuredClone(request));
   check(request.sources.length===3,"Combined inputs lost");
   check(request.sources[0].revisionId==="chapter-one" && request.sources[1].revisionId==="chapter-two" && request.sources[2].revisionId==="comic-old","Selected source was silently replaced");
   if(creates.length===1){newer=true;throw Error("建立回执暂不可用");}
   check(JSON.stringify(creates[0])===JSON.stringify(request),"Retry changed source or idempotency identity");
   return {...editions[1],id:"result",editionId:"result",medium:request.medium,sources:request.sources.map(source => Object.fromEntries(Object.entries(source).reverse()))};
 }
};
const root=createRoot(document.getElementById("root"));
const render = (authorKey="adaptation-author",projectId="project") => root.render(<StrictMode><CreateMediaEditionDialog authorKey={authorKey} api={api as never} projectId={projectId} source={null} onClose={()=>{closed=true}} onCreated={async()=>{opens++;if(opens===1)throw Error("打开失败，版本已建立");}} /></StrictMode>);
render();
(async()=>{try{
 await until(()=>document.querySelector('.adaptation-source-picker input'),"Novel sources missing");
 document.querySelector<HTMLInputElement>('.adaptation-source-picker input').click();await tick();
 button("更多已保存内容").click();await until(()=>document.querySelectorAll('.adaptation-source-picker input').length===2,"Second source page missing");
 document.querySelectorAll<HTMLInputElement>('.adaptation-source-picker input')[1].click();await tick();
 await select(document.querySelector('[aria-label="来源作品版本"]'),"comic");
 await until(()=>document.querySelector('.adaptation-source-picker input')?.parentElement?.textContent.includes("comic-old"),"Comic sources missing");
 document.querySelector<HTMLInputElement>('.adaptation-source-picker input').click();await tick();
 check(document.querySelector('[aria-label="已选改编来源"]')?.textContent.includes("已选 3 处"),"Cross-edition selections lost");
 window.testScreenshot="combined-adaptation-sources"; for(let n=0;n<100 && window.testScreenshot;n++) await tick();
 const originalSet = Storage.prototype.setItem;
 Storage.prototype.setItem = function(key,value) { if(key.includes("pending-edition")) throw Error("测试存储不可用"); return originalSet.call(this,key,value); };
 button("建立版本").click();await until(()=>document.body.textContent.includes("测试存储不可用"),"Storage failure not visible");
 check(creates.length===0 && !document.querySelector('fieldset').disabled,"Storage failure sent an unrecorded request");
 Storage.prototype.setItem = originalSet;
 button("建立版本").click();await until(()=>document.body.textContent.includes("建立回执暂不可用"),"Unknown receipt not shown");
 check(document.querySelector('fieldset')?.disabled,"Unknown create request allowed changing its identity");
 const originalGet = Storage.prototype.getItem;
 Storage.prototype.getItem = function(key) { if(key.includes("pending-edition")) throw Error("读取不可用"); return originalGet.call(this,key); };
 root.render(null);await tick();render();await tick();
 button("建立版本").click();await tick();check(creates.length===1,"Unreadable pending request was overwritten or sent");
 Storage.prototype.getItem = originalGet;
 root.render(null);await tick();render("another-author");await tick();
 check(button("建立版本") && !document.querySelector('fieldset').disabled,"Pending request crossed author");
 render("adaptation-author","another-project");await tick();
 check(button("建立版本") && !document.querySelector('fieldset').disabled,"Pending request crossed project");
 render();await tick();
 check(button("重试建立或打开") && document.querySelector('fieldset').disabled && !document.querySelector('.adaptation-source-picker'),"Reopen did not freeze the original request");
 button("重试建立或打开").click();await until(()=>document.body.textContent.includes("打开失败"),"Open failure not recoverable");
 root.render(null);await tick();render();await tick();
 button("重试建立或打开").click();await until(()=>closed,"Successful retry did not close");
 check(creates.length===3 && opens===2,"Reopen did not reconcile original edition before opening");
 check(!Object.keys(localStorage).some(key=>key.includes("pending-edition")),"Successful opening retained pending creation");
 root.render(null);await tick();render();await tick();check(button("建立版本") && !document.querySelector('fieldset').disabled,"Completed request restored as pending");
 root.unmount();window.testResult={passed:true};
}catch(e){window.testResult={passed:false,error:e.stack,dom:document.body.textContent};}})();
