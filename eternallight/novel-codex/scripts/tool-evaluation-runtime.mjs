// Isolated evaluator transport. No production dispatch, provider credentials or author data.
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { randomUUID } from "node:crypto";
const root = fileURLToPath(new URL("..", import.meta.url));
const binary = name => join(process.env.NOVEL_CODEX_NATIVE_DIR || join(root, "native/target/debug"), `${name}${process.platform === "win32" ? ".exe" : ""}`);
async function stop(child) {
  if (!child.pid || child.exitCode !== null || child.signalCode !== null) return;
  await new Promise(resolve => {
    const timer = setTimeout(() => child.kill("SIGKILL"), 5000);
    child.once("exit", () => { clearTimeout(timer); resolve(); });
    child.kill();
  });
}
export async function evaluationRuntime({ startupTimeoutMs = 30000 } = {}) {
  if (!Number.isSafeInteger(startupTimeoutMs) || startupTimeoutMs < 1 || startupTimeoutMs > 600000) throw Error("startupTimeoutMs must be within 1..600000");
  const directory = await mkdtemp(join(tmpdir(), "novel-tool-evaluation-")), children = [];
  const author = randomUUID(), candidate = randomUUID();
  const child = spawn(binary("novel-codex-story-runtime"), ["--port", "0", "--db", join(directory,"story.sqlite3"), "--engine-budget-db", join(directory,"engine.sqlite3"), "--token-stdin"], { stdio: ["pipe","pipe","pipe"] });
  children.push(child); child.stderr.resume();
  const close = async () => {
    for (const child of [...children].reverse()) await stop(child);
    await rm(directory, { recursive: true, force: true });
  };
  let origin;
  try {
    origin = await new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(Object.assign(Error("Evaluation Runtime startup timeout"),{code:"EVALUATION_TIMEOUT"})), startupTimeoutMs);
      const finish = (error, value) => { clearTimeout(timer); error ? reject(error) : resolve(value); };
      createInterface({ input: child.stdout }).on("line", line => {
        if (line.startsWith("Story Runtime listening on ")) finish(null, line.slice("Story Runtime listening on ".length));
      });
      child.once("error", finish); child.once("exit", () => finish(Error("Evaluation Runtime exited")));
      child.stdin.end(JSON.stringify({ authorToken: author, candidateToken: candidate }) + "\n");
    });
  } catch (error) { await close(); throw error; }
  return {
    directory, budgetPath:join(directory,"engine.sqlite3"),
    mcpEnvironment:capability=>({NOVEL_CODEX_RUNTIME_URL:origin,NOVEL_CODEX_CANDIDATE_TOKEN:candidate,NOVEL_CODEX_TASK_CAPABILITY:capability,NOVEL_CODEX_OPERATION:"generateChapter"}),
    close, binaries: { runtime: resolve(binary("novel-codex-story-runtime")), mcp: resolve(binary("novel-workspace-mcp")) },
    async request(method, path, body) {
      const response = await fetch(origin + path, { method, signal: AbortSignal.timeout(30000), headers: { authorization: `Bearer ${author}`, "content-type":"application/json" }, ...(body === undefined ? {} : { body:JSON.stringify(body) }) });
      const text = await response.text(); if (!response.ok) throw Error(`${method} ${path}: ${response.status} ${text.slice(0,1000)}`);
      return JSON.parse(text);
    },
    mcp(capability, thread) {
      const process = spawn(binary("novel-workspace-mcp"), [], { env: { PATH: globalThis.process.env.PATH, NOVEL_CODEX_RUNTIME_URL:origin, NOVEL_CODEX_CANDIDATE_TOKEN:candidate, NOVEL_CODEX_TASK_CAPABILITY:capability, NOVEL_CODEX_OPERATION:"generateChapter" }, stdio:["pipe","pipe","pipe"] });
      children.push(process); process.stderr.resume();
      let sequence=0; const pending=new Map();
      createInterface({input:process.stdout}).on("line", line => {
        let value;
        try { value=JSON.parse(line); }
        catch { rejectAll(Error("Evaluation MCP returned invalid JSON")); return; }
        const entry=pending.get(value.id);
        if (entry) { clearTimeout(entry.timer); pending.delete(value.id); value.error ? entry.reject(Error(JSON.stringify(value.error))) : entry.resolve(value.result); }
      });
      const rejectAll = error => { for (const entry of pending.values()) {clearTimeout(entry.timer);entry.reject(error);} pending.clear(); };
      process.on("error",rejectAll); process.on("exit",()=>rejectAll(Error("Evaluation MCP exited")));
      const rpc=(method,params={})=>new Promise((resolve,reject)=>{
        const id=++sequence, timer=setTimeout(()=>{pending.delete(id);reject(Object.assign(Error(`MCP timeout: ${method}`),{code:"EVALUATION_TIMEOUT"}));},30000);
        pending.set(id,{resolve,reject,timer}); process.stdin.write(JSON.stringify({jsonrpc:"2.0",id,method,params})+"\n");
      });
      return { rpc, close:()=>stop(process), call:(name,args)=>rpc("tools/call",{name,arguments:args,_meta:{rootThreadId:thread,callerThreadId:thread,parentThreadId:null,sessionId:thread,turnId:"evaluation-turn",callId:`call-${sequence+1}`}}) };
    },
  };
}
