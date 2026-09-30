import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { ConditionEvaluationPanel } from "../../../src/ConditionEvaluationPanel";

window.addEventListener("error", event => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
const tick = () => new Promise(resolve => setTimeout(resolve, 30));
const check = (value, message) => { if (!value) throw new Error(message); };
const text = () => document.body.textContent;
const click = async label => {
  const button = [...document.querySelectorAll("button")].find(item => item.textContent === label);
  check(button && !button.disabled, `Unavailable button: ${label}`); button.click(); await tick(); await tick();
};
const target = { snapshotId: null, sourceBatchId: "batch-event", assertionIndex: 0 };
let job; let starts = 0; let fail = false; let defer = false; let release;
const api = { runtime: {
  startConditionEvaluation: async (projectId, received) => {
    check(JSON.stringify(received) === JSON.stringify(target), "Wrong target"); starts++;
    job = { id: "condition-" + "a".repeat(32), projectId, target: { assertionIndex: 0, sourceBatchId: "batch-event", snapshotId: null }, bindingDigest: "binding", status: "queued", version: 0, scannedSources: 0, sourceBytes: 0, result: null };
    return structuredClone(job);
  },
  getConditionEvaluation: async () => {
    if (fail) throw new Error("模拟连接中断");
    if (defer) return new Promise(resolve => { release = resolve; });
    return structuredClone(job);
  },
  controlConditionEvaluation: async (projectId, id, command, expectedVersion) => {
    check(projectId === job.projectId && id === job.id && expectedVersion === job.version, "Missing scope or CAS");
    job = { ...job, version: job.version + 1, status: { pause: "paused", resume: "queued", cancel: "cancelled" }[command] };
    return structuredClone(job);
  },
  getConditionExplanations: async (_project, _id, cursor) => ({ data: [{ leafIndex: cursor ? 1 : 0, subjectId: cursor ? "反证页人物" : "第一页人物", operand: true, assessment: { result: "both", support: { sourceBatchId: "source-support", assertionIndex: 0 }, refute: { sourceBatchId: "source-counter", assertionIndex: 1 }, unresolvedChecks: 0 }, excerpts: { support: { sourceBatchId: "source-support", assertionIndex: 0, sourceObjectId: "draft-1", sourceRevisionId: "revision-1", quote: "甲把信握在手中。", start: 0, end: 8, truncated: true }, refute: null } }], nextCursor: cursor ? null : "next" }),
} };
const root = createRoot(document.getElementById("root"));
const render = (projectId, selectedTarget = target) => root.render(<main style={{ maxWidth: 680, margin: "48px auto", padding: 24 }}><h2>{projectId} · 事件条件检查</h2><ConditionEvaluationPanel api={api} projectId={projectId} target={selectedTarget} /></main>);
(async () => {
  render("作品 A"); await tick(); await tick();
  await click("完整条件检查"); check(starts === 0, "Expanding started a job");
  await click("开始完整检查"); check(text().includes("等待检查"), "Missing queued state");
  await click("暂停检查"); check(text().includes("已暂停"), "Pause failed");
  await click("收起条件检查"); await click("完整条件检查");
  check(text().includes("已暂停") && starts === 1, "Reopening resumed or recreated paused work");
  await click("恢复检查"); check(text().includes("等待检查"), "Resume failed");
  job = { ...job, version: job.version + 1, status: "completed", scannedSources: 203, result: { result: "both", coverage: { complete: true } } };
  await click("刷新检查"); await click("查看条件解释");
  check(text().includes("来源冲突") && text().includes("203") && text().includes("第一页人物"), "Missing completed explanation");
  check(text().includes("支持主张的原文（节选）：甲把信握在手中。") && text().includes("revision-1") && text().includes("[0, 8)"), "Missing anchored excerpt");
  window.uiReady = true; await new Promise(resolve => setTimeout(resolve, 300));
  await click("下一页解释"); check(text().includes("反证页人物") && !text().includes("第一页人物"), "Pages accumulated stale evidence");
  await click("回到解释第一页"); check(text().includes("第一页人物"), "First page navigation failed");
  fail = true; await click("刷新检查"); check(text().includes("模拟连接中断") && !text().includes("第一页人物") && !text().includes("本次结果"), "Failure retained old result");
  fail = false; job = { ...job, status: "stale", result: null }; await click("刷新检查");
  check(text().includes("输入已变化") && !text().includes("查看条件解释"), "Stale retained result");
  job = { ...job, status: "failed" }; await click("刷新检查");
  check(text().includes("检查失败") && text().includes("不会自动重试"), "Missing failure recovery explanation");
  await click("开始完整检查"); await click("取消检查"); check(text().includes("已取消"), "Cancel failed");
  await click("开始完整检查"); defer = true; await click("刷新检查"); check(release, "Missing pending request");
  render("作品 B"); await tick(); await tick(); await click("完整条件检查");
  release({ ...job, status: "completed", result: { result: "both", coverage: { complete: true } } }); await tick(); await tick();
  check(text().includes("作品 B") && text().includes("尚未启动") && !text().includes("本次结果"), "Late response crossed project boundary");
  defer = false; await click("开始完整检查");
  render("作品 B", { assertionIndex: 0, sourceBatchId: "batch-event", snapshotId: null }); await tick(); await tick();
  check(text().includes("等待检查"), "Equivalent target field ordering reset the panel");
  defer = true; release = undefined; await click("刷新检查"); check(release, "Missing event request");
  render("作品 B", { ...target, assertionIndex: 1 }); await tick(); await tick(); await click("完整条件检查");
  release({ ...job, status: "completed", result: { result: "both", coverage: { complete: true } } }); await tick(); await tick();
  check(text().includes("尚未启动") && !text().includes("本次结果"), "Late response crossed event boundary");
  root.unmount(); window.testResult = { passed: true };
})().catch(error => { window.testResult = { passed: false, error: error.stack }; });
