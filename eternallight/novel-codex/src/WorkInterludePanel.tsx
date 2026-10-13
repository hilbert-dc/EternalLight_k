import type {MediaTurnInput} from "../shared/multimedia-types";
import {useEffect,useRef,useState} from 'react';
import type {ConversationFocus,NovelCodexApi,WorkTaskSummary} from './types';
export function WorkInterludePanel({api,work,text,focus,media,blocked,acceptDraft,refresh}:{
  api:NovelCodexApi;work:WorkTaskSummary;text:string;focus:ConversationFocus|null;media?:MediaTurnInput;blocked:boolean;
  acceptDraft:()=>()=>void;refresh:()=>Promise<void>;
}) {
  const [autoReturn,setAutoReturn]=useState(false),[pending,setPending]=useState(false),[error,setError]=useState<string|null>(null);
  const starting=useRef(false);
  const source=media?{focus:null,media}:{focus:{...focus,readOnly:true}};
  const sameSource=(input:typeof source)=>JSON.stringify({focus:input.focus,media:input.media})===JSON.stringify({focus:source.focus,media:source.media});
  const item=work.interlude;
  const key=`novel-codex:interlude-submission:${JSON.stringify([work.projectId,work.id])}`;
  useEffect(()=>{
    let disposed=false;
    const previous=localStorage.getItem(key);
    if(!previous) return;
    const accept=acceptDraft();
    void (async()=>{
      try {
        const input=JSON.parse(previous);
        const receipt=await api.agent.readMessage(work.projectId,work.threadId,input.id);
        if(disposed || receipt.id!==input.id || receipt.trigger.kind!=='interlude'
          || receipt.trigger.workTaskId!==work.id || receipt.input.text!==input.text) return;
        localStorage.removeItem(key);
        if(text===input.text && sameSource(input)) accept();
      } catch { /* Keep the original submission and draft until a receipt is known. */ }
    })();
    return ()=>{disposed=true;};
  },[api,key,work.projectId,work.threadId,work.id,work.interlude?.id]);
  async function begin() {
    if(starting.current || blocked || !text.trim()) return;
    starting.current=true;setPending(true);setError(null);
    const accept=acceptDraft();
    try {
      const previous=localStorage.getItem(key);
      const input=previous?JSON.parse(previous):{id:crypto.randomUUID(),threadId:work.threadId,expectedControlRevision:work.controlRevision,
        parentId:item?.id??null,autoReturn,text,...source};
      if(input.text!==text || !sameSource(input)) throw Error('上次临时问题尚未确认，请恢复原输入后重试或重新读取任务。');
      localStorage.setItem(key,JSON.stringify(input));
      const result=await api.agent.beginInterlude(work.projectId,work.id,input);
      if(result.interlude?.id===input.id) {localStorage.removeItem(key);accept();}
      await refresh();
    } catch(e) {setError(e instanceof Error?e.message:'临时提问尚未确认，请核对原任务。');await refresh();}
    finally {starting.current=false;setPending(false);}
  }
  async function returnToParent() {
    if(!item || starting.current) return;
    starting.current=true;setPending(true);setError(null);
    try {
      await api.agent.returnInterlude(work.projectId,work.id,{commandId:crypto.randomUUID(),interludeId:item.id,threadId:work.threadId,
        expectedControlRevision:work.controlRevision,expectedRevision:item.revision});
      await refresh();
    } catch(e){setError(e instanceof Error?e.message:'尚不能返回，请核对执行状态。');await refresh();}
    finally{starting.current=false;setPending(false);}
  }
  return <section className="conversation-interlude" aria-label="临时提问与返回">
    {item && <p role="status">{item.phase==='waiting'?'原任务暂挂：等待当前步骤安全结束，再回答临时问题。':item.phase==='answering'?'原任务暂挂，正在回答临时问题。':item.phase==='failed'?'临时问题未完成，原任务保持暂挂。':item.autoReturn?'临时问题已回答；尚未自动返回，可手动核对并返回。':'临时问题已回答，等待返回原任务。'}</p>}
    {text.trim() && <><label><input type="checkbox" checked={autoReturn} disabled={pending||blocked} onChange={e=>setAutoReturn(e.target.checked)}/>问完继续原任务</label><button type="button" disabled={pending||blocked||!!work.pauseAfterStep} onClick={()=>void begin()}>{item?'再插入一个问题':'临时提问'}</button><small>沿用原任务预算；问题只读，当前步骤结束后回答。</small></>}
    {item && <button type="button" disabled={pending||item.phase==='answering'} onClick={()=>void returnToParent()}>{item.parentId?'返回上一层问题':'返回原任务'}</button>}
    {error && <p role="alert">{error}</p>}
  </section>;
}
