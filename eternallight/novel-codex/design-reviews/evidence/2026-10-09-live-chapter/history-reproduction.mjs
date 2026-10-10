import { mkdtempSync, mkdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { CodexAppServerAdapter } from '/Users/hilbert/private/codex/eternallight/novel-codex/electron/codex-adapter.mjs';
const dir=mkdtempSync(join(tmpdir(),'novel-history-'));
mkdirSync(join(dir,'sqlite'));
writeFileSync(join(dir,'config.toml'),'[mcp_servers.novel_workspace]\ncommand="/usr/bin/false"\nenabled=false\n');
const adapter=new CodexAppServerAdapter({command:['/Users/hilbert/private/codex/codex-rs/target/debug/codex','app-server'],cwd:dir,env:{PATH:process.env.PATH,HOME:process.env.HOME,CODEX_HOME:dir,CODEX_SQLITE_HOME:join(dir,'sqlite')},model:'gpt-5.6'});
try {
 const {thread}=await adapter.createThread();
 // Reproduce creation before the first turn/name persistence.
 try {const r=await adapter.readThread(thread.id);console.log(JSON.stringify({phase:'same-process',turns:r.turns.length}));}catch(e){console.log(JSON.stringify({phase:'same-process',error:e.message}));}
 await adapter.finishTaskOperation();
 try {const r=await adapter.readThread(thread.id);console.log(JSON.stringify({phase:'fresh-process',turns:r.turns.length}));}catch(e){console.log(JSON.stringify({phase:'fresh-process',error:e.message}));}
 console.log(JSON.stringify({directory:dir,thread:thread.id}));
}finally{await adapter.close();}
