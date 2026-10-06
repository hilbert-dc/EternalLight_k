import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { WorkspaceDiagnostics } from "../../../src/WorkspaceDiagnostics";

window.addEventListener("error", (event) => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
const tick = () => new Promise((resolve) => setTimeout(resolve, 30));
const check = (condition, message) => { if (!condition) throw new Error(message); };
const waitFor = async (condition, message) => {
  const deadline = performance.now() + 3000;
  while (!condition() && performance.now() < deadline) await tick();
  check(condition(), message);
};
const jobs = new Map();
let releaseOldRead, releaseStart, startCalls = 0, readCalls = 0, failRead = false, failStart = false, holdStart = true, foreign = false;
let firstRead = true;
const jobFor = (projectId) => ({ id: `audit-${projectId}`, projectId, status: "queued", version: 0,
  sourceCount: 12, indexGeneration: 0, nextRowid: 0, verifiedConsistent: null, error: null,
  audit: { revision_count: 0, mismatch_count: 0, orphaned_documents: 0, orphaned_grams: 0 } });
const api = { runtime: {
  getSearchAudit: async (projectId) => {
    ++readCalls;
    if (firstRead) { firstRead = false; return new Promise((resolve) => { releaseOldRead = resolve; }); }
    if (failRead) throw new Error("状态读取断开");
    return { job: foreign ? jobFor("foreign-project") : structuredClone(jobs.get(projectId) ?? null) };
  },
  startSearchAudit: async (projectId) => {
    ++startCalls;
    if (failStart) throw new Error("开始操作被拒绝");
    if (holdStart) return new Promise((resolve) => { releaseStart = () => {
      const next = jobFor(projectId); jobs.set(projectId, next); resolve(structuredClone(next));
    }; });
    const next = jobFor(projectId); jobs.set(projectId, next); return structuredClone(next);
  },
  controlSearchAudit: async (projectId, _id, command) => {
    const next = { ...jobs.get(projectId), status: command === "pause" ? "paused" : command === "resume" ? "queued" : "cancelled" };
    jobs.set(projectId, next); return structuredClone(next);
  },
  ...Object.fromEntries(["Entity", "Relation", "Knowledge", "SnapshotKnowledge", "Fact"].map((kind) => [`get${kind}Audit`, async () => ({ job: null })])),
} };
const root = createRoot(document.getElementById("root"));
const render = async (projectId = "a", refreshToken = 0) => {
  root.render(<main style={{ maxWidth: 900, margin: "40px auto", padding: "0 24px" }}>
    <h1>作品工作区</h1><WorkspaceDiagnostics key={projectId} api={api} projectId={projectId} refreshToken={refreshToken} />
    <label style={{ display: "block", marginTop: 24 }}>正文<textarea aria-label="作者正文" defaultValue="港城的灯仍然亮着。" style={{ display: "block", width: "100%", minHeight: 320, marginTop: 12 }} /></label>
  </main>); await tick(); await tick();
};
const summary = () => document.querySelector(".workspace-diagnostics > summary");
const group = () => document.querySelector(".workspace-diagnostics");
const startButton = () => [...document.querySelectorAll("button")].find((item) => item.textContent === "开始全量校验");
const openSearch = async () => {
  if (!group().open) summary().click();
  await tick();
  const panel = document.querySelector('[aria-label="检索索引校验"]');
  if (!panel.open) panel.querySelector("summary").click();
  await tick();
};

async function run() {
  await render();
  check(!group().open && summary().textContent.includes("排查检索或资料显示异常"), "Advanced diagnosis did not start collapsed");
  check(readCalls === 0 && group().querySelectorAll(".diagnostic-check").length === 0, "Opening settings eagerly loaded diagnostic jobs");
  await openSearch();
  check(group().querySelectorAll(".diagnostic-check").length === 6, "Diagnostic scopes were dropped");
  check(summary().textContent.includes("读取中"), "A pending read was presented as a completed diagnosis");
  check([...group().querySelectorAll(".diagnostic-check")].every(panel => getComputedStyle(panel).borderLeftWidth === "0px"), "Technical checks still render as alert cards");
  const start = startButton(); start.click(); start.click(); await tick();
  check(startCalls === 1 && releaseStart, "Same-frame duplicate clicks launched multiple operations");
  await render("a", 1);
  await render("a", 10); await render("a", 11);
  check(readCalls === 1, "Refresh bursts bypassed the one-read-in-flight bound");
  check(startButton().disabled, "Refresh forgot the in-flight operation");
  releaseStart(); await waitFor(() => summary().textContent.includes("1 项进行中"), "Operation result disappeared after refresh");
  releaseOldRead({ job: null }); await tick(); await tick();
  check(summary().textContent.includes("1 项进行中"), "An old read overwrote the new running job");
  const editor = document.querySelector('[aria-label="作者正文"]'); editor.value = "作者未保存的独立输入"; editor.focus();
  summary().click();
  jobs.set("a", { ...jobs.get("a"), status: "completed", verifiedConsistent: false,
    audit: { ...jobs.get("a").audit, revision_count: 12, mismatch_count: 1 } });
  await waitFor(() => summary().textContent.includes("1 项诊断异常"), "Collapsed diagnostics stopped polling or hid a mismatch");
  check(!group().open && document.activeElement === editor && editor.value === "作者未保存的独立输入", "Background diagnosis stole focus or discarded author input");
  check(summary().querySelector('[role="alert"]'), "Collapsed attention was not accessible");
  failRead = true; await render("a", 2);
  check(document.body.textContent.includes("状态读取断开") && summary().textContent.includes("诊断异常"), "Read failure was invisible");
  failRead = false; jobs.set("a", { ...jobs.get("a"), verifiedConsistent: true }); await render("a", 3);
  check(!document.body.textContent.includes("状态读取断开") && !summary().textContent.includes("诊断异常"), "Recovered read retained a stale transport error");
  check(summary().textContent.includes("按需排查") && !summary().textContent.includes("一致"), "Partial local checks were promoted to a global certificate");
  failRead = true; await render("a", 30);
  check(!document.querySelector('[aria-label="检索索引校验"] > summary').textContent.includes("一致"), "Read failure still presented the previous result as current consistency");
  const startsBeforeRetry = startCalls;
  failRead = false; await openSearch();
  [...document.querySelectorAll("button")].find((item) => item.textContent === "重试读取状态").click();
  await waitFor(() => !document.body.textContent.includes("状态读取断开"), "Read-only retry did not recover the connection");
  check(startCalls === startsBeforeRetry, "Reading status unexpectedly started a new audit");
  jobs.set("a", { ...jobs.get("a"), status: "stale" }); await render("a", 31);
  check(!summary().textContent.includes("诊断异常") && document.querySelector('[aria-label="检索索引校验"] > summary').textContent.includes("报告已过期"), "A source change was presented as an author obligation or a current consistency result");
  jobs.set("a", { ...jobs.get("a"), status: "completed", verifiedConsistent: null }); await render("a", 32);
  check(summary().textContent.includes("诊断异常") && document.body.textContent.includes("未完成核验"), "Unverified completion was certified");
  jobs.set("a", { ...jobs.get("a"), verifiedConsistent: true }); await render("a", 33);
  failStart = true; await openSearch(); startButton().click(); await tick(); await tick();
  await render("a", 4);
  check(document.body.textContent.includes("开始操作被拒绝") && summary().textContent.includes("诊断异常"), "Successful state reads concealed an operation failure");
  failStart = false; startButton().click(); await tick();
  const oldOperation = releaseStart;
  await render("b", 5);
  check(!group().open, "Project switch exposed diagnostics without author intent");
  await openSearch();
  check(startButton() && !startButton().disabled && !document.body.textContent.includes("开始操作被拒绝"), "Project switch inherited old busy/error state");
  oldOperation(); await tick(); await tick();
  check(!summary().textContent.includes("进行中") && !summary().textContent.includes("诊断异常"), "Late operation crossed project scopes");
  foreign = true; await render("b", 6);
  check(document.body.textContent.includes("校验报告不属于当前作品") && summary().textContent.includes("诊断异常"), "Foreign report was displayed as a valid diagnosis");
  check(!document.body.textContent.includes("已核对 0 / 12"), "Foreign report leaked its progress");
  foreign = false; await render("b", 7);
  check(!summary().textContent.includes("诊断异常"), "Scoped state did not recover");
  window.uiReady = true; await new Promise((resolve) => setTimeout(resolve, 200));
  window.testResult = { passed: true };
}
run().catch((error) => { window.testResult = { passed: false, error: error.stack ?? String(error) }; });
