import {useEffect,useState} from 'react';
import type {MediaTurnInput} from '../../shared/multimedia-types';
import type {NovelCodexApi,WorkTaskSummary} from '../types';
import {WorkInterludePanel} from '../WorkInterludePanel';
export function MediaInterludePanel({api,projectId,editionId,scopeId,branchId,threadId,text,selection,images,blocked,acceptDraft,onHoldChange}:{
  api:NovelCodexApi;projectId:string;editionId:string;scopeId:string;branchId:string;threadId:string|null;text:string;
  selection:MediaTurnInput['selection'];images:string[];blocked:boolean;acceptDraft:()=>()=>void;
  onHoldChange:(threadId:string|null,held:boolean)=>void;
}) {
  const [state,setState]=useState<{threadId:string;works:WorkTaskSummary[];revisionId:string|null}|null>(null);
  const [chosen,setChosen]=useState<string|null>(null),[error,setError]=useState<string|null>(null),[version,setVersion]=useState(0);
  const refresh=async()=>{setVersion(value=>value+1);};
  useEffect(()=>{
    let disposed=false;setError(null);
    if(!threadId){onHoldChange(null,false);return;}
    void(async()=>{
      const works:WorkTaskSummary[]=[];let cursor:string|null=null;
      do {
        const page=await api.runtime.listWorkTasks(projectId,cursor);
        if(disposed)return;
        works.push(...page.tasks.filter(work=>work.projectId===projectId && work.threadId===threadId && work.status==='running'
          && work.steps.some(step=>step.target?.scopeId===scopeId && step.target?.editionId===editionId)));
        cursor=page.nextCursor;
      }while(cursor!==null);
      const workspace=works.length?await api.multimedia.readWorkspace(projectId,editionId,branchId):null;
      if(disposed)return;
      if(workspace && (workspace.project.id!==projectId || workspace.edition.scopeId!==scopeId))throw Error('媒体任务来源不匹配');
      setState({threadId,works,revisionId:workspace?.revisionId??null});onHoldChange(threadId,works.some(work=>!!work.interlude));
    })().catch(reason=>{if(!disposed)setError(reason instanceof Error?reason.message:'原任务尚未读取');});
    return()=>{disposed=true;};
  },[api,projectId,editionId,scopeId,branchId,threadId,version,onHoldChange]);
  useEffect(()=>{setChosen(null);},[threadId]);
  useEffect(()=>api.agent.onEvent(event=>{
    const value=event as {method?:string;params?:{projectId?:string;workflow?:{task?:{projectId?:string}}}};
    if(['agent/workflow','agent/message-ended','agent/execution-ended','agent/message-updated'].includes(value.method??'')
      && (value.params?.projectId===projectId || value.params?.workflow?.task?.projectId===projectId))setVersion(value=>value+1);
  }),[api,projectId]);
  const current=state?.threadId===threadId?state:null;
  const work=current?.works.length===1?current.works[0]:current?.works.find(item=>item.id===chosen);
  return <>{error && <p role="alert">原任务状态读取失败：{error}</p>}
    {current && current.works.length>1 && <label>临时提问的原任务<select value={chosen??''} onChange={event=>setChosen(event.target.value)}><option value="">选择原任务</option>{current.works.map(item=><option key={item.id} value={item.id}>{item.goal}</option>)}</select></label>}
    {work && <WorkInterludePanel key={work.id} api={api} work={work} text={text} focus={null}
      media={{scope:{projectId,editionId,scopeId,branchId},expectedRevisionId:current!.revisionId,selection,imageAssetRevisionIds:images}}
      blocked={blocked} acceptDraft={acceptDraft} refresh={refresh}/>}</>;
}
