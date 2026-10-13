import React, { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import StartupGate from "../../../src/StartupGate";
import "../../../src/styles.css";

const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (value, message) => { if (!value) throw Error(message); };
const until = async (predicate, message) => { for (let i = 0; i < 200 && !predicate(); i++) await tick(); check(predicate(), message); };
let current = { state: "starting", mode: "local", error: null };
let mounts = 0, unmounts = 0, reads = 0, statusReads = 0, retries = 0, closes = 0, editorCloses = 0;
let completeRetry;
const closeListeners = new Set<() => void>();
window.novelCodex = {
  app: {
    confirmClose: async () => { closes++; return { closing: true }; },
    onCloseRequested: listener => { closeListeners.add(listener); return () => closeListeners.delete(listener); },
  },
  runtime: {
    status: async () => { statusReads++; return structuredClone(current); },
    retryStartup: async () => { retries++; current = { state: "starting", mode: "local", error: null }; return new Promise(resolve => { completeRetry = resolve; }); },
    listProjects: async () => { reads++; return { projects: [] }; },
  },
} as any;

function Editor() {
  const [body, setBody] = useState("");
  useEffect(() => {
    mounts++;
    void window.novelCodex.runtime.listProjects();
    const remove = window.novelCodex.app.onCloseRequested(() => { editorCloses++; });
    return () => { unmounts++; remove(); };
  }, []);
  return <textarea aria-label="未保存正文" value={body} onChange={event => setBody(event.target.value)} />;
}

(async () => {
  let root;
  try {
    root = createRoot(document.getElementById("root"));
    root.render(<StrictMode><StartupGate><Editor /></StartupGate></StrictMode>);
    await until(() => document.querySelector('[role="status"]')?.textContent.includes("正在启动"), "Missing initial startup feedback");
    check(mounts === 0 && reads === 0 && !document.querySelector("textarea"), "Workspace mounted or requested projects before readiness");
    closeListeners.forEach(listener => listener());
    check(closes === 1 && closeListeners.size === 1, "Startup close was duplicated or missing under StrictMode");

    current = { state: "slow", mode: "local", error: null };
    await until(() => document.querySelector('[role="status"]')?.textContent.includes("仍在等待"), "Slow startup was not visible");
    check(!document.querySelector("button, details") && reads === 0, "Slow startup offered a restart or mounted the workspace");

    current = { state: "offline", mode: "local", error: "fixture: child process exited" };
    await until(() => document.querySelector<HTMLButtonElement>(".startup-gate-retry"), "Missing genuine-failure retry");
    const details = document.querySelector<HTMLDetailsElement>(".startup-gate-details");
    check(!details.open && !details.querySelector("pre").checkVisibility() && !document.querySelector('[role="status"]').textContent.includes("child process"), "Technical failure was shown in primary startup copy");
    details.querySelector("summary").click();
    check(details.open && details.textContent.includes("child process exited"), "Diagnostics are not available on demand");
    const retry = document.querySelector<HTMLButtonElement>(".startup-gate-retry");
    retry.click(); retry.click();
    await tick();
    check(retries === 1 && mounts === 0 && !document.querySelector("details"), "Repeated retry created multiple requests or mounted early");
    completeRetry(structuredClone(current));
    await tick();

    current = { state: "online", mode: "local", error: null };
    await until(() => document.querySelector<HTMLTextAreaElement>('[aria-label="未保存正文"]'), "Online did not mount the workspace");
    const editor = document.querySelector<HTMLTextAreaElement>("textarea");
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(editor, "尚未保存的小说正文");
    editor.dispatchEvent(new Event("input", { bubbles: true }));
    await tick(); editor.focus(); editor.setSelectionRange(3, 7);
    const atOnline = { mounts, unmounts, reads, statusReads };
    check(reads > 0 && closeListeners.size === 1 && !document.querySelector(".startup-gate"), "Workspace or startup close ownership did not transfer");
    current = { state: "offline", mode: "local", error: "later disconnection" };
    await new Promise(resolve => setTimeout(resolve, 1150));
    check(document.querySelector("textarea") === editor && editor.value === "尚未保存的小说正文" && editor.selectionStart === 3 && editor.selectionEnd === 7, "Later disconnection replaced the editor, text or selection");
    check(mounts === atOnline.mounts && unmounts === atOnline.unmounts && statusReads === atOnline.statusReads, "Startup gate remounted or kept owning online connection polling");
    closeListeners.forEach(listener => listener());
    check(closes === 1 && editorCloses === 1, "Startup close bypassed the live editor after online");
    window.testResult = { passed: true, retries, closes, editorCloses, earlyReads: 0, preservedEditor: true };
  } catch (error) { window.testResult = { passed: false, error: String(error), dom: document.body.innerHTML }; }
  finally { root?.unmount(); }
})();
