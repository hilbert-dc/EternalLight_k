import assert from "node:assert/strict";
import { test } from "node:test";
import { prepareMediaTurn, prepareAdvisoryMedia, composeMediaPrompt, assertMediaImageRoute } from "../media-conversation.mjs";
import { composeNovelCodexPrompt } from "../codex-runtime.mjs";
import { taskAuthorPrompt, taskExecutionPrompt } from "../workflow-handoff.mjs";
import { authorIntentError } from "../../shared/author-input-budget.mjs";
import { presentAuthorHistory } from "../thread-history-presentation.mjs";
import { registerAdvisoryPromptHandler } from "../revision-repair-handlers.mjs";

const scope = { projectId:"book", editionId:"comic", scopeId:"comic-scope", branchId:"comic-branch" };
const image = { assetRevisionId:"image-r1", kind:"image", mimeType:"image/png", scopeId:scope.scopeId, width:1, height:1 };
const workspace = { project:{id:"book"}, edition:scope, revisionId:"comic-r1", assets:[image] };
const input = ids => ({ scope, expectedRevisionId:"comic-r1", selection:null, imageAssetRevisionIds:ids });
const host = { workspace:async()=>structuredClone(workspace), assetBytes:async(id,project,assetScope)=> { assert.equal(id,"image-r1"); assert.equal(project,"book"); assert.equal(assetScope,"comic-scope"); return {bytes:Buffer.from("fixture-image-bytes")}; } };

test("image-only turn retains empty original text through host, frozen task, model input and history", async () => {
  const media = await prepareMediaTurn(host, "book", input(["image-r1"]), "");
  const task = { id:"image-task", operation:"advisory", target:media.target, inputManifest:{authorIntent:""} };
  const binding = { task:{operation:task.operation,target:task.target} };
  assert.equal(taskAuthorPrompt(task), ""); assert.equal(taskExecutionPrompt(task), "");
  const prompt = composeMediaPrompt(task, binding, []); assert.deepEqual(prompt.input, []);
  prompt.input.push(...media.images); assert.equal(prompt.input.length, 1); assert.equal(prompt.input[0].type,"image");
  assert.doesNotMatch(JSON.stringify(prompt), /请描述图片|帮我分析图片/);
  assert.throws(()=>composeNovelCodexPrompt("", "", "", { imageAssetRevisionIds:["image-r1"] }), /本轮作者要求/);
  const history = presentAuthorHistory({thread:{id:"thread"},turns:[{id:"image-turn",items:[{type:"userMessage",text:"internal synthetic model framing"}]}]}, {threadId:"thread",messages:[{turnId:"image-turn",text:"",mediaAttachments:[{assetRevisionId:"image-r1",name:"参考图",previewUrl:"creative-media://private/image-r1"}]}]});
  assert.equal(history.turns[0].items[0].text, ""); assert.equal(history.turns[0].items[0].mediaAttachments.length, 1);
});

test("empty intent requires real owned media attachments and an executable verified image route", async () => {
  assert.ok(authorIntentError("")); assert.ok(authorIntentError("",{imageAssetRevisionIds:[""]}));
  await assert.rejects(prepareMediaTurn(host,"book",input([]),""), /不能为空/);
  await assert.rejects(prepareMediaTurn(host,"book",input(["foreign-image"]),""), /不属于/);
  assert.throws(()=>taskAuthorPrompt({operation:"advisory",target:{type:"project"},inputManifest:{authorIntent:""}}), /冻结的作者要求/);
  const media = await prepareMediaTurn(host,"book",input(["image-r1"]),"  ");
  assert.throws(()=>assertMediaImageRoute({defaultModel:"route",routes:[{logicalModel:"route",enabledExtensions:["imageInput"]}]},media.images), /未通过图片输入验证/);
  assert.doesNotThrow(()=>assertMediaImageRoute({defaultModel:"route",routes:[{logicalModel:"route",authorizedExtensions:["imageInput"]}]},media.images));
});

test("advisory IPC leaves image-only validation to the authoritative media host while ordinary blank intent stays rejected", async () => {
  let handler, mediaCalls=0;
  registerAdvisoryPromptHandler({ handle:(_name,value)=>{handler=value;} }, { routingState:()=>({}), withWorkFlow:async(_project,action)=>action(),
    runAdvisoryAgentTask:async(_event,project,text,...rest)=> { mediaCalls++; const media=rest.at(-1); await prepareMediaTurn(host,project,media,text); return {message:"按图回答"}; } });
  await assert.rejects(handler({},"book",""), /不能为空/); assert.equal(mediaCalls,0);
  await assert.rejects(handler({},"book","",null,null,null,input(["foreign-image"])), /不属于/);
  assert.equal((await handler({},"book","",null,null,null,input(["image-r1"]))).message,"按图回答");
});


test("media continuation reconstructs frozen context without granting a new media allowance", async () => {
  const original = await prepareMediaTurn(host, "book", input(["image-r1"]), "原要求");
  const step = {id:"original-step",target:{...original.target,mediaBudget:{maxRequests:3},readOnly:true,interludeId:"question"}};
  const media = await prepareAdvisoryMedia(host,"book",null,step,"原要求");
  assert.deepEqual(media.target,{...step.target,resumeStepId:step.id});
  assert.equal(media.images.length,1);
  assert.equal(await prepareAdvisoryMedia(host,"book",null,{id:"novel",target:{type:"project"}},"原要求"),null);
  await assert.rejects(prepareAdvisoryMedia({...host,workspace:async()=>({...workspace,revisionId:"changed"})},"book",null,step,"原要求"),/版本已变化/);
  await assert.rejects(prepareAdvisoryMedia(host,"book",input([]),step,"原要求"),/来源已变化/);
  const noGrant={id:"question-step",target:{...original.target}};delete noGrant.target.mediaBudget;
  assert.equal(Object.hasOwn((await prepareAdvisoryMedia(host,"book",null,noGrant,"原要求")).target,"mediaBudget"),false);
});

test('creative-design selection is frozen exactly and obeys the shared serialized byte limit', async () => {
  const { mediaSelectionFits, MAX_MEDIA_SELECTION_BYTES } = await import('../../shared/media-selection-budget.mjs');
  const value=input([]);
  value.selection={scope,revisionId:'comic-r1',label:'创作设计选段',assetRevisionId:null,selector:{kind:'fieldText',field:{kind:'creativeDesign'},startChar:1,endChar:3,quote:'🌙章'}};
  const media=await prepareMediaTurn(host,'book',value,'只讨论这一段结构');
  assert.deepEqual(media.target.selection,value.selection);
  assert.ok(mediaSelectionFits(value.selection));
  value.selection.selector.quote='长'.repeat(4000);value.selection.selector.endChar=4001;
  assert.ok(Buffer.byteLength(JSON.stringify(value.selection))>MAX_MEDIA_SELECTION_BYTES);
  assert.equal(mediaSelectionFits(value.selection),false);
  await assert.rejects(prepareMediaTurn(host,'book',value,'只讨论'),/选区引用/);
});
