import test from 'node:test';
import assert from 'node:assert/strict';
import {workspaceEditContext,messageBlockedByEdits} from '../workspace-edit-context.mjs';
const scope={projectId:'p',editionId:'edition-a',scopeId:'scope-a',branchId:'main'};
const message=(target=scope)=>({projectId:'p',input:{focus:null,media:{scope:target}},trigger:{kind:'immediate'}});
test('editing context validates the exact current scope contract and freezes the reported identity',()=>{
 const input={projectId:'p',dirty:true,chapterScope:null,mediaScope:{...scope}},context=workspaceEditContext(input);
 input.mediaScope.editionId='other';assert.equal(context.mediaScope.editionId,'edition-a');assert.ok(Object.isFrozen(context.mediaScope));
 for(const input of [{projectId:'p',dirty:true},{projectId:'p',dirty:true,chapterScope:null,mediaScope:{...scope,projectId:'other'}},{projectId:null,dirty:true,chapterScope:null,mediaScope:scope},{projectId:'p',dirty:true,chapterScope:null,mediaScope:{...scope,extra:true}},{projectId:'p',dirty:true,chapterScope:null,mediaScope:{...scope,scopeId:''}}])assert.throws(()=>workspaceEditContext(input));
});
test('exact media drafts and independent Editions allow saved-version messages; foreign branches and unknown scopes remain protected',()=>{
 const context=workspaceEditContext({projectId:'p',dirty:true,chapterScope:null,mediaScope:scope});
 for(const target of [{...scope,branchId:'other'},{...scope,editionId:'other'},{...scope,scopeId:'other'},{...scope,editionId:'other',scopeId:''},{...scope,editionId:'other',scopeId:'other',projectId:'wrong'}])assert.equal(messageBlockedByEdits(context,message(target)),true);
 assert.equal(messageBlockedByEdits(context,message(scope)),false);
 assert.equal(messageBlockedByEdits(context,message({...scope,editionId:'edition-b',scopeId:'scope-b'})),false);
 assert.equal(messageBlockedByEdits({...context,chapterScope:null,mediaScope:null},message({...scope,editionId:'edition-b',scopeId:'scope-b'})),true);
 assert.equal(messageBlockedByEdits(context,{...message(),input:{focus:null}}),true);
 assert.equal(messageBlockedByEdits(context,{...message(),trigger:{kind:'interlude'}}),false);
 assert.equal(messageBlockedByEdits(context,{...message(),input:{focus:{readOnly:true}}}),false);
 assert.equal(messageBlockedByEdits(context,{...message(),projectId:'other'}),false);
 assert.equal(messageBlockedByEdits({...context,dirty:false},message()),false);
});

test('only an explicit chapter-only draft permits focused saved-chapter messages',()=>{
 const chapterScope={projectId:'p',chapterId:'chapter-a'};
 const context=workspaceEditContext({projectId:'p',dirty:true,chapterScope,mediaScope:null});
 for(const chapterId of ['chapter-a','chapter-b']) assert.equal(messageBlockedByEdits(context,{...message(),input:{focus:{view:'chapters',chapterId}}}),false);
 for(const focus of [null,{view:'chapters'},{view:'outline',chapterId:'chapter-a'},{view:'chapters',chapterId:''}]) assert.equal(messageBlockedByEdits(context,{...message(),input:{focus}}),true);
 for(const chapter of [{...chapterScope,projectId:'wrong'},{...chapterScope,chapterId:''},{...chapterScope,extra:true}]) assert.throws(()=>workspaceEditContext({...context,chapterScope:chapter}));
 assert.throws(()=>workspaceEditContext({...context,mediaScope:scope}));
 assert.ok(Object.isFrozen(context.chapterScope));
});
