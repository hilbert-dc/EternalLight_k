import "../../../src/styles.css";
import React, { useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { ConversationTools } from "../../../src/ConversationTools";

const tick = () => new Promise(resolve => setTimeout(resolve, 25));
const check = (value, message) => { if (!value) throw Error(message); };
const until = async (predicate, message) => {
  for (let index = 0; index < 100 && !predicate(); index++) await tick();
  check(predicate(), message);
};
const root = createRoot(document.getElementById("root"));
const prefix = "讨论：" + "核对已读取的章节时间线与角色已知信息，".repeat(4);
let scope = "A", fullName = prefix + "灯税截止日是否与第七章一致";
function Harness({ scope, fullName }) {
  const reading = useRef<HTMLDivElement>(null);
  const [searchContainer, setSearchContainer] = useState<HTMLElement | null>(null);
  return <aside className="conversation-rail" style={{ width: 340 }}>
    <ConversationTools scope={scope} container={reading} searchContainer={searchContainer} inspectorOpen={false}
      onInspector={() => {}} savingCount={0} currentSavingCount={0} onRecovery={() => {}} />
    <div ref={setSearchContainer} />
    <div ref={reading} className="conversation-reading-area" tabIndex={0}>
      <div className="conversation-markdown"><p>{scope === "A" ? "作者的公开进度说明。" : "另一对话的公开进度说明。"}</p></div>
      <details className="conversation-operation-details"><summary>已核对章节时间线</summary>
        <div className="conversation-work-item"><div className="conversation-work-heading" aria-label={fullName} title={fullName}>{Array.from(fullName).slice(0, 60).join("")}…</div>
          <pre>技术日志：opaque-task-binding-secret</pre></div>
      </details>
    </div>
  </aside>;
}
const render = async () => { root.render(<Harness scope={scope} fullName={fullName} />); await tick(); await tick(); };
const input = () => document.querySelector<HTMLInputElement>('input[aria-label="查找当前对话消息"]');
const searchStatus = () => document.querySelector(".conversation-find [role='status']")?.textContent;
const stepName = () => document.querySelector<HTMLElement>(".conversation-work-heading");
const openSearch = async () => {
  const area = document.querySelector<HTMLElement>(".conversation-reading-area"); area.focus();
  area.dispatchEvent(new KeyboardEvent("keydown", { key: "f", ctrlKey: true, bubbles: true }));
  await until(() => input(), "Existing search shortcut did not open search");
};
const search = async value => {
  const element = input();
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(element, value);
  element.dispatchEvent(new Event("input", { bubbles: true })); await tick(); await tick();
};

(async () => {
  try {
    await render(); await openSearch();
    const originalName = stepName(), originalText = originalName.textContent;
    check(!originalText.includes("灯税截止日"), "Search fixture did not truncate the identifying suffix");
    check(!document.querySelector<HTMLDetailsElement>(".conversation-operation-details").open
, "Details did not start collapsed");
    await search("灯税截止日"); await until(() => searchStatus() === "1 处匹配", "Complete step name was not searchable after truncation");
    input().dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); await tick();
    check(document.querySelector<HTMLDetailsElement>(".conversation-operation-details").open
, "Search did not reveal both existing disclosure levels");
    check(document.activeElement === originalName && searchStatus() === "1 / 1", "Search did not focus the corresponding step name");

    fullName = prefix + "码头夜巡线索是否与第七章一致"; await render();
    check(stepName() === originalName && stepName().textContent === originalText, "Attribute-only update unexpectedly changed the visible search node");
    await until(() => searchStatus() === "无匹配", "Accessible-name update left a stale full-name match");
    await search("码头夜巡线索"); await until(() => searchStatus() === "1 处匹配", "Updated accessible name was not searchable");
    await search("opaque-task-binding-secret"); check(searchStatus() === "无匹配", "Conversation search exposed a technical log");
    await search("作者的公开进度"); check(searchStatus() === "1 处匹配", "Existing public message search regressed");

    scope = "B"; fullName = "第 12 章 · 生成章节"; await render();
    await until(() => !input(), "Scope switch retained the previous conversation search");
    await openSearch(); check(input().value === "" && searchStatus() === "无匹配", "Scope switch retained the previous query or match");
    await search("码头夜巡线索"); check(searchStatus() === "无匹配", "Previous conversation action leaked into the new scope");
    await search("第 12 章"); check(searchStatus() === "1 处匹配", "New conversation action did not become searchable");
    window.testResult = { passed: true, checks: ["full-name suffix", "attribute-only update", "existing disclosure reveal and focus", "scope isolation", "technical log excluded", "public message search"] };
  } catch (error) { window.testResult = { passed: false, error: `${error.stack}\nDOM:${document.body.innerHTML}` }; }
})();
