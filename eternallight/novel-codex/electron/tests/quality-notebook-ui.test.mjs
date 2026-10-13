import assert from "node:assert/strict";
import { test } from "node:test";
import { mkdtemp, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { build } from "esbuild";
import electron from "electron";
test("quiet quality notebook supports confirmation, trials, feedback and explicit reader start",async t=>{
 const dir=await mkdtemp(join(tmpdir(),"novel-quality-ui-"));t.after(()=>rm(dir,{recursive:true,force:true}));
 await build({entryPoints:[fileURLToPath(new URL("./fixtures/quality-notebook-ui.tsx",import.meta.url))],outfile:join(dir,"test.js"),bundle:true,platform:"browser",jsx:"automatic"});
 await writeFile(join(dir,"index.html"),'<html lang="zh-CN"><meta charset="utf-8"><link rel="stylesheet" href="test.css"><body><div id="root"></div><script src="test.js"></script></body></html>');
 let result;try{result=await promisify(execFile)(electron,[fileURLToPath(new URL("./fixtures/quality-notebook-ui-main.cjs",import.meta.url)),join(dir,"index.html")],{timeout:25000,env:{...process.env,ELECTRON_RUN_AS_NODE:""}});}catch(e){throw Error(`${e.stdout??""}\n${e.stderr??""}`);}
 const output=JSON.parse(result.stdout.trim());assert.equal(output.passed,true,output.error);assert.equal(output.readerRuns,1);assert.equal(output.feedbackKept,1);assert.equal(output.methodEffectKept,true);assert.deepEqual(output.mutations,["decide","packet","feedback","decide","note","note"]);
});
