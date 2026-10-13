import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { MetricsPanel } from "../../../src/MetricsPanel";

const tick = () => new Promise((resolve) => setTimeout(resolve, 30));
const check = (value, message) => { if (!value) throw new Error(message); };
const button = (text) => [...document.querySelectorAll("button")].find((item) => item.textContent === text);
let fail = true, abandoned, exportedBlob, downloaded;
let loseResponse = true;
const actionKeys = [];
let summary = { scope: "local-device", metrics: [{ metric: "brief.adopted", count: 2, totalSeconds: 120 }, { metric: "draft.changedSave", count: 3, totalSeconds: 0 }] };
URL.createObjectURL = (blob) => { exportedBlob = blob; return "blob:test-metrics"; };
URL.revokeObjectURL = () => {};
HTMLAnchorElement.prototype.click = function () { downloaded = this.download; };
const api = { runtime: {
  getLocalMetrics: async () => { if (fail) throw new Error("PRIVATE-ERROR-CANARY"); return summary; },
  recordStageAbandonment: async (stage, key) => {
    abandoned = stage; actionKeys.push(key);
    if (loseResponse) { summary = { ...summary, metrics: [...summary.metrics, { metric: `${stage}.abandoned`, count: 1, totalSeconds: 0 }] }; loseResponse = false; throw new Error("Response lost after commit"); }
    return summary;
  },
} };
async function run() {
  let root = createRoot(document.getElementById("root"));
  root.render(<MetricsPanel api={api} />); await tick(); await tick();
  check(document.body.textContent.includes("无法读取本地统计"), "Missing readable failure");
  check(!document.body.textContent.includes("PRIVATE-ERROR-CANARY"), "Raw error leaked");
  check(button("导出匿名汇总").disabled, "Export enabled before successful load");
  fail = false; button("刷新统计").click(); await tick(); await tick();
  check(document.body.textContent.includes("2 次 · 累计 120 秒") && document.body.textContent.includes("不能衡量实质重写或文学质量"), "Metrics or meaning missing");
  const select = document.querySelector("select"); select.value = "plan"; select.dispatchEvent(new Event("change", { bubbles: true })); await tick();
  button("记录一次放弃").click(); await tick(); await tick();
  check(button("重试确认放弃记录") && document.querySelector("select").disabled, "Lost response did not retain pending action");
  root.unmount(); root = createRoot(document.getElementById("root")); root.render(<MetricsPanel api={api} />); await tick(); await tick();
  button("重试确认放弃记录").click(); await tick(); await tick();
  check(actionKeys.length === 2 && actionKeys[0] === actionKeys[1], "Retry after remount did not reuse nonce");
  check(abandoned === "plan" && document.querySelector('[role="status"]').textContent.includes("已记录一次放弃"), "Abandonment failed");
  button("导出匿名汇总").click(); await tick();
  check(downloaded === "小说工坊-本地创作统计.json" && JSON.stringify(JSON.parse(await exportedBlob.text())) === JSON.stringify(summary), "Export differs from anonymous summary");
  window.uiReady = true;
  return { passed: true };
}
run().then((result) => { window.testResult = result; }).catch((error) => { window.testResult = { passed: false, error: error.stack }; });
