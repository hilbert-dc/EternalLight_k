import test from 'node:test';
import assert from 'node:assert/strict';
import {workspaceEditContext,messageBlockedByEdits} from '../workspace-edit-context.mjs';
const scope={projectId:'p',editionId:'edition-a',scopeId:'scope-a',branchId:'main'};
const message=(target=scope)=>({projectId:'p',input:{focus:null,media:{scope:target}},trigger:{kind:'immediate'}});
test('editing context validates the exact current scope contract and freezes the reported identity',()=>{
 const input={projectId:'p',dirty:true,mediaScope:{...scope}},context=workspaceEditContext(input);
 input.mediaScope.editionId='other';assert.equal(context.mediaScope.editionId,'edition-a');assert.ok(Object.isFrozen(context.mediaScope));
 for(const input of [{projectId:'p',dirty:true},{projectId:'p',dirty:true,mediaScope:{...scope,projectId:'other'}},{projectId:null,dirty:true,mediaScope:scope},{projectId:'p',dirty:true,mediaScope:{...scope,extra:true}},{projectId:'p',dirty:true,mediaScope:{...scope,scopeId:''}}])assert.throws(()=>workspaceEditContext(input));
});
test('only independent frozen Editions bypass an unsaved edit; branches and unknown scopes remain protected',()=>{
 const context=workspaceEditContext({projectId:'p',dirty:true,mediaScope:scope});
 for(const target of [scope,{...scope,branchId:'other'},{...scope,editionId:'other'},{...scope,scopeId:'other'},{...scope,editionId:'other',scopeId:''},{...scope,editionId:'other',scopeId:'other',projectId:'wrong'}])assert.equal(messageBlockedByEdits(context,message(target)),true);
 assert.equal(messageBlockedByEdits(context,message({...scope,editionId:'edition-b',scopeId:'scope-b'})),false);
 assert.equal(messageBlockedByEdits({...context,mediaScope:null},message({...scope,editionId:'edition-b',scopeId:'scope-b'})),true);
 assert.equal(messageBlockedByEdits(context,{...message(),input:{focus:null}}),true);
 assert.equal(messageBlockedByEdits(context,{...message(),trigger:{kind:'interlude'}}),false);
 assert.equal(messageBlockedByEdits(context,{...message(),input:{focus:{readOnly:true}}}),false);
 assert.equal(messageBlockedByEdits(context,{...message(),projectId:'other'}),false);
 assert.equal(messageBlockedByEdits({...context,dirty:false},message()),false);
});
