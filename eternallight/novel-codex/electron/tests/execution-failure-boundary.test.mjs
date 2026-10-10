import assert from "node:assert/strict";
import test from "node:test";
import { readFile } from "node:fs/promises";
import { runInNewContext } from "node:vm";
import { NativeRuntimeError } from "../runtime-request-error.mjs";

test("trusted Host and real preload preserve execution cancellation across serialized IPC", async () => {
  const source = await readFile(new URL("../main.mjs", import.meta.url), "utf8");
  const handlers = new Map();
  const context = { ipcMain: { handle: (channel, handler) => handlers.set(channel, handler) },
    NativeRuntimeError, assertTrustedIpcSender: () => {}, };
  runInNewContext(source.slice(source.indexOf("const electronIpcMain ="), source.indexOf("function isTrustedRendererUrl("))
    + "\nglobalThis.registry = trustedIpcMain;", context);
  const interruption = Object.assign(new Error("private diagnostic"), { code: "TASK_CANCELLED" });
  context.registry.handle("agent:resume-work-task", async () => { throw interruption; });
  let api;
  runInNewContext(await readFile(new URL("../preload.cjs", import.meta.url), "utf8"), {
    crypto: { randomUUID: () => "request-test" }, process: { platform: "test" },
    require: () => ({ contextBridge: { exposeInMainWorld: (key, value) => { if (key === "novelCodex") api = value; } },
      ipcRenderer: { invoke: async (channel, ...args) => JSON.parse(JSON.stringify(await handlers.get(channel)({}, ...args))) } }),
  });
  await assert.rejects(api.agent.resumeWorkTask("project", "work-original"), error => {
    assert.equal(error.code, "TASK_CANCELLED");
    assert.equal(error.message, "生成已停止");
    assert.equal(error.nativeFailure, undefined);
    return true;
  });
  const realFailure = new Error("provider unavailable");
  context.registry.handle("agent:resume-work-task", async () => { throw realFailure; });
  await assert.rejects(api.agent.resumeWorkTask("project", "work-original"), error => error === realFailure);
});

test("actual isolated Electron bridge preserves cancellation for the Renderer", async () => {
  const { mkdtemp, writeFile, rm } = await import("node:fs/promises");
  const { tmpdir } = await import("node:os");
  const { join } = await import("node:path");
  const { fileURLToPath } = await import("node:url");
  const { execFile } = await import("node:child_process");
  const { promisify } = await import("node:util");
  const { default: electron } = await import("electron");
  const directory = await mkdtemp(join(tmpdir(), "execution-failure-bridge-"));
  try {
    const entry = join(directory, "main.cjs");
    await writeFile(join(directory, "index.html"), "<!doctype html><html><body></body></html>");
    await writeFile(entry, `const {app,BrowserWindow,ipcMain}=require('electron');
      app.setPath('userData', ${JSON.stringify(join(directory, "user-data"))});
      ipcMain.handle('agent:resume-work-task',()=>({executionRequestFailure:{code:'TASK_CANCELLED',message:'生成已停止'}}));
      app.whenReady().then(async()=>{const w=new BrowserWindow({show:false,webPreferences:{sandbox:true,contextIsolation:true,nodeIntegration:false,preload:${JSON.stringify(fileURLToPath(new URL('../preload.cjs', import.meta.url)))}}});
      w.webContents.on("console-message",(_event,_level,message)=>process.stderr.write(message+"\\n"));
      await w.loadFile(${JSON.stringify(join(directory, "index.html"))});
      const result=await w.webContents.executeJavaScript("window.novelCodex.agent.resumeWorkTask('project','work-original').then(()=>({resolved:true}),error=>({code:error.code,message:error.message}))");
      process.stdout.write(JSON.stringify(result));app.quit();}).catch(error=>{process.stderr.write(String(error));app.exit(1)});`);
    const result = await promisify(execFile)(electron, [entry], { timeout: 15000, env: { ...process.env, ELECTRON_RUN_AS_NODE: "" } });
    assert.deepEqual(JSON.parse(result.stdout), { code: "TASK_CANCELLED", message: "生成已停止" });
  } finally { await rm(directory, { recursive: true, force: true }); }
});
