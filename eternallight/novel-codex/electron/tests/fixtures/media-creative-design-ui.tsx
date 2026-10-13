import React from "react";
import { createRoot } from "react-dom/client";
import { MultimediaWorkspaceView } from "../../../src/multimedia/MultimediaWorkspaceView";
import "../../../src/styles.css";
const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (value, message) => { if (!value) throw Error(message); };
const until = async (predicate, message) => { for (let i = 0; i < 150 && !predicate(); i++) await tick(); check(predicate(), message); };
const clone = value => structuredClone(value);
const make = kind => ({project:{id:"book",title:"作品"},edition:{id:kind,editionId:kind,projectId:"book",scopeId:`scope-${kind}`,branchId:`branch-${kind}`,medium:kind,title:kind,canonSnapshotId:null,sources:[]},revisionId:`${kind}-1`,assets:[],sources:[],document:kind === "comic" ? {kind,title:kind,format:"pages",pages:[],creativeDesign:"开端、转折、结局"} : {kind,title:kind,width:1280,height:720,frameRate:{numerator:24,denominator:1},shots:[],timeline:{tracks:[]}}});
const workspaces = {comic:make("comic"),animation:make("animation")};
const history = new Map();
let editionId="comic", refreshToken=0, busy=false, dirty=false, fail=true, saveHandler, discardHandler, selected=null, source=null;
const saves=[];const receipts=new Map();let corruptSave=value=>value;
const withSource=value=>({...value,documentSource:{projectId:"book",scopeId:value.edition.scopeId,branchId:value.edition.branchId,objectId:value.edition.id,revisionId:value.revisionId,digest:"a".repeat(64),selector:{kind:"whole"}}});
const api = {
  readWorkspace:async(project,id,branch,revision)=>{history.set(workspaces[id].revisionId,clone(workspaces[id]));return withSource(clone(revision ? history.get(revision) : workspaces[id]));},
  readSubtitleFont:async()=>null,
  saveDocument:async(project,id,request)=>{
    saves.push(clone(request));
    const prior=receipts.get(request.idempotencyKey);
    if(prior){check(JSON.stringify(prior.request)===JSON.stringify(request),"Save replay changed request");return corruptSave(clone(prior.receipt));}
    check(project === "book" && request.expectedRevisionId === workspaces[id].revisionId,"Design lost original CAS or project");
    if(fail){fail=false;throw Error("保存连接中断");}
    workspaces[id]={...workspaces[id],revisionId:`${id}-saved-${saves.length}`,document:clone(request.document)};
    history.set(workspaces[id].revisionId,clone(workspaces[id]));const receipt=withSource(clone(workspaces[id]));receipts.set(request.idempotencyKey,{request:clone(request),receipt});return corruptSave(clone(receipt));
  }
};
document.body.style.minWidth="0";
const root=createRoot(document.getElementById("root"));
const render=async()=>{root.render(<MultimediaWorkspaceView authorKey="author" api={api} projectId="book" editionId={editionId} busy={busy} refreshToken={refreshToken} onSelectionChange={value=>{selected=value;}} onSourceSelectionChange={value=>{source=value;}} onDirtyChange={value=>{dirty=value;}} onSaveHandlerChange={value=>{saveHandler=value;}} onDiscardHandlerChange={value=>{discardHandler=value;}}/>);await tick();};
const input=()=>document.querySelector<HTMLTextAreaElement>(".mm-creative-design textarea");
const button=text=>[...document.querySelectorAll<HTMLButtonElement>("button")].find(item=>item.textContent===text);
const change=async text=>{Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,"value").set.call(input(),text);input().dispatchEvent(new Event("input",{bubbles:true}));await tick();};
(async()=>{try{
  await render();await until(()=>input()&&!input().disabled,"Design did not load");
  check(!document.querySelector<HTMLDetailsElement>(".mm-creative-design").open && !dirty && saves.length===0,"Design forced itself open or created a modification");
  document.querySelector<HTMLDetailsElement>(".mm-creative-design").open=true;
  check(input().value==="开端、转折、结局","Saved design missing");
  const text="保留开端，逐步推进人物变化。\n".repeat(100)+"结局暂定，尚未批准制作。";
  await change(text);check(dirty,"Design edit was not a draft");
  busy=true;await render();check(input().disabled,"Busy design remained editable");busy=false;await render();
  check(!await saveHandler() && input().value===text && dirty,"Failed design save discarded input");
  check(await saveHandler(),"Design retry failed");await until(()=>!dirty,"Save receipt not applied");
  check(saves[0].document.creativeDesign===text && saves[1].document.creativeDesign===text && saves[0].idempotencyKey===saves[1].idempotencyKey,"Save retry changed exact draft or identity");
  await change("本地结构修改");
  workspaces.comic.document.title="后台改标题";workspaces.comic.revisionId="comic-independent";
  refreshToken++;await render();await until(()=>document.body.textContent.includes("已合并后台"),"Independent background edit did not merge");
  check(input().value==="本地结构修改" && await saveHandler(),"Independent merge lost design");
  check(workspaces.comic.document.title==="后台改标题","Design save overwrote independent title");await until(()=>!dirty,"Merged save not applied");
  await change("本地冲突设计");
  workspaces.comic.document.creativeDesign="远端冲突设计";workspaces.comic.revisionId="comic-conflict";
  refreshToken++;await render();await until(()=>button("查看最新保存版本"),"Design conflict missing");
  button("查看最新保存版本").click();await tick();
  const saved=document.querySelector<HTMLTextAreaElement>('[aria-label="最新保存版本（只读）"] textarea');
  check(saved?.readOnly && saved.value==="远端冲突设计" && input().value==="本地冲突设计","Conflict review concealed either design");
  for (const theme of ["light", "dark"]) {
    document.documentElement.dataset.theme=theme;
    window.testViewport={width:380,height:900}; await until(()=>!window.testViewport && innerWidth===380,"Narrow viewport missing");
    await tick();check(document.documentElement.scrollWidth<=innerWidth+1,"Design conflict overflowed narrow viewport");
    window.testScreenshot=`media-design-conflict-${theme}`;await until(()=>!window.testScreenshot,"Design screenshot missing");
  }
  const count=saves.length;button("冲突处使用已保存内容").click();await tick();check(saves.length===count && input().value==="远端冲突设计","Conflict selection autosaved or chose wrong design");
  await change("切换后保留草稿");editionId="animation";await render();await until(()=>input()?.value===""&&!input().disabled,"Animation empty optional design missing");
  await change("动画分集结构与镜头安排");check(await saveHandler(),"Animation design save failed");await until(()=>!dirty,"Animation receipt missing");
  await change("");check(await saveHandler(),"Clearing design failed");await until(()=>!dirty,"Clear receipt missing");check(!Object.hasOwn(workspaces.animation.document,"creativeDesign"),"Clear did not remove optional design");
  editionId="comic";await render();await until(()=>input()?.value==="切换后保留草稿","Edition-specific design draft not restored");check(dirty && discardHandler(),"Restored design could not be discarded");await until(()=>input()?.value==="远端冲突设计"&&!dirty,"Discard did not restore authoritative design");
  await change("序🌙章：人物相遇；终章暂定");check(await saveHandler(),"Selection setup save failed");await until(()=>!dirty,"Selection setup receipt missing");
  document.querySelector<HTMLDetailsElement>(".mm-creative-design").open=true;
  input().focus();input().setSelectionRange(1,4);input().dispatchEvent(new MouseEvent("mouseup",{bubbles:true}));await tick();
  check(selected?.selector.kind==="fieldText" && selected.selector.field.kind==="creativeDesign" && selected.selector.startChar===1 && selected.selector.endChar===3 && selected.selector.quote==="🌙章", "Design selection lost Unicode scalar coordinates or quote");
  check(selected.revisionId===workspaces.comic.revisionId && source?.refs[0].selector.quote==="🌙章", "Saved design did not become an exact adaptation source");
  workspaces.comic.document.creativeDesign="序🌟章：人物相遇；终章暂定";workspaces.comic.revisionId="design-changed";refreshToken++;await render();
  await until(()=>selected?.revisionId===null && source===null,"Changed design selection was silently rebound or widened");
  input().focus();input().setSelectionRange(1,4);input().dispatchEvent(new MouseEvent("mouseup",{bubbles:true}));await tick();
  check(selected.selector.quote==="🌟章" && source?.refs[0].revisionId==="design-changed","Reselection failed to bind the real new source");
  await change("未保存🌙设计");input().setSelectionRange(3,6);input().dispatchEvent(new MouseEvent("mouseup",{bubbles:true}));await tick();check(selected?.revisionId===null && source===null,"Unsaved design granted a durable source");
  await change("长".repeat(4000));check(await saveHandler(),"Long selection setup save failed");await until(()=>!dirty,"Long selection save missing");
  input().focus();input().setSelectionRange(0,4000);input().dispatchEvent(new MouseEvent("mouseup",{bubbles:true}));await tick();
  check(selected?.revisionId===null && source===null && document.body.textContent.includes("选段过长"),"Oversized selection was truncated or authorized");
  input().setSelectionRange(0,10);input().dispatchEvent(new MouseEvent("mouseup",{bubbles:true}));await tick();check(source?.refs[0].selector.quote==="长".repeat(10),"Smaller selection did not recover");
  await change("错误回执不能替换此草稿");
  const originalSaves=saves.length;
  for(const mutation of [value=>({...value,edition:{...value.edition,scopeId:"foreign"}}),value=>({...value,edition:{...value.edition,branchId:"foreign"}}),value=>({...value,edition:{...value.edition,projectId:"foreign"}}),value=>({...value,document:{kind:"animation"}})]){
    corruptSave=mutation;check(!await saveHandler(),"Wrong save receipt was accepted");await tick();check(dirty && input().value==="错误回执不能替换此草稿","Bad receipt consumed draft");
  }
  corruptSave=value=>value;check(await saveHandler(),"Original save receipt could not recover");await until(()=>!dirty,"Recovered save not projected");
  check(saves.slice(originalSaves).every(request=>JSON.stringify(request)===JSON.stringify(saves[originalSaves])),"Bad receipt retry changed identity or draft");
  root.unmount();window.testResult={passed:true};
}catch(error){window.testResult={passed:false,error:error.stack,dom:document.body.textContent};}})();
