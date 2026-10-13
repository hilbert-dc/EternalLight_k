import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { MarkdownContent } from "../../../src/MarkdownContent";

window.addEventListener("error", (event) => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });

const markdown = `# 故事结论

这是 **重点**、*语气*、~~废案~~ 和 \`rule-memory\`。

> 记录是抵抗遗忘的方式。

- 第一层
  - 第二层

| 类型 | 内容 |
| --- | --- |
| 世界规则 | 白塔每晚亮四十七秒 |

\`\`\`typescript
const witness = "林夕";
\`\`\`

[安全链接](https://example.com) [危险链接](javascript:alert("bad"))

<script>window.markdownExecuted = true</script>
<img src=x onerror="window.markdownExecuted = true">`;

const check = (condition: unknown, message: string) => { if (!condition) throw new Error(message); };

async function run() {
  window.markdownExecuted = false;
  createRoot(document.getElementById("root")!).render(<MarkdownContent text={markdown}/>);
  await new Promise((resolve) => setTimeout(resolve, 100));
  window.uiReady = true;
  check(document.querySelector("h2.conversation-heading"), "Heading was not rendered semantically");
  check(document.querySelector("strong")?.textContent === "重点", "Strong emphasis was not rendered");
  check(document.querySelector("em")?.textContent === "语气", "Emphasis was not rendered");
  check(document.querySelector("del")?.textContent === "废案", "GFM deletion was not rendered");
  check(document.querySelector("blockquote")?.textContent?.includes("抵抗遗忘"), "Blockquote was not rendered");
  check(document.querySelector("ul ul")?.textContent?.includes("第二层"), "Nested list was not rendered");
  check(document.querySelector("table")?.textContent?.includes("世界规则"), "GFM table was not rendered");
  check(document.querySelector(".conversation-table-scroll[tabindex='0']"), "Wide table is not keyboard-scrollable");
  check(document.querySelector(".conversation-code-card code.language-typescript")?.textContent?.includes("witness"), "Fenced code block lost its language or text");
  check(document.querySelector("button[aria-label='复制代码']"), "Code copy action is missing");
  check(document.querySelector("p code")?.textContent === "rule-memory", "Inline code was not rendered");
  const links = [...document.querySelectorAll("a")];
  check(links[0]?.getAttribute("href") === "https://example.com", "Safe link was removed");
  check(links.length === 1 && document.querySelector(".conversation-unsafe-link")?.textContent === "危险链接", "Dangerous URL remained interactive");
  check(!document.querySelector("#root script") && !document.querySelector("#root img") && window.markdownExecuted === false, "Raw HTML executed or reached the Markdown DOM");
  return { passed: true };
}

run().then((result) => { window.testResult = result; }).catch((error) => { window.testResult = { passed: false, error: `${error.stack}\nDOM:${document.body.innerText}` }; });
