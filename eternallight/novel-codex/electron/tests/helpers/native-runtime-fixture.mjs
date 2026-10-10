import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {mkdtemp,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {nativeTestBinary} from './native-binaries.mjs';

export async function nativeRuntimeFixture(t) {
  const directory=await mkdtemp(join(tmpdir(),'conversation-crash-'));
  const database=join(directory,'story.sqlite3'),budgetDb=join(directory,'engine.sqlite3');
  const authorToken='crash-fixture-author-123456789',candidateToken='crash-fixture-candidate-123456789';
  let child,origin;
  const stop=async()=>{
    if(!child || child.exitCode!==null || child.signalCode!==null)return;
    const exit=new Promise(resolve=>child.once('exit',resolve));child.kill();await exit;
  };
  t.after(async()=>{await stop();await rm(directory,{recursive:true,force:true});});
  const launch=async()=>{
    child=spawn(nativeTestBinary('novel-codex-story-runtime'),['--port','0','--db',database,'--engine-budget-db',budgetDb,'--token-stdin'],{stdio:['pipe','pipe','pipe']});
    let output='',diagnostic='';child.stderr.on('data',data=>{diagnostic=(diagnostic+data).slice(-4000);});
    origin=await new Promise((resolve,reject)=>{
      const timer=setTimeout(()=>reject(Error(`Native startup timeout: ${diagnostic}`)),300_000);
      child.stdout.on('data',data=>{output+=data;const address=/Story Runtime listening on (http:\/\/127\.0\.0\.1:\d+)/.exec(output)?.[1];if(address){clearTimeout(timer);resolve(address);}});
      child.once('error',error=>{clearTimeout(timer);reject(error);});
      child.once('exit',code=>{clearTimeout(timer);reject(Error(`Native exited ${code}: ${diagnostic}`));});
      child.stdin.end(JSON.stringify({authorToken,candidateToken})+'\n');
    });
  };
  const request=async(path,body,method=body===undefined?'GET':'POST')=>{
    const response=await fetch(origin+path,{method,headers:{authorization:`Bearer ${authorToken}`,'content-type':'application/json'},...(body===undefined?{}:{body:JSON.stringify(body)})});
    const value=await response.json();
    assert.ok(response.ok,`${method} ${path}: ${response.status} ${JSON.stringify(value)}`);return value;
  };
  await launch();
  return {directory,budgetDb,authorToken,candidateToken,request,get origin(){return origin;},restart:async()=>{await stop();await launch();}};
}
