import { app, BrowserWindow, ipcMain } from "electron";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { writeFile } from "node:fs/promises";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { setTimeout as delay } from "node:timers/promises";
import { fileURLToPath } from "node:url";
import { registerConditionEvaluationHandlers } from "../../condition-evaluation-handlers.mjs";
import { isTrustedIpcSender } from "../../navigation-policy.mjs";
import { seedConditionRuntime } from "./condition-runtime-seed.mjs";

const directory = process.argv[2];
const entry = join(directory, "index.html");
app.setPath("userData", join(directory, "electron-data"));
app.on("window-all-closed", () => {});
let runtime, window;
const author = "condition-desktop-author-123456789";
const candidate = "condition-desktop-candidate-123456";
const capability = "condition-desktop-capability-12345";

async function waitFor(check, description) {
  for (let attempt = 0; attempt < 150; attempt++) {
    if (await check()) return;
    await delay(100);
  }
  throw new Error(`Timed out: ${description}`);
}

async function main() {
try {
  await app.whenReady();
  const suffix = process.platform === "win32" ? ".exe" : "";
  runtime = spawn(fileURLToPath(new URL(`../../../native/target/debug/novel-codex-story-runtime${suffix}`, import.meta.url)),
    ["--port", "0", "--db", join(directory, "story.sqlite3"), "--engine-budget-db", join(directory, "engine-budget.sqlite3"), "--token-stdin"], { stdio: ["pipe", "pipe", "pipe"] });
  let stderr = "";
  runtime.stderr.on("data", chunk => { stderr = (stderr + chunk).slice(-4000); });
  const origin = await new Promise((resolve, reject) => {
    const lines = createInterface({ input: runtime.stdout });
    lines.on("line", line => { if (line.startsWith("Story Runtime listening on ")) resolve(line.slice("Story Runtime listening on ".length)); });
    runtime.once("error", reject);
    runtime.once("exit", () => reject(new Error(`Runtime exited: ${stderr}`)));
    runtime.stdin.end(JSON.stringify({ authorToken: author, candidateToken: candidate }) + "\n");
  });
  const transport = async (path, options = {}, token = author) => {
    const response = await fetch(origin + path, { ...options, headers: { authorization: `Bearer ${token}`, "content-type": "application/json", ...(token === candidate ? { "x-novel-task-capability": capability } : {}) } });
    const result = await response.json();
    if (!response.ok) throw new Error(`${response.status}: ${JSON.stringify(result)}`);
    return result;
  };
  const request = (path, body, token) => transport(path, body === undefined ? {} : { method: "POST", body: JSON.stringify(body) }, token);
  const fixture = await seedConditionRuntime(request, (path, body) => request(path, body, candidate), capability);
  registerConditionEvaluationHandlers({ handle(channel, listener) {
    ipcMain.handle(channel, (event, ...args) => {
      assert.ok(isTrustedIpcSender(event, window?.webContents, { isPackaged: true, packagedEntry: entry }), "Untrusted sender");
      return listener(event, ...args);
    });
  } }, transport);
  const open = async jobId => {
    await writeFile(entry, `<html><head><link rel="stylesheet" href="test.css"></head><body><div id="root"></div><script>window.conditionFixture=${JSON.stringify({ projectId: fixture.projectId, target: fixture.target, jobId })}</script><script src="test.js"></script></body></html>`);
    window = new BrowserWindow({ show: false, webPreferences: { preload: fileURLToPath(new URL("../../preload.cjs", import.meta.url)), contextIsolation: true, nodeIntegration: false, sandbox: true, backgroundThrottling: false } });
    await window.loadFile(entry);
  };
  const evaluate = code => window.webContents.executeJavaScript(code);
  const click = async label => {
    await waitFor(() => evaluate(`[...document.querySelectorAll('button')].some(b => b.textContent === ${JSON.stringify(label)} && !b.disabled)`), label);
    await evaluate(`[...document.querySelectorAll('button')].find(b => b.textContent === ${JSON.stringify(label)}).click()`);
  };
  const contains = text => evaluate(`document.body.textContent.includes(${JSON.stringify(text)})`);
  await open();
  await click("完整条件检查"); await click("开始完整检查");
  await waitFor(() => contains("本次结果：来源冲突"), "durable worker result");
  await click("查看条件解释");
  await waitFor(() => contains("条件 1 · letter"), "real explanation");
  const prefix = [...fixture.body].slice(0, 160).join("");
  assert.equal(await contains(`支持主张的原文（节选）：${prefix}`), true);
  assert.equal(await contains(`反对主张的原文（节选）：${prefix}`), true);
  assert.equal(await contains(fixture.body), false);
  assert.equal(await contains("字符 [0, 160)"), true);
  const base = `/api/projects/${fixture.projectId}`;
  const projection = await request(`${base}/knowledge/state?perspective=world&predicate=transfer`);
  const assessment = projection.entries[0].eventConditionAssessment;
  assert.equal(assessment.result, "both");
  assert.equal(assessment.worldTruthCertified, false);
  assert.ok(assessment.explanationJobId);
  window.destroy(); await open(assessment.explanationJobId);
  await click("完整条件检查");
  await waitFor(() => contains("本次结果：来源冲突"), "reopened durable report");
  await request(`${base}/drafts/checkpoints`, { expectedRevision: 1, expectedCandidateRevisionId: fixture.eventDraft.revisionId, idempotencyKey: "changed-source",
    payload: { chapterId: fixture.eventDraft.chapterId, chapterNumber: 2, title: "变更", body: fixture.body + "信已烧毁。" } });
  await click("刷新检查");
  await waitFor(() => contains("输入已变化"), "source invalidation");
  assert.equal(await contains("本次结果"), false);
  console.log(JSON.stringify({ passed: true }));
} catch (error) {
  console.log(JSON.stringify({ passed: false, error: error.stack }));
  process.exitCode = 1;
} finally {
  window?.destroy();
  if (runtime && runtime.exitCode === null && runtime.signalCode === null) {
    const stopped = new Promise(resolve => runtime.once("exit", resolve));
    runtime.kill(); await stopped;
  }
  app.exit(process.exitCode ?? 0);
}
}
void main();
