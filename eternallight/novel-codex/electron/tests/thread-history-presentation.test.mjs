import assert from "node:assert/strict";
import { test } from "node:test";
import { presentAuthorHistory } from "../thread-history-presentation.mjs";

test("restored discussion uses author records and never displays composed model prompts", () => {
  const assistant = { id: "answer", type: "agentMessage", phase: "final_answer", text: "人物分析" };
  const detail = { thread: { id: "thread-a" }, turns: [
    { id: "turn-a", startedAt: 123, items: [{ type: "userMessage", text: "SECRET composed system/task prompt" }, assistant] },
    { id: "turn-b", items: [{ type: "userMessage", text: "generated technical instruction" }, assistant] },
    { id: "turn-c", items: [{ type: "userMessage", text: "unbound technical instruction" }] },
  ] };
  const presented = presentAuthorHistory(detail, { threadId: "thread-a", messages: [{ turnId: "turn-a", text: "分析人物" }, { turnId: "turn-b", text: null }] });
  assert.deepEqual(presented.turns[0], { id: "turn-a", startedAt: 123, items: [
    { id: "author:turn-a", type: "userMessage", status: "completed", phase: null, text: "分析人物" }, assistant,
  ] });
  assert.deepEqual(presented.turns.slice(1).map(turn => turn.items), [[assistant], []]);
  assert.equal(detail.turns[0].items[0].text, "SECRET composed system/task prompt");
  assert.throws(() => presentAuthorHistory(detail, { threadId: "thread-b", messages: [] }), /不匹配/);
});

test("reopening author history removes private reasoning and opaque fields without rewriting source", () => {
  const raw = "PRIVATE reasoning and opaque payload";
  const detail = { thread: { id: "thread" }, turns: [{ id: "turn", items: [
    { id: "private", type: "reasoning", text: raw, summary: raw, encrypted_content: raw },
    { id: "progress", type: "agentMessage", phase: "commentary", text: "正在检查本章", encrypted_content: raw },
    { id: "reply", type: "agentMessage", phase: "final_answer", text: "本章已保存", opaque: raw },
    { id: "tool", type: "mcpToolCall", status: "completed", text: "保存成功", content: [{ text: raw }] },
  ] }] };
  const original = structuredClone(detail);
  for (let reopened = 0; reopened < 2; reopened++) {
    const view = presentAuthorHistory(detail, { threadId: "thread", messages: [{ turnId: "turn", text: "检查本章" }] });
    assert.deepEqual(view.turns[0].items, [
      { id: "author:turn", type: "userMessage", status: "completed", phase: null, text: "检查本章" },
      { id: "progress", type: "agentMessage", phase: "commentary", text: "正在检查本章" },
      { id: "reply", type: "agentMessage", phase: "final_answer", text: "本章已保存" },
      { id: "tool", type: "mcpToolCall", status: "completed", text: "保存成功" },
    ]);
    assert.equal(JSON.stringify(view).includes(raw), false);
  }
  assert.deepEqual(detail, original);
});
