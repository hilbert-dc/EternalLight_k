import { app, BrowserWindow, dialog } from 'electron';
import { creationIntakeCases, runCreationIntake, intakeObservationFailed } from './creation-intake-journey.mjs';
import { writeFileSync, renameSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { PersistentTaskBudget } from '@eternallight/agent-engine';
import { settleLiveProject, stopLiveProject, executeLiveHandoff, liveRemainingRequests, approveLiveCandidate } from '../task-live-lifecycle.mjs';

const [appRoot, reportPath] = process.argv.slice(2);
if (!process.env.NOVEL_CODEX_USER_DATA_DIR || process.env.NOVEL_TASK_LIVE_AUTHORIZED !== 'yes') throw Error('isolated authorization required');
const report = { startedAt: new Date().toISOString(), model: 'deepseek-flash', limits: { maxRequestsPerScenario: Number(process.env.NOVEL_TASK_LIVE_REQUESTS || 6), maxTokens: process.env.NOVEL_TASK_LIVE_TOKENS ? Number(process.env.NOVEL_TASK_LIVE_TOKENS) : null },
  scope: 'production Electron preload/main, real Core/MCP/Native/provider; automated API driving, not visual/manual UX',
  budgetInjection: 'tighten initial Native budget only; reject extra root budgets and topups', qualification: { status: 'notRun' }, scenarios: [] };
if (!Number.isSafeInteger(report.limits.maxRequestsPerScenario) || report.limits.maxRequestsPerScenario < 1 || report.limits.maxRequestsPerScenario > 80) throw Error('invalid authorized request cap');
if (report.limits.maxTokens !== null && (!Number.isSafeInteger(report.limits.maxTokens) || report.limits.maxTokens < 1)) throw Error('invalid token cap');
const qualitySuite=JSON.parse(readFileSync(join(appRoot,'scripts/fixtures/creative-quality-suite.json'),'utf8'));
const qualityNames=qualitySuite.cases.map(c=>'quality:'+c.id);
const scenarios=process.env.NOVEL_TASK_LIVE_SCENARIOS?.split(',') ?? ['delivery','stop','interlude','change','concurrentRecovery'];
if (!scenarios.length || new Set(scenarios).size !== scenarios.length || scenarios.some(name=>!['delivery','stop','resume','interlude','change','concurrentRecovery','multiChapter',...qualityNames,...Object.keys(creationIntakeCases)].includes(name))) throw Error('invalid scenarios');
let current = null;
const clean = value => String(value?.stack ?? value?.message ?? value).replaceAll(process.env.DEEPSEEK_API_KEY || 'NO_KEY_TO_REDACT', '[REDACTED]').replace(/sk-[A-Za-z0-9_-]+/g,'[REDACTED]').slice(0,5000);
const save = () => { writeFileSync(reportPath+'.tmp', JSON.stringify(report,null,2), {mode:0o600}); renameSync(reportPath+'.tmp',reportPath); };
const originalFetch = globalThis.fetch;
globalThis.fetch = async (input, options) => {
  const url = new URL(typeof input === 'string' ? input : input.url);
  const native = url.hostname === '127.0.0.1' && url.pathname.startsWith('/api/projects/');
  if (native && options?.method === 'POST' && options.body) {
    const body = JSON.parse(options.body);
    if (url.pathname.endsWith('/budget-authorization')) throw Error('LIVE_PROBE_NO_BUDGET_TOPUP');
    if (url.pathname.endsWith('/tasks') && body.initialBudget) {
      if (!current || current.rootBudgetGranted) throw Error('LIVE_PROBE_NO_SECOND_ROOT_BUDGET');
      current.rootBudgetGranted = true;
      current.projectId = url.pathname.split('/')[3];
      body.initialBudget = {...body.initialBudget, maxRequests:report.limits.maxRequestsPerScenario, maxTokens:report.limits.maxTokens};
      options = {...options, body:JSON.stringify(body)};
    }
    if (url.pathname.endsWith('/generation-runs')) {
      if (!current || !['delivery','multiChapter'].includes(current.name) || current.projectId !== url.pathname.split('/')[3] || current.generationGranted || body.targetChapters !== (current.name==='multiChapter'?2:1) || !body.generationProposalId) throw Error('LIVE_PROBE_UNAUTHORIZED_GENERATION');
      const receipts = current.tasks.map(task => PersistentTaskBudget.receipt(join(process.env.NOVEL_CODEX_USER_DATA_DIR, 'agent-engine/engine-state.sqlite3'), task.id));
      const remaining = liveRemainingRequests(report.limits.maxRequestsPerScenario, receipts);
      current.generationGranted = true; current.handoffAccounting = { receipts, remaining };
      if (report.limits.maxTokens !== null) throw Error('Finite-token generation handoff needs aggregate token accounting');
      body.maxProviderRequests = remaining; body.maxProviderTokens = null;
      options = {...options, body:JSON.stringify(body)};
    }
  }
  const response = await originalFetch(input,options);
  if (native && current?.projectId === url.pathname.split('/')[3] && options?.method === 'POST') {
    const value = await response.clone().json().catch(()=>null);
    if (url.pathname.endsWith('/tasks') && value?.id) current.tasks.push({id:value.id,operation:value.operation,threadId:value.threadId});
    if (!response.ok) current.nativeFailures.push({path:url.pathname,status:response.status,error:value?.error});
    save();
  }
  return response;
};
dialog.showMessageBox = async (_window, options) => { report.dialog = {title:options.title,buttons:options.buttons};save();throw Error('Unexpected native dialog requires explicit scenario handling'); };
let finishing = null;
async function finish(window, forced=false) {
  if(finishing)return finishing;
  finishing=(async()=>{
    clearTimeout(timer);
    const watchdog=setTimeout(()=>{report.cleanupFailure='cleanup deadline exceeded; ledger outcome may be unknown';save();app.exit(1);},25000);
    try {
      report.finalCleanup=[];
      for(const item of report.scenarios)if(item.projectId) {
        const result=await window.webContents.executeJavaScript(`window.taskLiveHelpers.stopLiveProject(window.novelCodex,${JSON.stringify(item.projectId)},{timeoutMs:15000})`);
        report.finalCleanup.push({projectId:item.projectId,...result});save();
      }
    } catch(error){report.cleanupFailure=clean(error);}
    finally{
      for(const item of report.scenarios)item.finalAccounting=item.tasks.map(task=>{
        try{return PersistentTaskBudget.receipt(join(process.env.NOVEL_CODEX_USER_DATA_DIR,'agent-engine/engine-state.sqlite3'),task.id);}
        catch(error){return {runId:task.id,status:'unresolved',error:clean(error)};}
      });
      clearTimeout(watchdog);report.forcedShutdown=forced;report.finishedAt=new Date().toISOString();save();console.log('LIVE_REPORT:'+reportPath);if(window&&!window.isDestroyed())window.destroy();app.quit();}
  })();
  return finishing;
}
let timer = setTimeout(()=>{report.fatal='overall timeout';save();void finish(BrowserWindow.getAllWindows()[0],true);},2_400_000);
process.on('uncaughtException',error=>{report.fatal=clean(error);save();void finish(BrowserWindow.getAllWindows()[0],true);});
app.once('browser-window-created',(_event,window)=>window.webContents.once('did-finish-load',async()=>{
  const js = code => window.webContents.executeJavaScript(code);
  try {
    await js(`(async()=>{while(true){const s=await window.novelCodex.runtime.status();if(s.state==='online')return;if(s.state==='failed')throw Error(JSON.stringify(s));await new Promise(r=>setTimeout(r,100));}})()`);
    await js(`(()=>{const settleLiveProject=${settleLiveProject.toString()};const stopLiveProject=${stopLiveProject.toString()};const executeLiveHandoff=${executeLiveHandoff.toString()};const approveLiveCandidate=${approveLiveCandidate.toString()};window.taskLiveHelpers={settleLiveProject,stopLiveProject,executeLiveHandoff,approveLiveCandidate};})()`);
    report.qualification.status='running';save();console.log('LIVE_STAGE:qualification');
    const existing = await js(`window.novelCodex.agent.status()`);
    const qualification = existing.configured ? { reusedCurrentQualifiedConnection: true } : await js(`window.novelCodex.agent.verifyConnection('deepseek','deepseek-flash')`);
    report.qualification={status:'returned',result:qualification};save();
    const configured = await js(`window.novelCodex.agent.configure('deepseek','deepseek-flash')`);
    report.configured=configured;save();
    for (const scenario of scenarios) {
      current={name:scenario,status:'running',tasks:[],nativeFailures:[]};report.scenarios.push(current);save();console.log('LIVE_STAGE:'+scenario);
      try {
        const result = creationIntakeCases[scenario] ? await js(`(${runCreationIntake.toString()})(${JSON.stringify(scenario)},${JSON.stringify(creationIntakeCases[scenario])})`) : await js(`(${journey.toString()})(${JSON.stringify(scenario)},${JSON.stringify(qualitySuite.cases.find(c=>scenario==='quality:'+c.id)??null)})`);
        Object.assign(current,result);
      } catch(error) { current.status='failed';current.error=clean(error); }
      save();
      // A preparation probe has no authorization to keep spending after its
      // bounded observation ended; stop using the real lifecycle API first.
      if (creationIntakeCases[scenario] && intakeObservationFailed(current.outcome)) {
        current.observationStop = await js(`window.taskLiveHelpers.stopLiveProject(window.novelCodex,window.liveProjectId,{timeoutMs:15000})`);
      }
      // Observe background continuation even after the foreground promise rejected.
      current.quiescence = await js(`window.taskLiveHelpers.settleLiveProject(window.novelCodex,window.liveProjectId,{timeoutMs:600000,onIdle:window.liveSeed?async()=>{const result=await window.taskLiveHelpers.approveLiveCandidate(window.novelCodex,window.liveProjectId,window.liveSeed.chapterId,window.liveApprovalIds);if(result)window.liveApprovals.push(result);}:null})`);
      if (!current.quiescence.settled) {
        current.cleanup = await js(`window.taskLiveHelpers.stopLiveProject(window.novelCodex,window.liveProjectId,{timeoutMs:15000})`);
        current.status = 'blocked';
        if (!current.cleanup.settled || current.cleanup.errors.length) report.blocked = 'cleanup has unresolved execution or settlement';
      }
      current.finalWorkspace = await js(`window.novelCodex.runtime.getCreativeWorkspace(window.liveProjectId)`);
      current.approvals = await js(`window.liveApprovals`);
      current.accounting = current.tasks.map(task => {
        try { return PersistentTaskBudget.receipt(join(process.env.NOVEL_CODEX_USER_DATA_DIR,'agent-engine/engine-state.sqlite3'),task.id); }
        catch(error) { return {runId:task.id,status:'unresolved',error:clean(error)}; }
      });
      save();
      if (report.blocked) break;
    }
  } catch(error){report.fatal=clean(error);save();}
  finally{await finish(window);}
}));

async function journey(name, qualityCase = null) {
  const api=window.novelCodex, sleep=ms=>new Promise(r=>setTimeout(r,ms));
  const wait=async fn=>{const end=Date.now()+600000;while(Date.now()<end){const x=await fn();if(x)return x;await sleep(100);}throw Error('scenario wait timeout');};
  const {project}=await api.runtime.createProject('真实任务验收 '+name,'虚构港城里，抄写员林夕保留一封密函。','novel',crypto.randomUUID());
  window.liveProjectId=project.id;window.liveSeed=null;window.liveApprovals=[];window.liveApprovalIds=new Set();
  const events=[];const unsubscribe=api.agent.onEvent(e=>events.push(e));
  let requestId,settled=false,outcome;
  let seed = null;
  if(['resume','interlude','change'].includes(name)) {
    const workspace=await api.runtime.getCreativeWorkspace(project.id);
    seed=await api.runtime.saveDraftCheckpoint(project.id,{chapterId:'chapter-live-'+crypto.randomUUID(),chapterNumber:1,title:'港城密函',body:'夜幕落在港城。抄写员林夕把密函藏进衣袋，走向钟楼。',sceneGroups:[]},workspace.revision,null);
  }
  if(qualityCase) {
    const workspace=await api.runtime.getCreativeWorkspace(project.id);
    const sceneGroups=qualityCase.source.groups.map(g=>({...g,sceneId:null}));
    seed=await api.runtime.saveDraftCheckpoint(project.id,{chapterId:'quality-'+crypto.randomUUID(),chapterNumber:1,title:qualityCase.source.title,body:sceneGroups.map(g=>g.body.trim()).join('\n\n---\n\n'),sceneGroups},workspace.revision,null);
  }
  window.liveSeed=qualityCase?.kind==='analysis'?null:seed;
  const prompt=qualityCase?`${qualityCase.authorIntent}\n原文已保存在第1章候选；${qualityCase.kind==='analysis'?'只给分析，不修改正文。':'实际保存候选正文，保留原标题和段组身份及顺序，不采用、不发布。'}`:name==='multiChapter'?'请连续创作两章，每章约400字：第一章林夕在港城藏好密函，第二章带着密函赶到海关，左手在途中受伤。保持情节连续，实际保存两章候选正文，不采用、不发布。':['resume','interlude'].includes(name)?'请读取已有第1章候选正文，将它改写成约100字的悬疑开场，林夕继续保留密函，加入港城雨夜与钟声。实际保存修改后的候选正文，不采用、不发布。':name==='change'?'请读取已有第1章候选正文，将它改写成约100字的开场，林夕在港城烧掉密函。实际保存修改后的候选正文，不采用、不发布。':name==='delivery'?'为这个虚构故事创作一段约100字的开场，林夕在港城藏起密函。实际保存候选正文，不采用，不发布；根据实际保存回执说明完成情况。':
    '请为这个虚构故事写一段约100字开场：林夕在港城烧掉密函。实际保存候选正文，不采用、不发布。先读取作品资料。';
  const before=await api.runtime.getCreativeWorkspace(project.id);
  const pending=api.agent.sendPrompt(project.id,prompt).then(value=>{settled=true;outcome={ok:true,value};},error=>{settled=true;outcome={ok:false,error:String(error?.message??error)};});
  try {
    requestId=await wait(()=>events.find(e=>e.method==='agent/execution-request')?.params.requestId);
    let control=null;
    if(['stop','resume','interlude','change','concurrentRecovery'].includes(name)){
      await wait(()=>settled||events.some(e=>e.method==='item/completed'&&e.params?.item?.type==='mcpToolCall'));
      if(settled)return {status:'blocked',reason:'turn settled before live intervention',outcome,events:events.map(e=>e.method)};
      const works=await api.runtime.listWorkTasks(project.id);
      const work=works.tasks.find(w=>w.status==='running');
      if(!work)throw Error('No running Native Work for intervention');
      if(name==='stop'||name==='resume'||name==='concurrentRecovery'){
        control=await api.agent.interrupt({projectId:project.id,requestId});
        await wait(()=>settled);
        const stopped=await api.runtime.getCreativeWorkspace(project.id);
        await sleep(1500);
        const later=await api.runtime.getCreativeWorkspace(project.id);
        if(stopped.headSnapshotId!==later.headSnapshotId||stopped.sourceDigest!==later.sourceDigest)throw Error('write after acknowledged stop');
        if(name==='stop' && (control.interrupted !== true || (await api.runtime.getWorkTask(project.id,work.id)).status !== 'paused'))throw Error('stop lacks durable pause receipt');
        if(name==='stop')return {status:'passed',control,outcome,work:await api.runtime.getWorkTask(project.id,work.id),noPostStopWrites:true,eventMethods:[...new Set(events.map(e=>e.method))]};
        if(name==='resume') {
          if(control.interrupted!==true||(await api.runtime.getWorkTask(project.id,work.id)).status!=='paused')throw Error('resume requires durable pause receipt');
          const resumed=await api.agent.resumeWorkTask(project.id,work.id);
          return {status:'needsReview',control,resumed,before,after:await api.runtime.getCreativeWorkspace(project.id),work:await api.runtime.getWorkTask(project.id,work.id),noPostStopWrites:true,limitation:'acknowledged pause/resume without source changes; not a simulated network disconnect'};
        }
        const payload={chapterId:'chapter-live',chapterNumber:1,title:'作者修改',body:'作者并发标记：林夕保留密函，门外响起钟声。',sceneGroups:[]};
        const saved=await api.runtime.saveDraftCheckpoint(project.id,payload,later.revision,null);
        let resumed;
        try{resumed={ok:true,value:await api.agent.resumeWorkTask(project.id,work.id)};}catch(error){resumed={ok:false,error:String(error?.message??error)};}
        const after=await api.runtime.getCreativeWorkspace(project.id);
        return {status:resumed.ok?'needsReview':'blocked',control,saved,resumed,after,work:await api.runtime.getWorkTask(project.id,work.id),limitation:'stop/resume with intervening author candidate, not actual network disconnect'};
      }
      if(name==='interlude')control=await api.agent.beginInterlude(project.id,work.id,{id:crypto.randomUUID(),threadId:work.threadId,expectedControlRevision:work.controlRevision,parentId:null,autoReturn:true,text:'先临时回答：主人公叫什么？不要修改作品。',focus:{view:'brief',readOnly:true}});
      if(name==='change')control=await api.agent.steer(project.id,'修改要求：林夕必须保留密函，绝不能烧掉。后续创作请采用这项新要求。',work.threadId);
    }
    await wait(()=>settled);await pending;
    let generation = null;
    if(['delivery','multiChapter'].includes(name) && outcome.ok && outcome.value.generationHandoff) {
      const parent = await window.taskLiveHelpers.settleLiveProject(api,project.id,{timeoutMs:600000});
      if(!parent.settled) throw Error('Parent execution/usage did not settle before generation approval');
      let generationSettled=false;
      const generating=window.taskLiveHelpers.executeLiveHandoff(api,project.id,outcome.value.generationHandoff,name==='multiChapter'?2:1)
        .then(value=>{generation={ok:true,value};},error=>{generation={ok:false,error:String(error?.message??error)};}).finally(()=>{generationSettled=true;});
      await wait(()=>generationSettled);await generating;
    }
    const after=await api.runtime.getCreativeWorkspace(project.id),works=await api.runtime.listWorkTasks(project.id);
    return {status:outcome.ok?'needsReview':'failed',qualityCase,outcome,generation,control,seed,before,after,works,eventMethods:[...new Set(events.map(e=>e.method))],tools:events.filter(e=>e.method==='item/completed'&&e.params?.item?.type==='mcpToolCall').map(e=>({tool:e.params.item.tool,status:e.params.item.status,result:e.params.item.result}))};
  } catch(error) {
    return {status:'failed',error:String(error?.message??error),outcome,works:await api.runtime.listWorkTasks(project.id),after:await api.runtime.getCreativeWorkspace(project.id),eventMethods:[...new Set(events.map(e=>e.method))]};
  } finally {
    if(!settled&&requestId){await api.agent.interrupt({projectId:project.id,requestId}).catch(()=>{});await Promise.race([pending,sleep(5000)]);}
    unsubscribe();
  }
}
await import(pathToFileURL(join(appRoot,'electron/main.mjs')));
