import test from 'node:test';
import assert from 'node:assert/strict';
import {WorkInterludes} from '../work-interludes.mjs';
const work=(interlude)=>({id:'work-a',projectId:'p',threadId:'t',controlRevision:3,status:'running',interlude});
const question={id:'question',messageId:'question',stepId:'step',parentId:null,autoReturn:true,phase:'answered',revision:0};
test('begin persists the readonly question and original budget identity before waking execution',async()=>{
 const calls=[];const transport=new WorkInterludes({messages:{sessionId:'host',connectionId:()=> 'connection'},poke:()=>calls.push('poke'),request:async(path,options)=>{calls.push(JSON.parse(options.body));return work(question);}});
 await transport.begin('p','work-a',{id:'question',threadId:'t',expectedControlRevision:3,parentId:null,autoReturn:false,text:'解释设定',focus:{view:'chapters'}});
 assert.equal(calls[0].message.input.focus.readOnly,true);assert.equal(calls[0].message.trigger.workTaskId,'work-a');assert.equal(calls[0].autoReturn,false);assert.equal(calls[1],'poke');
});
test('automatic return obeys each parent authorization and never starts another model',async()=>{
 const posts=[];let current=work({...question,parentId:'parent'});
 const transport=new WorkInterludes({messages:{sessionId:'host',read:async()=>({sessionId:'host',status:'delivered'})},poke:()=>{},request:async(path,options)=>{
  if(!options)return current; posts.push(JSON.parse(options.body));current=work({...question,id:'parent',autoReturn:false});return current;
 }});
 await transport.afterAnswer('p','work-a');assert.equal(posts.length,1);assert.equal(posts[0].interludeId,'question');assert.equal(posts[0].automatic,true);
});
for(const state of ['paused','failed','newHost'])test(`no automatic return for ${state}`,async()=>{
 let calls=0;const current=work({...question,phase:state==='failed'?'failed':'answered'});if(state==='paused')current.status='paused';
 const transport=new WorkInterludes({messages:{sessionId:'host',read:async()=>({sessionId:state==='newHost'?'old':'host',status:'delivered'})},poke:()=>{},request:async(path,options)=>{if(options)calls++;return current;}});
 await transport.afterAnswer('p','work-a');assert.equal(calls,0);
});

test('failed automatic return preserves the completed answer and does not replay',async()=>{
 let writes=0;
 const transport=new WorkInterludes({messages:{sessionId:'host',read:async()=>({sessionId:'host',status:'delivered'})},poke:()=>{},request:async(path,options)=>{
  if(!options)return work(question); writes++;throw Error('original inputs changed');
 }});
 const result=await transport.afterAnswer('p','work-a');
 assert.equal(result.state,'held');assert.match(result.error,/inputs changed/);assert.equal(writes,1);
});

test('media interlude freezes the original Edition source without novel focus or new budget',async()=>{
 let submitted;
 const transport=new WorkInterludes({messages:{sessionId:'host',connectionId:()=> 'connection'},poke:()=>{},request:async(_path,options)=>{submitted=JSON.parse(options.body);return work(question);}});
 const input={id:'question',threadId:'t',expectedControlRevision:3,parentId:null,autoReturn:false,text:'解释画格',focus:null,
   media:{scope:{projectId:'p',editionId:'e',scopeId:'s',branchId:'b',title:'UI only'},expectedRevisionId:'revision',selection:null,imageAssetRevisionIds:[]}};
 await transport.begin('p','work-a',input);
 assert.equal(submitted.message.input.focus,null);
 assert.deepEqual(submitted.message.input.media.scope,{projectId:'p',editionId:'e',scopeId:'s',branchId:'b'});
 assert.equal(submitted.message.trigger.workTaskId,'work-a');assert.equal(submitted.initialBudget,undefined);
 await assert.rejects(transport.begin('p','work-a',{...input,focus:{view:'chapters'}}),/来源/);
 await assert.rejects(transport.begin('p','work-a',{...input,media:{...input.media,scope:{...input.media.scope,projectId:'foreign'}}}),/来源/);
});
