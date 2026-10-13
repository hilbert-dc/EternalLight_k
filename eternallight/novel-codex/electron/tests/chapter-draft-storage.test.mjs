import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {transform} from 'esbuild';
const load=async path=>{const {code}=await transform(await readFile(new URL(path,import.meta.url),'utf8'),{loader:'ts',format:'esm'});return import(`data:text/javascript;base64,${Buffer.from(code).toString('base64')}`);};
const {chapterDraftKey,readChapterDraft}=await load('../../src/chapter-draft-storage.ts');
const {clearAuthorDrafts}=await load('../../src/author-draft-storage.ts');
const storage=()=>{const values=new Map();return {get length(){return values.size;},key:i=>[...values.keys()][i]??null,getItem:key=>values.get(key)??null,setItem:(key,value)=>values.set(key,value),removeItem:key=>values.delete(key)};};
test('chapter recovery is isolated by author/project/chapter and author removal preserves other drafts',()=>{
 const previous=globalThis.localStorage,s=storage();globalThis.localStorage=s;
 try{
  const key=chapterDraftKey('alice','project','chapter'), other=chapterDraftKey('bob','project','chapter');
  const draft={baseProjectRevision:1,baseCandidateRevisionId:'draft-1',baseContent:'原稿',title:'标题',sceneGroups:[{id:'scene-group',sceneId:'scene',body:'本地正文',locked:false}]};
  s.setItem(key,JSON.stringify(draft));s.setItem(other,JSON.stringify(draft));
  assert.deepEqual(readChapterDraft(key),draft);assert.equal(readChapterDraft(chapterDraftKey('alice','other','chapter')),null);assert.equal(readChapterDraft(chapterDraftKey('alice','project','other')),null);
  clearAuthorDrafts('alice',s);assert.equal(s.getItem(key),null);assert.ok(s.getItem(other));
 }finally{globalThis.localStorage=previous;}
});
test('invalid chapter recovery is rejected without modifying the stored record',()=>{
 const previous=globalThis.localStorage,s=storage();globalThis.localStorage=s;
 try{for(const raw of ['{invalid','null',JSON.stringify({title:'无版本',sceneGroups:[]})]){s.setItem('draft',raw);assert.throws(()=>readChapterDraft('draft'),/本地章节草稿无法读取/);assert.equal(s.getItem('draft'),raw);}}
 finally{globalThis.localStorage=previous;}
});
