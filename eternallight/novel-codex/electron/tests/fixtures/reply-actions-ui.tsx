import React from "react";
import { createRoot } from "react-dom/client";
import { ConversationReplyActions } from "../../../src/ConversationReplyActions";
import { GeneratedReplyDescription, SavedTaskReply } from "../../../src/SavedTaskReply";
import { ConversationTimeline } from "../../../src/ConversationTimeline";
import { MarkdownContent } from "../../../src/MarkdownContent";
import { createAgentRun } from "../../../src/agent-events";
import { captureTestSurface, layoutFrame, visibleElement } from "../helpers/page-geometry";
import "../../../src/styles.css";

const raw = "  ## 来信 📨\n\n原文 **强调** 与 [链接](https://example.com)。\n\n- 第一条\n- 第二条\n\n```text\n  保留缩进\n```\n\n  ";
const root = createRoot(document.getElementById("root")!);
const surface = () => document.querySelector<HTMLElement>(".conversation-panel")!;
const check = (ok, message) => { if (!ok) throw Error(message); };
const tick = () => new Promise(resolve => setTimeout(resolve, 15));
const until = async (predicate, message) => { for (let i = 0; i < 100 && !predicate(); i++) await tick(); check(predicate(), message); };
const buttons = () => [...surface().querySelectorAll<HTMLButtonElement>(".conversation-reply-actions button")];
const copied = [], snapshots = [];
let write = text => { copied.push(text); return Promise.resolve(); }, clipboardAvailable = true;
Object.defineProperty(navigator, "clipboard", { configurable: true, get: () => clipboardAvailable ? { writeText: text => write(text) } : undefined });
const render = async (children, width = 740) => { root.render(<section className="conversation-rail" style={{ width, margin: "20px auto", padding: 18, maxWidth: "100%", height: "auto", display: "block" }}><div className="conversation-panel">{children}</div></section>); await layoutFrame(); };
const action = (identity = "A", text = raw) => <ConversationReplyActions identity={identity} text={text} />;
const snap = name => snapshots.push({ name, buttons: buttons().map(button => button.textContent), status: surface().querySelector('[role="status"]')?.textContent ?? null, alert: surface().querySelector('[role="alert"]')?.textContent ?? null });
const attempt = { id: "attempt-A", threadId: "thread-A", turnId: "turn-A", status: "completed", startedAt: 1, completedAt: 2, attemptNumber: 1, error: null };
let receipt, failRead = false, reads = 0;
const api = { runtime: new Proxy({ getTaskFinalReply: async (projectId, taskId) => { reads++; check(projectId === "project-A" && taskId === attempt.id, "Saved reply read wrong owner"); if (failRead) throw Error("夹具读取失败"); return receipt; } }, { get(target, key) { if (key in target) return target[key]; throw Error(`Unexpected Runtime call ${String(key)}`); } }) };
const saved = (key, fold = false) => <SavedTaskReply key={key} api={api as any} projectId="project-A" attempt={attempt as any} foldDescription={fold} />;
const makeReceipt = (state = "recorded", truncated = false) => ({ projectId: "project-A", taskId: attempt.id, state, binding: { threadId: attempt.threadId, turnId: attempt.turnId, text: raw, truncated } });
let phase = "initial";
void (async () => {
  phase = "raw copy";
  await render(action()); const first = buttons()[0]; first.focus(); check(document.activeElement === first, "Copy button was not keyboard reachable"); first.click();
  await until(() => surface().querySelector('[role="status"]')?.textContent === "已复制", "Copy success was not announced");
  check(copied.at(-1) === raw && document.activeElement === first, "Copy altered raw Markdown or lost keyboard focus"); snap("raw-markdown-success");
  write = text => { copied.push(text); return Promise.reject(Error("private clipboard detail")); };
  await render(action("failure")); buttons()[0].click(); await until(() => surface().querySelector('[role="alert"]'), "Failure was swallowed");
  check(surface().querySelector('[role="alert"]')!.textContent === "复制失败，请选中文字复制。" && !surface().textContent.includes("private"), "Clipboard failure exposed implementation details"); snap("clipboard-rejected");
  clipboardAvailable = false; await render(action("missing")); buttons()[0].click(); await until(() => surface().querySelector('[role="alert"]'), "Missing clipboard did not report failure"); snap("clipboard-unavailable"); clipboardAvailable = true;
  await render(action("blank", " \n\t")); check(buttons().length === 0, "Blank reply advertised a fake copy");
  phase = "async ownership";
  for (const transition of ["identity", "text", "ABA", "unmount"]) {
    let resolve, reject;
    write = text => { copied.push(text); return new Promise((done, fail) => { resolve = done; reject = fail; }); };
    await render(action(`pending-${transition}`)); const button = buttons()[0], count = copied.length; button.focus(); button.click(); button.click(); await layoutFrame();
    check(button.getAttribute("aria-disabled") === "true", "Pending copy did not communicate its disabled state"); button.click(); check(copied.length === count + 1, "Double click issued two clipboard requests");
    check(document.activeElement === button, "Pending copy removed keyboard focus");
    if (transition === "identity") await render(action("B"));
    if (transition === "text") await render(action(`pending-${transition}`, `${raw}新文本`));
    if (transition === "ABA") { await render(action("B")); await render(action(`pending-${transition}`)); }
    if (transition === "unmount") { await render(null); await render(action(`pending-${transition}`)); }
    if (transition === "text") reject(Error("late private error")); else resolve(); await layoutFrame();
    check(buttons()[0].getAttribute("aria-disabled") !== "true" && !surface().querySelector('[role="status"],[role="alert"]'), `Late ${transition} completion polluted current feedback`);
    snap(`late-${transition}-ignored`);
  }
  write = text => { copied.push(text); return Promise.resolve(); };
  phase = "generated boundary";
  await render(<GeneratedReplyDescription projectId="project-A" taskId="generated" text={raw}><MarkdownContent text={raw} />{action("nested")}</GeneratedReplyDescription>);
  check(buttons().length === 1 && !visibleElement(buttons()[0]), "Generated result had duplicate or externally visible copy");
  surface().querySelector<HTMLDetailsElement>("details")!.open = true; await layoutFrame(); buttons()[0].click(); await layoutFrame();
  check(copied.at(-1) === raw && buttons()[0].textContent === "复制结果说明", "Generated result did not copy its exact original"); snap("generated-single-footer");
  phase = "saved reply";
  for (const [name, state, truncated, fold] of [["ordinary", "recorded", false, false], ["folded", "recorded", false, true], ["partial-state", "truncated", false, true], ["partial-binding", "recorded", true, true]]) {
    receipt = makeReceipt(state as string, truncated as boolean); await render(saved(name as string, fold as boolean)); await until(() => buttons().length === 1, `Saved ${name} did not expose one copy`);
    const details = surface().querySelector<HTMLDetailsElement>(".generated-reply-description"); if (details) { details.open = true; await layoutFrame(); }
    buttons()[0].click(); await layoutFrame(); check(copied.at(-1) === raw, `Saved ${name} copy altered original`);
    if (truncated || state === "truncated") check(buttons()[0].textContent === "复制已显示内容" && surface().querySelector('[role="status"]')?.textContent.includes("部分内容"), "Partial copy hid coverage warning");
    snap(`saved-${name}`);
  }
  for (const mode of ["null", "absent", "read-error", "foreign"]) {
    receipt = mode === "null" ? null : makeReceipt(mode === "absent" ? "absent" : "recorded"); failRead = mode === "read-error";
    if (mode === "foreign") receipt.projectId = "project-B";
    await render(saved(mode)); await until(() => !surface().textContent.includes("正在读取保存的回复"), `Saved ${mode} did not settle`);
    check(buttons().length === 0, `Saved ${mode} advertised unavailable reply text`); snap(`saved-${mode}`);
    if (mode === "read-error") { failRead = false; receipt = makeReceipt(); const count = reads; surface().querySelector<HTMLButtonElement>("button")!.click(); await until(() => buttons().length === 1, "Read retry did not recover"); check(reads === count + 1, "Read retry dispatched unexpected operations"); }
  }
  phase = "timeline and visual matrix";
  check(matchMedia("(prefers-reduced-motion: reduce)").matches, "Electron did not use real reduced-motion preference");
  const run = { ...createAgentRun("检查来信"), threadId: "thread-state", turnId: "turn-state", startedAt: 10000, completedAt: 11000, steps: [{ id: "tool", kind: "tool", title: "读取正文", status: "completed" }] };
  await render(<>{["running", "waiting", "error", "cancelled", "completed"].map(state => <ConversationTimeline key={state} run={{ ...run, state } as any} />)}</>);
  const markers = [...surface().querySelectorAll<HTMLElement>(".conversation-state-marker")];
  check(markers.length === 5 && markers.every(marker => marker.getAttribute("aria-hidden") === "true"), "Timeline omitted or duplicated status markers");
  check(getComputedStyle(markers[0]).animationName === "none", "Reduced-motion still spun progress indicator");
  snapshots.push({ name: "timeline-states", headlines: [...surface().querySelectorAll(".conversation-process-headline")].map(item => item.textContent), reducedMotion: true });
  for (const width of [330, 740]) for (const theme of ["light", "dark"]) {
    document.documentElement.dataset.theme = theme;
    await render(<article className="conversation-message assistant-message"><div className="conversation-message-body"><div className="conversation-answer"><MarkdownContent text={raw} /></div>{action(`visual-${theme}-${width}`)}</div></article>, width);
    buttons()[0].click(); await layoutFrame(); const markdown = surface().querySelector(".conversation-markdown")!, heading = markdown.querySelector(".conversation-heading"), following = heading?.nextElementSibling;
    check(getComputedStyle(markdown).display === "block" && heading && following && following.getBoundingClientRect().top - heading.getBoundingClientRect().bottom <= 24, "Markdown heading spacing accumulated two layout gaps");
    check(surface().scrollWidth <= surface().clientWidth + 1 && visibleElement(buttons()[0]), "Reply footer overflowed or became hidden");
    snap(`${theme}-${width}`); await captureTestSurface(`reply-actions-${theme}-${width}`);
  }
  window.testResult = { passed: true, snapshots, cases: 18, runtimeReads: reads, providerRequests: 0, writes: 0 };
})().catch(error => { window.testResult = { passed: false, error: `${phase}: ${error.message}`, html: document.getElementById("root")!.innerHTML }; });
