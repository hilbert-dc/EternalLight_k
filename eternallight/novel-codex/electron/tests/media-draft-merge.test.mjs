import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { transform } from 'esbuild';
const { code } = await transform(await readFile(new URL('../../src/multimedia/merge-media-draft.ts', import.meta.url), 'utf8'), { loader: 'ts', format: 'esm' });
const { mergeMediaDraft, resolveMediaDraft } = await import(`data:text/javascript;base64,${Buffer.from(code).toString('base64')}`);
const base = () => ({kind:'comic',title:'作品',pages:[{id:'a',title:'第一页',panels:[{id:'a1',description:'雨',locked:false}]},{id:'b',title:'第二页',panels:[]}]});
const copy = structuredClone;
test('independent object and field edits merge without mutating any input', () => {
 const b=base(), l=copy(b), r=copy(b); l.pages[0].panels[0].description='雪';r.pages[1].title='后台第二页';r.pages[0].panels[0].locked=true;
 const before=JSON.stringify([b,l,r]); const merged=mergeMediaDraft(b,l,r);
 assert.equal(merged.pages[0].panels[0].description,'雪');assert.equal(merged.pages[0].panels[0].locked,true);assert.equal(merged.pages[1].title,'后台第二页');
 merged.pages[0].title='别名检查';assert.equal(JSON.stringify([b,l,r]),before);
});
test('one-sided sequence changes retain independent edits and stable ordering', () => {
 const b=base(),l=copy(b),r=copy(b);l.pages.reverse();r.pages[0].title='远端第一页';
 assert.deepEqual(mergeMediaDraft(b,l,r).pages.map(p=>p.id),['b','a']);assert.equal(mergeMediaDraft(b,l,r).pages[1].title,'远端第一页');
 l.pages=l.pages.filter(p=>p.id!=='b');assert.deepEqual(mergeMediaDraft(b,l,r).pages.map(p=>p.id),['a']);
 l.pages.push({id:'c',title:'新增',panels:[]});assert.equal(mergeMediaDraft(b,l,r).pages[1].id,'c');
});
test('overlaps, deletion versus edit, ambiguous sequences and duplicate identities fail atomically', () => {
 const b=base();
 for(const change of [
  (l,r)=>{l.title='本地';r.title='远端';},
  (l,r)=>{l.pages.pop();r.pages[1].title='远端';},
  (l,r)=>{l.pages.reverse();r.pages.push({id:'c',title:'新增',panels:[]});},
  (l,r)=>{l.pages[1].id='a';r.pages[0].title='远端';},
 ]){const l=copy(b),r=copy(b);change(l,r);const before=JSON.stringify([b,l,r]);assert.equal(mergeMediaDraft(b,l,r),null);assert.equal(JSON.stringify([b,l,r]),before);}
 assert.equal(mergeMediaDraft(b,b,{kind:'animation'}),null);
});
test('animation shot and clip edits stay independent but the same clip time cannot be overwritten', () => {
 const b={kind:'animation',shots:[{id:'s',description:'远景'}],timeline:{tracks:[{id:'t',clips:[{id:'c',shotId:'s',start:{value:0,timescale:24}}]}]}},l=copy(b),r=copy(b);
 l.shots[0].description='近景';r.timeline.tracks[0].clips[0].start.value=24;
 assert.equal(mergeMediaDraft(b,l,r).shots[0].description,'近景');assert.equal(mergeMediaDraft(b,l,r).timeline.tracks[0].clips[0].start.value,24);
 l.timeline.tracks[0].clips[0].start.value=48;assert.equal(mergeMediaDraft(b,l,r),null);
});

test('explicit conflict choices preserve independent fields and do not modify either source', () => {
 const b=base(),l=copy(b),r=copy(b);l.title='作者标题';r.title='后台标题';l.pages[0].title='作者第一页';r.pages[1].title='后台第二页';
 const before=JSON.stringify([b,l,r]);
 for(const choice of ['local','remote']){
  const result=resolveMediaDraft(b,l,r,choice);
  assert.equal(result.title,choice==='local'?l.title:r.title);
  assert.equal(result.pages[0].title,l.pages[0].title);assert.equal(result.pages[1].title,r.pages[1].title);
 }
 assert.equal(JSON.stringify([b,l,r]),before);
});
test('deletion/edit and competing sequence choices use valid complete lists without holes', () => {
 const b=base(),l=copy(b),r=copy(b);l.pages.pop();r.pages[1].title='修改被删除页';
 assert.deepEqual(resolveMediaDraft(b,l,r,'local').pages.map(p=>p.id),['a']);
 assert.deepEqual(resolveMediaDraft(b,l,r,'remote').pages,r.pages);
 l.pages=copy(b.pages).reverse();r.pages.push({id:'c',title:'新增',panels:[]});
 assert.deepEqual(resolveMediaDraft(b,l,r,'local').pages,l.pages);
 assert.deepEqual(resolveMediaDraft(b,l,r,'remote').pages,r.pages);
 l.pages[1].id=l.pages[0].id;
 assert.equal(resolveMediaDraft(b,l,r,'local'),null);
});
