import assert from "node:assert/strict";
import { app, BrowserWindow, dialog } from "electron";
import { pathToFileURL } from "node:url";
import { join } from "node:path";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { DESKTOP_JOURNEY_MS, DESKTOP_RUNTIME_STARTUP_MS } from "../helpers/desktop-deadlines.mjs";

const appRoot = process.argv[2];
const scenario = JSON.parse(process.argv[3] ?? "{}");
const evidencePath = join(process.env.NOVEL_CODEX_USER_DATA_DIR, "agent-engine", "capability-evidence.json");
const rejectedEvidence = JSON.stringify({ version: 1, records: [{ dependencies: null }] });
if (scenario.invalidEvidence) {
  mkdirSync(join(process.env.NOVEL_CODEX_USER_DATA_DIR, "agent-engine"), { recursive: true });
  writeFileSync(evidencePath, rejectedEvidence, { mode: 0o600 });
}
const approvals = [];
process.on("uncaughtException", error => {
  console.log("DESKTOP_RESULT:" + JSON.stringify({ passed: false, error: String(error.stack).slice(0, 3000) }));
  app.exit(1);
});
// Record native dialogs without auto-approving them. Author submission must
// start through the actual production budget handlers without another dialog.
dialog.showMessageBox = async (_owner, options) => {
  approvals.push({ title: options.title, buttons: options.buttons });
  return { response: 0 };
};
function deadline(stage, durationMs) {
  return setTimeout(() => {
    console.log("DESKTOP_RESULT:" + JSON.stringify({ passed: false, error: `production desktop ${stage} deadline exceeded` }));
    for (const window of BrowserWindow.getAllWindows()) window.destroy();
    app.quit();
  }, durationMs);
}
let timeout = deadline("Runtime startup", DESKTOP_RUNTIME_STARTUP_MS);

app.once("browser-window-created", (_event, window) => {
  window.webContents.once("did-finish-load", async () => {
    clearTimeout(timeout);
    timeout = deadline("task journey", DESKTOP_JOURNEY_MS);
    try {
      if (scenario.invalidEvidence) {
        const result = await window.webContents.executeJavaScript(`(async () => {
          const api = window.novelCodex;
          const status = await api.agent.status();
          const connections = await api.agent.listConnections();
          const created = await api.runtime.createProject('认证缓存故障验收', '只测试本地读写，不执行模型。');
          const state = await api.runtime.getState(created.project.id);
          return { status, connections, project: state.project, runtime: await api.runtime.status() };
        })()`);
        assert.equal(result.status.configured, false);
        assert.match(result.status.error, /认证缓存无法验证/);
        assert.ok(result.connections.connections.every(connection => connection.status === "unverified"));
        assert.equal(result.project.title, "认证缓存故障验收");
        assert.equal(result.runtime.state, "online");
        assert.equal(readFileSync(evidencePath, "utf8"), rejectedEvidence);
        assert.equal(approvals.length, 0);
        console.log("DESKTOP_RESULT:" + JSON.stringify({ passed: true, cacheRejected: true, manualWorkspaceAvailable: true, approvals: 0 }));
        return;
      }
      const coldStartup = JSON.parse(process.argv[3] ?? "{}");
      let originalConfig;
      const configPath = join(process.env.NOVEL_CODEX_USER_DATA_DIR, "codex-home", "config.toml");
      if (coldStartup.delayMs || coldStartup.failStartup) {
        const config = readFileSync(configPath, "utf8");
        originalConfig = config;
        const section = /\[mcp_servers\.novel_workspace\]\n([\s\S]*)/.exec(config);
        assert.ok(section, "fixture lacks the production workspace MCP config");
        const command = JSON.parse(/^command = (.+)$/m.exec(section[1])[1]);
        const args = [fileURLToPath(new URL("./delayed-workspace-mcp.mjs", import.meta.url)), coldStartup.failStartup ? "fail" : String(coldStartup.delayMs), command];
        const delayed = section[1].replace(/^command = .+$/m, `command = ${JSON.stringify(coldStartup.nodeCommand)}`)
          .replace(/^args = .+$/m, `args = ${JSON.stringify(args)}`);
        writeFileSync(configPath, config.replace(section[1], delayed), { mode: 0o600 });
      }
      const journey = `(async () => {
        const api = window.novelCodex;
        if (!api || !document.querySelector('#root')) throw new Error('production renderer/preload missing');
        window.desktopEvents = [];
        const unsubscribe = api.agent.onEvent(event => window.desktopEvents.push(event));
        window.desktopProject ??= await api.runtime.createProject('生产桌面只读验收', '抄写员决定为未来留下证词。');
        const { project } = window.desktopProject;
        const before = await api.runtime.getCreativeWorkspace(project.id);
        let discussion;
        try { discussion = await api.agent.sendPrompt(project.id, '分析作品定位；不要修改内容。'); }
        catch (error) {
          if (!${JSON.stringify(!!coldStartup.failStartup)}) throw error;
          unsubscribe();
          return { projectId: project.id, startupError: String(error.message), before,
            after: await api.runtime.getCreativeWorkspace(project.id),
            works: await api.runtime.listWorkTasks(project.id) };
        }
        const after = await api.runtime.getCreativeWorkspace(project.id);
        const works = await api.runtime.listWorkTasks(project.id);
        const work = works.tasks.find(work => work.steps.some(step => step.currentAttemptId === discussion.workflow.task.id));
        const receipt = await api.runtime.getTaskFinalReply(project.id, discussion.workflow.task.id);
        const history = await api.agent.readThread(project.id, discussion.workflow.task.threadId);
        unsubscribe();
        return { projectId: project.id, before, after, discussion, work, receipt, history,
          eventMethods: window.desktopEvents.map(event => event.method) };
      })()`;
      let result = await window.webContents.executeJavaScript(journey);
      if (coldStartup.failStartup) {
        assert.match(result.startupError, /required MCP servers failed to initialize/);
        assert.equal(approvals.length, 0);
        assert.deepEqual(result.works.tasks, []);
        assert.equal(result.after.sourceDigest, result.before.sourceDigest);
        assert.equal(result.after.revision, result.before.revision);
        if (!coldStartup.retryAfterFailure) {
          console.log("DESKTOP_RESULT:" + JSON.stringify({ passed: true, startupBlocked: true, approvals: 0 }));
          return;
        }
        const failed = result;
        writeFileSync(configPath, originalConfig, { mode: 0o600 });
        result = await window.webContents.executeJavaScript(journey);
        assert.equal(result.projectId, failed.projectId);
        assert.equal(result.before.sourceDigest, failed.before.sourceDigest);
        assert.equal(result.before.revision, failed.before.revision);
      }
      assert.equal(result.discussion.message, "DESKTOP_CONTEXT_READ");
      assert.equal(result.discussion.workflow.task.operation, "advisory");
      assert.deepEqual(result.discussion.proposedActions, []);
      assert.equal(result.after.revision, result.before.revision);
      assert.equal(result.after.sourceDigest, result.before.sourceDigest);
      assert.equal(result.work.status, "completed");
      assert.equal(result.receipt.binding.text, "DESKTOP_CONTEXT_READ");
      const restoredTurn = result.history.turns.find(turn => turn.id === result.discussion.turn.id);
      assert.ok(restoredTurn, "completed engine turn must remain readable without inference");
      assert.deepEqual(restoredTurn.items.filter(item => item.type === "userMessage").map(item => item.text), ["分析作品定位；不要修改内容。"]);
      assert.ok(restoredTurn.items.some(item => item.type === "agentMessage" && item.text === "DESKTOP_CONTEXT_READ"));
      assert.ok(result.eventMethods.includes("item/completed"));
      assert.equal(approvals.length, 0);
      console.log("DESKTOP_RESULT:" + JSON.stringify({ passed: true, taskId: result.discussion.workflow.task.id,
        recoveredStartup: !!coldStartup.retryAfterFailure,
        workStatus: result.work.status, approvals: approvals.length, eventMethods: [...new Set(result.eventMethods)],
        receipt: result.receipt }));
    } catch (error) {
      console.log("DESKTOP_RESULT:" + JSON.stringify({ passed: false, error: String(error.stack).slice(0, 3000) }));
    } finally {
      clearTimeout(timeout);
      if (!window.isDestroyed()) window.destroy();
      app.quit();
    }
  });
});
await import(pathToFileURL(join(appRoot, "electron", "main.mjs")));
