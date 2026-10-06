import "../../../src/styles.css";
import React, { useState } from "react";
import { createRoot } from "react-dom/client";
import { SearchResults, searchNavigation } from "../../../src/SearchResults";
import { useWorkspaceSearch } from "../../../src/useWorkspaceSearch";
import type { NovelCodexApi, SearchResult } from "../../../src/types";

window.addEventListener("error", event => { window.testResult = { passed: false, error: event.error?.stack ?? event.message }; });
const tick = () => new Promise(resolve => setTimeout(resolve, 25));
const check = (condition, message) => { if (!condition) throw new Error(message); };
const until = async predicate => { for (let i = 0; i < 100 && !predicate(); i++) await tick(); check(predicate(), "Search UI did not settle"); };
const pending = [], targets = [], states = [];
const api = { runtime: { search: (projectId, query, limit) => new Promise((resolve, reject) => pending.push({ projectId, query, limit, resolve, reject })) } } as unknown as NovelCodexApi;
const chapter = { id: "chapter-one", number: 1, title: "第一章", status: "draft", summary: "", updated_at: "", passages: [] };
const result = (kind = "draft", sourcePath = "/sceneGroups/0/body", id = "draft-chapter-one") => ({ kind, id, title: "明日来信", snippet: "明日，主角收到一封信。", sourceRevisionId: "revision-one", sourcePath, source: "story-revision", evidence: { quote: "明日", start: 0, end: 2 } }) as SearchResult;
const response = (results = [], complete = true) => ({ results, nextCursor: null, coverage: { complete } });
function Harness() {
  const [projectId, setProjectId] = useState("A");
  const search = useWorkspaceSearch(api, projectId);
  return <><button onClick={() => setProjectId(value => value === "A" ? "B" : "A")}>切作品</button>
    <button onClick={search.toggle}>搜索</button>
    {search.open && <><input aria-label="搜索作品内容" value={search.query} onChange={event => search.setQuery(event.target.value)}
      onKeyDown={event => { if (event.key === "Enter" && !event.nativeEvent.isComposing) { event.preventDefault(); void search.submit(); } }} />
      <SearchResults state={search.state} onRetry={() => void search.submit()} onSelect={item => {
        const target = searchNavigation(item, [chapter]);
        if (!target) { search.reportError("对应章节已不可用，请重新搜索。"); return; }
        targets.push(target);
      }} /></>}
  </>;
}
const root = createRoot(document.getElementById("root")); root.render(<Harness />);
const button = text => [...document.querySelectorAll("button")].find(node => node.textContent === text);
const type = async value => { const input = document.querySelector("input"); Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(input, value); input.dispatchEvent(new Event("input", { bubbles: true })); await tick(); return input; };
const enter = input => input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
const panel = () => document.querySelector('[aria-label="作品搜索结果"]');
(async () => { try {
  await until(() => button("搜索")); button("搜索").click(); await tick();
  let input = await type("明日"); input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", isComposing: true, bubbles: true })); await tick();
  check(pending.length === 0, "IME confirmation submitted search"); enter(input); enter(input); await tick();
  check(pending.length === 1 && pending[0].limit === 20, "duplicate Enter or wrong bound");
  states.push({ loading: panel().textContent });
  pending[0].resolve(response([result(), result("draft", "/sceneGroups/1/body"), ...["brief", "outline", "plan"].map(kind => result(kind, "/title", kind))]));
  await until(() => panel()?.querySelectorAll("button").length === 5);
  states.push({ results: [...panel().querySelectorAll("button")].map(node => node.textContent) });
  [...panel().querySelectorAll("button")].forEach(node => node.click()); await tick();
  input = await type("修改关键词"); check(!panel(), "edited query retained old results"); enter(input); await tick();
  await type("新关键词"); pending[1].resolve(response([result()])); await tick(); check(!panel(), "late previous query replaced edited query");
  input = document.querySelector("input"); enter(input); await tick(); pending[2].reject(Error("受控读取失败"));
  await until(() => panel()?.querySelector('[role="alert"]')); states.push({ error: panel().textContent });
  button("重试搜索").click(); await tick(); check(pending.length === 4, "retry did not read once"); pending[3].resolve(response([], true));
  await until(() => panel()?.textContent.includes("未找到")); states.push({ empty: panel().textContent });
  input = await type("部分"); enter(input); await tick(); pending[4].resolve(response([], false));
  await until(() => panel()?.textContent.includes("未覆盖")); states.push({ partial: panel().textContent });
  input = await type("失效章节"); enter(input); await tick(); pending[5].resolve(response([result("draft", "/body", "draft-deleted")]));
  await until(() => panel()?.querySelector("button")); panel().querySelector("button").click(); await tick();
  check(panel().textContent.includes("对应章节已不可用"), "missing chapter silently navigated"); states.push({ missing: panel().textContent });
  input = await type("跨作品"); enter(input); await tick(); button("切作品").click(); await tick();
  check(!document.querySelector("input") && !panel(), "previous project search survived switch");
  pending[6].resolve(response([result()])); await tick(); check(!panel(), "late project A result leaked into B");
  button("搜索").click(); await tick(); input = await type("跨作品"); enter(input); await tick();
  check(pending[7].projectId === "B", "new project search was blocked or used A");
  button("搜索").click(); await tick(); pending[7].resolve(response([result()])); await tick();
  button("搜索").click(); await tick(); check(!panel(), "closed search resurrected late result");
  root.unmount(); window.testResult = { passed: true, states, targets };
} catch (error) { window.testResult = { passed: false, error: error.stack }; } })();
