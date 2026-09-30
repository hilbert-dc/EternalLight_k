import { app, BrowserWindow, ipcMain } from "electron";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { writeFile } from "node:fs/promises";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { setTimeout as delay } from "node:timers/promises";
import { fileURLToPath } from "node:url";
import { PersistentTaskBudget } from "@eternallight/agent-engine";
import { registerChapterRebindHandlers } from "../../chapter-rebind-handlers.mjs";
import { registerGenerationReplanHandlers } from "../../generation-replan-handlers.mjs";
import { isTrustedIpcSender } from "../../navigation-policy.mjs";
import { seedRebindRuntime } from "./rebind-runtime-seed.mjs";
import { completeReboundChapter } from "./rebind-runtime-continuation.mjs";

const directory = process.argv[2];
const entry = join(directory, "index.html");
app.setPath("userData", join(directory, "electron-data"));
app.on("window-all-closed", () => {});
const author = "rebind-desktop-author-123456789012";
const candidate = "rebind-desktop-candidate-123456789";
const capability = "rebind-desktop-capability-12345678";
let runtime, window;
async function startRuntime() {
  const suffix = process.platform === "win32" ? ".exe" : "";
  runtime = spawn(fileURLToPath(new URL(`../../../native/target/debug/novel-codex-story-runtime${suffix}`, import.meta.url)),
    ["--port", "0", "--db", join(directory, "story.sqlite3"), "--engine-budget-db", join(directory, "engine-budget.sqlite3"), "--token-stdin"], { stdio: ["pipe", "pipe", "pipe"] });
  let stderr = "";
  runtime.stderr.on("data", chunk => { stderr = (stderr + chunk).slice(-4000); });
  return new Promise((resolve, reject) => {
    const lines = createInterface({ input: runtime.stdout });
    const timeout = setTimeout(() => reject(new Error(`Runtime startup timed out: ${stderr}`)), 10000);
    lines.on("line", line => { if (line.startsWith("Story Runtime listening on ")) { clearTimeout(timeout); resolve(line.slice("Story Runtime listening on ".length)); } });
    runtime.once("error", error => { clearTimeout(timeout); reject(error); });
    runtime.once("exit", () => { clearTimeout(timeout); reject(new Error(`Runtime exited: ${stderr}`)); });
    runtime.stdin.end(JSON.stringify({ authorToken: author, candidateToken: candidate }) + "\n");
  });
}
async function stopRuntime() {
  if (runtime && runtime.exitCode === null && runtime.signalCode === null) {
    const stopped = new Promise(resolve => runtime.once("exit", resolve)); runtime.kill(); await stopped;
  }
}
async function waitFor(check, description) {
  for (let attempt = 0; attempt < 150; attempt++) { if (await check()) return; await delay(100); }
  throw new Error(`Timed out: ${description}`);
}
async function main() {
  try {
    await app.whenReady();
    let origin = await startRuntime();
    const transport = async (path, options = {}, token = author) => {
      const response = await fetch(origin + path, { ...options, headers: { authorization: `Bearer ${token}`, "content-type": "application/json",
        ...(token === candidate ? { "x-novel-task-capability": capability } : {}) } });
      const text = await response.text();
      let result;
      try { result = JSON.parse(text); }
      catch { throw new Error(`${options?.method ?? "GET"} ${path}: HTTP ${response.status}: ${text.slice(0, 1000)}`); }
      if (!response.ok) throw new Error(`${response.status}: ${JSON.stringify(result)}`);
      return result;
    };
    const request = (path, body, token) => transport(path, body === undefined ? {} : { method: "POST", body: JSON.stringify(body) }, token);
    const chargedTaskIds = new Set();
    const recordUsage = async (projectId, runId, taskId) => {
      if (!chargedTaskIds.has(taskId)) {
        const task = await request(`/api/projects/${projectId}/tasks/${taskId}`);
        assert.ok(["completed", "failed", "cancelled"].includes(task.status));
        const budget = new PersistentTaskBudget({ databasePath: join(directory, "engine-budget.sqlite3"), runId: taskId,
          id: `budget-${taskId}`, maxRequests: 64, maxTokens: 1_000_000 });
        budget.complete(task.status);
        chargedTaskIds.add(taskId);
      }
      return request(`/api/projects/${projectId}/generation-runs/${runId}/provider-usage`, { taskId });
    };
    const fixture = await seedRebindRuntime(request, (path, body) => request(path, body, candidate), capability, recordUsage);
    const trusted = { handle(channel, listener) { ipcMain.handle(channel, (event, ...args) => {
      assert.ok(isTrustedIpcSender(event, window?.webContents, { isPackaged: true, packagedEntry: entry }), "Untrusted sender");
      return listener(event, ...args);
    }); } };
    registerChapterRebindHandlers(trusted, transport);
    registerGenerationReplanHandlers(trusted, transport, async (projectId, runId) => {
      const run = await request(`/api/projects/${projectId}/generation-runs/${runId}`);
      if (run.activeTaskId && !run.providerBudget.charges.some(charge => charge.taskId === run.activeTaskId)) {
        const task = await request(`/api/projects/${projectId}/tasks/${run.activeTaskId}`);
        if (["completed", "failed", "cancelled"].includes(task.status)) await recordUsage(projectId, runId, run.activeTaskId);
      }
    });
    trusted.handle("runtime:get-creative-workspace", (_event, projectId) => {
      assert.equal(projectId, fixture.projectId);
      return request(`${fixture.base}/creative-workspace`);
    });
    // Startup/read registrars remain fixture boundaries; every returned domain value is real HTTP.
    for (const [channel, path] of [
      ["runtime:list-generation-runs", "generation-runs"],
      ["runtime:get-search-audit", "search-audits"], ["runtime:get-entity-audit", "entity-audits"],
      ["runtime:get-relation-audit", "relation-audits"], ["runtime:get-knowledge-audit", "knowledge-audits"],
      ["runtime:get-snapshot-knowledge-audit", "snapshot-knowledge-audits"], ["runtime:get-fact-audit", "fact-audits"],
    ]) trusted.handle(channel, (_event, projectId) => { assert.equal(projectId, fixture.projectId); return request(`${fixture.base}/${path}`); });
    trusted.handle("runtime:list-work-tasks", (_event, projectId, cursor) => {
      assert.equal(projectId, fixture.projectId);
      return request(`${fixture.base}/work-tasks${cursor ? `?cursor=${encodeURIComponent(cursor)}` : ""}`);
    });
    trusted.handle("runtime:list-planning-impacts", (_event, projectId, cursor, limit) => {
      assert.equal(projectId, fixture.projectId);
      return request(`${fixture.base}/impacts?${new URLSearchParams({ limit: String(limit), ...(cursor ? { cursor } : {}) })}`);
    });
    trusted.handle("runtime:get-writer-preview", (_event, projectId, projectRevision, readingChapter, sceneOrder, povCharacterId, storyTime) => {
      assert.equal(projectId, fixture.projectId);
      const query = new URLSearchParams({ purpose: "writer", projectRevision: String(projectRevision), readingChapter: String(readingChapter), sceneOrder: String(sceneOrder), budget: "24", byteBudget: "60000" });
      if (povCharacterId !== null) query.set("povCharacterId", povCharacterId);
      if (storyTime !== null) query.set("storyTime", String(storyTime));
      return request(`${fixture.base}/context?${query}`);
    });
    const open = async () => {
      await writeFile(entry, `<html><head><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script>window.fixtureErrors=[]; window.addEventListener('error',e=>window.fixtureErrors.push(String(e.error?.stack ?? e.message).slice(0,3000))); window.rebindFixture=${JSON.stringify({ projectId: fixture.projectId, chapterId: fixture.chapterId, runId: fixture.runId })}</script><script src="test.js"></script></body></html>`);
      window = new BrowserWindow({ show: false, webPreferences: { preload: fileURLToPath(new URL("../../preload.cjs", import.meta.url)), contextIsolation: true, nodeIntegration: false, sandbox: true, backgroundThrottling: false } });
      await window.loadFile(entry);
    };
    const evaluate = async code => {
      try { return await window.webContents.executeJavaScript(code); }
      catch (error) { throw new Error(`Renderer action failed: ${code}\n${error.message}`); }
    };
    const click = async label => {
      await waitFor(() => evaluate(`[...document.querySelectorAll('button')].some(b => b.textContent === ${JSON.stringify(label)} && !b.disabled)`), label);
      await evaluate(`[...document.querySelectorAll('button')].find(b => b.textContent === ${JSON.stringify(label)}).click()`);
    };
    const contains = text => evaluate(`document.body.textContent.includes(${JSON.stringify(text)})`);
    const enter = (selector, value) => evaluate(`(() => { const t=document.querySelector(${JSON.stringify(selector)}); const set=Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set; set.call(t,${JSON.stringify(value)}); t.dispatchEvent(new Event('input',{bubbles:true})); })()`);
    await open(); await click("预览章纲调整");
    await waitFor(() => evaluate(`document.querySelector('.chapter-plan-rebind .rebind-preserved-prose')?.textContent.includes('他藏好密函。')`), "preserved source in completed preview");
    assert.equal(await evaluate(`[...document.querySelectorAll('button')].find(b=>b.textContent==='确认替换章纲关联').disabled`), true);
    await enter('textarea[aria-label="场景 1 正文"]', "临时未保存的改稿");
    await waitFor(() => evaluate(`[...document.querySelectorAll('button')].find(b=>b.textContent==='确认替换章纲关联').disabled`), "dirty editor blocks rebind");
    assert.equal((await request(`${fixture.base}/creative-workspace`)).chapters[0].candidate.body, "他藏好密函。");
    await click("放弃未保存输入");
    await waitFor(() => evaluate(`document.querySelector('textarea[aria-label="场景 1 正文"]').value === '他藏好密函。'`), "discard reloads persisted author draft");
    await waitFor(() => contains("语义完整性尚未认证"), "real Writer Context Pack rendered");
    assert.equal(await evaluate(`document.querySelector('.standalone-planning-card') === null`), true);
    assert.equal(await evaluate(`document.querySelector('.writer-runtime-preview strong').textContent`), "焚信");
    assert.equal(await evaluate(`document.querySelector('.writer-runtime-preview').textContent.includes('主角焚烧密函。')`), true);
    const previewSnapshot = await evaluate(`(() => { const p=document.querySelector('.writer-runtime-preview'); return {title:p.querySelector('summary').textContent,description:p.querySelector('p').textContent,planTitle:p.querySelector('strong').textContent,guidance:p.querySelectorAll('p')[1].textContent,position:[...p.querySelectorAll('small')].find(e=>e.textContent.startsWith('公开承诺')).textContent,source:[...p.querySelectorAll('small')].find(e=>e.textContent.startsWith('当前目标稿')).textContent,coverage:[...p.querySelectorAll('small')].find(e=>e.textContent.startsWith('覆盖：')).textContent}; })()`);
    if (process.env.NOVEL_CODEX_UI_SCREENSHOT) {
      await evaluate(`document.querySelector('.writer-runtime-preview').open=true; document.querySelector('.writer-runtime-preview').scrollIntoView({block:'start'})`);
      await writeFile(process.env.NOVEL_CODEX_UI_SCREENSHOT, (await window.webContents.capturePage()).toPNG());
    }
    await evaluate(`document.querySelector('input[type="checkbox"]').click()`);
    await click("确认替换章纲关联");
    await waitFor(() => contains("关联已更新至作品版本"), "binding receipt");
    const rebound = await request(fixture.uri);
    const expected = { ...fixture.run, status: "blocked", blockedReason: "chapter_plan_rebound", version: fixture.run.version + 1 };
    assert.deepEqual(rebound, expected);
    assert.equal((await request(`${fixture.base}/work-tasks/work-${fixture.runId}`)).status, "blocked");
    const workspace = await request(`${fixture.base}/creative-workspace`);
    assert.equal(workspace.chapters[0].candidate.id, fixture.draft.revisionId);
    assert.equal(workspace.chapterPlanBindings[0].plan_revision_id, fixture.newPlan.revisionId);
    assert.equal(workspace.chapterPlanBindings[0].writerProjection.publicTitle, "藏信");
    await waitFor(() => evaluate(`document.querySelector('.writer-runtime-preview strong')?.textContent === '藏信'`), "parent preview follows confirmed immutable binding");
    const reboundPreviewSnapshot = await evaluate(`(() => { const p=document.querySelector('.writer-runtime-preview'); return {planTitle:p.querySelector('strong').textContent,guidance:p.querySelectorAll('p')[1].textContent,position:[...p.querySelectorAll('small')].find(e=>e.textContent.startsWith('公开承诺')).textContent}; })()`);
    assert.deepEqual(reboundPreviewSnapshot, { planTitle: "藏信", guidance: "保留作者藏信稿，记录证词。", position: "公开承诺：无 · 视角：zhangheng · 故事时间：20" });
    await click("简报页");
    await waitFor(() => contains("章纲已更新，需重新规划"), "parent shows new run state");
    const blockedSnapshot = await evaluate(`[...document.querySelectorAll('.creative-alert[role="status"]')].find(e=>e.textContent.includes('自动创作')).textContent.trim()`);
    await click("预览重规划");
    await waitFor(() => evaluate(`document.querySelector('textarea[aria-label="重规划补充要求"]') !== null`), "current chapter replan preview");
    await enter('textarea[aria-label="重规划补充要求"]', "保留作者藏信稿，按新章纲补全");
    await evaluate(`window.failFormalRefresh = true`);
    await click("确认重规划");
    await waitFor(() => contains("重规划已保存，但刷新失败"), "durable success reported separately from refresh failure");
    const refreshSnapshot = await evaluate(`document.querySelector('.creative-alert.error').textContent`);
    const renewed = await request(fixture.uri);
    for (const key of ["usedTaskAttempts", "maxTaskAttempts", "deadlineAt"]) assert.deepEqual(renewed[key], fixture.run[key]);
    assert.equal(renewed.providerBudget.maxRequests, fixture.run.providerBudget.maxRequests);
    assert.equal(renewed.providerBudget.maxTokens, fixture.run.providerBudget.maxTokens);
    assert.equal(renewed.providerBudget.charges.length, fixture.run.providerBudget.charges.length + 1);
    assert.equal(renewed.replans[0].previousTaskId, fixture.oldTaskId);
    assert.equal(renewed.intentGeneration, 1);
    assert.equal(await evaluate(`document.querySelector('.generation-replan') === null`), true);
    window.destroy(); await open();
    await click("简报页");
    await waitFor(() => contains("继续创作"), "reopened parent reads running state");
    assert.equal(await evaluate(`document.querySelector('.generation-replan') === null`), true);
    assert.deepEqual(await request(fixture.uri), renewed);
    const task = await fixture.createTask(await request(`${fixture.uri}/next`, null), "renewed", workspace.revision);
    assert.equal(task.inputManifest.expectedCandidateRevisionId, fixture.draft.revisionId);
    assert.equal(task.target.intentGeneration, 1);
    const contextQuery = new URLSearchParams({ purpose: "writer", projectRevision: String(workspace.revision), readingChapter: "1",
      sceneOrder: String(task.target.sceneOrder), taskId: task.id, threadId: "renewed", budget: "24", byteBudget: "60000" });
    assert.equal(task.target.povCharacterId, "zhangheng");
    assert.equal(task.target.storyTime, 20);
    contextQuery.set("povCharacterId", task.target.povCharacterId);
    contextQuery.set("storyTime", String(task.target.storyTime));
    const context = await request(`${fixture.base}/context?${contextQuery}`, undefined, candidate);
    assert.equal(context.plannedEffects[0].value.planRevisionId, fixture.newPlan.revisionId);
    assert.equal(context.plannedEffects[0].value.chapters[0].publicTitle, "藏信");
    const late = { expectedRevision: workspace.revision, expectedCandidateRevisionId: fixture.draft.revisionId,
      idempotencyKey: "late-old-task", taskId: fixture.oldTaskId, threadId: "draft", payload: fixture.payload };
    await assert.rejects(() => request(`${fixture.base}/drafts/checkpoints`, late, candidate), /401:.*task_capability_invalid/);
    await request(`${fixture.base}/drafts/checkpoints`, { ...late, idempotencyKey: "new-writer", taskId: task.id, threadId: "renewed" }, candidate);
    const result = await request(`${fixture.base}/creative-workspace`);
    assert.equal(result.chapters[0].candidate.sourcePlanRefs.planRevisionId, fixture.newPlan.revisionId);
    assert.equal(result.chapters[0].candidate.sourcePlanRefs.chapterPlanId, "plan-1");
    assert.equal(result.chapters[0].candidate.body, "他藏好密函。");
    assert.equal(result.chapters[0].accepted, null);
    const { done, nextRun } = await completeReboundChapter({ request, candidateRequest: (path, body) => request(path, body, candidate),
      fixture, writer: task, revision: workspace.revision, recordUsage });
    assert.equal(done.providerBudget.charges.length, done.usedTaskAttempts);
    const nextUri = `${fixture.base}/generation-runs/${nextRun.id}`;
    const beforeRestart = await request(nextUri);
    const beforeWorkspace = await request(`${fixture.base}/creative-workspace`);
    window.destroy(); await stopRuntime(); origin = await startRuntime(); await open();
    assert.deepEqual(await request(fixture.uri), done);
    assert.deepEqual(await request(nextUri), beforeRestart);
    assert.deepEqual(await request(`${fixture.base}/creative-workspace`), beforeWorkspace);
    assert.equal((await request(`${fixture.base}/work-tasks/work-${fixture.runId}`)).status, "completed");
    await click("简报页");
    await waitFor(() => contains("规划后续章节"), "next run restored in parent workspace");
    assert.equal(await evaluate(`document.querySelector('.generation-replan') === null`), true);
    const waiting = await request(`${nextUri}/next`, null);
    assert.equal(waiting.action.kind, "awaitTask");
    assert.equal(waiting.action.taskId, beforeRestart.activeTaskId);
    assert.deepEqual(waiting.run, beforeRestart);
    assert.deepEqual(await evaluate(`window.fixtureErrors`), []);
    console.log(JSON.stringify({ passed: true, blockedSnapshot, refreshSnapshot, previewSnapshot, reboundPreviewSnapshot }));
  } catch (error) {
    const ui = window && !window.isDestroyed() ? await window.webContents.executeJavaScript(`JSON.stringify({errors:window.fixtureErrors,view:document.querySelector('.chapter-plan-rebind')?.outerHTML ?? document.body.textContent.slice(0,4000)})`).catch(() => "unavailable") : "closed";
    console.log(JSON.stringify({ passed: false, error: `${error.stack}\nUI: ${ui}` })); process.exitCode = 1;
  }
  finally {
    window?.destroy();
    await stopRuntime();
    app.exit(process.exitCode ?? 0);
  }
}
void main();
