import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { ExportDialog } from "../../../src/ExportDialog";

const tick = () => new Promise((resolve) => setTimeout(resolve, 40));
const check = (condition, message) => { if (!condition) throw new Error(message); };
const root = createRoot(document.getElementById("root"));
let fail = true; let calls = []; let downloaded; let release;
HTMLAnchorElement.prototype.click = function () { downloaded = this.download; };
async function run() {
  document.getElementById("trigger").focus();
  root.render(<ExportDialog onClose={() => root.unmount()} onExport={async (options) => {
    calls.push(options);
    if (fail) throw new Error("可重试的读取失败");
    await new Promise((resolve) => { release = resolve; });
    return { fileName: "project-r7.md", content: "正式正文", mimeType: "text/plain", projectRevision: 7, exportedAt: "now", scopes: options.scopes };
  }} />);
  await tick();
  const boxes = [...document.querySelectorAll('input[type="checkbox"]')];
  check(boxes[0].checked && boxes.slice(1).every((box) => !box.checked), "Default export included optional/private content");
  check(document.activeElement === boxes[0], "Dialog initial focus missing");
  const download = [...document.querySelectorAll("button")].find((button) => button.textContent === "下载文件");
  download.click(); await tick();
  check(document.querySelector('[role="alert"]').textContent.includes("读取失败"), "Failure not visible");
  boxes[3].click(); await tick();
  check(document.querySelector('[role="note"]').textContent.includes("私密"), "Private scope warning missing");
  fail = false; download.click(); await tick();
  check(boxes.every((box) => box.matches(":disabled")) && download.disabled, "Pending export permits duplicate or scope changes");
  check(calls.length === 2 && calls[1].scopes.join(",") === "manuscript,private", "Selected export scope not forwarded");
  release(); await tick();
  check(downloaded === "project-r7.md" && document.querySelector('[role="status"]').textContent.includes("版本 7"), "Snapshot receipt/download missing");
  window.uiReady = true; await new Promise((resolve) => setTimeout(resolve, 200));
  document.querySelector('[role="dialog"]').dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); await tick();
  check(document.activeElement === document.getElementById("trigger"), "Dismiss did not restore focus");
  return { passed: true };
}
run().then((result) => { window.testResult = result; }).catch((error) => { window.testResult = { passed: false, error: error.stack }; });
