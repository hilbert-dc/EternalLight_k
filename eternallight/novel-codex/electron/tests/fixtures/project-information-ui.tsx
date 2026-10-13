import React from "react";
import { createRoot } from "react-dom/client";
import { flushSync } from "react-dom";
import { ProjectInformation } from "../../../src/ProjectInformation";
import "../../../src/styles.css";
import type { NovelCodexApi, ProjectSummary } from "../../../src/types";

const host = window as typeof window & { testResult?: unknown; testKey?: { keyCode: string; modifiers: string[] }; testViewport?: { width: number; height: number } };
const root = createRoot(document.getElementById("root")!);
const tick = () => new Promise(resolve => setTimeout(resolve, 20));
const check = (condition: unknown, message: string) => { if (!condition) throw new Error(message); };
const until = async (predicate: () => unknown, message: string) => { const deadline = performance.now() + 3000; while (!predicate() && performance.now() < deadline) await tick(); check(predicate(), message); };
const key = async (keyCode: string, modifiers: string[] = []) => { host.testKey = { keyCode, modifiers }; await until(() => !host.testKey, `Key ${keyCode} was not delivered`); await tick(); };
const button = (label: string) => [...document.querySelectorAll<HTMLButtonElement>("button")].find(element => element.textContent === label);
const dialog = () => document.querySelector<HTMLDialogElement>("dialog")!;
const source = () => document.querySelector(".project-information-source")?.textContent;
const project = (id: string): ProjectSummary => ({ id, title: `${id}的作品`, logline: "同一作品的独立简介", updated_at: "2026-10-03" });
const revision = (id: string, premise: string) => ({ id, version: 1, parentRevisionId: null, baseProjectRevision: 1, createdAt: null, source: "author", payload: { premise, promise: "可信的情感变化", stakes: "失去归属", endingDirection: "保留希望" } });
let currentProject = project("a"), open = false, briefNavigation = 0, reads: string[] = [];
let mode = "ready", accepted = false, newer = false, missing = false;
const releases: (() => void)[] = [];
const snapshots: unknown[] = [];
const api = { runtime: { getCreativeWorkspace: async (id: string) => {
  reads.push(id);
  const workspace = { project: project(id), artifacts: { brief: { candidate: missing ? null : revision("saved", `${id}的原始目标`),
    accepted: accepted ? revision(newer ? "adopted" : "saved", `${id}已采用目标`) : null, working: null, locked: false, workingSelection: null } } };
  const requestedMode = mode; mode = "ready";
  if (requestedMode === "delay") {
    workspace.artifacts.brief.candidate!.payload.premise = "旧A迟到目标，不可显示";
    await new Promise<void>(resolve => releases.push(resolve));
  }
  if (requestedMode === "error") throw new Error("只读读取失败");
  if (requestedMode === "foreign") workspace.project.id = "foreign";
  return workspace;
} } } as unknown as NovelCodexApi;
const render = () => flushSync(() => root.render(<><button id="launcher" data-project-information-trigger onClick={() => { open = true; render(); }}>作品信息</button><textarea id="editor" defaultValue="作者未保存的正文" />{open && <ProjectInformation api={api} project={currentProject} onClose={() => { open = false; render(); }} onOpenBrief={() => { briefNavigation++; }} />}</>));
const show = async () => { document.getElementById("launcher")!.focus(); button("作品信息")!.click(); await until(() => source() || document.querySelector("dialog [role=alert]"), "Read did not settle"); };
const close = async () => { dialog().querySelector<HTMLButtonElement>('[aria-label="关闭作品信息"]')!.click(); await tick(); };
const capture = (name: string) => snapshots.push({ name, modal: dialog().matches(":modal"), metadata: document.querySelector(".project-information-metadata")?.textContent,
  source: source(), fields: [...dialog().querySelectorAll("section dd")].map(element => element.textContent), check: dialog().querySelector("details")?.textContent ?? null });

async function run() {
  render();
  const editor = document.getElementById("editor") as HTMLTextAreaElement;
  editor.value = "尚未保存的原文与光标"; editor.setSelectionRange(2, 6);
  await show(); check(dialog().matches(":modal"), "Native modal was not opened"); check(!dialog().querySelector("input,textarea"), "Read-only metadata exposes an editor");
  check(!dialog().textContent?.includes("故事检查"), "Metadata must not claim story validation"); check(source()?.includes("尚未采用"), "Saved candidate mislabeled as adopted"); capture("saved brief");
  await key("Tab", ["shift"]); check(dialog().contains(document.activeElement), "Native Shift-Tab escaped the modal");
  await key("Tab"); check(dialog().contains(document.activeElement), "Native Tab escaped the modal");
  await key("Escape"); check(!document.querySelector("dialog") && document.activeElement?.id === "launcher", "Escape did not close and return focus");
  check(editor.isConnected && editor === document.getElementById("editor") && editor.value === "尚未保存的原文与光标" && editor.selectionStart === 2 && editor.selectionEnd === 6, "Dialog disturbed author edits or DOM");

  accepted = true; newer = true; await show(); check(dialog().textContent?.includes("a已采用目标") && !dialog().textContent?.includes("a的原始目标"), "Adopted source lost precedence"); capture("adopted with saved changes");
  for (const theme of ["light", "dark"]) for (const width of [1400, 900]) {
    document.documentElement.dataset.theme = theme; host.testViewport = { width, height: 600 }; await until(() => !host.testViewport, "Viewport was not resized"); await tick();
    const box = dialog().getBoundingClientRect(); check(box.left >= 0 && box.right <= innerWidth + 1 && box.top >= 0 && box.bottom <= innerHeight + 1, "Dialog escaped the viewport");
    const style = getComputedStyle(dialog()); check(style.backgroundColor !== style.color, "Dialog text is invisible");
    snapshots.push({ name: `${theme} ${width}`, inViewport: true, background: style.backgroundColor, text: style.color });
  }
  button("查看作品概况")!.click(); await tick(); check(briefNavigation === 1 && !document.querySelector("dialog"), "Brief navigation did not close the modal first");
  accepted = false; newer = false; missing = true; await show(); check(source() === "尚未填写创作简报" && !dialog().querySelector("section dl"), "Missing brief was falsely reported as saved or repeats empty fields"); capture("unfilled brief"); await close(); missing = false;

  mode = "error"; const beforeRetry = reads.length; await show(); check(dialog().querySelector('[role="alert"]')?.textContent?.includes("只读读取失败"), "Read failure was hidden as empty");
  button("重新读取作品信息")!.click(); await until(() => source(), "Retry did not settle"); check(reads.length === beforeRetry + 2, "Retry duplicated a read"); capture("read-only retry"); await close();
  mode = "foreign"; await show(); check(dialog().querySelector('[role="alert"]')?.textContent?.includes("来源不一致") && !source(), "Foreign source contaminated the dialog"); await close();

  currentProject = project("a"); mode = "delay"; open = true; render(); await until(() => releases.length === 1, "A read was not deferred");
  currentProject = project("b"); render(); await until(() => dialog().textContent?.includes("b的原始目标"), "B did not independently load");
  currentProject = project("a"); render(); await until(() => source(), "New A did not independently load");
  releases.shift()!(); await tick(); check(reads.slice(-3).join(",") === "a,b,a" && dialog().textContent?.includes("a的原始目标") && !dialog().textContent?.includes("旧A迟到目标"), "A-B-A owner isolation failed");
  capture("project scope isolated"); await close();
  check(reads.every(id => ["a", "b"].includes(id)), "Unexpected project was read");
  host.testResult = { passed: true, reads: reads.length, briefNavigation, authorDraftPreserved: editor.value === "尚未保存的原文与光标", snapshots };
}
void run().catch(error => { host.testResult = { passed: false, error: String(error.stack ?? error), reads }; });
