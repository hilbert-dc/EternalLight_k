import test from 'node:test';
import assert from 'node:assert/strict';
import { assessReading, summarizeComparablePairs, summarizePayloadCost, sumLedgerUsage } from '../tool-evaluation-comparison.mjs';
import { observeTool, validateLiveOptions } from '../evaluate-live-novel-tools.mjs';
const policy='completeOriginalAndFinal';
const item=(tool,args,result)=>observeTool({type:'mcpToolCall',tool,status:'completed',arguments:args,result:{structuredContent:result}});
const page=(source,start,end,total=4,cursor=start===0?null:'next',hash='hash')=>item('read_source_content',{sourceId:source,...(cursor?{cursor}:{})},{sourceId:source,contentHash:hash,coverage:{fromEntry:start,toEntry:end,totalEntries:total},nextCursor:end===total?null:'next',entries:[]});
const context=item('read_context',{}, {taskSources:{entries:[{sectionPath:'/relevantText',sourceId:'original'}]}});
const save=item('save_draft_checkpoint',{patch:{}},{saved:true,revisionId:'v2',savedSource:{status:'available',revisionId:'v2',sourceId:'candidate'}});
const complete=item('complete_task',{}, {status:'completed'});
const valid=()=>[context,page('original',0,2),page('original',2,4),save,page('candidate',0,2),page('candidate',2,4),complete];
test('ledger totals retain failed cases and unknown reserves without inventing missing receipts',()=>{
 const cases=[{status:'passed',usage:{requests:2,tokens:100,unknownTokens:0}},{status:'failed',usage:{requests:1,tokens:20,unknownTokens:7}}];
 assert.deepEqual(sumLedgerUsage(cases),{requests:3,tokens:120,unknownTokens:7});
 assert.equal(sumLedgerUsage([...cases,{usage:null}]),null);
 assert.equal(sumLedgerUsage([]),null);
 assert.equal(sumLedgerUsage([{usage:{requests:1,tokens:Number.MAX_SAFE_INTEGER,unknownTokens:0}},...cases]),null);
});
test('reading obligations use actual source identity, complete cursor chain and event order',()=>{
 assert.equal(assessReading(valid(),policy).honored,true);
 assert.equal(assessReading([], 'task').honored,null);
 assert.throws(()=>assessReading([],'guess'));
 const invalid=[
  rows=>rows.splice(2,1),
  rows=>rows.splice(5,1),
  rows=>{rows[1].read.view='locate';},
  rows=>{rows[2].read.contentHash='different';},
  rows=>{rows[2].read.cursor='wrong';},
  rows=>{rows[2].read.coverage.fromEntry=3;},
  rows=>{rows[2].read.coverage.totalEntries=3;},
  rows=>{rows[1].read.returnedSource='wrong';},
  rows=>{rows[1].error=true;},
  rows=>{rows[1].read.query='filtered';},
  rows=>{rows[1].read.coverage.toEntry=8;},
  rows=>{rows[3].save.source='other-version';},
  rows=>rows.splice(3,0,complete),
 ];
 // An early completion alone does not invalidate later valid completion; remove the latter.
 invalid[12]=rows=>{rows.splice(3,0,complete);rows.pop();};
 for(const mutate of invalid){const rows=structuredClone(valid());mutate(rows);assert.equal(assessReading(rows,policy).honored,false,JSON.stringify(rows));}
 const after=valid();after.push(structuredClone(save),complete);assert.equal(assessReading(after,policy).honored,false,'latest save must be read again');
 const late=valid();late.splice(3,0,...late.splice(1,2));assert.equal(assessReading(late,policy).honored,false,'reading after save cannot prove prior reading');
});
test('pairs exclude failed, missing, duplicated, changed and unknown evidence',()=>{
 const config={repeats:1,caseSelection:[{size:'long',mode:'full'},{size:'long',mode:'patch'}],readingPolicy:policy};
 const rows=['full','patch'].map(mode=>({repeat:0,size:'long',mode,status:'passed',reading:{honored:true},fixtureDigest:'same',admittedCatalogDigest:'catalog',usage:{tokens:mode==='full'?200:100,unknownTokens:0}}));
 assert.equal(summarizeComparablePairs(rows,config,true).pairs[0].patchToFullTokenRatio,0.5);
 for(const mutate of [r=>r.pop(),r=>r.push(r[0]),r=>r[0].status='failed',r=>r[0].reading.honored=false,r=>r[0].usage.unknownTokens=1,r=>r[0].fixtureDigest='different',r=>r[0].admittedCatalogDigest='different']){
  const copy=structuredClone(rows);mutate(copy);const result=summarizeComparablePairs(copy,config,true);assert.equal(result.eligiblePairs,0);assert.equal(result.pairs[0].patchToFullTokenRatio,null);
 }
 assert.equal(summarizeComparablePairs(rows,config,false).eligiblePairs,0);
 assert.equal(summarizeComparablePairs(rows,{...config,readingPolicy:'task'},true).eligiblePairs,0);
});
test('payload categories retain bytes without persisting prose or calling them tokens',()=>{
 const tool=item('read_tool_guide',{}, {description:'private prose'});
 assert.equal(tool.resultBytes,Buffer.byteLength(JSON.stringify({description:'private prose'})));
 assert.doesNotMatch(JSON.stringify(tool),/private prose/);
 const result=summarizePayloadCost({tools:[tool],requests:[]});assert.equal(result.toolResults.toolGuidance.calls,1);
 assert.match(result.interpretation,/not tokens/);
 const config={provider:'x',model:'x',evidencePath:'x',credentialSaltPath:'x',maxRequests:1,maxTokens:1,timeoutSeconds:1,repeats:1};
 assert.equal(validateLiveOptions({...config,readingPolicy:policy}).readingPolicy,policy);
 assert.throws(()=>validateLiveOptions({...config,readingPolicy:'skip'}));
});
