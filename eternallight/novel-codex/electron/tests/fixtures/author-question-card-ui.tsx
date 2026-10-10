import "../../../src/styles.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { AuthorQuestionCard, type UserInputQuestion, type UserInputResponse } from "../../../src/AuthorQuestionCard";

const tick = () => new Promise(resolve => setTimeout(resolve, 25));
const check = (value, message) => { if (!value) throw Error(message); };
const until = async (predicate, message) => {
  for (let index = 0; index < 100 && !predicate(); index++) await tick();
  check(predicate(), message);
};
const root = createRoot(document.getElementById("root"));
const calls: { identity: string; response: UserInputResponse }[] = [];
let identity = "project-lantern:thread-author:turn-7:request-1";
let pending: { resolve: (accepted: boolean) => void; reject: (error: Error) => void } | null = null;
const questions: UserInputQuestion[] = [
  { id: "direction", header: "故事方向", question: "这两种推进方式，你希望怎样取舍？", isOther: false,
    options: [{ label: "从人物误解开始", description: "先呈现关系变化，再揭示线索。" }, { label: "从夜巡事件开始", description: "直接进入事件，让人物在行动中暴露分歧。" }] },
  { id: "tone", header: "叙事节奏", question: "这一段的节奏希望怎样安排？", isOther: true,
    options: [{ label: "克制推进", description: "让紧张感逐步累积。" }, { label: "快速展开", description: "更早出现决定性变化。" }] },
  { id: "preserve", header: "保留内容", question: "有哪些内容你希望保留？", isOther: false, options: [] },
];
function render() {
  const current = identity;
  root.render(<main style={{ padding: 30, maxWidth: 800, margin: "auto" }}>
    <p>可以继续比较、组合建议，也可以直接说自己的想法。</p>
    <div className="conversation-interaction"><div className="conversation-interaction-heading"><span>需要你的回答</span><small>回答后创作引擎会继续当前回合</small></div>
      <AuthorQuestionCard requestIdentity={current} questions={questions} onResolve={response => {
        calls.push({ identity: current, response: structuredClone(response) });
        return new Promise<boolean>((resolve, reject) => { pending = { resolve, reject }; });
      }} />
    </div>
  </main>);
}
const input = (questionIndex = 0) => document.querySelectorAll<HTMLTextAreaElement>(".approval-input")[questionIndex];
const submit = () => document.querySelector<HTMLButtonElement>(".approval-card-actions button");
const option = (label: string) => document.querySelector<HTMLButtonElement>(`button[aria-label="使用建议：${label}"]`);
async function type(index: number, value: string) {
  const element = input(index);
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value").set.call(element, value);
  element.dispatchEvent(new Event("input", { bubbles: true })); await tick();
}
function key(element: HTMLElement, options = {}) {
  const event = new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true, ...options });
  element.dispatchEvent(event); return event;
}

(async () => {
  try {
    render(); await until(() => input(2), "Questions did not render");
    check(document.querySelectorAll(".approval-input").length === 3 && !document.querySelector('input[type="radio"]'), "Options limited free answers or introduced a forced Other choice");
    check(Array.from(document.querySelectorAll<HTMLTextAreaElement>("textarea")).every(element => element.value === "") && submit().disabled && calls.length === 0, "A suggestion was preselected or submitted");
    await type(0, "把两种方式结合：先让人物误解，再由夜巡暴露分歧。");
    check(input().value.includes("结合") && submit().disabled, "A false isOther flag blocked own words or skipped unanswered questions");
    option("从人物误解开始").focus(); option("从人物误解开始").click(); await tick();
    check(input().value === "从人物误解开始" && document.activeElement === input(), "Suggestion did not fill and focus the editable answer");
    await type(0, "从人物误解开始，再与夜巡事件结合；先只分析。\n保留人物的不同立场。");
    await type(1, "整体克制，但夜巡发生时加快；不只选择一种节奏。");
    check(input(1).value.includes("不只") && submit().disabled, "A true isOther flag still forced the Other control or lost question separation");
    await type(2, "保留灯税期限、旧港的场景和林遥的疑虑。");
    check(!submit().disabled && input().maxLength === 240, "Complete answers could not be sent or changed the bounded answer contract");
    check(!key(input(), {}).defaultPrevented && !key(input(), { shiftKey: true }).defaultPrevented, "Ordinary Enter no longer permits natural multiline answers");
    key(input(), { ctrlKey: true, isComposing: true }); await tick();
    check(calls.length === 0, "Input-method confirmation submitted answers");
    window.testScreenshot = "author questions free answers";
    await until(() => !window.testScreenshot, "Screenshot harness did not capture the questions");

    const exactAnswers = { direction: input().value, tone: input(1).value, preserve: input(2).value };
    key(input(), { ctrlKey: true }); key(input(), { ctrlKey: true }); submit().click(); await tick();
    check(calls.length === 1 && JSON.stringify(calls[0].response.answers) === JSON.stringify(exactAnswers), "Repeated submits duplicated delivery or changed free text");
    check(submit().disabled && Array.from(document.querySelectorAll<HTMLFieldSetElement>("fieldset")).every(element => element.disabled)
      && input().value === exactAnswers.direction, "Pending answers remained mutable or disappeared");
    pending.reject(new Error("Fixture delivery interrupted"));
    await until(() => document.querySelector(".user-input-error"), "Failure did not allow recovery");
    check(!submit().disabled && input().value === exactAnswers.direction && input(1).value === exactAnswers.tone && input(2).value === exactAnswers.preserve,
      "Failed delivery lost an author answer");
    submit().click(); await tick(); check(calls.length === 2, "Failure could not be retried");
    pending.resolve(false); await until(() => !submit().disabled && document.querySelector(".user-input-error"), "Rejected acknowledgment consumed the answer");
    check(input().value === exactAnswers.direction, "Rejected acknowledgment erased the response");
    key(input(), { metaKey: true }); await tick(); check(calls.length === 3, "Command+Enter did not submit the editable answers");
    pending.resolve(true); await until(() => input().value === "", "Successful acknowledgment did not clear the completed answer");
    check(submit().disabled && !document.querySelector(".user-input-error"), "Completed questions retained an error or old answers");

    await type(0, "旧问题的回答"); identity = "project-lantern:thread-author:turn-7:request-2"; render(); await tick();
    check(input().value === "" && input(1).value === "" && submit().disabled, "A consecutive request reused answers for the same question IDs");
    await type(0, "旧对话待提交的回答"); await type(1, "旧节奏"); await type(2, "旧保留项"); submit().click(); await tick();
    const oldPending = pending; identity = "project-lantern:thread-another:turn-8:request-2"; render(); await tick();
    check(input().value === "" && submit().disabled && !document.querySelector(".user-input-error"), "Thread switch retained pending question state");
    await type(0, "新对话自己的回答"); await type(1, "新节奏"); await type(2, "新保留项");
    oldPending.resolve(true); await tick();
    check(input().value === "新对话自己的回答" && !submit().disabled && calls.length === 4, "An old acknowledgment altered or submitted the new request");
    await type(0, "月".repeat(241)); check(input().value.length === 240, "Input exceeded the 240-character answer contract");
    window.testResult = { passed: true, checks: ["free answers with either option flag", "editable suggestion shortcuts", "all questions and exact answers", "keyboard and composition", "failure retains answers", "synchronous duplicate guard", "request identity reset", "old acknowledgment isolation", "240-character limit"] };
  } catch (error) { window.testResult = { passed: false, error: `${error.stack}\nDOM:${document.body.innerHTML}` }; }
})();
