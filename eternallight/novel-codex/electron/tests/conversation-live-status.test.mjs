import test from 'node:test';
import assert from 'node:assert/strict';
import { build } from 'esbuild';
const result = await build({ entryPoints: ['src/ConversationExecutionStatus.tsx'], bundle: true, write: false, platform: 'node', format: 'esm', packages: 'external' });
// Resolve React from this project's package environment.
const { mkdtemp, writeFile, rm } = await import('node:fs/promises');
const dir = await mkdtemp(new URL('./.live-status-', import.meta.url));
try {
  await writeFile(`${dir}/component.mjs`, result.outputFiles[0].text);
  const { executionPresentation: show } = await import(`${dir}/component.mjs`);
  const props = { run: { state: 'running', steps: [], answer: '' }, busy: true, connection: 'ready', works: [], health: [] };
  test('live execution differs from finished turn and awaiting Work dispatch', () => {
    assert.equal(show(props).active, true);
    assert.equal(show({...props,run:{...props.run,state:'completed'}}).label,'正在接续任务');
    assert.equal(show({...props,busy:false,works:[{status:'running'}]}).active,false);
    assert.equal(show({...props,busy:false,run:{...props.run,state:'completed'}}).label,'本轮已结束');
  });
  test('automatic steps expose exact chapter, operation and their own start time', () => {
    const step = {status:'running',operation:'generateChapter',target:{chapterNumber:11,scope:{type:'wholeChapter'}},currentAttemptId:'a',attempts:[{id:'a',startedAt:100}]};
    const writing = show({...props,busy:false,run:{...props.run,state:'completed'},works:[{status:'running',steps:[step]}]});
    assert.match(writing.label,/11/); assert.match(writing.label,/写作/); assert.equal(writing.startedAt,100000);
    const checking = show({...props,busy:false,works:[{status:'running',steps:[{...step,operation:'assessRevisionOutcome',currentAttemptId:'b',attempts:[{id:'b',startedAt:160}]}]}]});
    assert.notEqual(checking.label,writing.label); assert.equal(checking.startedAt,160000);
  });
  test('waiting, disconnect, recovery and author pause never animate as writing', () => {
    for (const change of [{run:{...props.run,state:'waiting'}},{connection:'reconnecting'},{busy:false,works:[{status:'paused'}]},{busy:false,works:[{status:'blocked',resumeBlockReason:'pendingUsage'}]}]) assert.equal(show({...props,...change}).active,false);
  });
  test('background recovery warnings cannot hide actual live chapter or its public progress', () => {
    const attempt = {id:'a',threadId:'writer',turnId:'turn',startedAt:100};
    const step = {status:'running',operation:'generateChapter',target:{chapterNumber:14,scope:{type:'wholeChapter'}},currentAttemptId:'a',attempts:[attempt]};
    const run = {...props.run,taskId:'a',threadId:'writer',turnId:'turn',steps:[{id:'message',kind:'system',status:'completed',messagePhase:'commentary',text:'正在核对第十四章的承接资料。'}]};
    const input = {...props,run,works:[{status:'running',steps:[step]}],health:[{state:'waiting'}]};
    const status = show(input);
    assert.equal(status.active,true); assert.match(status.label,/14/); assert.equal(status.startedAt,100000);
    assert.equal(status.detail,'正在核对第十四章的承接资料。');
    assert.equal(show({...input,run:{...run,taskId:'another'}}).detail,undefined);
    assert.equal(show({...input,run:{...run,state:'completed'}}).detail,undefined);
    assert.equal(show({...props,busy:false,run:{...run,state:'completed'},health:input.health}).active,false);
  });
  test('condition waits differ from author decisions and cannot overwrite a paused goal', () => {
    const idle={...props,busy:false,run:{...props.run,state:'completed'}};
    assert.match(show({...idle,health:[{reason:'unsettledUsage'}]}).label,/费用登记/);
    assert.match(show({...idle,health:[{reason:'generationScopeApprovalRequired'}]}).label,/需要你确认/);
    assert.equal(show({...idle,health:[{reason:'snapshotChanged'}],works:[{status:'paused'}]}).label,'任务已暂停');
    const step={status:'running',operation:'generateChapter',target:{chapterNumber:16},currentAttemptId:'queued',attempts:[{id:'queued',status:'queued',startedAt:100}]};
    const queued=show({...idle,works:[{status:'running',steps:[step]}]});
    assert.match(queued.label,/等待执行/);assert.equal(queued.active,false);assert.equal(queued.startedAt,undefined);
  });
  test('idle other conversation does not inherit a global busy indicator', () => {
    assert.equal(show({...props,busy:false,run:{...props.run,state:'idle'}}),null);
  });
  test('Native running step remains visible between conversational turns; queued is not execution proof', () => {
    const completed = {...props,busy:false,run:{...props.run,state:'completed'}};
    assert.equal(show({...completed,works:[{status:'running',steps:[{status:'running',operation:'generateChapter',target:{chapterNumber:11,scope:{type:'wholeChapter'}},attempts:[{id:'a',startedAt:100}],currentAttemptId:'a'}]}]}).active,true);
    assert.equal(show({...completed,works:[{status:'running',steps:[{status:'queued',operation:'generateChapter',target:{chapterNumber:12},attempts:[]}]}]}).active,false);
    assert.equal(show({...props,readError:'offline'}).active,false);
    assert.equal(show({...completed,works:[{status:'paused',steps:[{status:'running',operation:'generateChapter',target:{chapterNumber:11,scope:{type:'wholeChapter'}},attempts:[{id:'a',startedAt:100}],currentAttemptId:'a'}]}]}).active,false);
  });
} finally { await rm(dir,{recursive:true,force:true}); }
