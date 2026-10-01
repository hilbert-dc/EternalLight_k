import assert from "node:assert/strict";
import { app, BrowserWindow, dialog } from "electron";
import { pathToFileURL } from "node:url";
import { join } from "node:path";

const appRoot = process.argv[2];
const approvals = [];
process.on("uncaughtException", error => {
  console.log("DESKTOP_RESULT:" + JSON.stringify({ passed: false, error: String(error.stack).slice(0, 3000) }));
  app.exit(1);
});
// Explicit author actor at the OS dialog boundary; Runtime approval/budget
// contracts remain the actual production handlers and transactions.
dialog.showMessageBox = async (_owner, options) => {
  approvals.push({ title: options.title, buttons: options.buttons });
  return { response: 1 };
};
const timeout = setTimeout(() => {
  console.log("DESKTOP_RESULT:" + JSON.stringify({ passed: false, error: "production desktop deadline exceeded" }));
  for (const window of BrowserWindow.getAllWindows()) window.destroy();
  app.quit();
}, 45000);

app.once("browser-window-created", (_event, window) => {
  window.webContents.once("did-finish-load", async () => {
    try {
      const result = await window.webContents.executeJavaScript(`(async () => {
        const api = window.novelCodex;
        if (!api || !document.querySelector('#root')) throw new Error('production renderer/preload missing');
        window.desktopEvents = [];
        const unsubscribe = api.agent.onEvent(event => window.desktopEvents.push(event));
        const { project } = await api.runtime.createProject('生产桌面只读验收', '抄写员决定为未来留下证词。');
        const before = await api.runtime.getCreativeWorkspace(project.id);
        const discussion = await api.agent.sendPrompt(project.id, '分析作品定位；不要修改内容。');
        const after = await api.runtime.getCreativeWorkspace(project.id);
        const works = await api.runtime.listWorkTasks(project.id);
        const work = works.tasks.find(work => work.steps.some(step => step.currentAttemptId === discussion.workflow.task.id));
        const receipt = await api.runtime.getTaskFinalReply(project.id, discussion.workflow.task.id);
        unsubscribe();
        return { projectId: project.id, before, after, discussion, work, receipt,
          eventMethods: window.desktopEvents.map(event => event.method) };
      })()`);
      assert.equal(result.discussion.message, "DESKTOP_CONTEXT_READ");
      assert.equal(result.discussion.workflow.task.operation, "advisory");
      assert.deepEqual(result.discussion.proposedActions, []);
      assert.equal(result.after.revision, result.before.revision);
      assert.equal(result.after.sourceDigest, result.before.sourceDigest);
      assert.equal(result.work.status, "completed");
      assert.equal(result.receipt.binding.text, "DESKTOP_CONTEXT_READ");
      assert.ok(result.eventMethods.includes("item/completed"));
      assert.equal(approvals.length, 1);
      console.log("DESKTOP_RESULT:" + JSON.stringify({ passed: true, taskId: result.discussion.workflow.task.id,
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
