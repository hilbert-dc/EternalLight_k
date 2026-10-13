import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {transform} from 'esbuild';
const {code}=await transform(await readFile(new URL('../../src/merge-chapter-draft.ts',import.meta.url),'utf8'),{loader:'ts',format:'esm'});
const {mergeChapterDraft,resolveChapterDraft}=await import(`data:text/javascript;base64,${Buffer.from(code).toString('base64')}`);
const base=()=>({title:'标题',sceneGroups:[{id:'group-a',sceneId:'a',body:'原场景一',locked:false},{id:'group-b',sceneId:'b',body:'原场景二',locked:false}]});
test('chapter merges independent scene bodies using latest group IDs and preserves inputs',()=>{
 const b=base(),l=structuredClone(b),r=structuredClone(b);l.sceneGroups[0].body='作者第一场';r.sceneGroups[1].body='后台第二场';r.title='新标题';r.sceneGroups.forEach(s=>s.id+='-new');
 const before=JSON.stringify([b,l,r]),m=mergeChapterDraft(b,l,r);
 assert.equal(m.title,'新标题');assert.deepEqual(m.sceneGroups.map(s=>[s.id,s.body]),[['group-a-new','作者第一场'],['group-b-new','后台第二场']]);
 m.sceneGroups[0].body='改变返回值';assert.equal(JSON.stringify([b,l,r]),before);
});
test('chapter rejects competing text, competing sequences, ambiguous identities and protection conflicts',()=>{
 for(const change of [
 (l,r)=>{l.title='作者';r.title='后台';},
 (l,r)=>{l.sceneGroups[0].body='作者';r.sceneGroups[0].body='后台';},
 (l,r)=>{l.sceneGroups.reverse();r.sceneGroups.pop();},
 (l,r)=>{l.sceneGroups[1].body="edited";r.sceneGroups.pop();},
 (l,r)=>{r.sceneGroups[1].sceneId='a';},
 (l,r)=>{r.sceneGroups[1].id=r.sceneGroups[0].id;},
 (l,r)=>{l.sceneGroups[0].body='作者';r.sceneGroups[0].locked=true;},
 (l,r)=>{l.sceneGroups[0].locked=true;r.sceneGroups[0].body='后台';},
 ]){const b=base(),l=structuredClone(b),r=structuredClone(b);change(l,r);assert.equal(mergeChapterDraft(b,l,r),null);}
});
test('identical changes converge and independent protection changes are retained',()=>{
 const b=base(),l=structuredClone(b),r=structuredClone(b);l.sceneGroups[0].body=r.sceneGroups[0].body='共同修改';r.sceneGroups[1].locked=true;
 assert.deepEqual(mergeChapterDraft(b,l,r),r);
});

test('one-sided insert, delete and reorder preserve independent edits from either side',()=>{
 for (const swap of [false,true]) for(const operation of ['insert','delete','reorder']) {
  const b=base(),l=structuredClone(b),r=structuredClone(b);
  l.sceneGroups[0].body='本地正文';
  if(operation==='insert') r.sceneGroups.splice(1,0,{id:'new',sceneId:'new',body:'新增场景',locked:false});
  if(operation==='delete') r.sceneGroups.pop();
  if(operation==='reorder') r.sceneGroups.reverse();
  const m=mergeChapterDraft(b,...(swap?[r,l]:[l,r]));assert.ok(m);
  assert.deepEqual(m.sceneGroups.map(s=>s.sceneId),r.sceneGroups.map(s=>s.sceneId));
  assert.equal(m.sceneGroups.find(s=>s.sceneId==='a').body,'本地正文');
 }
});
test('structural merge rejects protected deletion, conflicting additions and output ID collisions',()=>{
 const b=base(),l=structuredClone(b),r=structuredClone(b);
 b.sceneGroups[1].locked=l.sceneGroups[1].locked=r.sceneGroups[1].locked=true;r.sceneGroups.pop();
 assert.equal(mergeChapterDraft(b,l,r),null);
 for(const kind of ['addition','collision']) {
  const b=base(),l=structuredClone(b),r=structuredClone(b);
  l.sceneGroups.push({id:'new',sceneId:'new',body:'local'});
  if(kind==='addition') r.sceneGroups.push({id:'new-r',sceneId:'new',body:'remote'});
  else r.sceneGroups[0].id='new';
  assert.equal(mergeChapterDraft(b,l,r),null);
 }
});

test('explicit conflict resolution keeps independent edits and chooses competing fields or structure',()=>{
 for(const preference of ['local','remote']) {
  const b=base(),l=structuredClone(b),r=structuredClone(b);
  l.title='作者标题';l.sceneGroups[0].body='作者冲突';r.sceneGroups[0].body='后台冲突';r.sceneGroups[1].body='后台独立';
  const m=resolveChapterDraft(b,l,r,preference);
  assert.equal(m.title,'作者标题');assert.equal(m.sceneGroups[0].body,preference==='local'?'作者冲突':'后台冲突');assert.equal(m.sceneGroups[1].body,'后台独立');
  r.sceneGroups.shift();
  const structural=resolveChapterDraft(b,l,r,preference);assert.ok(structural);
  assert.deepEqual(structural.sceneGroups.map(s=>s.sceneId),(preference==='local'?l:r).sceneGroups.map(s=>s.sceneId));
  b.sceneGroups[0].locked=true;
  assert.equal(resolveChapterDraft(b,l,r,preference),null,'selection bypassed protection');
 }
});

test('conflict selection cannot delete a scene newly protected by either side',()=>{
 for(const preference of ['local','remote']) for(const swap of [false,true]) {
  const b=base(),l=structuredClone(b),r=structuredClone(b);
  l.sceneGroups.shift();r.sceneGroups[0].locked=true;
  assert.equal(resolveChapterDraft(b,...(swap?[r,l]:[l,r]),preference),null);
 }
});
