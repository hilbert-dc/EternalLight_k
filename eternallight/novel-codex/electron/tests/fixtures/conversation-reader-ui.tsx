import "../../../src/styles.css";
import "../../../src/conversation-workspace.css";
import "../../../src/conversation-presentation.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { ConversationReader } from "../../../src/ConversationReader";
const check = (value, message) => { if (!value) throw Error(message); };
const tick = () => new Promise(resolve => setTimeout(resolve, 40));
const root = createRoot(document.getElementById("root"));
const copies = [], forks = [];
Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: async text => copies.push(text) } });
const prompt = "保持第一章原文，从第二章继续写作，保存完整正文。".repeat(40);
const answer = "**第二章已保存**\n\n灯火照亮了旧铜牌。";
const item = (id, type, text, phase = null) => ({ id, type, text, phase, status: "completed" });
const detail = { thread: { id: "thread" }, turns: [
  { id: "complete", status: "completed", items: [item("u", "userMessage", prompt), item("p", "agentMessage", "正在核对灯税期限。", "commentary"), item("f", "agentMessage", answer, "final_answer")] },
  { id: "failed", status: "failed", items: [item("fp", "agentMessage", "正在重试保存。", "commentary")] },
] };
async function render(busy = false) {
  root.render(<aside className="conversation-rail" style={{ width: 320 }}><ConversationReader detail={detail} loading={false} error={null} busy={busy} onRetry={() => {}} onFork={id => forks.push(id)} /></aside>);
  await tick(); await tick();
}
(async () => { try {
  await render();
  const turns = document.querySelectorAll(".conversation-reader-turn");
  const process = turns[0].querySelector<HTMLDetailsElement>(".conversation-reader-process");
  check(process.tagName === "DIV" && !turns[0].querySelector(".conversation-answer").textContent.includes("核对"), "Process mixed into final answer");
  check(process.innerText.includes("正在核对灯税期限"), "Process cannot be read");
  const request = turns[0].querySelector<HTMLDetailsElement>(".conversation-request");
  check(!request.open && request.dataset.overflow === "true", "Long instruction is not collapsed");
  request.querySelector<HTMLElement>("summary").focus();
  check(document.activeElement === request.querySelector("summary"), "Disclosure cannot receive keyboard focus");
  request.querySelector<HTMLElement>("summary").click(); await tick();
  check(request.open && request.querySelector("p").textContent === prompt, "Expanded instruction lost content");
  turns[0].querySelector<HTMLButtonElement>('[aria-label="复制回复"]').click(); await tick();
  check(copies.length === 1 && copies[0] === answer, "Copy lost original Markdown or included process");
  const branch = turns[0].querySelector<HTMLButtonElement>(".conversation-reader-actions > button");
  branch.click(); check(forks[0] === "complete", "Fork targeted wrong turn");
  check(turns[1].textContent.includes("本轮未完成") && !turns[1].querySelector('[aria-label="复制回复"]') && turns[1].querySelector<HTMLButtonElement>(".conversation-reader-actions > button").disabled, "Failed process pretended to be final answer");
  for (const theme of ["light", "dark"]) {
    document.documentElement.dataset.theme = theme; await tick();
    const rail = document.querySelector<HTMLElement>(".conversation-rail");
    check(rail.scrollWidth <= rail.clientWidth + 1, "History overflows at 320px");
  }
  await render(true); check(document.querySelector<HTMLButtonElement>(".conversation-reader-actions > button").disabled, "Busy fork still available");
  window.testResult = { passed: true, copied: true };
} catch (error) { window.testResult = { passed: false, error: error.stack }; } })();
