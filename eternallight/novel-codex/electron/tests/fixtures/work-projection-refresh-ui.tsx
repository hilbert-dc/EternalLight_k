import React, { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { useWorkProjectionRefresh } from "../../../src/useWorkProjectionRefresh";
import { useWorkTaskList } from "../../../src/useWorkTaskList";
import { useWorkCatalog } from "../../../src/useWorkCatalog";

const root = createRoot(document.getElementById("root")!);
const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (ok, message) => { if (!ok) throw Error(message); };
const until = async (f, message) => { for (let i=0;i<120&&!f();i++) await tick(); check(f(), message); };
let project = "a", work, catalog, invalidations = 0, reads = [], hold = null;
const make = id => ({ id:`work-${id}`,projectId:id,threadId:`thread-${id}`,status:"running",version:1,
 deliveryReport:{evidence:[],unsettledAttemptIds:[]},steps:[{id:"step",target:{chapterId:`chapter-${id}`},outputRefs:[],changes:[],attempts:[]}] });
const records = {a:make("a"),b:make("b")};
const api = { runtime: {
 listWorkTasks: async id => { const result={tasks:[structuredClone(records[id])],activeCount:1,nextCursor:null}; if(hold) { const gate=hold;hold=null;await gate; } return result; },
 getWorkCatalog: async id => { reads.push(id); return {projectId:id,revision:records[id].steps[0].outputRefs.length,volumes:[]}; },
}};
function Probe(){ const [token,setToken]=useState(0); work=useWorkTaskList(api as any,project,token,true,true);
 useWorkProjectionRefresh(work.scope,project,work.tasks,work.loaded,()=>{ invalidations++;setToken(x=>x+1); });
 catalog=useWorkCatalog(api as any,project,token,true);
 return <div><input aria-label="unsaved" defaultValue="作者未保存草稿"/><span>{catalog.catalog?.projectId}:{catalog.catalog?.revision}</span></div>;
}
const render=async()=>{root.render(<Probe/>);await tick();};
(async()=>{try{
 await render();await until(()=>work.loaded&&catalog.catalog,"initial read");await tick();
 const baseline=invalidations;
 records.a.version++;await work.refresh();await tick();check(invalidations===baseline,"usage/version churn refreshed full projection");
 const editor=document.querySelector("input")!;editor.value="作者正在编辑";
 records.a.steps[0].outputRefs.push({kind:"draft",objectId:"chapter-a",revisionId:"saved-1"});
 await work.refresh();await until(()=>catalog.catalog?.revision===1,"saved chapter did not invalidate catalog during active work");
 check(editor.value==="作者正在编辑","background read changed editor draft");
 const once=invalidations;await work.refresh();await work.refresh();await tick();check(invalidations===once,"same saved receipt caused a refresh loop");
 records.a.steps[0].changes.push({commitId:"adopt",resultRevision:2,undoneByCommitId:null});await work.refresh();await tick();check(invalidations===once+1,"formal mutation did not invalidate");
 let release;hold=new Promise(r=>release=r);const stale=work.refresh();project="b";await render();await until(()=>catalog.catalog?.projectId==="b"&&work.loaded,"project B failed to load");
 const bCount=invalidations;release();await stale;await tick();check(catalog.catalog.projectId==="b"&&invalidations===bCount,"late A snapshot invalidated B");
 await work.refresh();await tick();check(invalidations===bCount,"B reread loop");
 window.testResult={passed:true,invalidations,reads};
}catch(error){window.testResult={passed:false,error:error.stack??String(error)};}})();
