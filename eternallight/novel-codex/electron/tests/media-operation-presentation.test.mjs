import assert from "node:assert/strict";
import { mkdtemp,rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import test from "node:test";
import { build } from "esbuild";
test("partial media edits show a saved result only with a successful bound revision receipt",async t=>{
 const directory=await mkdtemp(join(tmpdir(),"media-operation-presentation-"));t.after(()=>rm(directory,{recursive:true,force:true}));
 const output=join(directory,"presentation.mjs");await build({entryPoints:[new URL("../../src/operation-presentation.ts",import.meta.url).pathname],outfile:output,bundle:true,format:"esm",platform:"node"});
 const {operationPresentation}=await import(pathToFileURL(output).href);
 const result={edition:{id:"comic"},revisionId:"revision",documentSource:{revisionId:"revision"},documentHeader:{kind:"comic"},partialView:true};
 const item={tool:"patch_media_document",status:"completed",arguments:{request:{operations:[{kind:"setTitle",title:"雨夜"}]}},result:{structuredContent:result}};
 const success=operationPresentation(item);assert.equal(success.category,"write");assert.equal(success.outcome,"saved");assert.equal(success.referenceId,"revision");
 for(const changed of [{...item,status:"failed"},{...item,result:{structuredContent:{...result,documentSource:{revisionId:"other"}}}},{...item,result:{structuredContent:{...result,revisionId:null}}},{...item,arguments:{request:{operations:[]}}}])assert.equal(operationPresentation(changed).outcome,undefined);
});
