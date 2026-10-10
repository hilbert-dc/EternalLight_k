import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp,rm,writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { build } from "esbuild";
import electron from "electron";
test("author canon adoption and source inspection preserve fixed content, drafts and uncertain save identities",async t=>{
  const directory=await mkdtemp(join(tmpdir(),"media-context-ui-"));t.after(()=>rm(directory,{recursive:true,force:true}));
  await build({entryPoints:[fileURLToPath(new URL("./fixtures/media-context-ui.tsx",import.meta.url))],outfile:join(directory,"test.js"),bundle:true,platform:"browser",jsx:"automatic"});
  await writeFile(join(directory,"index.html"),'<html lang="zh-CN"><meta charset="utf-8"><link rel="stylesheet" href="test.css"><div id="root"></div><script src="test.js"></script></html>');
  let output;try{output=await promisify(execFile)(electron,[fileURLToPath(new URL("./fixtures/creative-workspace-ui-main.cjs",import.meta.url)),join(directory,"index.html")],{timeout:30000,env:{...process.env,ELECTRON_RUN_AS_NODE:""}});}catch(error){throw Error(`${error.stdout??""}\n${error.stderr??""}`,{cause:error});}
  const result=JSON.parse(output.stdout.trim());assert.equal(result.passed,true,result.error);assert.equal(result.adoptions,2);assert.equal(result.canonSaves,2);assert.equal(result.modelCalls,0);
});
